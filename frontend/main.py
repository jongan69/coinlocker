import os
import time
import logging
import hashlib
import hmac
import base64
from decimal import Decimal, InvalidOperation
from urllib.parse import urlencode
from threading import Thread
from dotenv import load_dotenv
from flask import Flask
import aiohttp
from telegram import Update, InlineKeyboardButton, InlineKeyboardMarkup
from telegram.ext import Application, CommandHandler, CallbackContext, MessageHandler, filters, CallbackQueryHandler
from pymongo import MongoClient

# Load environment variables
load_dotenv()

# Flask app for health checks
app = Flask(__name__)

@app.route('/')
def home():
    return "Bot is running"

def run_flask():
    app.run(host='0.0.0.0', port=80)

# Environment variables
TELEGRAM_BOT_TOKEN = os.getenv("TELEGRAM_BOT_TOKEN")
API_KEY_KRAKEN = os.getenv('KRAKEN_API_KEY')
API_SEC_KRAKEN = os.getenv('KRAKEN_API_SECRET')
RUST_BACKEND_URL = os.getenv("RUST_BACKEND_URL")
MONGO_URI = os.getenv("MONGO_URI")

# Logging configuration
logging.basicConfig(level=logging.DEBUG)
logger = logging.getLogger(__name__)

# Initialize MongoDB client
mongo_client = MongoClient(MONGO_URI)
db = mongo_client['telegram_bot']
users_collection = db['users']
transactions_collection = db['transactions']

MIN_BTC_AMOUNT = Decimal('0.0001')
MAX_BTC_AMOUNT = Decimal('1')
API_URL = "https://api.kraken.com"

def get_kraken_signature(uri_path, data, api_sec):
    postdata = urlencode(data)
    encoded = (str(data['nonce']) + postdata).encode()
    message = uri_path.encode() + hashlib.sha256(encoded).digest()
    mac = hmac.new(base64.b64decode(api_sec), message, hashlib.sha512)
    return base64.b64encode(mac.digest()).decode()

async def kraken_request(uri_path, data, api_key, api_sec):
    headers = {
        'API-Key': api_key,
        'API-Sign': get_kraken_signature(uri_path, data, api_sec)
    }
    try:
        async with aiohttp.ClientSession() as session:
            async with session.post(API_URL + uri_path, headers=headers, data=data) as response:
                response.raise_for_status()
                return await response.json()
    except aiohttp.ClientError as e:
        logger.error(f"Kraken request error: {e}")
        return {"error": [str(e)]}

def get_user(update: Update):
    if update.message:
        return update.message.from_user
    elif update.callback_query:
        return update.callback_query.from_user
    return None

async def start(update: Update, context: CallbackContext):
    user = get_user(update)
    if not user:
        await update.message.reply_text("Error: Unable to retrieve user information.")
        return

    if users_collection.find_one({"user_id": user.id}):
        await update.message.reply_text("You are already registered. Use /menu to see more options.")
        return

    user_data = {
        "user_id": user.id,
        "username": user.username,
        "first_name": user.first_name,
        "last_name": user.last_name,
        "total_deposit": 0,
        "lockin_total": 0,
        "autobuy_amount": None
    }
    users_collection.insert_one(user_data)
    headers = {'Content-Type': 'application/json'}
    async with aiohttp.ClientSession() as session:
        async with session.post(f"{RUST_BACKEND_URL}/register", json={"user_id": user.id}, headers=headers) as req:
            if req.status == 200:
                await update.message.reply_text("Registration successful! Use /menu to see more options.")
            else:
                logger.error(f"Failed to register user with Rust backend: {await req.text()}")
                await update.message.reply_text("Registration failed. Please try again later.")

async def lockin(update: Update, context: CallbackContext, user):
    existing_user = users_collection.find_one({"user_id": user.id})
    if not users_collection.find_one({"user_id": user.id}):
        await update.callback_query.message.reply_text("You are not registered. Please register first using /start.")
        return

    autobuy_amount = existing_user.get("autobuy_amount")
    if autobuy_amount:
        await create_deposit_address(update, context, user, autobuy_amount)
    else:
        await update.callback_query.message.reply_text("Please enter the amount of BTC you want to lock in (minimum 0.0001 BTC, maximum 1 BTC):")
        context.user_data["handler"] = handle_btc_amount

async def create_deposit_address(update: Update, context: CallbackContext, user, amount: Decimal):
    try:
        deposit_response = await kraken_request(
            '/0/private/DepositAddresses', {
                "nonce": str(int(1000 * time.time())),
                "asset": "XXBT",
                "method": "Bitcoin Lightning",
                "new": True,
                "amount": float(amount),
            }, API_KEY_KRAKEN, API_SEC_KRAKEN)

        if 'error' in deposit_response and deposit_response['error']:
            logger.error(f"Kraken deposit address error: {deposit_response['error']}")
            await update.callback_query.message.reply_text("Error generating deposit address. Please try again later.")
            return

        kraken_deposit_address = deposit_response['result'][0]['address']
        transaction = {
            "user_id": user.id,
            "amount": float(amount),
            "processed": False,
            "status": "unofficial",
            "address": kraken_deposit_address,
            "timestamp": time.time(),
        }
        transactions_collection.insert_one(transaction)

        await update.message.reply_text(
            f"Transaction recorded.\nPlease send {amount} BTC to the following address:\n\n<code>{kraken_deposit_address}</code>",
            parse_mode="HTML")
    except Exception as e:
        logger.error(f"Error creating deposit address: {e}")
        await update.callback_query.message.reply_text("An error occurred. Please try again later.")

async def handle_btc_amount(update: Update, context: CallbackContext):
    user = get_user(update)
    if not user:
        await update.message.reply_text("Error: Unable to retrieve user information.")
        return

    amount_text = update.message.text
    try:
        amount = Decimal(amount_text)
        if amount < MIN_BTC_AMOUNT or amount > MAX_BTC_AMOUNT:
            await update.message.reply_text(f"Invalid amount. Please enter an amount between {MIN_BTC_AMOUNT} and {MAX_BTC_AMOUNT} BTC.")
            return

        await create_deposit_address(update, context, user, amount)
    except InvalidOperation:
        logger.error(f"Invalid amount format: {amount_text}")
        await update.message.reply_text("Invalid amount format. Please enter a numeric value.")
    except Exception as e:
        logger.error(f"Error handling BTC amount: {e}")
        await update.message.reply_text("An error occurred. Please try again later.")

async def show_menu(update: Update, context: CallbackContext):
    user = get_user(update)
    if not user:
        await update.message.reply_text("Error: Unable to retrieve user information.")
        return
    existing_user = users_collection.find_one({"user_id": user.id})
    if not existing_user:
        await update.message.reply_text("You are not registered. Please register first using /start.")
        return

    keyboard = [
        [InlineKeyboardButton("Lockin", callback_data='lockin')],
        [InlineKeyboardButton("Export Key", callback_data='export_key')],
        [InlineKeyboardButton("Autobuy Settings", callback_data='autobuy_settings')]
    ]

    reply_markup = InlineKeyboardMarkup(keyboard)
    welcome_message = f"Welcome, {user.username}!\n\nSolana Receive Address: {existing_user.get('solana_public_key', 'Not set')}\n\n"
    await update.message.reply_text(welcome_message, reply_markup=reply_markup)

async def button(update: Update, context: CallbackContext):
    query = update.callback_query
    await query.answer()

    user = query.from_user
    logger.info(f"Button pressed by user: {user.id}, callback data: {query.data}")

    if query.data == 'lockin':
        await lockin(query, context, user)
    elif query.data == 'export_key':
        await export_key(query, context, user)
    elif query.data == 'autobuy_settings':
        await set_autobuy_amount(query, context, user)

async def set_autobuy_amount(update: Update, context: CallbackContext, user):
    if not user:
        await update.message.reply_text("Error: Unable to retrieve user information.")
        return

    if not users_collection.find_one({"user_id": user.id}):
        await update.message.reply_text("You are not registered. Please register first using /start.")
        return

    await update.message.reply_text("Please enter the amount of BTC for Autobuy (minimum 0.0001 BTC, maximum 1 BTC):")
    context.user_data["handler"] = save_autobuy_amount

async def save_autobuy_amount(update: Update, context: CallbackContext):
    user = get_user(update)
    if not user:
        await update.message.reply_text("Error: Unable to retrieve user information.")
        return

    amount_text = update.message.text
    try:
        amount = Decimal(amount_text)
        if amount < MIN_BTC_AMOUNT or amount > MAX_BTC_AMOUNT:
            await update.message.reply_text(f"Invalid amount. Please enter an amount between {MIN_BTC_AMOUNT} and {MAX_BTC_AMOUNT} BTC.")
            return

        users_collection.update_one({"user_id": user.id}, {"$set": {"autobuy_amount": float(amount)}})
        await update.message.reply_text(f"Autobuy amount set to {amount} BTC.")
    except InvalidOperation:
        logger.error(f"Invalid amount format: {amount_text}")
        await update.message.reply_text("Invalid amount format. Please enter a numeric value.")
    except Exception as e:
        logger.error(f"Error saving Autobuy amount: {e}")
        await update.message.reply_text("An error occurred. Please try again later.")

async def export_key(update: Update, context: CallbackContext, user):
    if not user:
        await update.message.reply_text("Error: Unable to retrieve user information.")
        return

    existing_user = users_collection.find_one({"user_id": user.id})
    if not existing_user:
        await update.message.reply_text("You are not registered. Please register first using /start.")
        return

    async with aiohttp.ClientSession() as session:
        async with session.get(f"{RUST_BACKEND_URL}/decrypt_keys", json={"api_key": existing_user['api_key']}) as response:
            if response.status == 200:
                data = await response.json()
                solana_private_key = data.get('solana', {}).get('private_key')
                await update.message.reply_text(f"Your Solana Private Key:\n<code>{solana_private_key}</code>\n", parse_mode="HTML")
            else:
                logger.error(f"Failed to export key: {await response.text()}")
                await update.message.reply_text("Failed to export key. Please try again later.")

async def handle_user_message(update: Update, context: CallbackContext):
    handler = context.user_data.get("handler")
    if handler:
        await handler(update, context)
        context.user_data["handler"] = None
    else:
        await update.message.reply_text("Please use the /menu to navigate the bot options.")

def main():
    # Start Flask app in a separate thread
    Thread(target=run_flask).start()

    application = Application.builder().token(TELEGRAM_BOT_TOKEN).build()
    application.add_handler(CommandHandler("start", start))
    application.add_handler(CommandHandler("menu", show_menu))
    application.add_handler(CallbackQueryHandler(button))
    application.add_handler(MessageHandler(filters.TEXT & ~filters.COMMAND, handle_user_message))

    application.run_polling()

if __name__ == '__main__':
    main()
