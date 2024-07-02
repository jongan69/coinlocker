# Coinlocker API

A Rust AXUM API that automates the transaction process from bitcoin to Solana SPL Tokens using MongoDB, Kraken API, Jupiter Swap API, and Solana.

## Prerequisites

1. **Install Rust**: Follow the instructions at [rust-lang.org](https://www.rust-lang.org/tools/install) to install Rust on your machine.
2. **API Keys and Configuration**:
   - Create a `.env` file in the root of your project directory.
   - Add your Kraken API keys, MongoDB URI, and Solana private key for the wallet that is registered in Kraken for withdrawals to the `.env` file:

     ```env
     KRAKEN_API_KEY=your_kraken_api_key
     KRAKEN_API_SECRET=your_kraken_api_secret
     MONGO_URL=your_mongodb_uri
     PRIVATE_KEY=your_solana_private_key
     ```

## Local Development

3. **Build the Project**:
   - Run the following command to build the project:

     ```sh
     cargo build
     ```

4. **Run the Project**:
   - Use the following command to run the project:

     ```sh
     cargo run
     ```

## Docker Usage

### Prerequisites

- **Install Docker**: Follow the instructions at [docker.com](https://www.docker.com/products/docker-desktop) to install Docker on your machine.
- **Install Docker Compose**: Docker Desktop includes Docker Compose, so no additional installation is needed if you have Docker Desktop.

### Making Scripts Executable

Use `chmod +x scripts/clean_build.sh scripts/debug.sh` to ensure both scripts are executable:

```sh
chmod +x scripts/clean_build.sh scripts/debug.sh
```

Run Clean Build

```sh
scripts/clean_build.sh
```


# Useful to knows:
- `docker buildx build .` or `docker compose up` or `scripts/clean_build.sh` use docker to build and run, not operable yet
- `cargo build` and `cargo run` run the rust axum api locally
- Kraken has a minimum 0.0001 BTC trade minimum
- Private key is needed for anything in `lockin.rs` to work