// Deecrypt.rs
use axum::{extract::{State, Json}, http::StatusCode, response::IntoResponse, Json as ResponseJson};
use mongodb::bson::doc;
use serde::Deserialize;
use serde_json::json;
use tracing::error;
use aes_gcm::{Aes256Gcm, Key, Nonce};
use aes_gcm::aead::{Aead, KeyInit};
use hex;
use std::sync::Arc;

use crate::mongo::{AppState, User};
use crate::error_handling::AppError;

#[derive(Debug, Deserialize)]
pub struct ApiKeyPayload {
    api_key: String,
}

pub async fn decrypt_keys_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ApiKeyPayload>,
) -> impl IntoResponse {
    let api_key = payload.api_key;

    let user = match get_user_by_api_key(&state.db, &api_key).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "User not found").into_response();
        }
        Err(err) => {
            error!("Failed to query database: {}", err);
            return err.into_response();
        }
    };

    // Ensure the key is 32 bytes for AES-256
    let key_bytes = {
        let mut key_bytes = vec![0; 32];
        let api_key_bytes = api_key.as_bytes();
        let len = std::cmp::min(api_key_bytes.len(), 32);
        key_bytes[..len].copy_from_slice(&api_key_bytes[..len]);
        key_bytes
    };

    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);

    let solana_private_key = match decrypt_data(&user.solana_private_key.unwrap_or_default(), key) {
        Ok(key) => key,
        Err(_) => {
            error!("Failed to decrypt Solana private key");
            return AppError::DecryptionError.into_response();
        }
    };

    let bitcoin_private_key = match decrypt_data(&user.bitcoin_private_key.unwrap_or_default(), key) {
        Ok(key) => key,
        Err(_) => {
            error!("Failed to decrypt Bitcoin private key");
            return AppError::DecryptionError.into_response();
        }
    };

    let ethereum_private_key = match decrypt_data(&user.ethereum_private_key.unwrap_or_default(), key) {
        Ok(key) => key,
        Err(_) => {
            error!("Failed to decrypt Ethereum private key");
            return AppError::DecryptionError.into_response();
        }
    };

    let response = json!({
        "solana": {
            "private_key": solana_private_key,
        },
        "bitcoin": {
            "private_key": bitcoin_private_key,
        },
        "ethereum": {
            "private_key": ethereum_private_key,
        }
    });

    (StatusCode::OK, ResponseJson(response)).into_response()
}

async fn get_user_by_api_key(db: &mongodb::Database, api_key: &str) -> Result<Option<User>, AppError> {
    let collection = db.collection::<User>("users");
    let filter = doc! { "api_key": api_key };
    let user = collection.find_one(filter, None).await.map_err(AppError::DatabaseError)?;
    Ok(user)
}

fn decrypt_data(data: &str, key: &Key<Aes256Gcm>) -> Result<String, AppError> {
    let cipher = Aes256Gcm::new(key);
    let decoded_data = hex::decode(data).map_err(|_| AppError::DecryptionError)?;

    if decoded_data.len() < 12 {
        return Err(AppError::DecryptionError);
    }

    let (nonce_bytes, ciphertext) = decoded_data.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher.decrypt(nonce, ciphertext).map_err(|_| AppError::DecryptionError)?;
    String::from_utf8(plaintext).map_err(|_| AppError::DecryptionError)
}
