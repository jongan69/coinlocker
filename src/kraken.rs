// kraken.rs
use crate::error_handling::AppError; // Import the custom error type
use dotenv::dotenv;
use kraken_rest_client::{Client, Error, OrderSide}; // Replace with the actual crate name
use reqwest::Client as SimpleClient;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
}; // Use reqwest client

// Structs
#[derive(Debug, Deserialize, Serialize)]
struct ApiResponse {
    id: String,
    success: bool,
    data: HashMap<String, String>,
}

// Function to get the current nonce
pub fn get_nonce() -> String {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    let in_ms = since_the_epoch.as_millis();
    in_ms.to_string()
}

// Function to format the volume
pub fn format_volume(volume: f64) -> String {
    format!("{:.8}", volume)
}

// Function to check the minimum volume
pub fn check_minimum_volume(asset: &str, volume: f64) -> Result<(), AppError> {
    let min_volume = match asset {
        "BTC" => 0.0001, // Example minimum volume for BTC
        // Add other assets and their minimum volumes as needed
        _ => 0.0,
    };

    if volume < min_volume {
        println!("Volume too small: {} < {}", volume, min_volume);
        return Err(AppError::InternalServerError);
    }

    Ok(())
}

// Function to get asset trading value in USD from Kraken
pub async fn get_asset_value(asset: &str) -> Result<f64, AppError> {
    // Construct the trading pair (e.g., "XBTUSD")
    let pair = format!("{}USD", asset);

    // Define the Kraken API endpoint
    let api_url = format!("https://api.kraken.com/0/public/Ticker?pair={}", pair);

    // Create a reqwest client
    let client = SimpleClient::new();

    // Send the GET request
    let response = client.get(&api_url).send().await?.text().await?;
    println!("Kraken API response: {}", response); // Debug print

    // Parse the JSON response
    let json: Value = serde_json::from_str(&response).map_err(|e| {
        println!("Error parsing JSON response: {:?}", e); // Debug print
        AppError::InternalServerError
    })?;

    // Extract the trading value in USD
    if let Some(result) = json["result"].as_object() {
        for (key, value) in result {
            if key.contains(asset) || key.contains("USD") {
                if let Some(price) = value["c"][0].as_str() {
                    let price: f64 = price.parse().map_err(|e| {
                        println!("Error parsing price value: {:?}", e); // Debug print
                        AppError::InternalServerError
                    })?;
                    return Ok(price);
                } else {
                    println!("Price value not found in JSON response"); // Debug print
                    return Err(AppError::InternalServerError);
                }
            }
        }
        println!("No matching asset pair found in JSON response"); // Debug print
    } else {
        println!("Result field not found in JSON response"); // Debug print
    }

    Err(AppError::InternalServerError)
}

// Function to execute a market swap on Kraken
pub async fn execute_swap(pair: &str, side: OrderSide, volume: f64) -> Result<Value, AppError> {
    dotenv().ok(); // Load environment variables from the ".env" file

    // Read Kraken API key and secret stored in environment variables
    let api_key = std::env::var("KRAKEN_API_KEY").map_err(|e| {
        println!("Error reading KRAKEN_API_KEY: {}", e); // Debug print
        AppError::InternalServerError
    })?;
    let api_secret = std::env::var("KRAKEN_API_SECRET").map_err(|e| {
        println!("Error reading KRAKEN_API_SECRET: {}", e); // Debug print
        AppError::InternalServerError
    })?;

    // Check the minimum volume
    let asset = &pair[..3]; // Assuming the asset is the first three characters of the pair
    check_minimum_volume(asset, volume)?;

    // Get the asset value in USD
    let asset_value_in_usd = get_asset_value(asset).await?;

    // Calculate the notional USD value of the swap
    let notional_usd_value = volume * asset_value_in_usd;

    // Get the SOL value in USD
    let sol_value_in_usd = get_asset_value("SOL").await?;

    // Calculate the notional SOL value of the swap
    let notional_sol_value = notional_usd_value / sol_value_in_usd;

    // Create the client
    let client = Client::new(api_key, api_secret);

    // Format the volume
    let formatted_volume = format_volume(volume);

    // Construct the request payload
    let payload = json!({
        "nonce": get_nonce(),
        "pair": pair,
        "type": side.to_string(),
        "ordertype": "market",
        "volume": formatted_volume
    });
    println!("Payload: {}", payload); // Debug print

    // Send the order request
    let response: Result<Value, Error> = client
        .send_private_json("/0/private/AddOrder", payload)
        .await;

    match response {
        Ok(mut value) => {
            println!("Response: {}", value); // Debug print
                                             // Add notional USD value to the response
            value["notional_usd_value"] = json!(notional_usd_value);
            // Add notional SOL value to the response
            value["notional_sol_value"] = json!(notional_sol_value);
            Ok(value)
        }
        Err(e) => {
            match e {
                Error::Api(api_err) => {
                    if api_err.starts_with('{') {
                        match serde_json::from_str::<Value>(&api_err) {
                            Ok(json) => {
                                if let Some(errors) = json.get("error") {
                                    println!("Kraken API Error: {:?}", errors);
                                } else {
                                    println!("Error sending order: {:?}", api_err);
                                    // Fallback debug print
                                }
                            }
                            Err(parse_err) => {
                                println!("Failed to parse error JSON: {:?}", parse_err); // Debug print
                                println!("Error sending order: {:?}", api_err); // Fallback debug print
                            }
                        }
                    } else {
                        // Print non-JSON error string directly
                        println!("Kraken API Error: {}", api_err);
                    }
                }
                other_err => {
                    println!("Error sending order: {:?}", other_err); // Debug print
                }
            }
            Err(AppError::InternalServerError)
        }
    }
}

// Function to create a new wallet for deposit using BTC Lightning in Kraken
// pub async fn deposit_btc_lightning(asset: &str, amount: f64) -> Result<Value, AppError> {
//     dotenv().ok(); // Load environment variables from the ".env" file

//     // Read Kraken API key and secret stored in environment variables
//     let api_key = std::env::var("KRAKEN_API_KEY")?;
//     let api_secret = std::env::var("KRAKEN_API_SECRET")?;

//     // Create the client
//     let client = Client::new(api_key, api_secret);

//     // Construct the request payload
//     let payload = json!({
//         "nonce": get_nonce(),
//         "asset": asset, // Ticker in Kraken
//         "method": "Bitcoin Lightning", // Method
//         "new": true, // Always use a new wallet for deposit
//         "amount": amount // Amount to deposit
//     });

//     // Send the request
//     let response: Value = client
//         .send_private_json("/0/private/DepositAddresses", payload)
//         .await?;

//     Ok(response)
// }

// Function to Get Kraken BTC deposit status
pub async fn get_deposit_status(asset: &str, method: &str) -> Result<Value, AppError> {
    dotenv().ok(); // Load environment variables from the ".env" file

    // Read Kraken API key and secret stored in environment variables
    let api_key = std::env::var("KRAKEN_API_KEY")?;
    let api_secret = std::env::var("KRAKEN_API_SECRET")?;

    // Create the client
    let client = Client::new(api_key, api_secret);

    // Construct the request payload
    let payload = json!({
        "nonce": get_nonce(),
        "asset": asset, // Asset Ticker in Kraken
        "method": method, // Name of Method ie "Bitcoin Lightning"
    });

    // Send the request
    let response: Value = client
        .send_private_json("/0/private/DepositStatus", payload)
        .await?;

    Ok(response)
}

// Function to withdraw assets from Kraken
pub async fn withdraw_assets(
    asset: &str,
    key: &str,
    address: &str,
    amount: f64,
) -> Result<Value, AppError> {
    dotenv().ok(); // Load environment variables from the ".env" file

    // Read Kraken API key and secret stored in environment variables
    let api_key = std::env::var("KRAKEN_API_KEY")?;
    let api_secret = std::env::var("KRAKEN_API_SECRET")?;

    // Create the client
    let client = Client::new(api_key, api_secret);

    // Construct the request payload
    let payload = json!({
        "nonce": get_nonce(),
        "asset": asset, // Ticker in Kraken
        "key": key, // Name of Wallet in Kraken
        "address": address, // Address of Wallet in kraken
        "amount": amount // Amount to withdraw
    });

    // Send the withdrawal request
    let response: Value = client
        .send_private_json("/0/private/Withdraw", payload)
        .await?;

    Ok(response)
}

// // Function to execute a limit order on Kraken
// pub async fn execute_limit(pair: &str, side: OrderSide, volume: &str) -> Result<Value, AppError> {
//     dotenv().ok(); // Load environment variables from the ".env" file

//     // Read Kraken API key and secret stored in environment variables
//     let api_key = std::env::var("KRAKEN_API_KEY").map_err(|e| {
//         println!("Error reading KRAKEN_API_KEY: {}", e); // Debug print
//         AppError::InternalServerError
//     })?;
//     let api_secret = std::env::var("KRAKEN_API_SECRET").map_err(|e| {
//         println!("Error reading KRAKEN_API_SECRET: {}", e); // Debug print
//         AppError::InternalServerError
//     })?;

//     // Get the asset value for the given pair
//     let asset = &pair[..3]; // Assuming the asset is the first three characters of the pair
//     let price = get_asset_value(asset).await.map_err(|e| {
//         println!("Error getting asset value: {:?}", e); // Debug print
//         AppError::InternalServerError
//     })?;
//     println!("{}ing {} at price: {}", side, pair, price);

//     // Create the client
//     let client = Client::new(api_key, api_secret);

//     // Construct the request payload
//     let payload = json!({
//         "nonce": get_nonce(),
//         "pair": pair,
//         "type": side.to_string(),
//         "ordertype": "limit",
//         "volume": volume,
//         "price": price.to_string()
//     });
//     println!("Payload: {}", payload); // Debug print

//     // Send the order request
//     let response: Value = client
//         .send_private_json("/0/private/AddOrder", payload)
//         .await.map_err(|e| {
//             println!("Error sending order: {:?}", e); // Debug print
//             AppError::InternalServerError
//         })?;

//     println!("Response: {}", response); // Debug print
//     Ok(response)
// }

// // Function to fetch SPL token price from Raydium
// pub async fn fetch_token_price(token_mint: &str, api_url: &str) -> Result<f64, AppError> {
//     let client = SimpleClient::new();
//     let url = format!("{}/mint/price?mints={}", api_url, token_mint);

//     let response = client
//         .get(&url)
//         .header("accept", "application/json")
//         .send()
//         .await?;

//     // Check if the response status is success
//     if !response.status().is_success() {
//         return Err(AppError::InternalServerError);
//     }

//     let response_text = response.text().await?;

//     let response_json: ApiResponse = serde_json::from_str(&response_text)?;

//     if let Some(price_str) = response_json.data.get(token_mint) {
//         let price: f64 = price_str.parse()?;
//         Ok(price)
//     } else {
//         Err(AppError::InternalServerError)
//     }
// }

// // Combined function to get the SOL to SPL token price
// pub async fn get_sol_to_spl_token_price(
//     sol_asset: &str,
//     spl_token_address: &str,
//     raydium_api_url: &str,
// ) -> Result<f64, AppError> {
//     // Get the SOL value in USD from Kraken
//     let sol_usd_price = get_asset_value(sol_asset).await?;

//     // Get the SPL token price from Raydium
//     let spl_token_price = fetch_token_price(spl_token_address, raydium_api_url).await?;

//     // Calculate the SOL to SPL token price
//     let sol_to_spl_token_price = sol_usd_price / spl_token_price;

//     Ok(sol_to_spl_token_price)
// }
