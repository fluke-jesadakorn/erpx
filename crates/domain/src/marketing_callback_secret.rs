//! Validated opaque references for Marketing provider callback secrets.
//!
//! The reference identifies a secret-manager entry; it is never the secret
//! material itself.  This module deliberately knows nothing about how a
//! reference is resolved.  Server and infrastructure adapters can therefore
//! validate and route references without passing secret bytes through the
//! domain contract.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// Maximum UTF-8 byte length of a callback secret reference.
pub const MAX_MARKETING_CALLBACK_SECRET_REFERENCE_BYTES: usize = 255;

/// Maximum UTF-8 byte length of a pilot environment variable name.
pub const MAX_MARKETING_CALLBACK_SECRET_ENV_NAME_BYTES: usize = 128;

const REFERENCE_SCHEMES: [&str; 4] = ["secret://", "sm://", "vault://", "aws-sm://"];

/// Stable, redacted validation failures for callback secret references.
///
/// The rejected value is intentionally not retained or interpolated into the
/// error.  This keeps malformed raw credentials out of logs and responses.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum MarketingCallbackSecretReferenceError {
    #[error("marketing callback secret reference is invalid")]
    InvalidReference,
}

/// A bounded opaque reference to callback verification secret material.
///
/// Supported references identify a secret-manager entry with one of
/// `secret://`, `sm://`, `vault://`, or `aws-sm://`.  The explicitly bounded
/// `env:NAME` form is retained for the local/pilot resolver only; it names an
/// environment variable and never carries its value.
#[derive(Clone, PartialEq, Eq)]
pub struct MarketingCallbackSecretReference(String);

impl MarketingCallbackSecretReference {
    /// Validate and construct an opaque callback secret reference.
    pub fn new(value: impl Into<String>) -> Result<Self, MarketingCallbackSecretReferenceError> {
        let value = value.into();
        validate_reference(&value)?;
        Ok(Self(value))
    }

    /// Borrow the opaque reference without resolving it.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for MarketingCallbackSecretReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("MarketingCallbackSecretReference(REDACTED)")
    }
}

impl Serialize for MarketingCallbackSecretReference {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // The value is only a locator, not secret material.  Debug and error
        // output remain redacted so normal diagnostics cannot expose it.
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for MarketingCallbackSecretReference {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value)
            .map_err(|_| serde::de::Error::custom("marketing callback secret reference is invalid"))
    }
}

fn validate_reference(value: &str) -> Result<(), MarketingCallbackSecretReferenceError> {
    if value.is_empty()
        || value.len() > MAX_MARKETING_CALLBACK_SECRET_REFERENCE_BYTES
        || value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err(MarketingCallbackSecretReferenceError::InvalidReference);
    }

    if let Some(name) = value.strip_prefix("env:") {
        return validate_environment_name(name);
    }

    let scheme = REFERENCE_SCHEMES
        .iter()
        .find(|scheme| value.starts_with(**scheme))
        .ok_or(MarketingCallbackSecretReferenceError::InvalidReference)?;
    let path = &value[scheme.len()..];

    if path.is_empty() || path.starts_with('/') || path.ends_with('/') {
        return Err(MarketingCallbackSecretReferenceError::InvalidReference);
    }

    for segment in path.split('/') {
        if segment.is_empty()
            || segment == "."
            || segment == ".."
            || segment.contains("..")
            || !segment
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        {
            return Err(MarketingCallbackSecretReferenceError::InvalidReference);
        }
    }

    Ok(())
}

fn validate_environment_name(name: &str) -> Result<(), MarketingCallbackSecretReferenceError> {
    if name.is_empty() || name.len() > MAX_MARKETING_CALLBACK_SECRET_ENV_NAME_BYTES {
        return Err(MarketingCallbackSecretReferenceError::InvalidReference);
    }

    let mut bytes = name.bytes();
    let first = bytes
        .next()
        .ok_or(MarketingCallbackSecretReferenceError::InvalidReference)?;
    if !(first.is_ascii_alphabetic() || first == b'_')
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(MarketingCallbackSecretReferenceError::InvalidReference);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_supported_secret_manager_schemes_and_pilot_env_reference() {
        for value in [
            "secret://marketing/callback-signing",
            "sm://marketing/callback-signing",
            "vault://kv/marketing/callback-signing",
            "aws-sm://prod/marketing/callback-signing",
            "env:ERPX_MARKETING_CALLBACK_SECRET",
        ] {
            let reference = MarketingCallbackSecretReference::new(value).unwrap();
            assert_eq!(reference.as_str(), value);
        }
    }

    #[test]
    fn rejects_raw_secret_values_and_unsafe_reference_forms() {
        for value in [
            "",
            "plain-secret",
            "sk_live_raw_secret_value",
            "whsec_raw_secret_value",
            "secret://",
            "secret:///leading-slash",
            "secret://callback?version=1",
            "secret://callback#fragment",
            "secret://../callback",
            "secret://callback/../other",
            "secret://callback//other",
            "secret://callback%2Fother",
            "secret://callback\\other",
            "secret://callback:name",
            "SECRET://callback",
            "env:",
            "env:1_NOT_A_NAME",
            "env:NAME/OTHER",
            "env:NAME?VALUE",
        ] {
            assert!(matches!(
                MarketingCallbackSecretReference::new(value),
                Err(MarketingCallbackSecretReferenceError::InvalidReference)
            ));
        }
    }

    #[test]
    fn enforces_reference_and_environment_name_boundaries() {
        let prefix = "secret://";
        let accepted = format!(
            "{prefix}{}",
            "a".repeat(MAX_MARKETING_CALLBACK_SECRET_REFERENCE_BYTES - prefix.len())
        );
        assert!(MarketingCallbackSecretReference::new(accepted).is_ok());

        let rejected = format!(
            "{prefix}{}",
            "a".repeat(MAX_MARKETING_CALLBACK_SECRET_REFERENCE_BYTES - prefix.len() + 1)
        );
        assert!(matches!(
            MarketingCallbackSecretReference::new(rejected),
            Err(MarketingCallbackSecretReferenceError::InvalidReference)
        ));

        let env = format!(
            "env:{}",
            "A".repeat(MAX_MARKETING_CALLBACK_SECRET_ENV_NAME_BYTES)
        );
        assert!(MarketingCallbackSecretReference::new(env).is_ok());

        let env_too_long = format!(
            "env:{}",
            "A".repeat(MAX_MARKETING_CALLBACK_SECRET_ENV_NAME_BYTES + 1)
        );
        assert!(matches!(
            MarketingCallbackSecretReference::new(env_too_long),
            Err(MarketingCallbackSecretReferenceError::InvalidReference)
        ));
    }

    #[test]
    fn debug_and_errors_never_echo_reference_values() {
        let value = "vault://prod/marketing/callback-signing";
        let reference = MarketingCallbackSecretReference::new(value).unwrap();
        let debug = format!("{reference:?}");
        let error = MarketingCallbackSecretReferenceError::InvalidReference.to_string();

        assert!(debug.contains("REDACTED"));
        assert!(!debug.contains(value));
        assert!(!error.contains(value));
    }

    #[test]
    fn serde_round_trip_validates_and_serializes_only_the_locator() {
        let reference = MarketingCallbackSecretReference::new("sm://prod/callback").unwrap();
        let encoded = serde_json::to_string(&reference).unwrap();
        assert_eq!(encoded, "\"sm://prod/callback\"");
        let decoded: MarketingCallbackSecretReference = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, reference);
        assert!(
            serde_json::from_str::<MarketingCallbackSecretReference>("\"raw-secret\"").is_err()
        );
    }
}
