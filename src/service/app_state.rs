use dotenvy::dotenv;
use reqwest::Client;
use std::{env, sync::Arc, time::Duration};

use crate::service::io_service::{IoServiceError, build_db_pool, build_weather_url};

pub struct AppState {
    pub db_pool: mysql_async::Pool,
    pub kafka_brokers: Vec<String>,
    pub kafka_topic: String,
    pub http_client: Client,
    pub weather_url: String,
}

impl AppState {
    pub async fn from_env() -> Result<Arc<Self>, IoServiceError> {
        let _ = dotenv();

        let db_url = env::var("DB_URL")
            .map_err(|_| IoServiceError::Config("DB_URL is not set".to_string()))?;
        let kafka_broker = env::var("KAFKA_BROKER")
            .map_err(|_| IoServiceError::Config("KAFKA_BROKER is not set".to_string()))?;
        let kafka_topic = env::var("KAFKA_TOPIC")
            .map_err(|_| IoServiceError::Config("KAFKA_TOPIC is not set".to_string()))?;
        let kafka_brokers = kafka_broker
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();

        if kafka_brokers.is_empty() {
            return Err(IoServiceError::Config("KAFKA_BROKER is empty".to_string()));
        }

        let db_pool = build_db_pool(&db_url)?;
        let http_client = Client::builder()
            .pool_max_idle_per_host(100)
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(IoServiceError::Http)?;

        Ok(Arc::new(Self {
            db_pool,
            kafka_brokers,
            kafka_topic,
            http_client,
            weather_url: build_weather_url(),
        }))
    }
}
