use sha1::{Digest, Sha1};

pub fn sha1(input: &[u8]) -> u128 {
    let mut hasher = Sha1::new();
    hasher.update(input);
    u128::from_be_bytes(hasher.finalize().as_slice().try_into().unwrap())
}
