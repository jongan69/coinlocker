// lockin.rs
use anyhow::{Context, Result};
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
use solana_client::rpc_client::RpcClient;
use solana_program::{
    instruction::Instruction,
    native_token::LAMPORTS_PER_SOL,
    pubkey::Pubkey,
    system_instruction,
};
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use spl_associated_token_account::{
    instruction::create_associated_token_account, get_associated_token_address,
};
use spl_token::id as token_program_id;
use thiserror::Error;
use tokio::time::{sleep, Duration};

#[derive(Error, Debug)]
pub enum LockinClientError {
    #[error("Failed to get minimum balance for rent exemption: {0}")]
    RentExemptionError(String),
    #[error("Failed to get balance: {0}")]
    BalanceError(String),
    #[error("Failed to get quote: {0}")]
    QuoteError(String),
    #[error("Failed to perform swap: {0}")]
    SwapError(String),
    #[error("Failed to get swap instructions: {0}")]
    SwapInstructionsError(String),
    #[error("Failed to create transaction: {0}")]
    TransactionError(String),
    #[error("Failed to check transaction confirmation: {0}")]
    TransactionConfirmationError(String),
    #[error("Failed to process refund: {0}")]
    RefundError(String),
}

pub struct LockinClient {
    client: Client,
    rpc_url: String,
    keypair: Keypair,
    jupiter_swap_api_client: JupiterSwapApiClient,
    rpc_client: RpcClient,
}

impl LockinClient {
    pub async fn new() -> Result<Self> {
        dotenv().ok();
        let base58privatekey = std::env::var("PRIVATE_KEY").context("PRIVATE_KEY not set")?;
        let private_key_bytes = bs58::decode(base58privatekey)
            .into_vec()
            .context("Invalid base58 string")?;
        let keypair = Keypair::from_bytes(&private_key_bytes).context("Invalid keypair bytes")?;
        let rpc_url = "https://api.mainnet-beta.solana.com".to_string();
        let jupiter_swap_api_client = JupiterSwapApiClient::new("https://quote-api.jup.ag/v6".to_string());
        let rpc_client = RpcClient::new(rpc_url.clone());

        Ok(Self {
            client: Client::new(),
            rpc_url,
            keypair,
            jupiter_swap_api_client,
            rpc_client,
        })
    }

    async fn send_rpc_request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.client
            .post(&self.rpc_url)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": method,
                "params": params
            }))
            .send()
            .await
            .context(format!("Failed to send request for {}", method))?
            .json::<serde_json::Value>()
            .await
            .context(format!("Failed to parse response for {}", method))
    }

    pub async fn get_minimum_balance_for_rent_exemption(&self, data_length: usize) -> Result<u64> {
        let response = self.send_rpc_request(
            "getMinimumBalanceForRentExemption",
            json!([data_length]),
        )
        .await?;
        response["result"].as_u64().ok_or_else(|| {
            LockinClientError::RentExemptionError("Invalid response format".to_string()).into()
        })
    }

    pub async fn get_balance(&self, wallet_pubkey: &Pubkey) -> Result<u64> {
        let response = self.send_rpc_request(
            "getBalance",
            json!([wallet_pubkey.to_string()]),
        )
        .await?;
        response["result"]["value"].as_u64().ok_or_else(|| {
            LockinClientError::BalanceError("Invalid response format".to_string()).into()
        })
    }

    pub async fn get_quote(
        &self,
        amount: u64,
        input_mint: Pubkey,
        output_mint: Pubkey,
        slippage_bps: u16,
    ) -> Result<QuoteResponse> {
        let quote_request = QuoteRequest {
            amount,
            input_mint,
            output_mint,
            slippage_bps,
            ..QuoteRequest::default()
        };
        self.jupiter_swap_api_client
            .quote(&quote_request)
            .await
            .context("Failed to get quote from Jupiter swap API")
            .map_err(|e| LockinClientError::QuoteError(e.to_string()).into())
    }

    pub async fn perform_swap(
        &self,
        test_wallet: Pubkey,
        receiving_address: Pubkey,
        quote_response: QuoteResponse,
    ) -> Result<SwapResponse> {
        let config = TransactionConfig {
            destination_token_account: Some(receiving_address),
            ..TransactionConfig::default()
        };
        self.jupiter_swap_api_client
            .swap(&SwapRequest {
                user_public_key: test_wallet,
                quote_response: quote_response.clone(),
                config,
            })
            .await
            .context("Failed to perform swap with Jupiter swap API")
            .map_err(|e| LockinClientError::SwapError(e.to_string()).into())
    }

    pub async fn get_swap_instructions(
        &self,
        test_wallet: Pubkey,
        receiving_address: Pubkey,
        quote_response: QuoteResponse,
    ) -> Result<SwapInstructionsResponse> {
        let config = TransactionConfig {
            destination_token_account: Some(receiving_address),
            ..TransactionConfig::default()
        };
        self.jupiter_swap_api_client
            .swap_instructions(&SwapRequest {
                user_public_key: test_wallet,
                quote_response,
                config,
            })
            .await
            .context("Failed to get swap instructions from Jupiter swap API")
            .map_err(|e| LockinClientError::SwapInstructionsError(e.to_string()).into())
    }

    pub async fn create_transaction(&self, instructions: Vec<Instruction>) -> Result<Transaction> {
        let recent_blockhash = self.send_rpc_request("getRecentBlockhash", json!([]))
            .await?["result"]["value"]["blockhash"]
            .as_str()
            .ok_or_else(|| {
                LockinClientError::TransactionError("Invalid response format for blockhash".to_string())
            })?
            .parse()
            .context("Failed to parse blockhash")?;
        let mut transaction = Transaction::new_with_payer(&instructions, Some(&self.keypair.pubkey()));
        transaction.sign(&[&self.keypair], recent_blockhash);
        Ok(transaction)
    }

    pub async fn send_transaction(&self, transaction: &Transaction) -> Result<serde_json::Value> {
        let serialized_transaction = bincode::serialize(transaction).context("Failed to serialize transaction")?;
        let base64_transaction = base64_engine.encode(&serialized_transaction);
        self.send_rpc_request(
            "sendTransaction",
            json!([base64_transaction, { "encoding": "base64" }]),
        )
        .await
        .context("Failed to send transaction")
    }

    pub async fn check_transaction_confirmation(
        &self,
        transaction_signature: &str,
    ) -> Result<serde_json::Value> {
        self.send_rpc_request(
            "getTransaction",
            json!([transaction_signature, { "encoding": "json" }]),
        )
        .await
        .context("Failed to send request for transaction confirmation")
    }

    pub async fn simulate_transaction(
        &self,
        transaction: &Transaction,
    ) -> Result<serde_json::Value> {
        let serialized_transaction = bincode::serialize(transaction).context("Failed to serialize transaction")?;
        let base64_transaction = base64_engine.encode(&serialized_transaction);
        self.send_rpc_request(
            "simulateTransaction",
            json!([base64_transaction, { "encoding": "base64" }]),
        )
        .await
        .context("Failed to send transaction simulation")
    }

    pub async fn get_or_create_associated_token_address(
        &self,
        wallet_address: Pubkey,
        token_mint_address: Pubkey,
    ) -> Result<Pubkey> {
        let associated_token_address = get_associated_token_address(&wallet_address, &token_mint_address);
        match self.rpc_client.get_account(&associated_token_address) {
            Ok(_) => Ok(associated_token_address),
            Err(_) => {
                let create_ata_instruction = create_associated_token_account(
                    &self.keypair.pubkey(),
                    &wallet_address,
                    &token_mint_address,
                    &token_program_id(),
                );
                let transaction = Transaction::new_signed_with_payer(
                    &[create_ata_instruction],
                    Some(&self.keypair.pubkey()),
                    &[&self.keypair],
                    self.rpc_client.get_latest_blockhash().context("Failed to get latest blockhash")?,
                );
                self.rpc_client
                    .send_and_confirm_transaction(&transaction)
                    .context("Failed to create associated token account")?;
                Ok(associated_token_address)
            }
        }
    }

    pub async fn execute(
        &self,
        input_mint: Pubkey,
        output_mint: Pubkey,
        amount: f64,
        receiving_address: Pubkey,
        initial_slippage_bps: u16,
    ) -> Result<()> {
        const SMALL_FEE: f64 = 0.0001;
        const RETRY_LIMIT: usize = 3;
        const _CONFIRMATION_RETRIES: usize = 5;
        const MAX_SLIPPAGE_BPS: u16 = 2500;

        let sending_wallet = self.keypair.pubkey();
        let sol_balance = self.get_balance(&sending_wallet).await? as f64 / LAMPORTS_PER_SOL as f64;
        println!("SOL balance in Bot Wallet: {} SOL", sol_balance);

        let max_spendable_amount = (amount * 0.9) - SMALL_FEE;
        let gas_fees = 0.004 * LAMPORTS_PER_SOL as f64;
        let rent_exemption_fee = self.get_minimum_balance_for_rent_exemption(165).await? as f64;
        let total_fees = gas_fees + rent_exemption_fee + SMALL_FEE * LAMPORTS_PER_SOL as f64;
        let max_swap_amount = (max_spendable_amount * LAMPORTS_PER_SOL as f64 - total_fees) as u64;

        if max_swap_amount <= 0 {
            eprintln!(
                "Insufficient balance for swap after accounting for fees. Swap Amount: {} lamports, Total fees: {} lamports",
                max_spendable_amount * LAMPORTS_PER_SOL as f64,
                total_fees as u64
            );
            return Ok(());
        }

        println!("SOL Swap Amount: {}", max_spendable_amount);
        println!("Estimated Gas Fees: {}", gas_fees as u64);
        println!("Estimated Rent Exemption Fees: {}", rent_exemption_fee as u64);
        println!("Small Fee: {}", SMALL_FEE * LAMPORTS_PER_SOL as f64);
        println!("Max Swap Amount: {}", max_swap_amount);

        let mut slippage_bps = initial_slippage_bps;

        for attempt in 0..RETRY_LIMIT {
            let quote_response = self
                .get_quote(max_swap_amount, input_mint, output_mint, slippage_bps)
                .await?;
            println!("Quote Response: {:#?}", quote_response);

            let receiving_token_address = self
                .get_or_create_associated_token_address(receiving_address, output_mint)
                .await?;
            println!(
                "Associated Token Address for Receiving: {}",
                receiving_token_address
            );

            match self
                .perform_swap(sending_wallet, receiving_token_address, quote_response.clone())
                .await
            {
                Ok(_) => {
                    let swap_instructions_response = self
                        .get_swap_instructions(sending_wallet, receiving_token_address, quote_response)
                        .await?;
                    println!(
                        "Swap Instructions Response: {:#?}",
                        swap_instructions_response
                    );

                    let instructions = self.collect_swap_instructions(swap_instructions_response);

                    let transaction = self.create_transaction(instructions).await?;
                    println!("Transaction: {:#?}", transaction);

                    let simulation_response = self.simulate_transaction(&transaction).await?;
                    println!("Simulation Response: {:#?}", simulation_response);

                    if simulation_response["result"]["err"].is_null() {
                        let send_transaction_response = self.send_transaction(&transaction).await?;
                        println!(
                            "Send Transaction Response: {:#?}",
                            send_transaction_response
                        );

                        if self
                            .confirm_transaction(&send_transaction_response["result"].as_str().unwrap())
                            .await
                        {
                            return Ok(());
                        }

                        self.initiate_refund(receiving_address, max_swap_amount).await?;
                        return Err(LockinClientError::TransactionConfirmationError(
                            "Transaction failed or not yet confirmed.".to_string(),
                        )
                        .into());
                    } else {
                        eprintln!("Simulation failed: {:#?}", simulation_response);
                        slippage_bps = (slippage_bps * 2).min(MAX_SLIPPAGE_BPS);
                    }
                }
                Err(e) => {
                    eprintln!("Error performing swap: {:?}", e);
                    if attempt == RETRY_LIMIT - 1 {
                        self.initiate_refund(receiving_address, max_swap_amount).await?;
                        return Err(e);
                    }
                }
            }
        }

        eprintln!("Failed to execute swap after {} attempts", RETRY_LIMIT);
        Ok(())
    }

    async fn confirm_transaction(&self, transaction_signature: &str) -> bool {
        const CONFIRMATION_RETRIES: usize = 5;
        let mut backoff = 5;
        for _ in 0..CONFIRMATION_RETRIES {
            match self.check_transaction_confirmation(transaction_signature).await {
                Ok(response) => {
                    if !response["result"].is_null() {
                        println!("Confirmation Response: {:#?}", response);
                        return true;
                    }
                    eprintln!("Transaction not yet confirmed. Retrying...");
                }
                Err(e) => {
                    eprintln!("Error checking transaction confirmation: {:?}", e);
                }
            }
            sleep(Duration::from_secs(backoff)).await;
            backoff *= 2;
        }
        false
    }

    pub async fn initiate_refund(&self, recipient: Pubkey, amount: u64) -> Result<()> {
        let recent_blockhash = self.rpc_client.get_latest_blockhash().context("Failed to get latest blockhash")?;
        let refund_instruction = system_instruction::transfer(
            &self.keypair.pubkey(),
            &recipient,
            amount,
        );
        let refund_transaction = Transaction::new_signed_with_payer(
            &[refund_instruction],
            Some(&self.keypair.pubkey()),
            &[&self.keypair],
            recent_blockhash,
        );
        let send_refund_response = self.rpc_client.send_and_confirm_transaction(&refund_transaction);
        match send_refund_response {
            Ok(signature) => {
                println!("Refund Transaction ID: {}", signature);
                Ok(())
            }
            Err(e) => {
                eprintln!("Failed to send refund transaction: {:?}", e);
                Err(LockinClientError::RefundError(e.to_string()).into())
            }
        }
    }

    fn collect_swap_instructions(
        &self,
        response: SwapInstructionsResponse,
    ) -> Vec<Instruction> {
        let mut instructions = Vec::new();
        instructions.extend(response.setup_instructions);
        instructions.push(response.swap_instruction);
        if let Some(cleanup_instruction) = response.cleanup_instruction {
            instructions.push(cleanup_instruction);
        }
        instructions
    }
}
