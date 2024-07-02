// ethereum.rs
use std::time::{SystemTime, UNIX_EPOCH};
use secp256k1::{rand::rngs, PublicKey, SecretKey};
use serde::{Serialize, Deserialize};
use tiny_keccak::keccak256;

#[derive(Serialize, Deserialize, Debug)]
pub struct EthereumWallet {
    pub secret_key: String,
    pub public_key: String,
    pub public_address: String,
}

pub fn generate_keypair() -> (SecretKey, PublicKey, String) {
    let secp = secp256k1::Secp256k1::new();
    let mut rng = rngs::JitterRng::new_with_timer(get_nstime);
    let (secret_key, public_key) = secp.generate_keypair(&mut rng);
    let public_address = public_key_address(&public_key);
    (secret_key, public_key, public_address)
}

pub fn public_key_address(public_key: &PublicKey) -> String {
    let public_key = public_key.serialize_uncompressed();
    debug_assert_eq!(public_key[0], 0x04);
    let hash = keccak256(&public_key[1..]);
    format!("0x{}", hex::encode(&hash[12..]))
}

pub fn get_nstime() -> u64 {
    let dur = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    dur.as_secs() << 30 | dur.subsec_nanos() as u64
}
