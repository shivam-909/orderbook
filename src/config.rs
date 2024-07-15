use serde::Deserialize;

#[derive(Deserialize)]
pub struct ApplicationConfig {
    pub exchanges: Vec<ExchangeConfig>,
}

#[derive(Deserialize)]
pub struct ExchangeConfig {
    pub name: String,
    pub ws_endpoint: String,
    pub rest_endpoint: String,
    pub symbols: Vec<String>,
}
