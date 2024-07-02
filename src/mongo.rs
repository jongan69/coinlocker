// mongo.rs
use mongodb::{
    bson::{doc, DateTime as BsonDateTime, Document},
    Client, Collection, Database,
};
use serde::{Deserialize, Serialize};
use crate::error_handling::AppError;
use mongodb::bson::oid::ObjectId;

#[derive(Clone)]
pub struct AppState {
    pub db: mongodb::Database,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Transaction {
    pub txid: String,
    pub amount: f64,
    pub user_id: i64,
    pub status: String, // New field for transaction status
    pub processed: bool,
    pub timestamp: BsonDateTime,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct User {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub user_id: i64,
    pub username: String,
    pub first_name: String,
    pub last_name: Option<String>,
    pub api_key: Option<String>,
    pub btc_address: String,
    pub total_deposit: f64,
    pub lockin_total: f64,
    pub solana_public_key: Option<String>,
    pub solana_private_key: Option<String>,
    pub bitcoin_public_key: Option<String>,
    pub bitcoin_private_key: Option<String>,
    pub bitcoin_mnemonic: Option<String>,
    pub ethereum_public_key: Option<String>,
    pub ethereum_private_key: Option<String>,
}

pub async fn get_database() -> Result<Database, AppError> {
    let url = std::env::var("MONGO_URL")?;
    let client = Client::with_uri_str(&url).await?;
    Ok(client.database("telegram_bot"))
}

pub async fn get_users_collection() -> Result<Collection<User>, AppError> {
    let db = get_database().await?;
    Ok(db.collection("users"))
}

pub async fn get_transactions_collection() -> Result<Collection<Document>, AppError> {
    let db = get_database().await?;
    Ok(db.collection("transactions"))
}