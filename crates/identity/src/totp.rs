use std::{env, fmt};

use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use uuid::Uuid;
use zeroize::Zeroize;

use crate::{IdentityError, SecretBytes};

pub const TOTP_ALGORITHM: &str = "SHA256";
pub const TOTP_DIGITS: u32 = 6;
pub const TOTP_PERIOD_SECONDS: i64 = 30;
pub const TOTP_CLOCK_SKEW_STEPS: i64 = 1;
pub const MAX_TOTP_ENROLLMENT_LIFETIME: Duration = Duration::minutes(15);
pub const MAX_RECOVERY_SESSION_LIFETIME: Duration = Duration::minutes(15);

pub trait KeyResolver {
    fn resolve(&self, reference: &str) -> Result<[u8; 32], IdentityError>;
}

/// A resolver for one configured identity-encryption key. The key bytes are
/// held only in process memory and are never serialized or included in debug
/// output. Production HTTP handlers construct this from environment-backed
/// configuration; tests can use `from_bytes` with a disposable key.
#[derive(Clone)]
pub struct TotpKeyResolver {
    reference: String,
    key: [u8; 32],
}

impl TotpKeyResolver {
    pub fn from_bytes(reference: impl Into<String>, key: [u8; 32]) -> Result<Self, IdentityError> {
        let reference = reference.into();
        if reference.trim().is_empty() || reference.len() > 128 {
            return Err(IdentityError::InvalidKeyReference);
        }
        Ok(Self { reference, key })
    }

    pub fn from_env() -> Result<Self, IdentityError> {
        let reference = env::var("ERPX_IDENTITY_TOTP_KEY_REFERENCE")
            .unwrap_or_else(|_| "erpx-identity-totp".to_owned());
        let encoded = env::var("ERPX_IDENTITY_TOTP_KEY")
            .map_err(|_| IdentityError::KeyReferenceNotFound("ERPX_IDENTITY_TOTP_KEY".into()))?;
        let key = decode_key(&encoded)?;
        Self::from_bytes(reference, key)
    }

    pub fn reference(&self) -> &str {
        &self.reference
    }
}

impl fmt::Debug for TotpKeyResolver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TotpKeyResolver")
            .field("reference", &self.reference)
            .field("key", &"REDACTED")
            .finish()
    }
}

impl KeyResolver for TotpKeyResolver {
    fn resolve(&self, reference: &str) -> Result<[u8; 32], IdentityError> {
        if reference == self.reference {
            Ok(self.key)
        } else {
            Err(IdentityError::KeyReferenceNotFound(reference.to_owned()))
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TotpSecretEnvelope {
    key_reference: String,
    principal_ref: String,
    nonce: [u8; 12],
    ciphertext: Vec<u8>,
}

impl TotpSecretEnvelope {
    pub(crate) fn from_parts(
        key_reference: String,
        principal_ref: String,
        nonce: Vec<u8>,
        ciphertext: Vec<u8>,
    ) -> Result<Self, IdentityError> {
        if key_reference.trim().is_empty()
            || principal_ref.trim().is_empty()
            || key_reference.len() > 128
            || principal_ref.len() > 256
            || nonce.len() != 12
            || ciphertext.len() <= 16
        {
            return Err(IdentityError::InvalidCredential);
        }
        Ok(Self {
            key_reference,
            principal_ref,
            nonce: nonce
                .try_into()
                .map_err(|_| IdentityError::InvalidCredential)?,
            ciphertext,
        })
    }

    pub fn key_reference(&self) -> &str {
        &self.key_reference
    }
    pub fn principal_ref(&self) -> &str {
        &self.principal_ref
    }
    pub fn nonce(&self) -> &[u8; 12] {
        &self.nonce
    }
    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }
}

impl fmt::Debug for TotpSecretEnvelope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TotpSecretEnvelope")
            .field("key_reference", &self.key_reference)
            .field("principal_ref", &self.principal_ref)
            .field("nonce", &"REDACTED")
            .field("ciphertext", &"REDACTED")
            .finish()
    }
}

pub fn encrypt_totp_secret<K: KeyResolver>(
    keys: &K,
    key_reference: &str,
    principal_ref: &str,
    secret: &[u8],
) -> Result<TotpSecretEnvelope, IdentityError> {
    if key_reference.trim().is_empty() || principal_ref.trim().is_empty() || secret.is_empty() {
        return Err(IdentityError::InvalidKeyReference);
    }
    let key = keys.resolve(key_reference)?;
    let cipher =
        Aes256Gcm::new_from_slice(&key).map_err(|_| IdentityError::TotpEncryptionFailed)?;
    let mut nonce = [0_u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            aes_gcm::aead::Payload {
                msg: secret,
                aad: principal_ref.as_bytes(),
            },
        )
        .map_err(|_| IdentityError::TotpEncryptionFailed)?;
    Ok(TotpSecretEnvelope {
        key_reference: key_reference.into(),
        principal_ref: principal_ref.into(),
        nonce,
        ciphertext,
    })
}

pub fn decrypt_totp_secret<K: KeyResolver>(
    keys: &K,
    principal_ref: &str,
    envelope: &TotpSecretEnvelope,
) -> Result<SecretBytes, IdentityError> {
    if principal_ref != envelope.principal_ref {
        return Err(IdentityError::TotpDecryptionFailed);
    }
    let key = keys.resolve(&envelope.key_reference)?;
    let cipher =
        Aes256Gcm::new_from_slice(&key).map_err(|_| IdentityError::TotpDecryptionFailed)?;
    let secret = cipher
        .decrypt(
            Nonce::from_slice(&envelope.nonce),
            aes_gcm::aead::Payload {
                msg: envelope.ciphertext.as_ref(),
                aad: principal_ref.as_bytes(),
            },
        )
        .map_err(|_| IdentityError::TotpDecryptionFailed)?;
    Ok(SecretBytes::from_slice(&secret))
}

/// An enrollment secret is shown exactly once by the enrollment-start route.
/// Its encoded and binary representations are both zeroized on drop, and its
/// debug representation is deliberately non-sensitive.
pub struct TotpSecret {
    encoded: String,
    bytes: Vec<u8>,
}

impl TotpSecret {
    pub(crate) fn generate() -> Result<Self, IdentityError> {
        let mut bytes = vec![0_u8; 20];
        OsRng.fill_bytes(&mut bytes);
        Self::from_bytes(bytes)
    }

    pub(crate) fn from_bytes(bytes: Vec<u8>) -> Result<Self, IdentityError> {
        if bytes.len() < 10 || bytes.len() > 64 {
            return Err(IdentityError::InvalidCredential);
        }
        let encoded = base32_encode(&bytes);
        Ok(Self { encoded, bytes })
    }

    pub fn as_str(&self) -> &str {
        &self.encoded
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl fmt::Debug for TotpSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TotpSecret(REDACTED)")
    }
}

impl Drop for TotpSecret {
    fn drop(&mut self) {
        self.encoded.zeroize();
        self.bytes.zeroize();
    }
}

pub struct TotpEnrollmentStart {
    id: Uuid,
    principal_ref: String,
    secret: TotpSecret,
    otpauth_uri: String,
    issued_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

impl TotpEnrollmentStart {
    pub(crate) fn new(
        id: Uuid,
        principal_ref: String,
        secret: TotpSecret,
        issuer: &str,
        account: &str,
        issued_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<Self, IdentityError> {
        if principal_ref.trim().is_empty()
            || issuer.trim().is_empty()
            || account.trim().is_empty()
            || expires_at <= issued_at
        {
            return Err(IdentityError::InvalidIdentityReference);
        }
        let label = format!("{}:{}", percent_encode(issuer), percent_encode(account));
        let otpauth_uri = format!(
            "otpauth://totp/{label}?secret={}&issuer={}&algorithm={}&digits={}&period={}",
            secret.as_str(),
            percent_encode(issuer),
            TOTP_ALGORITHM,
            TOTP_DIGITS,
            TOTP_PERIOD_SECONDS,
        );
        Ok(Self {
            id,
            principal_ref,
            secret,
            otpauth_uri,
            issued_at,
            expires_at,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn principal_ref(&self) -> &str {
        &self.principal_ref
    }

    pub fn secret(&self) -> &TotpSecret {
        &self.secret
    }

    pub fn otpauth_uri(&self) -> &str {
        &self.otpauth_uri
    }

    pub fn issued_at(&self) -> DateTime<Utc> {
        self.issued_at
    }

    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }
}

impl fmt::Debug for TotpEnrollmentStart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TotpEnrollmentStart")
            .field("id", &self.id)
            .field("principal_ref", &self.principal_ref)
            .field("secret", &"REDACTED")
            .field("otpauth_uri", &"REDACTED")
            .field("issued_at", &self.issued_at)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// Verify a six-digit SHA-256 TOTP code for the current 30-second period and
/// one adjacent period in either direction. The matched step is returned so
/// the write-side store can reject replay and persist the last accepted step.
pub fn verify_totp_code(
    secret: &[u8],
    presented_code: &str,
    at: DateTime<Utc>,
    last_verified_step: Option<i64>,
) -> Result<i64, IdentityError> {
    if secret.len() < 10
        || presented_code.len() != TOTP_DIGITS as usize
        || !presented_code.as_bytes().iter().all(u8::is_ascii_digit)
    {
        return Err(IdentityError::InvalidTotpCode);
    }
    let current_step = at.timestamp().div_euclid(TOTP_PERIOD_SECONDS);
    let mut matched_step = None;
    for offset in [0, -TOTP_CLOCK_SKEW_STEPS, TOTP_CLOCK_SKEW_STEPS] {
        let step = current_step.saturating_add(offset);
        let expected = totp_code(secret, step)?;
        if bool::from(expected.ct_eq(presented_code.as_bytes())) && matched_step.is_none() {
            matched_step = Some(step);
        }
    }
    let step = matched_step.ok_or(IdentityError::InvalidTotpCode)?;
    if last_verified_step.is_some_and(|last| step <= last) {
        return Err(IdentityError::TotpReplay);
    }
    Ok(step)
}

pub(crate) fn validate_enrollment_lifetime(lifetime: Duration) -> Result<(), IdentityError> {
    if lifetime <= Duration::zero() || lifetime > MAX_TOTP_ENROLLMENT_LIFETIME {
        return Err(IdentityError::InvalidTotpEnrollmentLifetime);
    }
    Ok(())
}

pub(crate) fn validate_recovery_session_lifetime(lifetime: Duration) -> Result<(), IdentityError> {
    if lifetime <= Duration::zero() || lifetime > MAX_RECOVERY_SESSION_LIFETIME {
        return Err(IdentityError::InvalidRecoverySessionLifetime);
    }
    Ok(())
}

fn totp_code(secret: &[u8], step: i64) -> Result<[u8; 6], IdentityError> {
    type HmacSha256 = Hmac<Sha256>;
    let mut counter = [0_u8; 8];
    counter.copy_from_slice(&step.to_be_bytes());
    let mut mac =
        <HmacSha256 as Mac>::new_from_slice(secret).map_err(|_| IdentityError::InvalidTotpCode)?;
    mac.update(&counter);
    let digest = mac.finalize().into_bytes();
    let offset = usize::from(digest[31] & 0x0f);
    let binary = (u32::from(digest[offset]) & 0x7f) << 24
        | u32::from(digest[offset + 1]) << 16
        | u32::from(digest[offset + 2]) << 8
        | u32::from(digest[offset + 3]);
    let value = binary % 1_000_000;
    let text = format!("{value:06}");
    let mut result = [0_u8; 6];
    result.copy_from_slice(text.as_bytes());
    Ok(result)
}

fn decode_key(encoded: &str) -> Result<[u8; 32], IdentityError> {
    let bytes = if encoded.len() == 64 && encoded.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        let mut bytes = Vec::with_capacity(32);
        for pair in encoded.as_bytes().chunks_exact(2) {
            let high = hex_value(pair[0]).ok_or(IdentityError::InvalidKeyReference)?;
            let low = hex_value(pair[1]).ok_or(IdentityError::InvalidKeyReference)?;
            bytes.push((high << 4) | low);
        }
        bytes
    } else {
        STANDARD
            .decode(encoded)
            .or_else(|_| URL_SAFE_NO_PAD.decode(encoded))
            .map_err(|_| IdentityError::InvalidKeyReference)?
    };
    bytes
        .try_into()
        .map_err(|_| IdentityError::InvalidKeyReference)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn base32_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut output = String::with_capacity((bytes.len() * 8).div_ceil(5));
    let mut buffer = 0_u16;
    let mut bits = 0_u8;
    for &byte in bytes {
        buffer = (buffer << 8) | u16::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            output.push(char::from(ALPHABET[usize::from((buffer >> bits) & 0x1f)]));
        }
    }
    if bits > 0 {
        output.push(char::from(
            ALPHABET[usize::from((buffer << (5 - bits)) & 0x1f)],
        ));
    }
    output
}

fn percent_encode(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b' ') {
            if byte == b' ' {
                result.push_str("%20");
            } else {
                result.push(char::from(byte));
            }
        } else {
            result.push('%');
            result.push(char::from(b"0123456789ABCDEF"[usize::from(byte >> 4)]));
            result.push(char::from(b"0123456789ABCDEF"[usize::from(byte & 0x0f)]));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn now() -> DateTime<Utc> {
        Utc.timestamp_opt(1_650_000_000, 0).single().expect("time")
    }

    #[test]
    fn generated_secret_is_base32_and_uri_does_not_leak_in_debug() {
        let secret = TotpSecret::generate().expect("secret");
        assert!(
            secret
                .as_str()
                .bytes()
                .all(|byte| { byte.is_ascii_uppercase() || byte.is_ascii_digit() })
        );
        let start = TotpEnrollmentStart::new(
            Uuid::new_v4(),
            "principal-1".into(),
            secret,
            "ERPX",
            "principal-1",
            now(),
            now() + Duration::minutes(10),
        )
        .expect("enrollment");
        assert!(start.otpauth_uri().contains("algorithm=SHA256"));
        let debug = format!("{start:?}");
        assert!(debug.contains("secret: \"REDACTED\""));
        assert!(debug.contains("otpauth_uri: \"REDACTED\""));
        assert!(!debug.contains(start.secret().as_str()));
    }

    #[test]
    fn current_and_adjacent_codes_verify_but_replays_do_not() {
        let secret = b"12345678901234567890";
        let at = now();
        let current = totp_code(secret, at.timestamp() / TOTP_PERIOD_SECONDS).expect("code");
        let current = std::str::from_utf8(&current).expect("digits");
        let step = verify_totp_code(secret, current, at, None).expect("verify");
        assert!(matches!(
            verify_totp_code(secret, current, at, Some(step)),
            Err(IdentityError::TotpReplay)
        ));

        let previous = totp_code(
            secret,
            at.timestamp() / TOTP_PERIOD_SECONDS - TOTP_CLOCK_SKEW_STEPS,
        )
        .expect("previous code");
        let previous = std::str::from_utf8(&previous).expect("digits");
        assert_eq!(
            verify_totp_code(secret, previous, at, None).expect("skewed verify"),
            at.timestamp() / TOTP_PERIOD_SECONDS - 1
        );
    }

    #[test]
    fn key_resolver_accepts_hex_and_rejects_wrong_reference() {
        let resolver = TotpKeyResolver::from_bytes("key-a", [7; 32]).expect("resolver");
        assert_eq!(resolver.resolve("key-a").expect("key"), [7; 32]);
        assert!(matches!(
            resolver.resolve("key-b"),
            Err(IdentityError::KeyReferenceNotFound(_))
        ));
    }
}
