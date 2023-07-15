use sha2::{Digest, Sha256};

pub type Checksum = sha2::digest::crypto_common::Output<Sha256>;

#[inline]
pub fn checksum(bytes: &[u8]) -> Checksum {
    Sha256::digest(bytes)
}
