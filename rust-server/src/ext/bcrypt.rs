use bcrypt::{hash, verify, BcryptError};

use crate::constants::SALT_ROUNDS;

pub trait BcryptCompare {
    fn compare_bcrypt(&self, encrypted: &str) -> Result<bool, BcryptError>;
}

impl BcryptCompare for &str {
    fn compare_bcrypt(&self, encrypted: &str) -> Result<bool, BcryptError> {
        verify(self, encrypted)
    }
}

pub fn hash_bcrypt(password: &str) -> Result<String, BcryptError> {
    hash(password, SALT_ROUNDS)
}
