#[allow(dead_code)]
// get_address_from_txid.rs
use bdk::bitcoin::{Txid, Network};
use bdk::bitcoin::util::address::Address;
use bdk::electrum_client::Client as ElectrumClient;
use bdk::bitcoin::consensus::encode::deserialize;
use bdk::bitcoin::Transaction as BitcoinTransaction;
use bdk::electrum_client::ElectrumApi;
use bdk::bitcoin::psbt::serialize::Serialize;
use std::str::FromStr;

use crate::error_handling::AppError;

// Function for getting the senders address using the 
pub fn get_sender_addresses(txid_str: &str, electrum_url: &str) -> Result<Vec<Address>, AppError> {
    let txid = Txid::from_str(txid_str).map_err(|_| AppError::BitcoinConsensusError(bdk::bitcoin::consensus::encode::Error::ParseFailed("Failed to parse Txid".into())))?;
    let client = ElectrumClient::new(electrum_url)?;

    let raw_tx = client.transaction_get(&txid)?;
    let raw_tx_bytes = raw_tx.serialize();
    let tx: BitcoinTransaction = deserialize(&raw_tx_bytes)?;

    let mut sender_addresses = Vec::new();

    for input in &tx.input {
        let prev_txid = &input.previous_output.txid;
        let prev_tx_raw = client.transaction_get(prev_txid)?;
        let prev_raw_tx_bytes = prev_tx_raw.serialize();
        let prev_tx: BitcoinTransaction = deserialize(&prev_raw_tx_bytes)?;
        let script_pubkey = &prev_tx.output[input.previous_output.vout as usize].script_pubkey;

        match Address::from_script(script_pubkey, Network::Bitcoin) {
            Ok(sender_address) => {
                sender_addresses.push(sender_address);
            },
            Err(_) => {
                // Log the error or handle it accordingly
                eprintln!("Invalid script_pubkey for address conversion: {:?}", script_pubkey);
                // return Err(AppError::AddressConversionError);
            },
        }
    }

    Ok(sender_addresses)
}
