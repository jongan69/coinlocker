// error_handling.rs
use axum::response::{IntoResponse, Response};
use axum::http::StatusCode;
use serde_json::json;
use thiserror::Error;
use kraken_rest_client::Error as KrakenError;
use std::num::ParseFloatError;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error")]
    DatabaseError(#[from] mongodb::error::Error),

    #[error("Environment variable error")]
    EnvVarError(#[from] std::env::VarError),

    #[error("Uuid parse error")]
    UuidError(#[from] uuid::Error),

    #[error("Internal server error")]
    InternalServerError,

    #[error("Decryption error")]
    DecryptionError,

    #[error("Bitcoin consensus error")]
    BitcoinConsensusError(#[from] bdk::bitcoin::consensus::encode::Error),

    #[error("Electrum client error")]
    ElectrumClientError(#[from] bdk::electrum_client::Error),

    #[error("Kraken API error")]
    KrakenError(#[from] KrakenError),

    #[error("Reqwest error")]
    ReqwestError(#[from] reqwest::Error),

    #[error("Serde JSON error")]
    SerdeJsonError(#[from] serde_json::Error),

    #[error("Custom error")]
    CustomError(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::DatabaseError(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            AppError::EnvVarError(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            AppError::UuidError(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::InternalServerError => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            AppError::DecryptionError => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::BitcoinConsensusError(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            AppError::ElectrumClientError(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            AppError::KrakenError(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            AppError::ReqwestError(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            AppError::SerdeJsonError(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            AppError::CustomError(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };

        (status, axum::Json(json!({"error": error_message}))).into_response()
    }
}

impl From<ParseFloatError> for AppError {
    fn from(parse_error: ParseFloatError) -> Self {
        let error_message = format!("Error converting string to float: {}", parse_error);
        AppError::CustomError(error_message)
    }
}

impl From<anyhow::Error> for AppError {
    fn from(error: anyhow::Error) -> Self {
        let _ = error;
        // Convert the `anyhow::Error` into an `AppError`
        // based on the specific error handling logic you want.
        // You can handle different error types here and map them to `AppError`.
        // For example:
        // match error.downcast_ref::<ParseFloatError>() {
        //     Some(parse_float_error) => AppError::from(parse_float_error),
        //     None => AppError::OtherError("Unknown error".to_string()),
        // }
        AppError::CustomError("An error occurred".to_string())
    }
}