//! SQLx driven adapter for opaque Web BFF sessions.
//!
//! This crate owns the only SQL boundary for `erpx_web` session state. It has
//! no access to ERPX business context tables.

use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use erpx_web_session::{
    SafeActiveRole, SensitiveToken, WebSessionAccount, WebSessionBundle, WebSessionError,
    WebSessionSelectorDigest, WebSessionStore,
};
use rand_core::{OsRng, RngCore};
use sqlx::{PgPool, Row};
use std::{env, fmt};
use uuid::Uuid;
use zeroize::Zeroizing;

const KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;
const MAX_KEY_REFERENCE_BYTES: usize = 128;

#[derive(Clone)]
pub struct WebSessionCipher {
    key_reference: String,
    key: Zeroizing<[u8; KEY_BYTES]>,
}

impl WebSessionCipher {
    pub fn from_env() -> Result<Self, WebSessionError> {
        let key_reference =
            env::var("ERPX_WEB_SESSION_KEY_REFERENCE").map_err(|_| WebSessionError::Protection)?;
        let encoded = env::var("ERPX_WEB_SESSION_KEY").map_err(|_| WebSessionError::Protection)?;
        Self::from_base64(key_reference, &encoded)
    }

    pub fn from_base64(
        key_reference: impl Into<String>,
        encoded: &str,
    ) -> Result<Self, WebSessionError> {
        let key_reference = key_reference.into();
        if key_reference.trim().is_empty()
            || key_reference.len() > MAX_KEY_REFERENCE_BYTES
            || key_reference.chars().any(char::is_control)
        {
            return Err(WebSessionError::Protection);
        }
        let decoded = STANDARD
            .decode(encoded)
            .map_err(|_| WebSessionError::Protection)?;
        let key: [u8; KEY_BYTES] = decoded
            .try_into()
            .map_err(|_| WebSessionError::Protection)?;
        Ok(Self {
            key_reference,
            key: Zeroizing::new(key),
        })
    }

    pub fn key_reference(&self) -> &str {
        &self.key_reference
    }

    fn encrypt(
        &self,
        bundle_id: Uuid,
        session_id: Uuid,
        kind: &'static str,
        token: &SensitiveToken,
    ) -> Result<EncryptedToken, WebSessionError> {
        let mut nonce = [0_u8; NONCE_BYTES];
        OsRng
            .try_fill_bytes(&mut nonce)
            .map_err(|_| WebSessionError::Entropy)?;
        let aad = token_aad(bundle_id, session_id, kind, &self.key_reference);
        let cipher = Aes256Gcm::new_from_slice(self.key.as_slice())
            .map_err(|_| WebSessionError::Protection)?;
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: token.expose().as_bytes(),
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| WebSessionError::Protection)?;
        Ok(EncryptedToken { nonce, ciphertext })
    }

    fn decrypt(
        &self,
        bundle_id: Uuid,
        session_id: Uuid,
        kind: &'static str,
        key_reference: &str,
        nonce: &[u8],
        ciphertext: &[u8],
    ) -> Result<SensitiveToken, WebSessionError> {
        if key_reference != self.key_reference || nonce.len() != NONCE_BYTES {
            return Err(WebSessionError::Protection);
        }
        let aad = token_aad(bundle_id, session_id, kind, key_reference);
        let cipher = Aes256Gcm::new_from_slice(self.key.as_slice())
            .map_err(|_| WebSessionError::Protection)?;
        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| WebSessionError::Protection)?;
        let value = String::from_utf8(plaintext).map_err(|_| WebSessionError::Protection)?;
        SensitiveToken::new(value).map_err(|_| WebSessionError::Protection)
    }
}

impl fmt::Debug for WebSessionCipher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WebSessionCipher")
            .field("key_reference", &self.key_reference)
            .field("key", &"[redacted]")
            .finish()
    }
}

fn token_aad(bundle_id: Uuid, session_id: Uuid, kind: &'static str, key_reference: &str) -> String {
    format!("erpx-web-session-v1/{bundle_id}/{session_id}/{kind}/{key_reference}")
}

struct EncryptedToken {
    nonce: [u8; NONCE_BYTES],
    ciphertext: Vec<u8>,
}

#[derive(Clone)]
pub struct SqlxWebSessionStore {
    pool: PgPool,
    cipher: WebSessionCipher,
}

impl SqlxWebSessionStore {
    pub fn new(pool: PgPool, cipher: WebSessionCipher) -> Self {
        Self { pool, cipher }
    }

    async fn insert_accounts(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        bundle: &WebSessionBundle,
    ) -> Result<(), WebSessionError> {
        for (position, account) in bundle.accounts.iter().enumerate() {
            let access = self.cipher.encrypt(
                bundle.id,
                account.session_id,
                "access",
                &account.access_token,
            )?;
            let refresh = self.cipher.encrypt(
                bundle.id,
                account.session_id,
                "refresh",
                &account.refresh_token,
            )?;
            let development_context = account
                .development_context_token
                .as_ref()
                .map(|token| {
                    self.cipher
                        .encrypt(bundle.id, account.session_id, "development-context", token)
                })
                .transpose()?;
            sqlx::query(
                "INSERT INTO erpx_web.session_accounts
                    (bundle_id, session_id, principal_ref, key_reference,
                     access_nonce, access_ciphertext, refresh_nonce, refresh_ciphertext,
                     access_expires_at, refresh_expires_at, last_used_at, mru_position,
                     active_role_source, active_role_selection_id, active_role_key,
                     active_scope_key, authorization_context_version,
                     development_context_nonce, development_context_ciphertext)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                         $13, $14, $15, $16, $17, $18, $19)",
            )
            .bind(bundle.id)
            .bind(account.session_id)
            .bind(&account.principal_ref)
            .bind(self.cipher.key_reference())
            .bind(access.nonce.as_slice())
            .bind(access.ciphertext)
            .bind(refresh.nonce.as_slice())
            .bind(refresh.ciphertext)
            .bind(account.access_expires_at)
            .bind(account.refresh_expires_at)
            .bind(account.last_used_at)
            .bind(i16::try_from(position).map_err(|_| WebSessionError::Invalid)?)
            .bind(account.active_role.as_ref().map(|role| &role.source))
            .bind(account.active_role.as_ref().map(|role| &role.selection_id))
            .bind(
                account
                    .active_role
                    .as_ref()
                    .and_then(|role| role.role_key.as_ref()),
            )
            .bind(
                account
                    .active_role
                    .as_ref()
                    .and_then(|role| role.scope_key.as_ref()),
            )
            .bind(
                account
                    .authorization_context_version
                    .map(i64::try_from)
                    .transpose()
                    .map_err(|_| WebSessionError::Invalid)?,
            )
            .bind(
                development_context
                    .as_ref()
                    .map(|encrypted| encrypted.nonce.as_slice()),
            )
            .bind(
                development_context
                    .as_ref()
                    .map(|encrypted| encrypted.ciphertext.as_slice()),
            )
            .execute(&mut **transaction)
            .await
            .map_err(|_| WebSessionError::Unavailable)?;
        }
        Ok(())
    }
}

impl fmt::Debug for SqlxWebSessionStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SqlxWebSessionStore")
            .field("pool", &"[configured]")
            .field("cipher", &self.cipher)
            .finish()
    }
}

#[async_trait]
impl WebSessionStore for SqlxWebSessionStore {
    async fn insert(&self, bundle: &WebSessionBundle) -> Result<(), WebSessionError> {
        bundle.validate()?;
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| WebSessionError::Unavailable)?;
        sqlx::query(
            "INSERT INTO erpx_web.session_bundles
                (id, selector_digest, active_session_id, version, created_at, updated_at, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(bundle.id)
        .bind(bundle.selector_digest.as_bytes().as_slice())
        .bind(bundle.active_session_id)
        .bind(i64::try_from(bundle.version).map_err(|_| WebSessionError::Invalid)?)
        .bind(bundle.created_at)
        .bind(bundle.updated_at)
        .bind(bundle.expires_at)
        .execute(&mut *transaction)
        .await
        .map_err(|_| WebSessionError::Unavailable)?;
        self.insert_accounts(&mut transaction, bundle).await?;
        transaction
            .commit()
            .await
            .map_err(|_| WebSessionError::Unavailable)
    }

    async fn load(
        &self,
        selector: WebSessionSelectorDigest,
        at: DateTime<Utc>,
    ) -> Result<Option<WebSessionBundle>, WebSessionError> {
        let Some(bundle_row) = sqlx::query(
            "SELECT id, selector_digest, active_session_id, version,
                    created_at, updated_at, expires_at
             FROM erpx_web.session_bundles
             WHERE selector_digest = $1 AND expires_at > $2",
        )
        .bind(selector.as_bytes().as_slice())
        .bind(at)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| WebSessionError::Unavailable)?
        else {
            return Ok(None);
        };
        let bundle_id: Uuid = bundle_row
            .try_get("id")
            .map_err(|_| WebSessionError::Unavailable)?;
        let rows = sqlx::query(
            "SELECT session_id, principal_ref, key_reference,
                    access_nonce, access_ciphertext, refresh_nonce, refresh_ciphertext,
                    access_expires_at, refresh_expires_at, last_used_at,
                    active_role_source, active_role_selection_id, active_role_key,
                    active_scope_key, authorization_context_version,
                    development_context_nonce, development_context_ciphertext
             FROM erpx_web.session_accounts
             WHERE bundle_id = $1
             ORDER BY mru_position",
        )
        .bind(bundle_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| WebSessionError::Unavailable)?;
        let mut accounts = Vec::with_capacity(rows.len());
        for row in rows {
            let session_id: Uuid = row
                .try_get("session_id")
                .map_err(|_| WebSessionError::Unavailable)?;
            let key_reference: String = row
                .try_get("key_reference")
                .map_err(|_| WebSessionError::Unavailable)?;
            let access_nonce: Vec<u8> = row
                .try_get("access_nonce")
                .map_err(|_| WebSessionError::Unavailable)?;
            let access_ciphertext: Vec<u8> = row
                .try_get("access_ciphertext")
                .map_err(|_| WebSessionError::Unavailable)?;
            let refresh_nonce: Vec<u8> = row
                .try_get("refresh_nonce")
                .map_err(|_| WebSessionError::Unavailable)?;
            let refresh_ciphertext: Vec<u8> = row
                .try_get("refresh_ciphertext")
                .map_err(|_| WebSessionError::Unavailable)?;
            let development_context_nonce: Option<Vec<u8>> = row
                .try_get("development_context_nonce")
                .map_err(|_| WebSessionError::Unavailable)?;
            let development_context_ciphertext: Option<Vec<u8>> = row
                .try_get("development_context_ciphertext")
                .map_err(|_| WebSessionError::Unavailable)?;
            let development_context_token =
                match (development_context_nonce, development_context_ciphertext) {
                    (Some(nonce), Some(ciphertext)) => Some(self.cipher.decrypt(
                        bundle_id,
                        session_id,
                        "development-context",
                        &key_reference,
                        &nonce,
                        &ciphertext,
                    )?),
                    (None, None) => None,
                    _ => return Err(WebSessionError::Protection),
                };
            accounts.push(WebSessionAccount {
                session_id,
                principal_ref: row
                    .try_get("principal_ref")
                    .map_err(|_| WebSessionError::Unavailable)?,
                access_token: self.cipher.decrypt(
                    bundle_id,
                    session_id,
                    "access",
                    &key_reference,
                    &access_nonce,
                    &access_ciphertext,
                )?,
                refresh_token: self.cipher.decrypt(
                    bundle_id,
                    session_id,
                    "refresh",
                    &key_reference,
                    &refresh_nonce,
                    &refresh_ciphertext,
                )?,
                development_context_token,
                access_expires_at: row
                    .try_get("access_expires_at")
                    .map_err(|_| WebSessionError::Unavailable)?,
                refresh_expires_at: row
                    .try_get("refresh_expires_at")
                    .map_err(|_| WebSessionError::Unavailable)?,
                last_used_at: row
                    .try_get("last_used_at")
                    .map_err(|_| WebSessionError::Unavailable)?,
                active_role: match (
                    row.try_get::<Option<String>, _>("active_role_source")
                        .map_err(|_| WebSessionError::Unavailable)?,
                    row.try_get::<Option<String>, _>("active_role_selection_id")
                        .map_err(|_| WebSessionError::Unavailable)?,
                ) {
                    (Some(source), Some(selection_id)) => Some(SafeActiveRole {
                        source,
                        selection_id,
                        role_key: row
                            .try_get("active_role_key")
                            .map_err(|_| WebSessionError::Unavailable)?,
                        scope_key: row
                            .try_get("active_scope_key")
                            .map_err(|_| WebSessionError::Unavailable)?,
                    }),
                    (None, None) => None,
                    _ => return Err(WebSessionError::Protection),
                },
                authorization_context_version: row
                    .try_get::<Option<i64>, _>("authorization_context_version")
                    .map_err(|_| WebSessionError::Unavailable)?
                    .map(u64::try_from)
                    .transpose()
                    .map_err(|_| WebSessionError::Protection)?,
            });
        }
        let stored_digest: Vec<u8> = bundle_row
            .try_get("selector_digest")
            .map_err(|_| WebSessionError::Unavailable)?;
        let stored_digest: [u8; 32] = stored_digest
            .try_into()
            .map_err(|_| WebSessionError::Protection)?;
        let version: i64 = bundle_row
            .try_get("version")
            .map_err(|_| WebSessionError::Unavailable)?;
        let bundle = WebSessionBundle {
            id: bundle_id,
            selector_digest: WebSessionSelectorDigest::from_bytes(stored_digest),
            active_session_id: bundle_row
                .try_get("active_session_id")
                .map_err(|_| WebSessionError::Unavailable)?,
            accounts,
            version: u64::try_from(version).map_err(|_| WebSessionError::Protection)?,
            created_at: bundle_row
                .try_get("created_at")
                .map_err(|_| WebSessionError::Unavailable)?,
            updated_at: bundle_row
                .try_get("updated_at")
                .map_err(|_| WebSessionError::Unavailable)?,
            expires_at: bundle_row
                .try_get("expires_at")
                .map_err(|_| WebSessionError::Unavailable)?,
        };
        bundle.validate().map_err(|_| WebSessionError::Protection)?;
        Ok(Some(bundle))
    }

    async fn replace(
        &self,
        bundle: &WebSessionBundle,
        expected_version: u64,
    ) -> Result<(), WebSessionError> {
        bundle.validate()?;
        if bundle.version != expected_version.saturating_add(1) {
            return Err(WebSessionError::Invalid);
        }
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| WebSessionError::Unavailable)?;
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT version FROM erpx_web.session_bundles
             WHERE id = $1 AND selector_digest = $2
             FOR UPDATE",
        )
        .bind(bundle.id)
        .bind(bundle.selector_digest.as_bytes().as_slice())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| WebSessionError::Unavailable)?;
        if current.and_then(|value| u64::try_from(value).ok()) != Some(expected_version) {
            return Err(WebSessionError::Conflict);
        }
        sqlx::query(
            "UPDATE erpx_web.session_bundles
             SET active_session_id = $2, version = $3, updated_at = $4, expires_at = $5
             WHERE id = $1",
        )
        .bind(bundle.id)
        .bind(bundle.active_session_id)
        .bind(i64::try_from(bundle.version).map_err(|_| WebSessionError::Invalid)?)
        .bind(bundle.updated_at)
        .bind(bundle.expires_at)
        .execute(&mut *transaction)
        .await
        .map_err(|_| WebSessionError::Unavailable)?;
        sqlx::query("DELETE FROM erpx_web.session_accounts WHERE bundle_id = $1")
            .bind(bundle.id)
            .execute(&mut *transaction)
            .await
            .map_err(|_| WebSessionError::Unavailable)?;
        self.insert_accounts(&mut transaction, bundle).await?;
        transaction
            .commit()
            .await
            .map_err(|_| WebSessionError::Unavailable)
    }

    async fn delete(&self, selector: WebSessionSelectorDigest) -> Result<bool, WebSessionError> {
        let result = sqlx::query("DELETE FROM erpx_web.session_bundles WHERE selector_digest = $1")
            .bind(selector.as_bytes().as_slice())
            .execute(&self.pool)
            .await
            .map_err(|_| WebSessionError::Unavailable)?;
        Ok(result.rows_affected() == 1)
    }

    async fn purge_expired(&self, at: DateTime<Utc>) -> Result<u64, WebSessionError> {
        let result = sqlx::query("DELETE FROM erpx_web.session_bundles WHERE expires_at <= $1")
            .bind(at)
            .execute(&self.pool)
            .await
            .map_err(|_| WebSessionError::Unavailable)?;
        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cipher_round_trip_is_bound_to_account_and_kind() {
        let key = STANDARD.encode([11_u8; KEY_BYTES]);
        let cipher = WebSessionCipher::from_base64("test-key", &key).unwrap();
        let bundle_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();
        let token = SensitiveToken::new("server-held-token").unwrap();
        let encrypted = cipher
            .encrypt(bundle_id, session_id, "access", &token)
            .unwrap();
        let decrypted = cipher
            .decrypt(
                bundle_id,
                session_id,
                "access",
                "test-key",
                &encrypted.nonce,
                &encrypted.ciphertext,
            )
            .unwrap();
        assert_eq!(decrypted.expose(), token.expose());
        assert!(
            cipher
                .decrypt(
                    bundle_id,
                    session_id,
                    "refresh",
                    "test-key",
                    &encrypted.nonce,
                    &encrypted.ciphertext,
                )
                .is_err()
        );
        assert!(!format!("{cipher:?}").contains(&key));
    }

    #[test]
    fn development_context_ciphertext_is_bound_to_its_own_kind() {
        let key = STANDARD.encode([19_u8; KEY_BYTES]);
        let cipher = WebSessionCipher::from_base64("test-key", &key).unwrap();
        let bundle_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();
        let token = SensitiveToken::new("server-held-development-context").unwrap();
        let encrypted = cipher
            .encrypt(bundle_id, session_id, "development-context", &token)
            .unwrap();
        assert!(
            cipher
                .decrypt(
                    bundle_id,
                    session_id,
                    "development-context",
                    "test-key",
                    &encrypted.nonce,
                    &encrypted.ciphertext,
                )
                .is_ok()
        );
        assert!(
            cipher
                .decrypt(
                    bundle_id,
                    session_id,
                    "access",
                    "test-key",
                    &encrypted.nonce,
                    &encrypted.ciphertext,
                )
                .is_err()
        );
    }
}
