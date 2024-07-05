use base64::engine::general_purpose::STANDARD as base64_engine;
use base64::Engine;
use bs58;
use dotenv::dotenv;
use jupiter_swap_api_client::{
    quote::{QuoteRequest, QuoteResponse},
    swap::{SwapInstructionsResponse, SwapRequest, SwapResponse},
    transaction_config::TransactionConfig,
    JupiterSwapApiClient,
};
use reqwest::Client;
use serde_json::json;
use solana_sdk::instruction::Instruction;
use solana_sdk::native_token::LAMPORTS_PER_SOL;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::transaction::Transaction;

use crate::error_handling::AppError;

pub struct LockinClient {
    client: Client,
    rpc_url: String,
    keypair: Keypair,
    jupiter_swap_api_client: JupiterSwapApiClient,
}

impl LockinClient {
    pub async fn new() -> Self {
        dotenv().ok();
        let base58privatekey = std::env::var("PRIVATE_KEY").expect("PRIVATE_KEY not set");

        // Decode the base58 private key and derive the keypair
        let private_key_bytes = bs58::decode(base58privatekey)
            .into_vec()
            .expect("Invalid base58 string");
        let keypair = Keypair::from_bytes(&private_key_bytes).expect("Invalid keypair bytes");

        let rpc_url = "https://api.mainnet-beta.solana.com".to_string();
        let jupiter_swap_api_client =
            JupiterSwapApiClient::new("https://quote-api.jup.ag/v6".to_string());

        Self {
            client: Client::new(),
            rpc_url,
            keypair,
            jupiter_swap_api_client,
        }
    }

    async fn get_user_private_key(&self, api_key: &str) -> Result<String, AppError> {
        let response = self
            .client
            .get("https://your-api-endpoint/decrypt_keys")
            .json(&json!({
                "api_key": api_key,
            }))
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        
        let encrypted_private_key = response["solana"]["private_key"]
            .as_str()
            .ok_or(AppError::CustomError("No private key in response".to_string()))?;
        
        Ok(encrypted_private_key.to_string())
    }

    pub async fn get_balance(&self, wallet_pubkey: &Pubkey) -> Result<u64, AppError> {
        let balance_response = self
            .client
            .post(&self.rpc_url)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getBalance",
                "params": [wallet_pubkey.to_string()]
            }))
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        balance_response["result"]["value"].as_u64().ok_or(AppError::CustomError("Failed to get balance".to_string()))
    }

    pub async fn get_quote(
        &self,
        amount: u64,
        input_mint: Pubkey,
        output_mint: Pubkey,
    ) -> Result<QuoteResponse, AppError> {
        let quote_request = QuoteRequest {
            amount,
            input_mint,
            output_mint,
            slippage_bps: 50,
            ..QuoteRequest::default()
        };

        self.jupiter_swap_api_client
            .quote(&quote_request)
            .await
            .map_err(AppError::from)
    }

    pub async fn perform_swap(
        &self,
        quote_response: QuoteResponse,
        receiving_address: Pubkey,
    ) -> Result<SwapResponse, AppError> {
        self.jupiter_swap_api_client
            .swap(&SwapRequest {
                user_public_key: receiving_address,
                quote_response: quote_response.clone(),
                config: TransactionConfig::default(),
            })
            .await
            .map_err(AppError::from)
    }

    pub async fn get_swap_instructions(
        &self,
        quote_response: QuoteResponse,
        receiving_address: Pubkey,
    ) -> Result<SwapInstructionsResponse, AppError> {
        self.jupiter_swap_api_client
            .swap_instructions(&SwapRequest {
                user_public_key: receiving_address,
                quote_response,
                config: TransactionConfig::default(),
            })
            .await
            .map_err(AppError::from)
    }

    pub async fn create_transaction(&self, instructions: Vec<Instruction>) -> Result<Transaction, AppError> {
        let recent_blockhash = self
            .client
            .post(&self.rpc_url)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getRecentBlockhash",
                "params": []
            }))
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?["result"]["value"]["blockhash"]
            .as_str()
            .ok_or(AppError::CustomError("Failed to get recent blockhash".to_string()))?
            .to_string();

        let recent_blockhash = recent_blockhash.parse().unwrap();
        
        let mut transaction = Transaction::new_with_payer(&instructions, Some(&self.keypair.pubkey()));
        transaction.sign(&[&self.keypair], recent_blockhash);

        Ok(transaction)
    }

    pub async fn send_transaction(&self, transaction: &Transaction) -> Result<serde_json::Value, AppError> {
        let serialized_transaction = bincode::serialize(transaction).unwrap();
        let base64_transaction = base64_engine.encode(&serialized_transaction);

        self.client
            .post(&self.rpc_url)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "sendTransaction",
                "params": [
                    base64_transaction,
                    { "encoding": "base64" }
                ]
            }))
            .send()
            .await?
            .json::<serde_json::Value>()
            .await
            .map_err(AppError::from)
    }

    pub async fn check_transaction_confirmation(
        &self,
        transaction_signature: &str,
    ) -> Result<serde_json::Value, AppError> {
        self.client
            .post(&self.rpc_url)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getTransaction",
                "params": [
                    transaction_signature,
                    { "encoding": "json" }
                ]
            }))
            .send()
            .await?
            .json::<serde_json::Value>()
            .await
            .map_err(AppError::from)
    }

    pub async fn execute(
        &self,
        input_mint: Pubkey,
        output_mint: Pubkey,
        receiving_address: Pubkey,
        user_api_key: String,
    ) -> Result<(), AppError> {
        let sending_address = self.keypair.pubkey();
        // let receiving_private_key = self.get_user_private_key(&user_api_key).await?;
        // let private_key_bytes = bs58::decode(receiving_private_key).into_vec().expect("Invalid base58 string");
        // let receiving_keypair = Keypair::from_bytes(&private_key_bytes).expect("Invalid keypair bytes");

        // Get balance
        let sol_balance = self.get_balance(&sending_address).await?;

        // Estimate gas fees and rent exemption fees
        let gas_fees = 0.001 * LAMPORTS_PER_SOL as f64;
        let rent_exemption_fee = 0.004 * LAMPORTS_PER_SOL as f64;
        let total_fees = gas_fees + rent_exemption_fee;
        let max_swap_amount = (sol_balance as f64 - total_fees) as u64;

        if max_swap_amount <= 0 {
            return Err(AppError::CustomError(format!("Insufficient balance for swap after accounting for fees. Balance: {} lamports, Total fees: {} lamports", sol_balance, total_fees as u64)));
        }

        println!("SOL Balance: {}", sol_balance);
        println!("Estimated Gas Fees: {}", gas_fees as u64);
        println!(
            "Estimated Rent Exemption Fees: {}",
            rent_exemption_fee as u64
        );
        println!("Max Swap Amount: {}", max_swap_amount);

        // Get quote
        let quote_response = self.get_quote(max_swap_amount, input_mint, output_mint).await?;

        // Perform swap
        let swap_response = self.perform_swap(quote_response.clone(), receiving_address).await?;
        println!("Raw tx len: {}", swap_response.swap_transaction.len());

        // Get swap instructions
        let swap_instructions_response = self.get_swap_instructions(quote_response, receiving_address).await?;
        println!("{swap_instructions_response:#?}");

        // Collect all instructions
        let mut instructions = Vec::new();
        instructions.extend(swap_instructions_response.setup_instructions);
        instructions.push(swap_instructions_response.swap_instruction);
        if let Some(cleanup_instruction) = swap_instructions_response.cleanup_instruction {
            instructions.push(cleanup_instruction);
        }

        // Create transaction with sender's keypair
        let transaction = self.create_transaction(instructions).await?;

        // Send transaction
        let send_transaction_response = self.send_transaction(&transaction).await?;
        println!("{send_transaction_response:#?}");

        // Check transaction confirmation status
        let confirmation_response = self.check_transaction_confirmation(send_transaction_response["result"].as_str().unwrap()).await?;
        println!("{confirmation_response:#?}");

        Ok(())
    }
}
