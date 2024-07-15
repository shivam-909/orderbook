use std::{collections::BTreeMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use rust_decimal::Decimal;
use tokio::{sync::RwLock, time::interval};

#[async_trait]
pub trait OrderBookOperations: Send + Sync {
    async fn update(&self, price: Decimal, vol: Decimal, is_bid: bool);
    async fn best_bid(&self) -> Option<(Decimal, Decimal)>;
    async fn best_ask(&self) -> Option<(Decimal, Decimal)>;
    async fn set_last_update_id(&self, id: u64);
    async fn get_last_update_id(&self) -> u64;
}

pub struct BTreeOrderBook {
    last_update_id: RwLock<u64>,
    pub bids: RwLock<BTreeMap<Decimal, Decimal>>,
    pub asks: RwLock<BTreeMap<Decimal, Decimal>>,
}

impl BTreeOrderBook {
    pub fn new() -> Self {
        BTreeOrderBook {
            bids: RwLock::new(BTreeMap::new()),
            asks: RwLock::new(BTreeMap::new()),
            last_update_id: RwLock::new(0),
        }
    }
}

impl Default for BTreeOrderBook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl OrderBookOperations for BTreeOrderBook {
    async fn update(&self, price: Decimal, vol: Decimal, is_bid: bool) {
        let mut book = if is_bid {
            self.bids.write().await
        } else {
            self.asks.write().await
        };

        if vol.is_zero() {
            // println!("Removing order book entry with price: {}", price);
            book.remove(&price);
        } else {
            // println!("Updating order book with price: {}, volume: {}", price, vol);
            book.insert(price, vol);
        }
    }

    async fn best_bid(&self) -> Option<(Decimal, Decimal)> {
        let bids = self.bids.read().await;
        bids.iter().next_back().map(|(price, vol)| (*price, *vol))
    }

    async fn best_ask(&self) -> Option<(Decimal, Decimal)> {
        let asks = self.asks.read().await;
        asks.iter().next().map(|(price, vol)| (*price, *vol))
    }

    async fn set_last_update_id(&self, id: u64) {
        *self.last_update_id.write().await = id;
    }

    async fn get_last_update_id(&self) -> u64 {
        *self.last_update_id.read().await
    }
}

pub async fn log_best_entries<O: OrderBookOperations>(
    order_book: Arc<O>,
    symbol: &str,
    exchange: &str,
) {
    let mut interval = interval(Duration::from_millis(100));

    loop {
        interval.tick().await;
        let best_bid = order_book.best_bid().await;
        let best_ask = order_book.best_ask().await;

        match (best_bid, best_ask) {
            (Some((bid_price, bid_vol)), Some((ask_price, ask_vol))) => {
                println!(
                    "[{}] {} - Best bid: {} @ {}, Best ask: {} @ {}",
                    exchange, symbol, bid_vol, bid_price, ask_vol, ask_price
                );
            }
            _ => {
                println!("[{}] {} - No data available", exchange, symbol);
            }
        }
    }
}
