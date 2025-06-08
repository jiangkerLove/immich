use bcrypt::{verify, BcryptError};

pub trait BcryptCompare {
    fn compare_bcrypt(&self, encrypted: &str) -> Result<bool, BcryptError>;
}

impl BcryptCompare for &str {
    fn compare_bcrypt(&self, encrypted: &str) -> Result<bool, BcryptError> {
        verify(self, encrypted)
    }
}