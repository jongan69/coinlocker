// solana.rs
use serde::Serialize; // Importing serde for serialization
use solana_sdk::bs58; // Importing bs58 for base58 encoding
use solana_sdk::signer::keypair::Keypair; // Importing Keypair from solana_sdk for key generation
use solana_sdk::signer::Signer; // Importing Signer trait for signing operations

use crate::error_handling::AppError; // Importing custom error handling

// Define the structure for the response of the Solana wallet generation
#[derive(Serialize)]
pub struct SolWalletResponse {
    pub public_key: String,
    pub private_key: String,
}

// Asynchronous function to generate a Solana wallet
pub(crate) async fn generate_solana_wallet() -> Result<SolWalletResponse, AppError> {
    let keypair = Keypair::new(); // Generate a new keypair
    let public_key = keypair.pubkey().to_string(); // Get the public key as a string
    let private_key = bs58::encode(keypair.to_bytes()).into_string(); // Encode the private key in base58
    Ok(SolWalletResponse {
        public_key,
        private_key,
    }) // Return the public and private keys in the response struct
}
