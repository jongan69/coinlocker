# Python Telegram Bot

This bot acts as the front end for the Rust API by interacting with endpoints in the Rust API and creating Kraken Lightning Network Deposit addresses that get stored in a mongo database.

# Setup

1. Create .env file with the following contents:

    MONGO_URI=YOUR_MONGO_URI
    TELEGRAM_BOT_TOKEN=YOUR_TELEGRAM_BOT_TOKEN
    KRAKEN_API_KEY=YOUR_KRAKEN_API_KEY
    KRAKEN_API_SECRET=YOUR_KRAKEN_API_SECRET
    RUST_BACKEND_URL=http://localhost:8080  # Replace with your Rust backend URL

2. `pip install -r requirements.txt`

# Usage

`python3 main.py`

# Dependencies
- python-dotenv==1.0.1
- python-telegram-bot==21.0.1
- aiohttp==3.9.3
- pymongo==4.6.3
- telegram==0.0.1
- flask==3.0.3