// solana.rs
use serde::Serialize;
use solana_sdk::bs58;
use solana_sdk::signer::keypair::Keypair;
use solana_sdk::signer::Signer;

use crate::error_handling::AppError;

#[derive(Serialize)]
pub struct SolWalletResponse {
    pub public_key: String,
    pub private_key: String,
}

pub(crate) async fn generate_solana_wallet() -> Result<SolWalletResponse, AppError> {
    let keypair = Keypair::new();
    let public_key = keypair.pubkey().to_string();
    let private_key = bs58::encode(keypair.to_bytes()).into_string();
    Ok(SolWalletResponse {
        public_key,
        private_key,
    })
}