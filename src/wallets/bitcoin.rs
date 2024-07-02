// bitcoin.rs
use bdk::bitcoin::Network;
use bdk::database::MemoryDatabase;
use bdk::keys::{DerivableKey, GeneratableKey, GeneratedKey, ExtendedKey, bip39::{Mnemonic, WordCount, Language}};
use bdk::template::Bip84;
use bdk::{miniscript, Wallet, KeychainKind};
use serde::Serialize;

use crate::error_handling::AppError;

#[derive(Serialize)]
pub struct WalletResponse {
    pub mnemonic: String,
    pub public_key: String,
    pub private_key: String,
}

pub(crate) async fn generate_bitcoin_wallet() -> Result<WalletResponse, AppError> {
    let network = Network::Testnet; // Or this can be Network::Bitcoin, Network::Signet or Network::Regtest

    // Generate fresh mnemonic
    let mnemonic: GeneratedKey<_, miniscript::Segwitv0> = Mnemonic::generate((WordCount::Words12, Language::English)).unwrap();
    // Convert mnemonic to string
    let mnemonic_words = mnemonic.to_string();
    // Parse a mnemonic
    let mnemonic  = Mnemonic::parse(&mnemonic_words).unwrap();
    // Generate the extended key
    let xkey: ExtendedKey = mnemonic.into_extended_key().unwrap();
    // Get xprv from the extended key
    let xprv = xkey.into_xprv(network).unwrap();

    // Create a BDK wallet structure using BIP 84 descriptor ("m/84h/1h/0h/0" and "m/84h/1h/0h/1")
    let wallet = Wallet::new(
        Bip84(xprv, KeychainKind::External),
        Some(Bip84(xprv, KeychainKind::Internal)),
        network,
        MemoryDatabase::default(),
    ).unwrap();

    let public_key = wallet.get_descriptor_for_keychain(KeychainKind::External).to_string();
    let private_key = xprv.to_string(); // Extract the private key

    Ok(WalletResponse {
        mnemonic: mnemonic_words,
        public_key,
        private_key,
    })
}