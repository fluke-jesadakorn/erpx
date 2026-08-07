//! Framework-neutral Web BFF session port.
//!
//! Browser cookies contain only a random selector. Credential material crosses
//! this boundary only in redacted, zeroizing values and is never serializable.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use thiserror::Error;
use uuid::Uuid;
use zeroize::Zeroizing;

pub const WEB_SESSION_COOKIE_NAME: &str = "erpx_web_session_v1";
pub const MAX_WEB_SESSION_ACCOUNTS: usize = 5;
pub const WEB_SESSION_SELECTOR_BYTES: usize = 32;
const MAX_TOKEN_BYTES: usize = 16 * 1024;
const MAX_PRINCIPAL_REF_BYTES: usize = 256;

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum WebSessionError {
    #[error("web session input is invalid")]
    Invalid,
    #[error("web session selector generation failed")]
    Entropy,
    #[error("web session was concurrently changed")]
    Conflict,
    #[error("web session persistence is unavailable")]
    Unavailable,
    #[error("web session credential protection failed")]
    Protection,
}

/// Random browser-held selector. It is deliberately not serializable or printable.
#[derive(Clone, PartialEq, Eq)]
pub struct WebSessionSelector(Zeroizing<[u8; WEB_SESSION_SELECTOR_BYTES]>);

impl WebSessionSelector {
    pub fn generate() -> Result<Self, WebSessionError> {
        let mut bytes = [0_u8; WEB_SESSION_SELECTOR_BYTES];
        OsRng
            .try_fill_bytes(&mut bytes)
            .map_err(|_| WebSessionError::Entropy)?;
        Ok(Self(Zeroizing::new(bytes)))
    }

    pub fn from_bytes(bytes: [u8; WEB_SESSION_SELECTOR_BYTES]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; WEB_SESSION_SELECTOR_BYTES] {
        &self.0
    }

    pub fn digest(&self) -> WebSessionSelectorDigest {
        let digest: [u8; 32] = Sha256::digest(self.as_bytes()).into();
        WebSessionSelectorDigest(digest)
    }
}

impl fmt::Debug for WebSessionSelector {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WebSessionSelector(REDACTED)")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WebSessionSelectorDigest([u8; 32]);

impl WebSessionSelectorDigest {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Server-held credential that clears its allocation on drop.
#[derive(Clone, PartialEq, Eq)]
pub struct SensitiveToken(Zeroizing<String>);

impl SensitiveToken {
    pub fn new(value: impl Into<String>) -> Result<Self, WebSessionError> {
        let value = value.into();
        if value.is_empty() || value.len() > MAX_TOKEN_BYTES || value.chars().any(char::is_control)
        {
            return Err(WebSessionError::Invalid);
        }
        Ok(Self(Zeroizing::new(value)))
    }

    pub fn expose(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for SensitiveToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SensitiveToken(REDACTED)")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSessionAccount {
    pub session_id: Uuid,
    pub principal_ref: String,
    pub access_token: SensitiveToken,
    pub refresh_token: SensitiveToken,
    pub development_context_token: Option<SensitiveToken>,
    pub access_expires_at: DateTime<Utc>,
    pub refresh_expires_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
    pub active_role: Option<SafeActiveRole>,
    pub authorization_context_version: Option<u64>,
}

impl WebSessionAccount {
    pub fn validate(&self) -> Result<(), WebSessionError> {
        if self.session_id.is_nil()
            || self.principal_ref.trim().is_empty()
            || self.principal_ref.len() > MAX_PRINCIPAL_REF_BYTES
            || self.principal_ref.chars().any(char::is_control)
            || self.access_expires_at >= self.refresh_expires_at
            || self.last_used_at > self.refresh_expires_at
            || self
                .active_role
                .as_ref()
                .is_some_and(|role| role.validate().is_err())
            || self.authorization_context_version == Some(0)
        {
            return Err(WebSessionError::Invalid);
        }
        Ok(())
    }
}

/// Credential-free active-role facts that may cross the SSR/hydration boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafeActiveRole {
    pub source: String,
    pub selection_id: String,
    pub role_key: Option<String>,
    pub scope_key: Option<String>,
}

impl SafeActiveRole {
    pub fn validate(&self) -> Result<(), WebSessionError> {
        let valid_optional = |value: &Option<String>| {
            value.as_ref().is_none_or(|value| {
                !value.trim().is_empty()
                    && value.len() <= MAX_PRINCIPAL_REF_BYTES
                    && !value.chars().any(char::is_control)
            })
        };
        if !matches!(self.source.as_str(), "local" | "directory")
            || self.selection_id.trim().is_empty()
            || self.selection_id.len() > MAX_PRINCIPAL_REF_BYTES
            || self.selection_id.chars().any(char::is_control)
            || !valid_optional(&self.role_key)
            || !valid_optional(&self.scope_key)
            || self.role_key.is_some() != self.scope_key.is_some()
        {
            return Err(WebSessionError::Invalid);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSessionBundle {
    pub id: Uuid,
    pub selector_digest: WebSessionSelectorDigest,
    pub active_session_id: Uuid,
    pub accounts: Vec<WebSessionAccount>,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl WebSessionBundle {
    pub fn validate(&self) -> Result<(), WebSessionError> {
        if self.id.is_nil()
            || self.active_session_id.is_nil()
            || self.version == 0
            || self.accounts.is_empty()
            || self.accounts.len() > MAX_WEB_SESSION_ACCOUNTS
            || self.created_at > self.updated_at
            || self.updated_at > self.expires_at
        {
            return Err(WebSessionError::Invalid);
        }
        let mut sessions = std::collections::BTreeSet::new();
        let mut principals = std::collections::BTreeSet::new();
        for account in &self.accounts {
            account.validate()?;
            if !sessions.insert(account.session_id)
                || !principals.insert(account.principal_ref.as_str())
            {
                return Err(WebSessionError::Invalid);
            }
        }
        if !sessions.contains(&self.active_session_id)
            || self
                .accounts
                .iter()
                .map(|account| account.refresh_expires_at)
                .max()
                != Some(self.expires_at)
        {
            return Err(WebSessionError::Invalid);
        }
        Ok(())
    }

    pub fn active_account(&self) -> Option<&WebSessionAccount> {
        self.accounts
            .iter()
            .find(|account| account.session_id == self.active_session_id)
    }
}

/// Credential-free session data permitted in HTML and hydration payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafeWebSessionSnapshot {
    pub authenticated: bool,
    pub principal_ref: Option<String>,
    pub session_id: Option<Uuid>,
    pub account_count: u8,
    pub access_expires_at: Option<DateTime<Utc>>,
    pub refresh_expires_at: Option<DateTime<Utc>>,
    pub role_selection_required: bool,
    pub active_role: Option<SafeActiveRole>,
}

impl SafeWebSessionSnapshot {
    pub fn signed_out() -> Self {
        Self {
            authenticated: false,
            principal_ref: None,
            session_id: None,
            account_count: 0,
            access_expires_at: None,
            refresh_expires_at: None,
            role_selection_required: false,
            active_role: None,
        }
    }

    pub fn from_bundle(bundle: &WebSessionBundle) -> Result<Self, WebSessionError> {
        bundle.validate()?;
        let active = bundle.active_account().ok_or(WebSessionError::Invalid)?;
        Ok(Self {
            authenticated: true,
            principal_ref: Some(active.principal_ref.clone()),
            session_id: Some(active.session_id),
            account_count: u8::try_from(bundle.accounts.len())
                .map_err(|_| WebSessionError::Invalid)?,
            access_expires_at: Some(active.access_expires_at),
            refresh_expires_at: Some(active.refresh_expires_at),
            role_selection_required: active.active_role.is_none(),
            active_role: active.active_role.clone(),
        })
    }
}

#[async_trait]
pub trait WebSessionStore: Send + Sync + fmt::Debug {
    async fn insert(&self, bundle: &WebSessionBundle) -> Result<(), WebSessionError>;

    async fn load(
        &self,
        selector: WebSessionSelectorDigest,
        at: DateTime<Utc>,
    ) -> Result<Option<WebSessionBundle>, WebSessionError>;

    /// Replace a bundle only when its durable version equals `expected_version`.
    async fn replace(
        &self,
        bundle: &WebSessionBundle,
        expected_version: u64,
    ) -> Result<(), WebSessionError>;

    async fn delete(&self, selector: WebSessionSelectorDigest) -> Result<bool, WebSessionError>;

    async fn purge_expired(&self, at: DateTime<Utc>) -> Result<u64, WebSessionError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn account(at: DateTime<Utc>) -> WebSessionAccount {
        WebSessionAccount {
            session_id: Uuid::new_v4(),
            principal_ref: "local:test-user".into(),
            access_token: SensitiveToken::new("access-secret").unwrap(),
            refresh_token: SensitiveToken::new("refresh-secret").unwrap(),
            development_context_token: None,
            access_expires_at: at + Duration::minutes(5),
            refresh_expires_at: at + Duration::days(30),
            last_used_at: at,
            active_role: None,
            authorization_context_version: None,
        }
    }

    #[test]
    fn selector_and_tokens_are_redacted() {
        let selector = WebSessionSelector::from_bytes([7; WEB_SESSION_SELECTOR_BYTES]);
        let token = SensitiveToken::new("must-not-leak").unwrap();
        assert_eq!(format!("{selector:?}"), "WebSessionSelector(REDACTED)");
        assert_eq!(format!("{token:?}"), "SensitiveToken(REDACTED)");
        assert_ne!(selector.digest().as_bytes(), selector.as_bytes());
    }

    #[test]
    fn safe_snapshot_contains_no_credentials() {
        let at = Utc::now();
        let account = account(at);
        let bundle = WebSessionBundle {
            id: Uuid::new_v4(),
            selector_digest: WebSessionSelector::from_bytes([9; 32]).digest(),
            active_session_id: account.session_id,
            expires_at: account.refresh_expires_at,
            accounts: vec![account],
            version: 1,
            created_at: at,
            updated_at: at,
        };
        let json =
            serde_json::to_string(&SafeWebSessionSnapshot::from_bundle(&bundle).unwrap()).unwrap();
        assert!(!json.contains("access-secret"));
        assert!(!json.contains("refresh-secret"));
        assert!(!json.contains("access_token"));
        assert!(!json.contains("refresh_token"));
        assert!(!json.contains("development_context"));
        assert!(json.contains("role_selection_required"));
    }

    #[test]
    fn safe_role_metadata_is_bounded_and_paired() {
        assert!(
            SafeActiveRole {
                source: "local".into(),
                selection_id: "platform-admin".into(),
                role_key: Some("platform_admin".into()),
                scope_key: Some("platform".into()),
            }
            .validate()
            .is_ok()
        );
        assert!(
            SafeActiveRole {
                source: "local".into(),
                selection_id: "platform-admin".into(),
                role_key: Some("platform_admin".into()),
                scope_key: None,
            }
            .validate()
            .is_err()
        );
    }
}
