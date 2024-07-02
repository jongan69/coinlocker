use crate::mongo::{get_users_collection, get_transactions_collection, Transaction};
use crate::kraken::{execute_swap, get_nonce, withdraw_assets};
use crate::utils::get_address_from_txid::get_sender_addresses;
use crate::lockin::LockinClient;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use serde_json::json;
use kraken_rest_client::{Client, Error, OrderSide}; // Replace with the actual crate name
use std::time::Duration;
use chrono::{DateTime, Utc};
use tokio::time::interval;
use mongodb::bson::{doc, to_bson, DateTime as BsonDateTime, Document};

fn convert_timestamp(unix_timestamp: i64) -> BsonDateTime {
    let datetime: DateTime<Utc> = DateTime::from_timestamp(unix_timestamp, 0).expect("Invalid timestamp");
    BsonDateTime::from_millis(datetime.timestamp_millis())
}

pub async fn start_poller() -> Result<(), Error> {
    let api_key = std::env::var("KRAKEN_API_KEY").expect("KRAKEN_API_KEY not set");
    let api_secret = std::env::var("KRAKEN_API_SECRET").expect("KRAKEN_API_SECRET not set");
    let client = Client::new(api_key, api_secret);

    let mut interval = interval(Duration::from_secs(60));

    loop {
        interval.tick().await;
        match poll_kraken(&client).await {
            Ok(_) => println!("Polling successful."),
            Err(e) => eprintln!("Polling failed: {:?}", e),
        }
    }
}

async fn poll_kraken(client: &Client) -> Result<(), Error> {
    let users_collection = get_users_collection().await.unwrap();
    let transactions_collection = get_transactions_collection().await.unwrap();

    let payload = json!({
        "nonce": get_nonce(),
        "asset": "XBT",
    });

    let response: serde_json::Value = client.send_private_json("/0/private/DepositStatus", payload).await?;

    if let Some(transactions) = response.as_array() {
        for transaction in transactions {
            let amount = transaction["amount"].as_str().unwrap_or("0.0").parse::<f64>().unwrap();
            let _asset = transaction["asset"].as_str().unwrap_or("Unknown");
            let status = transaction["status"].as_str().unwrap_or("Unknown");
            let time = transaction["time"].as_i64().unwrap_or(0);
            let txid = transaction["txid"].as_str().unwrap_or("Unknown");

            let electrum_url = "ssl://electrum.blockstream.info:50002";

            match get_sender_addresses(txid, electrum_url).await {
                Ok(sender_addresses) => {
                    for sender_address in sender_addresses {
                        let user_doc = users_collection.find_one(doc! { "btc_address": sender_address.to_string() }, None).await.unwrap();

                        if let Some(user_doc) = user_doc {
                            let existing_transaction: Option<Document> = transactions_collection.find_one(doc! { "txid": txid }, None).await.unwrap();

                            let should_process = if let Some(existing_transaction) = &existing_transaction {
                                let existing_status = existing_transaction.get_str("status").unwrap_or("");
                                let existing_processed = existing_transaction.get_bool("processed").unwrap_or(false);
                                existing_status == "Success" && !existing_processed
                            } else {
                                false
                            };

                            if existing_transaction.is_none() || should_process {
                                let user_id = user_doc.user_id;
                                let current_total_deposit = user_doc.total_deposit;
                                let new_total_deposit = current_total_deposit + amount;

                                let transaction_doc = Transaction {
                                    txid: txid.to_string(),
                                    amount,
                                    user_id,
                                    status: status.to_string(),
                                    processed: false,
                                    timestamp: convert_timestamp(time),
                                };

                                if existing_transaction.is_none() {
                                    let transaction_bson = to_bson(&transaction_doc).unwrap().as_document().unwrap().clone();
                                    transactions_collection.insert_one(transaction_bson, None).await.unwrap();
                                    println!("Added transaction for user: {:?}", user_doc);

                                    // Increment the user's total deposit
                                    users_collection.update_one(
                                        doc! { "btc_address": sender_address.to_string() },
                                        doc! { "$set": { "total_deposit": new_total_deposit } },
                                        None,
                                    ).await.unwrap();
                                    println!("Updated total deposit for user: {:?}", user_doc);
                                }

                                if status.to_string() == "Success" {
                                    println!("Processing Transaction with txid {}", txid);
                                    // Step 1: Swap BTC Deposit Amount Minus TX fee to USD
                                    let btc_swap_fee = 0.000005;
                                    let swap_amount = amount - btc_swap_fee;
                                    if swap_amount <= 0.0 {
                                        eprintln!("Swap amount is non-positive, skipping swap for txid {}", txid);
                                        continue;
                                    }
                                    println!("Selling {} BTC", swap_amount);
                                    match execute_swap("BTCUSD", OrderSide::Sell, swap_amount).await {
                                        Ok(btc_usd_response) => {
                                            println!("BTC to USD swap response: {:?}", btc_usd_response);

                                            // Get the USD amount from the response
                                            let usd_amount = btc_usd_response["notional_usd_value"].as_f64().unwrap_or(0.0);
                                            let sol_amount = btc_usd_response["notional_sol_value"].as_f64().unwrap_or(0.0);
                                            println!("BTC USD Amount Sold: ${:?}, Buying ${:?} SOL", usd_amount, sol_amount);

                                            // Step 2: Swap USD to SOL
                                            match execute_swap("SOLUSD", OrderSide::Buy, sol_amount).await {
                                                Ok(usd_sol_response) => {
                                                    println!("USD to SOL swap response: {:?}", usd_sol_response);

                                                    // Step 3: Withdraw SOL to a wallet
                                                    let amount_to_withdraw = usd_sol_response["notional_sol_value"].as_f64().unwrap_or(0.0);
                                                    println!("Withdrawing {} SOL", amount_to_withdraw);
                                                    match withdraw_assets("SOL", "bottest", "fdXt9eYUTCCeDdrURxS9u6ALnHPLXBNuc1MNqmSR7jA", amount_to_withdraw).await {
                                                        Ok(withdraw_response) => {
                                                            println!("Withdrawal response: {:?}", withdraw_response);

                                                            // Step 4: Swap SOL for Lockin using Lockin
                                                            let lockin_client = LockinClient::new().await;
                                                            let lockin_mint = Pubkey::from_str("8Ki8DpuWNxu9VsS3kQbarsCWMcFGWkzzA8pUPto9zBd5").unwrap();
                                                            let native_sol_mint = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
                                                            lockin_client.execute(native_sol_mint, lockin_mint).await;

                                                            // Step 5: Update the transaction status
                                                            let updated_transaction = transactions_collection.update_one(doc! { "txid": txid }, doc! { "$set": { "processed": true } }, None).await.unwrap();
                                                            println!("Updated transaction status: {:?}", updated_transaction);
                                                        },
                                                        Err(e) => {
                                                            eprintln!("Error withdrawing assets: {:?}", e);
                                                        }
                                                    }
                                                },
                                                Err(e) => {
                                                    eprintln!("Error swapping USD to SOL: {:?}", e);
                                                }
                                            }
                                        },
                                        Err(e) => {
                                            eprintln!("Error swapping BTC to USD: {:?}", e);
                                        }
                                    }
                                } else {
                                    println!("Transaction with txid {} is not ready to be processed", txid);
                                }
                            } else {
                                println!("Transaction with txid {} already exists and has been processed", txid);
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error retrieving sender addresses: {:?}", e);
                }
            }
        }
    }

    Ok(())
}
