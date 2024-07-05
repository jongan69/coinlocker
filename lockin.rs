use dotenv::dotenv;
use base64::Engine;
use jupiter_swap_api_client::{
    quote::{QuoteRequest, QuoteResponse}, swap::{SwapRequest, SwapResponse, SwapInstructionsResponse}, transaction_config::TransactionConfig,
    JupiterSwapApiClient,
};
use reqwest::Client;
use serde_json::json;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::native_token::LAMPORTS_PER_SOL;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::transaction::Transaction;
use solana_sdk::instruction::Instruction;
use base64::engine::general_purpose::STANDARD as base64_engine;
use bs58;

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
        let private_key_bytes = bs58::decode(base58privatekey).into_vec().expect("Invalid base58 string");
        let keypair = Keypair::from_bytes(&private_key_bytes).expect("Invalid keypair bytes");

        let rpc_url = "https://api.mainnet-beta.solana.com".to_string();
        let jupiter_swap_api_client = JupiterSwapApiClient::new("https://quote-api.jup.ag/v6".to_string());

        Self {
            client: Client::new(),
            rpc_url,
            keypair,
            jupiter_swap_api_client,
        }
    }

    pub async fn get_balance(&self, wallet_pubkey: &Pubkey) -> u64 {
        let balance_response = self.client.post(&self.rpc_url)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getBalance",
                "params": [wallet_pubkey.to_string()]
            }))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();

        balance_response["result"]["value"].as_u64().unwrap()
    }

    pub async fn get_quote(&self, amount: u64, input_mint: Pubkey, output_mint: Pubkey) -> QuoteResponse {
        let quote_request = QuoteRequest {
            amount,
            input_mint,
            output_mint,
            slippage_bps: 50,
            ..QuoteRequest::default()
        };

        self.jupiter_swap_api_client.quote(&quote_request).await.unwrap()
    }

    pub async fn perform_swap(&self, test_wallet: Pubkey, quote_response: QuoteResponse) -> SwapResponse {
        self.jupiter_swap_api_client.swap(&SwapRequest {
            user_public_key: test_wallet,
            quote_response: quote_response.clone(),
            config: TransactionConfig::default(),
        })
        .await
        .unwrap()
    }

    pub async fn get_swap_instructions(&self, test_wallet: Pubkey, quote_response: QuoteResponse) -> SwapInstructionsResponse {
        self.jupiter_swap_api_client.swap_instructions(&SwapRequest {
            user_public_key: test_wallet,
            quote_response,
            config: TransactionConfig::default(),
        })
        .await
        .unwrap()
    }

    pub async fn create_transaction(&self, instructions: Vec<Instruction>) -> Transaction {
        let recent_blockhash = self.client.post(&self.rpc_url)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getRecentBlockhash",
                "params": []
            }))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap()["result"]["value"]["blockhash"]
            .as_str()
            .unwrap()
            .to_string();

        let recent_blockhash = recent_blockhash.parse().unwrap();

        let mut transaction = Transaction::new_with_payer(&instructions, Some(&self.keypair.pubkey()));
        transaction.sign(&[&self.keypair], recent_blockhash);

        transaction
    }

    pub async fn send_transaction(&self, transaction: &Transaction) -> serde_json::Value {
        let serialized_transaction = bincode::serialize(transaction).unwrap();
        let base64_transaction = base64_engine.encode(&serialized_transaction);

        self.client.post(&self.rpc_url)
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
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap()
    }

    pub async fn check_transaction_confirmation(&self, transaction_signature: &str) -> serde_json::Value {
        self.client.post(&self.rpc_url)
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
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap()
    }

    // pub async fn transfer_spl(&self, from: Pubkey, to: Pubkey, amount: u64) {
    //     let instructions = vec![system_instruction::transfer(&from, &to, amount)];
    //     let transaction = self.create_transaction(instructions).await;
    //     self.send_transaction(&transaction).await;
    // }

    pub async fn execute(&self, input_mint: Pubkey, output_mint: Pubkey) {
        let test_wallet = self.keypair.pubkey();

        // Get balance
        let sol_balance = self.get_balance(&test_wallet).await;

        // Estimate gas fees and rent exemption fees
        let gas_fees = 0.001 * LAMPORTS_PER_SOL as f64;
        let rent_exemption_fee = 0.004 * LAMPORTS_PER_SOL as f64;
        let total_fees = gas_fees + rent_exemption_fee;
        let max_swap_amount = (sol_balance as f64 - total_fees) as u64;

        if max_swap_amount <= 0 {
            eprintln!("Insufficient balance for swap after accounting for fees. Balance: {} lamports, Total fees: {} lamports", sol_balance, total_fees as u64);
            return;
        }

        println!("SOL Balance: {}", sol_balance);
        println!("Estimated Gas Fees: {}", gas_fees as u64);
        println!("Estimated Rent Exemption Fees: {}", rent_exemption_fee as u64);
        println!("Max Swap Amount: {}", max_swap_amount);

        // Get quote
        let quote_response = self.get_quote(max_swap_amount, input_mint, output_mint).await;

        // Perform swap
        let swap_response = self.perform_swap(test_wallet, quote_response.clone()).await;

        println!("Raw tx len: {}", swap_response.swap_transaction.len());

        // Get swap instructions
        let swap_instructions_response = self.get_swap_instructions(test_wallet, quote_response).await;
        println!("{swap_instructions_response:#?}");

        // Collect all instructions
        let mut instructions = Vec::new();
        instructions.extend(swap_instructions_response.setup_instructions);
        instructions.push(swap_instructions_response.swap_instruction);
        if let Some(cleanup_instruction) = swap_instructions_response.cleanup_instruction {
            instructions.push(cleanup_instruction);
        }

        // Create transaction
        let transaction = self.create_transaction(instructions).await;

        // Send transaction
        let send_transaction_response = self.send_transaction(&transaction).await;
        println!("{send_transaction_response:#?}");

        // Check transaction confirmation status
        let confirmation_response = self.check_transaction_confirmation(send_transaction_response["result"].as_str().unwrap()).await;
        println!("{confirmation_response:#?}");
    }
}