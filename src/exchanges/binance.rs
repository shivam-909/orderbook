use super::connector::{Connector, ConnectorError};
use crate::order_book::OrderBookOperations;
use async_trait::async_trait;
use futures::StreamExt;
use rand::{self, Rng};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::{error::Error, io::Read, str::FromStr, sync::Arc};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;

#[derive(Deserialize)]
struct DepthUpdateEvent {
    #[serde(rename = "e")]
    _e: String, // Event type
    #[serde(rename = "E")]
    _t: u64, // Event time
    #[serde(rename = "s")]
    _s: String, // Symbol
    #[serde(rename = "U")]
    first_update_id: u64,
    #[serde(rename = "u")]
    final_update_id: u64,
    #[serde(rename = "b")]
    b: Vec<(String, String)>, // Bids to be updated
    #[serde(rename = "a")]
    a: Vec<(String, String)>, // Asks to be updated
}

#[derive(Deserialize)]
struct OrderBookSnapshot {
    #[serde(rename = "lastUpdateId")]
    last_update_id: u64,
    bids: Vec<(String, String)>,
    asks: Vec<(String, String)>,
}

pub struct BinanceConnector {
    ws_endpoint: String,
    rest_endpoint: String,
}

// New enum to wrap events and errors
enum ConnectorEvent {
    Data(DepthUpdateEvent),
    Error(ConnectorError),
}

#[async_trait]
impl<O: OrderBookOperations + 'static> Connector<O> for BinanceConnector {
    async fn connect(
        &self,
        order_book: Arc<O>,
        symbol: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        println!(
            "Connecting to {}...",
            <Self as Connector<O>>::get_name(self)
        );
        self.initialize_and_process_events(order_book, symbol).await
    }

    async fn reconnect(
        &self,
        order_book: Arc<O>,
        symbol: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        println!(
            "Starting reconnection to {}...",
            <Self as Connector<O>>::get_name(self)
        );
        self.connect(order_book, symbol).await
    }

    fn get_name(&self) -> &str {
        "Binance"
    }
}

impl BinanceConnector {
    pub fn new(ws_endpoint: String, rest_endpoint: String) -> Self {
        Self {
            ws_endpoint,
            rest_endpoint,
        }
    }

    async fn retrieve_snapshot(
        &self,
        symbol: &str,
    ) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
        let snapshot_url = format!(
            "{}/api/v3/depth?symbol={}&limit=1000",
            self.rest_endpoint,
            symbol.to_uppercase(),
        );

        let snapshot = reqwest::get(&snapshot_url)
            .await
            .map_err(ConnectorError::NetworkError)?;

        let snapshot_bytes = snapshot
            .bytes()
            .await
            .map_err(ConnectorError::NetworkError)?
            .to_vec();

        Ok(snapshot_bytes)
    }

    // Retrieves snapshot and applies updates to entire orderbook
    // Orderbook is assumed to contain relevant updates at every price level
    // That is, if the connection was to drop and a snapshot was reapplied
    // There would be no missing updates that would lead to old data being retained
    // in the orderbook, since the orderbook is not cleared every time a snapshot
    // is applied.
    async fn initialize_book_from_snapshot<O: OrderBookOperations + 'static>(
        &self,
        snapshot_bytes: &[u8],
        order_book: Arc<O>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let snapshot: OrderBookSnapshot =
            serde_json::from_slice(snapshot_bytes).map_err(ConnectorError::DeserializationError)?;

        order_book.set_last_update_id(snapshot.last_update_id).await;

        for (price_str, amount_str) in snapshot.bids {
            let price = Decimal::from_str(&price_str)
                .map_err(|e| ConnectorError::ParseError(e.to_string()))?;

            let amount = Decimal::from_str(&amount_str)
                .map_err(|e| ConnectorError::ParseError(e.to_string()))?;

            order_book.update(price, amount, true).await;
        }

        for (price_str, amount_str) in snapshot.asks {
            let price = Decimal::from_str(&price_str)
                .map_err(|e| ConnectorError::ParseError(e.to_string()))?;

            let amount = Decimal::from_str(&amount_str)
                .map_err(|e| ConnectorError::ParseError(e.to_string()))?;

            order_book.update(price, amount, false).await;
        }

        Ok(())
    }

    // Reads WebSocket events and continuously reads raw bytes into transaction
    // channel as a defined event.
    // Until read, events will be buffered in the channel.
    async fn receive_events(
        &self,
        symbol: &str,
        tx: mpsc::UnboundedSender<ConnectorEvent>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let ws_url = format!(
            "{}/ws/{}@depth@100ms",
            self.ws_endpoint,
            symbol.to_lowercase()
        );

        Url::parse(&ws_url).map_err(|e| ConnectorError::ParseError(e.to_string()))?;

        let (ws_stream, _) = connect_async(&ws_url)
            .await
            .map_err(ConnectorError::WebSocketError)?;

        let (_, mut read) = ws_stream.split();

        tokio::spawn(async move {
            while let Some(message) = read.next().await {
                match message {
                    Ok(msg) => {
                        let bytes = match msg {
                            Message::Text(text) => text.into_bytes(),
                            Message::Binary(bin) => bin,
                            _ => continue, // Skip other message types
                        };

                        // Simulate a connection drop
                        if rand::thread_rng().gen_bool(0.0001) {
                            println!("Simulated connection drop!");
                            let _ = tx.send(ConnectorEvent::Error(ConnectorError::SimulatedDrop));
                            return;
                        }

                        match serde_json::from_slice::<DepthUpdateEvent>(&bytes) {
                            Ok(event) => {
                                if tx.send(ConnectorEvent::Data(event)).is_err() {
                                    return;
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(ConnectorEvent::Error(
                                    ConnectorError::DeserializationError(e),
                                ));
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(ConnectorEvent::Error(ConnectorError::WebSocketError(e)));
                        return;
                    }
                }
            }
        });

        Ok(())
    }

    // Reads events (buffered or live) from the transaction channel and applies updates
    async fn process_events<O: OrderBookOperations + 'static>(
        &self,
        mut rx: mpsc::UnboundedReceiver<ConnectorEvent>,
        order_book: Arc<O>,
    ) -> Result<(), ConnectorError> {
        let mut processed_first = false;

        while let Some(event) = rx.recv().await {
            match event {
                ConnectorEvent::Data(event) => {
                    let last_update_id = order_book.get_last_update_id().await;

                    if event.final_update_id <= last_update_id {
                        continue;
                    }

                    if !processed_first {
                        if event.first_update_id <= last_update_id + 1
                            && event.final_update_id >= last_update_id
                        {
                            processed_first = true;
                        } else {
                            continue;
                        }
                    } else if event.first_update_id != last_update_id + 1 {
                        println!("Event sequence mismatch. Re-syncing...");
                        return Err(ConnectorError::InconsistentSequence);
                    }

                    self.update_order_book_from_event(&event, order_book.clone())
                        .await?;
                    order_book.set_last_update_id(event.final_update_id).await;
                }
                ConnectorEvent::Error(e) => return Err(e),
            }
        }

        Err(ConnectorError::ChannelClosed)
    }

    async fn update_order_book_from_event<O: OrderBookOperations + 'static>(
        &self,
        event: &DepthUpdateEvent,
        order_book: Arc<O>,
    ) -> Result<(), ConnectorError> {
        for (price_str, amount_str) in &event.b {
            let price = Decimal::from_str(price_str)
                .map_err(|e| ConnectorError::ParseError(e.to_string()))?;
            let amount = Decimal::from_str(amount_str)
                .map_err(|e| ConnectorError::ParseError(e.to_string()))?;
            order_book.update(price, amount, true).await;
        }

        for (price_str, amount_str) in &event.a {
            let price = Decimal::from_str(price_str)
                .map_err(|e| ConnectorError::ParseError(e.to_string()))?;
            let amount = Decimal::from_str(amount_str)
                .map_err(|e| ConnectorError::ParseError(e.to_string()))?;
            order_book.update(price, amount, false).await;
        }

        Ok(())
    }

    // Spawns WebSocket handler, applies snapshot update, and spawns event handler.
    async fn initialize_and_process_events<O: OrderBookOperations + 'static>(
        &self,
        order_book: Arc<O>,
        symbol: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let (tx, rx) = mpsc::unbounded_channel();

        // Start receiving events
        self.receive_events(symbol, tx).await?;

        // Initialize the order book with snapshot
        let snapshot_bytes = self.retrieve_snapshot(symbol).await?;
        self.initialize_book_from_snapshot(&snapshot_bytes, order_book.clone())
            .await?;

        // Process buffered and live events
        self.process_events(rx, order_book).await?;

        Ok(())
    }
}
