// Deecrypt.rs
// Import necessary modules and libraries
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

// Struct for deserializing API key payload from the request body
#[derive(Debug, Deserialize)]
pub struct ApiKeyPayload {
    api_key: String,
}

// Asynchronous handler function for decrypting user keys
pub async fn decrypt_keys_handler(
    State(state): State<Arc<AppState>>, // Extract shared application state
    Json(payload): Json<ApiKeyPayload>,  // Extract JSON payload from request body
) -> impl IntoResponse {
    let api_key = payload.api_key;

    // Fetch user from the database by API key
    let user = match get_user_by_api_key(&state.db, &api_key).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            // If no user is found, respond with 404 status code
            return (StatusCode::NOT_FOUND, "User not found").into_response();
        }
        Err(err) => {
            // If database query fails, log the error and respond accordingly
            error!("Failed to query database: {}", err);
            return err.into_response();
        }
    };

    // Ensure the API key is 32 bytes long for AES-256 encryption
    let key_bytes = {
        let mut key_bytes = vec![0; 32];
        let api_key_bytes = api_key.as_bytes();
        let len = std::cmp::min(api_key_bytes.len(), 32);
        key_bytes[..len].copy_from_slice(&api_key_bytes[..len]);
        key_bytes
    };

    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);

    // Decrypt Solana private key
    let solana_private_key = match decrypt_data(&user.solana_private_key.unwrap_or_default(), key) {
        Ok(key) => key,
        Err(_) => {
            error!("Failed to decrypt Solana private key");
            return AppError::DecryptionError.into_response();
        }
    };

    // Decrypt Bitcoin private key
    let bitcoin_private_key = match decrypt_data(&user.bitcoin_private_key.unwrap_or_default(), key) {
        Ok(key) => key,
        Err(_) => {
            error!("Failed to decrypt Bitcoin private key");
            return AppError::DecryptionError.into_response();
        }
    };

    // Decrypt Ethereum private key
    let ethereum_private_key = match decrypt_data(&user.ethereum_private_key.unwrap_or_default(), key) {
        Ok(key) => key,
        Err(_) => {
            error!("Failed to decrypt Ethereum private key");
            return AppError::DecryptionError.into_response();
        }
    };

    // Create JSON response with decrypted keys
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

    // Respond with 200 status code and JSON payload
    (StatusCode::OK, ResponseJson(response)).into_response()
}

// Asynchronous function to get a user from the database by API key
async fn get_user_by_api_key(db: &mongodb::Database, api_key: &str) -> Result<Option<User>, AppError> {
    let collection = db.collection::<User>("users");
    let filter = doc! { "api_key": api_key };
    let user = collection.find_one(filter, None).await.map_err(AppError::DatabaseError)?;
    Ok(user)
}

// Function to decrypt data using AES-256-GCM
fn decrypt_data(data: &str, key: &Key<Aes256Gcm>) -> Result<String, AppError> {
    let cipher = Aes256Gcm::new(key);
    let decoded_data = hex::decode(data).map_err(|_| AppError::DecryptionError)?;

    // Ensure there is enough data for a nonce and ciphertext
    if decoded_data.len() < 12 {
        return Err(AppError::DecryptionError);
    }

    let (nonce_bytes, ciphertext) = decoded_data.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    // Decrypt the data and convert to a UTF-8 string
    let plaintext = cipher.decrypt(nonce, ciphertext).map_err(|_| AppError::DecryptionError)?;
    String::from_utf8(plaintext).map_err(|_| AppError::DecryptionError)
}
