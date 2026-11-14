use crate::domain::account_mfa::base32;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use erpx_server_shared::public::{BusinessError, FailureKind};
use ring::{
    aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey},
    hmac,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub(crate) fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ACCOUNT_MFA_KEY_UNAVAILABLE",
        "Authenticator keys are temporarily unavailable.",
    )
}
/// Secret bytes and injected keys never implement Debug or Serialize.
pub struct MfaSecret([u8; 20]);
impl MfaSecret {
    pub fn from_bytes(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }
    pub fn base32(&self) -> String {
        base32(&self.0)
    }
    pub fn code(&self, counter: i64) -> Result<String, BusinessError> {
        let counter = u64::try_from(counter).map_err(|_| unavailable())?;
        // Existing RFC4226/6238 SHA1 TOTP only; not a signature algorithm choice.
        let key = hmac::Key::new(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, &self.0);
        let tag = hmac::sign(&key, &counter.to_be_bytes());
        let bytes = tag.as_ref();
        let offset = usize::from(bytes[19] & 15);
        let value = u32::from_be_bytes(
            bytes[offset..offset + 4]
                .try_into()
                .map_err(|_| unavailable())?,
        ) & 0x7fff_ffff;
        Ok(format!("{:06}", value % 1_000_000))
    }
}
impl Drop for MfaSecret {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}
pub struct AccountMfaKeyring {
    active: String,
    keys: BTreeMap<String, LessSafeKey>,
}
impl AccountMfaKeyring {
    pub fn new(active: String, keys: Vec<(String, [u8; 32])>) -> Result<Self, BusinessError> {
        if keys.is_empty() || keys.len() > 4 {
            return Err(unavailable());
        }
        let mut configured = BTreeMap::new();
        for (id, bytes) in keys {
            if id.is_empty()
                || id.len() > 32
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                || bytes == [0; 32]
                || configured.contains_key(&id)
            {
                return Err(unavailable());
            }
            configured.insert(
                id,
                LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &bytes).map_err(|_| unavailable())?),
            );
        }
        if !configured.contains_key(&active) {
            return Err(unavailable());
        }
        Ok(Self {
            active,
            keys: configured,
        })
    }
    pub fn generate(&self) -> Result<MfaSecret, BusinessError> {
        let mut bytes = [0; 20];
        getrandom::fill(&mut bytes).map_err(|_| unavailable())?;
        Ok(MfaSecret(bytes))
    }
    pub fn seal(&self, user: &str, secret: &MfaSecret) -> Result<String, BusinessError> {
        let aad = self.aad(&self.active, user)?;
        let mut nonce = [0; 12];
        getrandom::fill(&mut nonce).map_err(|_| unavailable())?;
        let mut data = secret.0.to_vec();
        self.keys
            .get(&self.active)
            .ok_or_else(unavailable)?
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(aad.as_bytes()),
                &mut data,
            )
            .map_err(|_| unavailable())?;
        Ok(format!(
            "mfa1.{}.{}.{}",
            self.active,
            hex::encode(nonce),
            hex::encode(data)
        ))
    }
    pub fn open(&self, user: &str, envelope: &str) -> Result<MfaSecret, BusinessError> {
        if envelope.len() > 256 {
            return Err(unavailable());
        }
        let pieces = envelope.split('.').collect::<Vec<_>>();
        if pieces.len() == 3 {
            return Self::open_legacy(envelope);
        }
        if pieces.len() != 4 || pieces[0] != "mfa1" {
            return Err(unavailable());
        }
        let aad = self.aad(pieces[1], user)?;
        let nonce = hex::decode(pieces[2]).map_err(|_| unavailable())?;
        let nonce = Nonce::try_assume_unique_for_key(&nonce).map_err(|_| unavailable())?;
        let mut data = hex::decode(pieces[3]).map_err(|_| unavailable())?;
        let plain = self
            .keys
            .get(pieces[1])
            .ok_or_else(unavailable)?
            .open_in_place(nonce, Aad::from(aad.as_bytes()), &mut data)
            .map_err(|_| unavailable())?;
        let bytes = plain.try_into().map_err(|_| unavailable())?;
        Ok(MfaSecret(bytes))
    }
    /// Installed TypeScript MFA enrolments use AES-256-GCM with a key derived
    /// from BETTER_AUTH_SECRET. Opening one keeps the original secret and the
    /// native verify path immediately re-seals it under the current key.
    fn open_legacy(envelope: &str) -> Result<MfaSecret, BusinessError> {
        let secret = std::env::var("BETTER_AUTH_SECRET").map_err(|_| unavailable())?;
        if secret.is_empty() || secret.len() > 4096 {
            return Err(unavailable());
        }
        let mut digest = Sha256::new();
        digest.update(secret.as_bytes());
        digest.update(b"erpx-iam-mfa-v1");
        let key = LessSafeKey::new(
            UnboundKey::new(&AES_256_GCM, &digest.finalize()).map_err(|_| unavailable())?,
        );
        let pieces = envelope.split('.').collect::<Vec<_>>();
        let nonce = URL_SAFE_NO_PAD
            .decode(pieces.first().copied().unwrap_or_default())
            .map_err(|_| unavailable())?;
        let tag = URL_SAFE_NO_PAD
            .decode(pieces.get(1).copied().unwrap_or_default())
            .map_err(|_| unavailable())?;
        let mut data = URL_SAFE_NO_PAD
            .decode(pieces.get(2).copied().unwrap_or_default())
            .map_err(|_| unavailable())?;
        data.extend_from_slice(&tag);
        let plain = key
            .open_in_place(
                Nonce::try_assume_unique_for_key(&nonce).map_err(|_| unavailable())?,
                Aad::from(&[]),
                &mut data,
            )
            .map_err(|_| unavailable())?;
        let bytes = if let Ok(text) = std::str::from_utf8(plain)
            && text.len() == 32
        {
            decode_legacy_base32(text).ok_or_else(unavailable)?
        } else {
            plain.try_into().map_err(|_| unavailable())?
        };
        Ok(MfaSecret(bytes))
    }
    fn aad(&self, key: &str, user: &str) -> Result<String, BusinessError> {
        if user.is_empty() || user.len() > 128 || user.chars().any(char::is_control) {
            return Err(unavailable());
        }
        Ok(format!(
            "erpx-native-account-mfa:v1:{}:{key}:{user}",
            user.len()
        ))
    }
}

fn decode_legacy_base32(value: &str) -> Option<[u8; 20]> {
    let mut bits = 0u32;
    let mut accumulator = 0u32;
    let mut output = Vec::with_capacity(20);
    for byte in value.bytes() {
        let index = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'2'..=b'7' => byte - b'2' + 26,
            _ => return None,
        };
        accumulator = (accumulator << 5) | u32::from(index);
        bits += 5;
        if bits >= 8 {
            output.push((accumulator >> (bits - 8)) as u8);
            bits -= 8;
        }
    }
    output.try_into().ok()
}
