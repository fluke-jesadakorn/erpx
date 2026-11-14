//! Native Customer Portal invitation signing.
//!
//! Reproduces the retained TypeScript token exactly: base64url JSON claims
//! signed with HMAC-SHA256 under a derived per-audience key, plus the SHA-256
//! digest stored in the Holding database. Only the digest is persisted; the
//! token is recomputed from immutable invitation facts at delivery time.
use crate::domain::portal_invitation_delivery::{
    INVITATION_AUDIENCE, InvitationClaims, signing_secret_changed,
};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use erpx_server_shared::public::BusinessError;
use ring::hmac;
use sha2::{Digest, Sha256};

fn signing_key(secret: &str) -> hmac::Key {
    let root = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
    let derived = hmac::sign(
        &root,
        format!("{INVITATION_AUDIENCE}:signing-key").as_bytes(),
    );
    hmac::Key::new(hmac::HMAC_SHA256, derived.as_ref())
}

/// Base64url-encoded claims plus their signature, no padding.
pub fn invitation_token(secret: &str, claims: &InvitationClaims) -> String {
    let encoded =
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims).unwrap_or_else(|_| b"{}".to_vec()));
    let signature = hmac::sign(&signing_key(secret), encoded.as_bytes());
    format!("{encoded}.{}", URL_SAFE_NO_PAD.encode(signature.as_ref()))
}

pub fn invitation_token_hash(token: &str) -> String {
    hex_encode(&Sha256::digest(token.as_bytes()))
}

/// Constant-time digest comparison; a non-hex expectation never matches.
pub fn invitation_token_hash_matches(token: &str, expected_hex: &str) -> bool {
    let Some(expected) = hex_decode(expected_hex) else {
        return false;
    };
    if expected.len() != 32 {
        return false;
    }
    let actual = Sha256::digest(token.as_bytes());
    // Length is fixed at 32 bytes; the fold never exits early on a mismatch.
    let mut difference = 0_u8;
    for (actual, expected) in actual.iter().zip(expected.iter()) {
        difference |= actual ^ expected;
    }
    difference == 0
}

/// Verify the HMAC signature and return the immutable claims. A malformed,
/// oversized or tampered token yields `None`; callers fail closed.
pub fn verify_invitation_token(secret: &str, token: &str) -> Option<InvitationClaims> {
    if token.is_empty() || token.len() > 2048 {
        return None;
    }
    let (encoded, provided) = token.split_once('.')?;
    if encoded.is_empty() || provided.is_empty() {
        return None;
    }
    let signature = URL_SAFE_NO_PAD.decode(provided.as_bytes()).ok()?;
    hmac::verify(&signing_key(secret), encoded.as_bytes(), &signature).ok()?;
    let bytes = URL_SAFE_NO_PAD.decode(encoded.as_bytes()).ok()?;
    serde_json::from_slice::<InvitationClaims>(&bytes).ok()
}

/// Reproduce the token and prove it still matches the stored digest.
pub fn require_invitation_token(
    secret: Option<&str>,
    claims: &InvitationClaims,
    stored_hash: &str,
) -> Result<String, BusinessError> {
    let secret = secret
        .filter(|value| !value.is_empty())
        .ok_or_else(crate::domain::portal_invitation_delivery::signing_secret_unavailable)?;
    let token = invitation_token(secret, claims);
    if !invitation_token_hash_matches(&token, stored_hash) {
        return Err(signing_secret_changed());
    }
    Ok(token)
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push_str(&format!("{byte:02x}"));
    }
    value
}

fn hex_decode(value: &str) -> Option<Vec<u8>> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return None;
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).ok())
        .collect()
}
