// ethereum.rs
use std::time::{SystemTime, UNIX_EPOCH}; // Importing system time libraries
use secp256k1::{rand::rngs, PublicKey, SecretKey}; // Importing secp256k1 for key generation
use serde::{Serialize, Deserialize}; // Importing serde for serialization and deserialization
use tiny_keccak::keccak256; // Importing Keccak256 for hashing

// Define the structure for an Ethereum wallet
#[derive(Serialize, Deserialize, Debug)]
pub struct EthereumWallet {
    pub secret_key: String,
    pub public_key: String,
    pub public_address: String,
}

// Function to generate a key pair (secret key, public key) and the corresponding public address
pub fn generate_keypair() -> (SecretKey, PublicKey, String) {
    let secp = secp256k1::Secp256k1::new(); // Create a new secp256k1 context
    let mut rng = rngs::JitterRng::new_with_timer(get_nstime); // Initialize a random number generator with jitter entropy source
    let (secret_key, public_key) = secp.generate_keypair(&mut rng); // Generate a key pair
    let public_address = public_key_address(&public_key); // Generate the public address from the public key
    (secret_key, public_key, public_address) // Return the key pair and public address
}

// Function to derive the public address from a public key
pub fn public_key_address(public_key: &PublicKey) -> String {
    let public_key = public_key.serialize_uncompressed(); // Serialize the public key in uncompressed format
    debug_assert_eq!(public_key[0], 0x04); // Ensure the public key starts with the correct prefix
    let hash = keccak256(&public_key[1..]); // Perform Keccak256 hashing on the public key (excluding the prefix)
    format!("0x{}", hex::encode(&hash[12..])) // Format the last 20 bytes of the hash as a hex string
}

// Function to get the current time in nanoseconds since the UNIX epoch
pub fn get_nstime() -> u64 {
    let dur = SystemTime::now().duration_since(UNIX_EPOCH).unwrap(); // Get the duration since the UNIX epoch
    dur.as_secs() << 30 | dur.subsec_nanos() as u64 // Combine seconds and nanoseconds into a single u64 value
}
