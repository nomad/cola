use sha2::{Digest, Sha256};

pub type Checksum = Vec<u8>;

#[inline]
pub fn checksum(bytes: &[u8]) -> Checksum {
    Sha256::digest(bytes)[..].to_vec()
}
