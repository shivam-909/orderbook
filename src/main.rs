use std::error::Error;
use std::fs;
use std::sync::Arc;

use rustorderbook::config::ApplicationConfig;
use rustorderbook::exchanges::binance::BinanceConnector;
use rustorderbook::exchanges::connector::ConnectorManager;
use rustorderbook::order_book::{log_best_entries, BTreeOrderBook};

mod config;

fn load_config() -> Result<ApplicationConfig, Box<dyn Error + Send + Sync>> {
    let config_data = fs::read_to_string("config.toml")?;
    let config: ApplicationConfig = toml::from_str(&config_data)?;
    Ok(config)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let cfg = load_config()?;

    let mut handles = vec![];

    for exchange in cfg.exchanges {
        match exchange.name.as_str() {
            "binance" => {
                let connector = Arc::new(BinanceConnector::new(
                    exchange.ws_endpoint.clone(),
                    exchange.rest_endpoint.clone(),
                ));

                for symbol in exchange.symbols {
                    let order_book = Arc::new(BTreeOrderBook::new());

                    let connector_manager =
                        ConnectorManager::new(connector.clone(), order_book.clone());

                    let book_handle = tokio::spawn({
                        let symbol = symbol.clone();
                        async move {
                            connector_manager.run(&symbol).await;
                        }
                    });

                    handles.push(book_handle);

                    let log_handle = tokio::spawn({
                        let order_book = order_book.clone();
                        let symbol = symbol.clone();
                        let ex = exchange.name.clone();
                        async move {
                            log_best_entries(order_book, &symbol, &ex).await;
                        }
                    });

                    handles.push(log_handle);
                }
            }
            _ => panic!("Unknown exchange: {}", exchange.name),
        }
    }

    for handle in handles {
        handle.await?;
    }

    Ok(())
}
