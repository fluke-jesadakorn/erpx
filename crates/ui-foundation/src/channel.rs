use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_ENDPOINT_LENGTH: usize = 2_048;

/// Runtime family selected by the application composition root.
///
/// This is deliberately explicit instead of using target `cfg`s so context UI
/// code can be compiled and tested identically for native and WebAssembly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTarget {
    WebAssembly,
    Native,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelKind {
    SameOriginHttp,
    DirectHttp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelDescriptor {
    pub kind: ChannelKind,
    pub endpoint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelPolicy {
    web: ChannelDescriptor,
    native: ChannelDescriptor,
}

impl ChannelPolicy {
    pub fn try_new(
        web_endpoint: impl Into<String>,
        native_endpoint: impl Into<String>,
    ) -> Result<Self, ChannelPolicyError> {
        let web_endpoint = web_endpoint.into();
        let native_endpoint = native_endpoint.into();
        validate_web_endpoint(&web_endpoint)?;
        validate_native_endpoint(&native_endpoint)?;
        Ok(Self {
            web: ChannelDescriptor {
                kind: ChannelKind::SameOriginHttp,
                endpoint: web_endpoint,
            },
            native: ChannelDescriptor {
                kind: ChannelKind::DirectHttp,
                endpoint: native_endpoint,
            },
        })
    }

    pub const fn select(&self, target: RuntimeTarget) -> &ChannelDescriptor {
        match target {
            RuntimeTarget::WebAssembly => &self.web,
            RuntimeTarget::Native => &self.native,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ChannelPolicyError {
    #[error("endpoint is empty")]
    Empty,
    #[error("endpoint is too long")]
    TooLong,
    #[error("the browser endpoint must be a same-origin absolute path")]
    InvalidWebEndpoint,
    #[error("the native endpoint must use HTTP or HTTPS and must not contain credentials")]
    InvalidNativeEndpoint,
}

fn validate_length(value: &str) -> Result<(), ChannelPolicyError> {
    if value.trim().is_empty() {
        return Err(ChannelPolicyError::Empty);
    }
    if value.len() > MAX_ENDPOINT_LENGTH {
        return Err(ChannelPolicyError::TooLong);
    }
    Ok(())
}

fn validate_web_endpoint(value: &str) -> Result<(), ChannelPolicyError> {
    validate_length(value)?;
    if !value.starts_with('/')
        || value.starts_with("//")
        || value.contains('\r')
        || value.contains('\n')
    {
        return Err(ChannelPolicyError::InvalidWebEndpoint);
    }
    Ok(())
}

fn validate_native_endpoint(value: &str) -> Result<(), ChannelPolicyError> {
    validate_length(value)?;
    let authority_and_path = value
        .strip_prefix("http://")
        .or_else(|| value.strip_prefix("https://"))
        .ok_or(ChannelPolicyError::InvalidNativeEndpoint)?;
    let authority = authority_and_path.split('/').next().unwrap_or_default();
    if authority.is_empty()
        || authority.contains('@')
        || authority_and_path.contains('\r')
        || authority_and_path.contains('\n')
    {
        return Err(ChannelPolicyError::InvalidNativeEndpoint);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_selects_a_channel_without_target_specific_code() {
        let policy = ChannelPolicy::try_new("/api", "http://127.0.0.1:3004").unwrap();
        assert_eq!(
            policy.select(RuntimeTarget::WebAssembly),
            &ChannelDescriptor {
                kind: ChannelKind::SameOriginHttp,
                endpoint: "/api".into(),
            }
        );
        assert_eq!(
            policy.select(RuntimeTarget::Native).kind,
            ChannelKind::DirectHttp
        );
    }

    #[test]
    fn policy_rejects_cross_origin_browser_and_credentialed_native_endpoints() {
        assert_eq!(
            ChannelPolicy::try_new("https://example.test", "http://127.0.0.1:3004"),
            Err(ChannelPolicyError::InvalidWebEndpoint)
        );
        assert_eq!(
            ChannelPolicy::try_new("/api", "https://user:secret@example.test"),
            Err(ChannelPolicyError::InvalidNativeEndpoint)
        );
    }
}
