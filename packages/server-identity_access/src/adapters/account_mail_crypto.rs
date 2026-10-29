pub use crate::domain::account_mail::OpenedAccountMail;
use crate::{
    application::account_credentials::AccountMailCryptography,
    domain::{
        account_auth::OpaqueSecret,
        account_credentials::{CredentialProofKind, account_return_path},
    },
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use ring::aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ACCOUNT_MAIL_KEY_UNAVAILABLE",
        "Account email preparation is temporarily unavailable.",
    )
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MailPayload {
    kind: String,
    account_origin: String,
    return_path: String,
    token: String,
}
/// Key bytes are injected from private runtime configuration and are never serialized or debug printed.
pub struct AccountMailKeyring {
    active: String,
    keys: BTreeMap<String, LessSafeKey>,
}
impl AccountMailKeyring {
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
                    .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-')
                || bytes == [0; 32]
                || configured.contains_key(&id)
            {
                return Err(unavailable());
            }
            let key =
                LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &bytes).map_err(|_| unavailable())?);
            configured.insert(id, key);
        }
        if !configured.contains_key(&active) {
            return Err(unavailable());
        }
        Ok(Self {
            active,
            keys: configured,
        })
    }
    pub fn open(
        &self,
        proof_id: &str,
        ciphertext: &str,
    ) -> Result<OpenedAccountMail, BusinessError> {
        validate_context(proof_id)?;
        if ciphertext.len() > 40_000 {
            return Err(unavailable());
        }
        let pieces = ciphertext.split('.').collect::<Vec<_>>();
        if pieces.len() != 4 || pieces[0] != "v1" {
            return Err(unavailable());
        }
        let key = self.keys.get(pieces[1]).ok_or_else(unavailable)?;
        let nonce = hex::decode(pieces[2]).map_err(|_| unavailable())?;
        let nonce = Nonce::try_assume_unique_for_key(&nonce).map_err(|_| unavailable())?;
        let mut data = hex::decode(pieces[3]).map_err(|_| unavailable())?;
        let aad = format!("erpx-native-account-mail-v1:{}:{proof_id}", pieces[1]);
        let plain = key
            .open_in_place(nonce, Aad::from(aad.as_bytes()), &mut data)
            .map_err(|_| unavailable())?;
        let value: MailPayload = serde_json::from_slice(plain).map_err(|_| unavailable())?;
        let kind = match value.kind.as_str() {
            "email_verification" => CredentialProofKind::EmailVerification,
            "password_reset" => CredentialProofKind::PasswordReset,
            _ => return Err(unavailable()),
        };
        if !value.account_origin.starts_with("https://")
            || value.account_origin[8..].contains(['/', '?', '#', '@'])
        {
            return Err(unavailable());
        }
        Ok(OpenedAccountMail {
            kind,
            account_origin: value.account_origin,
            return_path: account_return_path(Some(&value.return_path))?,
            token: OpaqueSecret::new(value.token)?,
        })
    }
}
impl crate::application::account_mail::AccountMailOpening for AccountMailKeyring {
    fn open_mail(
        &self,
        proof_id: &str,
        ciphertext: &str,
    ) -> Result<OpenedAccountMail, BusinessError> {
        self.open(proof_id, ciphertext)
    }
}
impl AccountMailCryptography for AccountMailKeyring {
    fn seal(
        &self,
        proof_id: &str,
        kind: CredentialProofKind,
        account_origin: &str,
        return_path: &str,
        token: &OpaqueSecret,
    ) -> Result<String, BusinessError> {
        validate_context(proof_id)?;
        let mut nonce = [0; 12];
        getrandom::fill(&mut nonce).map_err(|_| unavailable())?;
        let value = MailPayload {
            kind: kind.as_str().into(),
            account_origin: account_origin.into(),
            return_path: account_return_path(Some(return_path))?,
            token: token.expose().into(),
        };
        let mut data = serde_json::to_vec(&value).map_err(|_| unavailable())?;
        let aad = format!("erpx-native-account-mail-v1:{}:{proof_id}", self.active);
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
            "v1.{}.{}.{}",
            self.active,
            hex::encode(nonce),
            hex::encode(data)
        ))
    }
}
fn validate_context(proof_id: &str) -> Result<(), BusinessError> {
    if proof_id.is_empty()
        || proof_id.len() > 128
        || !proof_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-')
    {
        return Err(unavailable());
    }
    Ok(())
}
