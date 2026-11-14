//! One-time Executive bearer secret minting and the retained SHA-256 storage.
use crate::domain::executive_connections::{
    EXECUTIVE_CONNECTION_SECRET_LENGTH, EXECUTIVE_CONNECTION_SECRET_PREFIX,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

/// 32 random bytes in the retained unpadded URL-safe alphabet (43 characters).
pub fn mint_secret() -> Result<String, &'static str> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| "EXECUTIVE_SECRET_UNAVAILABLE")?;
    let body = URL_SAFE_NO_PAD.encode(bytes);
    debug_assert_eq!(body.len(), EXECUTIVE_CONNECTION_SECRET_LENGTH);
    Ok(format!("{EXECUTIVE_CONNECTION_SECRET_PREFIX}{body}"))
}

/// Lowercase hexadecimal SHA-256, matching the retained Control check.
pub fn hash_secret(secret: &str) -> String {
    hex::encode(Sha256::digest(secret.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minted_secrets_have_the_retained_prefix_length_and_alphabet() {
        let first = mint_secret().unwrap();
        let second = mint_secret().unwrap();
        assert_ne!(first, second);
        assert_eq!(
            first.len(),
            EXECUTIVE_CONNECTION_SECRET_PREFIX.len() + EXECUTIVE_CONNECTION_SECRET_LENGTH
        );
        assert!(first.starts_with(EXECUTIVE_CONNECTION_SECRET_PREFIX));
        assert!(
            first[EXECUTIVE_CONNECTION_SECRET_PREFIX.len()..]
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        );
    }

    #[test]
    fn secret_hash_is_the_retained_lowercase_sha256() {
        assert_eq!(
            hash_secret("erpx_exec_test"),
            "046a48186a9191e1353af3371d5cb025699841216f328527490b38188c2d2d9a"
        );
        assert_eq!(hash_secret("a").len(), 64);
        assert!(hash_secret("a").bytes().all(|b| b.is_ascii_hexdigit()));
    }
}
