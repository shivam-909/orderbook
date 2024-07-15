use crate::order_book::OrderBookOperations;
use async_trait::async_trait;
use std::{error::Error, sync::Arc};

#[derive(Debug)]
pub enum ConnectorError {
    WebSocketError(tokio_tungstenite::tungstenite::Error),
    DeserializationError(serde_json::Error),
    ParseError(String),
    NetworkError(reqwest::Error),
    ChannelClosed,
    SimulatedDrop,
    InconsistentSequence,
}

impl std::fmt::Display for ConnectorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Error for ConnectorError {}

#[async_trait]
pub trait Connector<O: OrderBookOperations>: Send + Sync {
    // The connect method should establish a connection to the exchange and start receiving updates.
    async fn connect(
        &self,
        order_book: Arc<O>,
        symbol: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;
    // The reconnect method should attempt to reconnect to the exchange in case of a disconnection.
    // It should also manage order book reconstruction if necessary.
    async fn reconnect(
        &self,
        order_book: Arc<O>,
        symbol: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;
    fn get_name(&self) -> &str;
}

// The ConnectorManager struct is responsible for managing the connection to the exchange.
// It will attempt to connect to the exchange and continue to reconnect in case of a failure.
pub struct ConnectorManager<C, O>
where
    C: Connector<O>,
    O: OrderBookOperations,
{
    connector: Arc<C>,
    order_book: Arc<O>,
}

impl<C, O> ConnectorManager<C, O>
where
    C: Connector<O>,
    O: OrderBookOperations,
{
    pub fn new(connector: Arc<C>, order_book: Arc<O>) -> Self {
        Self {
            connector,
            order_book,
        }
    }

    pub async fn run(&self, symbol: &str) {
        loop {
            match self
                .connector
                .connect(self.order_book.clone(), symbol)
                .await
            {
                Ok(_) => {
                    println!("{} connected successfully", self.connector.get_name());
                    break;
                }

                Err(e) => {
                    println!("{} connection failed: {}", self.connector.get_name(), e);
                    println!("Attempting reconnect...");
                    if let Err(e) = self
                        .connector
                        .reconnect(self.order_book.clone(), symbol)
                        .await
                    {
                        println!("Reconnection failed: {}. Retrying in 5 seconds...", e);
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    }
                }
            }
        }
    }
}
