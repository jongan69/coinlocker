// main.rs
use dotenv::dotenv;
use mongo::get_database;
use tracing_subscriber;
use poller::start_poller;
use crate::server::{create_app, shutdown_signal};

mod error_handling;
mod mongo;
mod server;
mod handlers;
mod wallets;
mod poller;
mod kraken;
mod lockin;


#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    dotenv().ok();
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
