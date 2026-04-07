use axum::http::StatusCode;
use kafka::producer::{Producer, Record, RequiredAcks};
use mysql_async::{Pool, params, prelude::Queryable};
use serde::Serialize;
use serde_json::{Value, json};
use std::{env, fmt, time::Duration};
use uuid::Uuid;

use crate::service::app_state::AppState;

#[derive(Debug)]
pub enum IoServiceError {
    Config(String),
    Database(mysql_async::Error),
    Kafka(kafka::Error),
    Http(reqwest::Error),
    Json(serde_json::Error),
}

impl IoServiceError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::Config(_) | Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Kafka(_) | Self::Http(_) | Self::Json(_) => StatusCode::BAD_GATEWAY,
        }
    }
}

impl fmt::Display for IoServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(message) => write!(f, "{message}"),
            Self::Database(error) => write!(f, "database error: {error}"),
            Self::Kafka(error) => write!(f, "kafka error: {error}"),
            Self::Http(error) => write!(f, "http error: {error}"),
            Self::Json(error) => write!(f, "json error: {error}"),
        }
    }
}

impl std::error::Error for IoServiceError {}

impl From<mysql_async::Error> for IoServiceError {
    fn from(value: mysql_async::Error) -> Self {
        Self::Database(value)
    }
}

impl From<kafka::Error> for IoServiceError {
    fn from(value: kafka::Error) -> Self {
        Self::Kafka(value)
    }
}

impl From<reqwest::Error> for IoServiceError {
    fn from(value: reqwest::Error) -> Self {
        Self::Http(value)
    }
}

impl From<serde_json::Error> for IoServiceError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[derive(Serialize)]
struct UserRecord {
    id: u64,
    name: String,
    email: String,
    password: String,
}

pub async fn read_db(state: &AppState) -> Result<Value, IoServiceError> {
    let mut conn = state.db_pool.get_conn().await?;
    let users = conn
        .exec_map(
            "SELECT id, name, email, password FROM user WHERE name LIKE ? LIMIT 10 OFFSET 0",
            ("%huy%",),
            |(id, name, email, password)| UserRecord {
                id,
                name,
                email,
                password,
            },
        )
        .await?;
    drop(conn);

    Ok(json!({
        "operation": "io-read-db",
        "data": users,
    }))
}

pub async fn write_db(state: &AppState) -> Result<Value, IoServiceError> {
    let mut conn = state.db_pool.get_conn().await?;
    let token = Uuid::new_v4().to_string();
    let name = format!("huy-{token}");
    let email = "vanhuy2612@gmail.com".to_string();
    let password = token;

    conn.exec_drop(
        "INSERT INTO user (name, email, password) VALUES (:name, :email, :password)",
        params! {
            "name" => &name,
            "email" => &email,
            "password" => &password,
        },
    )
    .await?;

    let inserted_id = conn.last_insert_id().unwrap_or_default();
    drop(conn);

    Ok(json!({
        "operation": "io-write-db",
        "data": {
            "id": inserted_id,
            "name": name,
            "email": email,
            "password": password,
        }
    }))
}

pub async fn publish_to_queue(state: &AppState) -> Result<Value, IoServiceError> {
    let brokers = state.kafka_brokers.clone();
    let topic = state.kafka_topic.clone();
    tokio::task::spawn_blocking(move || -> Result<(), IoServiceError> {
        let mut producer = Producer::from_hosts(brokers)
            .with_ack_timeout(Duration::from_secs(10))
            .with_required_acks(RequiredAcks::One)
            .create()?;
        let payload: &[u8] = b"bar";
        let record = Record::from_value(&topic, payload);
        producer.send(&record)?;
        Ok(())
    })
    .await
    .map_err(|error| IoServiceError::Config(format!("kafka task join error: {error}")))??;

    Ok(json!({
        "operation": "io-publish-2-queue",
        "data": {
            "topic": state.kafka_topic,
            "payload": "bar",
            "published": true,
        }
    }))
}

pub async fn call_external_api(state: &AppState) -> Result<Value, IoServiceError> {
    let response = state
        .http_client
        .get(&state.weather_url)
        .send()
        .await?
        .error_for_status()?;
    let weather: Value = response.json().await?;

    Ok(json!({
        "operation": "io-call-external-api",
        "data": weather,
    }))
}

pub fn parse_go_mysql_dsn(dsn: &str) -> Result<mysql_async::Opts, IoServiceError> {
    let (credentials, rest) = dsn
        .split_once("@tcp(")
        .ok_or_else(|| IoServiceError::Config("invalid DB_URL format".to_string()))?;
    let (host_port, database_and_query) = rest
        .split_once(")/")
        .ok_or_else(|| IoServiceError::Config("invalid DB_URL host/database format".to_string()))?;
    let (user, password) = credentials
        .split_once(':')
        .ok_or_else(|| IoServiceError::Config("invalid DB_URL credentials format".to_string()))?;
    let (database, query) = database_and_query
        .split_once('?')
        .unwrap_or((database_and_query, ""));
    let (host, port) = match host_port.rsplit_once(':') {
        Some((host, port)) => {
            let port = port
                .parse::<u16>()
                .map_err(|_| IoServiceError::Config("invalid DB_URL port".to_string()))?;
            (host.to_string(), port)
        }
        None => (host_port.to_string(), 3306),
    };

    let mut builder = mysql_async::OptsBuilder::default()
        .ip_or_hostname(host)
        .tcp_port(port)
        .user(Some(user))
        .pass(Some(password))
        .db_name(Some(database));

    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        if let Some((key, value)) = pair.split_once('=') {
            if key.eq_ignore_ascii_case("charset") {
                builder = builder.init(vec![format!("SET NAMES {value}")]);
            }
        }
    }

    Ok(mysql_async::Opts::from(builder))
}

pub fn build_weather_url() -> String {
    env::var("OPENWEATHER_URL").unwrap_or_else(|_| {
        "https://api.openweathermap.org/data/2.5/onecall?lat=21.1167&lon=105.8833&units=metric&appid=5796abbde9106b7da4febfae8c44c232".to_string()
    })
}

pub fn build_db_pool(dsn: &str) -> Result<Pool, IoServiceError> {
    let options = parse_go_mysql_dsn(dsn)?;
    Ok(Pool::new(options))
}
