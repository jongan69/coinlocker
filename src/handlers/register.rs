// register.rs
// Import necessary modules and libraries
use axum::{extract::Json, http::StatusCode, response::IntoResponse};
use mongodb::bson::doc;
use serde::Deserialize;
use serde_json::json;
use tracing::error;
use uuid::Uuid as UuidGenerator;
use aes_gcm::{Aes256Gcm, Key, Nonce};
use aes_gcm::aead::{Aead, KeyInit};
use rand::RngCore;
use hex;
use typenum::U12;

use crate::mongo::{get_users_collection, User};
use crate::wallets::solana::SolWalletResponse;
use crate::wallets::bitcoin::WalletResponse;
use crate::wallets::ethereum::EthereumWallet;
use crate::wallets::{bitcoin::generate_bitcoin_wallet, ethereum::generate_keypair, solana::generate_solana_wallet};
use crate::error_handling::AppError;

// Struct for deserializing the register request payload
#[derive(Deserialize)]
pub struct RegisterRequest {
    user_id: i64,
}

// Function to encrypt data using AES-256-GCM
fn encrypt(data: &str, key: &Key<Aes256Gcm>, nonce: &Nonce<U12>) -> Result<String, AppError> {
    let cipher = Aes256Gcm::new(key);
    let mut ciphertext = cipher.encrypt(nonce, data.as_bytes())
        .map_err(|_| AppError::InternalServerError)?;

    // Prepend the nonce to the ciphertext
    let mut result = nonce.to_vec();
    result.append(&mut ciphertext);
    Ok(hex::encode(result))
}

// Asynchronous handler function for registering a user and generating wallets
pub async fn register(Json(payload): Json<RegisterRequest>) -> impl IntoResponse {
    // Get the users collection from the database
    let users_collection = match get_users_collection().await {
        Ok(collection) => collection,
        Err(err) => {
            error!("Failed to get users collection: {}", err);
            return AppError::InternalServerError.into_response();
        }
    };

    // Check if the user exists in the database
    let user_filter = doc! { "user_id": payload.user_id };
    let mut user = match users_collection.find_one(user_filter.clone(), None).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, Json("User not found".to_string())).into_response();
        }
        Err(err) => {
            error!("Database query error for user {}: {}", payload.user_id, err);
            return AppError::InternalServerError.into_response();
        }
    };

    // Check if the user already has wallets
    if user_has_wallets(&user) {
        return (StatusCode::BAD_REQUEST, Json("User already has wallets".to_string())).into_response();
    }

    // Generate and save wallets for the user
    let (solana_wallet, bitcoin_wallet, ethereum_wallet, api_key) = match generate_and_save_wallets(&mut user).await {
        Ok(wallets) => wallets,
        Err(err) => {
            error!("Failed to generate wallets: {}", err);
            return AppError::InternalServerError.into_response();
        }
    };

    // Update the user in the database with the new wallet information
    if let Err(err) = users_collection.replace_one(user_filter, user, None).await {
        error!("Failed to update user: {}", err);
        return AppError::InternalServerError.into_response();
    }

    // Create JSON response with generated API key and wallet information
    let response = json!({
        "api_key": api_key,
        "solana_public_key": solana_wallet.public_key,
        "solana_private_key": solana_wallet.private_key,
        "bitcoin_mnemonic": bitcoin_wallet.mnemonic,
        "bitcoin_public_key": bitcoin_wallet.public_key,
        "bitcoin_private_key": bitcoin_wallet.private_key,
        "ethereum_public_key": ethereum_wallet.public_key,
        "ethereum_private_key": ethereum_wallet.secret_key,
    });

    // Respond with 200 status code and JSON payload
    (StatusCode::OK, Json(response)).into_response()
}

// Function to check if a user already has wallets
fn user_has_wallets(user: &User) -> bool {
    user.solana_public_key.is_some() && user.solana_private_key.is_some() &&
    !user.solana_public_key.as_ref().unwrap().is_empty() && !user.solana_private_key.as_ref().unwrap().is_empty()
}

// Asynchronous function to generate and save wallets for a user
async fn generate_and_save_wallets(user: &mut User) -> Result<(SolWalletResponse, WalletResponse, EthereumWallet, String), AppError> {
    // Generate a new API key
    let api_key = UuidGenerator::new_v4().to_string();
    user.api_key = Some(api_key.clone());

    // Ensure the key is 32 bytes for AES-256
    let key = Key::<Aes256Gcm>::from_slice(&api_key.as_bytes()[..32]);

    // Generate a random nonce of exactly 12 bytes
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Generate Solana wallet and encrypt the private key
    let solana_wallet = generate_solana_wallet().await?;
    user.solana_public_key = Some(solana_wallet.public_key.clone());
    user.solana_private_key = Some(encrypt(&solana_wallet.private_key, key, nonce)?);

    // Generate Bitcoin wallet and encrypt the mnemonic and private key
    let bitcoin_wallet = generate_bitcoin_wallet().await?;
    user.bitcoin_mnemonic = Some(encrypt(&bitcoin_wallet.mnemonic, key, nonce)?);
    user.bitcoin_public_key = Some(bitcoin_wallet.public_key.clone());
    user.bitcoin_private_key = Some(encrypt(&bitcoin_wallet.private_key, key, nonce)?);

    // Generate Ethereum wallet and encrypt the private key
    let (secret_key, pub_key, pub_address) = generate_keypair();
    user.ethereum_public_key = Some(pub_key.to_string());
    user.ethereum_private_key = Some(encrypt(&secret_key.to_string(), key, nonce)?);

    // Return generated wallets and API key
    Ok((solana_wallet, bitcoin_wallet, EthereumWallet {
        public_key: pub_key.to_string(),
        secret_key: secret_key.to_string(),
        public_address: pub_address.to_string(),
    }, api_key))
}
