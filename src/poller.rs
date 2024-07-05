// poller.rs
use crate::error_handling::AppError;
use crate::kraken::{execute_swap, get_deposit_status, withdraw_assets};
use crate::lockin::LockinClient;
use crate::mongo::{get_transactions_collection, get_users_collection, User};
use kraken_rest_client::OrderSide;
use log::info;
use mongodb::bson::{doc, Bson, Document};
use mongodb::Collection;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::time::Duration;
use tokio::time::interval;

// Converts a Unix timestamp (in seconds) to a BSON DateTime format
// fn convert_timestamp(unix_timestamp: i64) -> BsonDateTime {
//     let datetime: DateTime<Utc> =
//         DateTime::from_timestamp(unix_timestamp, 0).expect("Invalid timestamp");
//     BsonDateTime::from_millis(datetime.timestamp_millis())
// }

// Starts a poller that runs every 60 seconds
pub async fn start_poller() -> Result<(), AppError> {
    let mut interval = interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        match poll_kraken().await {
            Ok(_) => println!("Polling successful."),
            Err(e) => eprintln!("Polling failed: {:?}", e),
        }
    }
}

// Polls Kraken for deposit status and processes any new transactions
async fn poll_kraken() -> Result<(), AppError> {
    println!("Polling Kraken for deposit status...");

    // Retrieve MongoDB collections for users and transactions
    let users_collection = get_users_collection().await?;
    println!("Users collection retrieved.");
    let transactions_collection = get_transactions_collection().await?;
    println!("Transactions collection retrieved.");

    // Fetch the deposit status from Kraken for Bitcoin Lightning deposits
    let response = get_deposit_status("XBT", "Bitcoin Lightning").await?;
    // println!("Kraken Deposit Response: {:?}", response);

    // Process each transaction from the response
    if let Some(transactions) = response.as_array() {
        for transaction in transactions {
            let amount = transaction["amount"]
                .as_str()
                .unwrap_or("0.0")
                .parse::<f64>()?;
            let status = transaction["status"].as_str().unwrap_or("Unknown");
            let time = transaction["time"].as_i64().unwrap_or(0);
            let address = transaction["info"].as_str().unwrap_or("Unknown");

            // Print the user_id, info, amount, time, and status
            println!(
                "Transaction info - address: {}, amount: {}, time: {}, status: {}",
                address, amount, time, status
            );

            // Check if the transaction already exists in the database
            if let Some(tx) = transactions_collection
                .find_one(doc! { "address": address }, None)
                .await?
            {
                let user_id_result = tx.get("user_id");
                match user_id_result {
                    Some(Bson::Int32(user_id)) => {
                        println!(
                            "Transaction found for user_id (i32)={}, address: {}, amount: {}, time: {}, status: {}",
                            user_id, address, amount, time, status
                        );
                        handle_transaction(
                            &users_collection,
                            &transactions_collection,
                            *user_id as i64,
                            amount,
                            address,
                            status,
                            time,
                            tx.clone(),
                        )
                        .await?;
                    }
                    Some(Bson::Int64(user_id)) => {
                        println!(
                            "Transaction found for user_id (i64)={}, address: {}, amount: {}, time: {}, status: {}",
                            user_id, address, amount, time, status
                        );
                        handle_transaction(
                            &users_collection,
                            &transactions_collection,
                            *user_id,
                            amount,
                            address,
                            status,
                            time,
                            tx.clone(),
                        )
                        .await?;
                    }
                    Some(other) => {
                        eprintln!("Unexpected type for user_id: {:?}", other.element_type());
                    }
                    None => {
                        eprintln!("user_id field is missing");
                    }
                }
            } else {
                println!("Transaction not found in database. Skipping...");
            }
        }
    }

    Ok(())
}

// Handles the processing of a transaction based on user_id type
async fn handle_transaction(
    users_collection: &Collection<User>,
    transactions_collection: &Collection<Document>,
    user_id: i64,
    amount: f64,
    address: &str,
    status: &str,
    time: i64,
    tx: Document,
) -> Result<(), AppError> {
    // If the user exists in the database, process their transaction
    if let Some(user_doc) = users_collection
        .find_one(doc! { "user_id": user_id }, None)
        .await?
    {
        // Update the status of the transaction
        transactions_collection
            .update_one(
                doc! { "address": address },
                doc! { "$set": { "status": status.to_string() } },
                None,
            )
            .await?;
        println!("Transaction status updated to {}", status);
        if should_process_transaction(&tx) {
            println!("Processing user transaction...");

            process_user_transaction(
                amount,
                user_id,
                address,
                status,
                time,
                user_doc,
                users_collection,
                // transactions_collection,
            )
            .await?;

            // Mark the transaction as processed
            transactions_collection
                .update_one(
                    doc! { "address": address },
                    doc! { "$set": { "processed": true } },
                    None,
                )
                .await?;
            println!("Transaction marked as processed.");
        } else {
            println!("Transaction already exists and has been processed.");
        }
    }
    Ok(())
}

// Determines if a transaction should be processed based on its status and processed flag
fn should_process_transaction(tx: &Document) -> bool {
    println!("Checking if transaction should be processed...");
    match tx.get_str("status") {
        Ok(existing_status)
            if existing_status == "Success" && !(tx.get_bool("processed").unwrap()) =>
        {
            println!("\nProcessed is: {}\n", tx.get_bool("processed").unwrap());
            true
        }
        _ => {
            println!("\nNot Processing tx: {}\n", tx);
            false
        }
    }
}

// Processes a user's transaction, updating their deposit and performing necessary swaps and withdrawals
async fn process_user_transaction(
    amount: f64,
    user_id: i64,
    address: &str,
    status: &str,
    time: i64,
    user_doc: User,
    users_collection: &Collection<User>,
    // transactions_collection: &Collection<Document>,
) -> Result<(), AppError> {
    println!(
        "Processing user transaction: amount={}, user_id={}, address={}, status={}, time={}",
        amount, user_id, address, status, time
    );

    // Calculate the new total deposit for the user
    let current_total_deposit = user_doc.total_deposit;
    let new_total_deposit = current_total_deposit + amount;
    let found_address = user_doc.solana_public_key.unwrap_or(Default::default());

    println!(
        "User current total deposit: {}, new total deposit: {}",
        current_total_deposit, new_total_deposit
    );
    println!("User Solana address: {}", found_address);

    // Parse the user's Solana public key
    let user_sol_address = Pubkey::from_str(&found_address).unwrap_or_else(|_| {
        eprintln!("Invalid user Solana address.");
        Pubkey::default()
    });

    // Update the user's total deposit in the users collection
    users_collection
        .update_one(
            doc! { "user_id": user_id },
            doc! { "$set": { "total_deposit": new_total_deposit } },
            None,
        )
        .await?;
    println!("Updated total deposit for user: {:?}", user_id);

    // If the transaction status is "Success", process the transaction further
    if status == "Success" {
        println!("Transaction status is Success. Processing further...");
        process_successful_transaction(
            amount,
            user_sol_address,
            user_id,
            users_collection,
            // transactions_collection,
            new_total_deposit,
        )
        .await?;
    } else {
        println!("Transaction is not ready to be processed.\n");
    }

    Ok(())
}

use tokio::task::spawn;

// Processes a successful transaction, including swapping BTC to USD, buying SOL, and withdrawing assets
async fn process_successful_transaction(
    amount: f64,
    user_sol_address: Pubkey,
    user_id: i64,
    users_collection: &Collection<User>,
    // transactions_collection: &Collection<Document>,
    new_total_deposit: f64,
) -> Result<(), AppError> {
    println!("Processing successful transaction for user_id={}", user_id);

    let swap_amount = amount;
    if swap_amount <= 0.0 {
        eprintln!(
            "Swap amount is non-positive, skipping swap for user: {:?}",
            user_id
        );
        return Ok(());
    }

    if swap_amount < 0.0001 {
        eprintln!("Volume too small: {} < 0.0001", swap_amount);
        return Err(AppError::CustomError("Volume too small".to_string()));
    }

    // Perform BTC to USD swap
    println!("Selling {} BTC", swap_amount);
    let btc_usd_response = execute_swap("BTCUSD", OrderSide::Sell, swap_amount).await?;
    println!("BTC to USD swap response: {:?}", btc_usd_response);

    // Calculate the amount of SOL to buy with the USD obtained from the BTC swap
    let sol_amount = btc_usd_response["notional_sol_value"]
        .as_f64()
        .unwrap_or_else(|| {
            btc_usd_response["notional_usd_value"]
                .as_f64()
                .unwrap_or(0.0)
        });
    println!("Buying {} SOL", sol_amount);

    // Perform USD to SOL swap
    let usd_sol_response = execute_swap("SOLUSD", OrderSide::Buy, sol_amount).await?;
    println!("USD to SOL swap response: {:?}", usd_sol_response);

    // Withdraw the SOL to the user's address
    let amount_to_withdraw = usd_sol_response["notional_sol_value"]
        .as_f64()
        .unwrap_or(0.0);
    if amount_to_withdraw < 0.0001 {
        eprintln!(
            "Amount to withdraw too small: {} < 0.0001",
            amount_to_withdraw
        );
        return Err(AppError::CustomError(
            "Amount to withdraw too small".to_string(),
        ));
    }
    println!("Withdrawing {} SOL", amount_to_withdraw);
    withdraw_assets(
        "SOL",
        "bottest",
        "fdXt9eYUTCCeDdrURxS9u6ALnHPLXBNuc1MNqmSR7jA",
        amount_to_withdraw,
    )
    .await?;

    // Execute a lockin transaction on the Solana blockchain in a new thread
    let slippage_bps = 1500; // Slippage tolerance in basis points
    info!("Creating LockinClient...");

    spawn(async move {
        match LockinClient::new().await {
            Ok(lockin_client) => {
                let lockin_mint = Pubkey::from_str("8Ki8DpuWNxu9VsS3kQbarsCWMcFGWkzzA8pUPto9zBd5").unwrap();
                let native_sol_mint = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
                info!("Executing swap to user Solana address: {:?}", user_sol_address);

                match lockin_client
                    .execute(
                        native_sol_mint,
                        lockin_mint,
                        amount_to_withdraw,
                        user_sol_address,
                        slippage_bps,
                    )
                    .await
                {
                    Ok(_) => info!("Lockin transaction executed successfully on Solana blockchain."),
                    Err(e) => {
                        eprintln!("Error executing Lockin transaction: {:?}", e);
                        if let Err(refund_error) = lockin_client
                            .initiate_refund(user_sol_address, amount_to_withdraw as u64)
                            .await
                        {
                            eprintln!("Error processing refund: {:?}", refund_error);
                        }
                    }
                }
            }
            Err(e) => eprintln!("Failed to create LockinClient: {:?}", e),
        }
    });

    // Update the user's total purchased amount in the users collection
    users_collection
        .update_one(
            doc! { "user_id": user_id },
            doc! { "$set": { "total_purchased": new_total_deposit } },
            None,
        )
        .await?;
    println!("Updated total purchased amount for user: {:?}", user_id);

    Ok(())
}
