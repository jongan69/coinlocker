// main.rs
use dotenv::dotenv;
use mongo::get_database;
use tracing_subscriber;
// use tokio::task;

mod error_handling;
mod mongo;
mod server;
mod handlers;
mod wallets;
mod poller;
mod kraken;
mod utils;
mod lockin;

use crate::server::{create_app, shutdown_signal};
use poller::start_poller;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    dotenv().ok();
    // let api_key = std::env::var("KRAKEN_API_KEY").expect("KRAKEN_API_KEY not set");
    // let api_secret = std::env::var("KRAKEN_API_SECRET").expect("KRAKEN_API_SECRET not set");
    let db = get_database().await.unwrap();
    let app = create_app(db);

    let server = axum::Server::bind(&"0.0.0.0:8080".parse().unwrap())
        .serve(app.into_make_service());

        // Start the polling in a separate async task
        tokio::spawn(async {
            if let Err(e) = start_poller().await {
                eprintln!("Polling error: {}", e);
            }
        });

    let graceful = server.with_graceful_shutdown(shutdown_signal());

    if let Err(err) = graceful.await {
        tracing::error!("Server error: {}", err);
    }
}
