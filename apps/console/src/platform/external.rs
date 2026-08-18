//! Validated external actions for Web and macOS.

use crate::platform::{ExternalActionError, ExternalActionHost};

/// Accept only HTTPS and `mailto:` targets with no ambiguous URL characters.
pub fn validate_external_url(url: &str) -> Result<&str, ExternalActionError> {
    let lowercase = url.to_ascii_lowercase();
    if url.is_empty()
        || url.len() > 4_096
        || url.chars().any(char::is_control)
        || url.chars().any(char::is_whitespace)
        || url.contains('\\')
        || ["%00", "%0a", "%0d"]
            .iter()
            .any(|encoded| lowercase.contains(encoded))
    {
        return Err(ExternalActionError::UnsafeUrl);
    }

    if let Some(authority_and_path) = url.strip_prefix("https://") {
        let authority = authority_and_path
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default();
        let hostname = authority
            .rsplit_once(':')
            .map_or(authority, |(hostname, port)| {
                if port.parse::<u16>().is_ok_and(|port| port != 0) {
                    hostname
                } else {
                    ""
                }
            });
        if authority.contains('@') || !valid_hostname(hostname) {
            return Err(ExternalActionError::UnsafeUrl);
        }
        return Ok(url);
    }

    if let Some(address_and_query) = url.strip_prefix("mailto:") {
        let address = address_and_query
            .split_once('?')
            .map_or(address_and_query, |pair| pair.0);
        let valid_address = address.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && local.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'+' | b'-')
                })
                && valid_hostname(domain)
        });
        if valid_address && !address_and_query.contains('#') {
            return Ok(url);
        }
    }

    Err(ExternalActionError::UnsafeUrl)
}

fn valid_hostname(hostname: &str) -> bool {
    !hostname.is_empty()
        && hostname.len() <= 253
        && hostname.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Default)]
pub struct MacOsExternalActionHost;

#[cfg(target_os = "macos")]
impl ExternalActionHost for MacOsExternalActionHost {
    fn restart(&self) -> Result<(), ExternalActionError> {
        // Relaunch policy belongs to the process supervisor. Spawning a second
        // unmanaged console instance here would be unsafe.
        Err(ExternalActionError::Unsupported)
    }

    fn open_external_url(&self, url: &str) -> Result<(), ExternalActionError> {
        let url = validate_external_url(url)?;
        std::process::Command::new("/usr/bin/open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|_| ExternalActionError::Unsupported)
    }

    fn system_locale(&self) -> Option<String> {
        std::env::var("LANG")
            .ok()
            .and_then(|locale| locale.split(['.', '@']).next().map(str::to_owned))
            .filter(|locale| !locale.is_empty())
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Default)]
pub struct TypedWebExternalActionHost;

#[cfg(target_arch = "wasm32")]
impl ExternalActionHost for TypedWebExternalActionHost {
    fn restart(&self) -> Result<(), ExternalActionError> {
        web_sys::window()
            .ok_or(ExternalActionError::Unsupported)?
            .location()
            .reload()
            .map_err(|_| ExternalActionError::Unsupported)
    }

    fn open_external_url(&self, url: &str) -> Result<(), ExternalActionError> {
        let url = validate_external_url(url)?;
        web_sys::window()
            .ok_or(ExternalActionError::Unsupported)?
            .open_with_url_and_target(url, "_blank")
            .map(|_| ())
            .map_err(|_| ExternalActionError::Unsupported)
    }

    fn system_locale(&self) -> Option<String> {
        web_sys::window()
            .and_then(|window| window.navigator().language())
            .filter(|locale| !locale.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_unambiguous_https_and_mailto_targets() {
        for safe in [
            "https://docs.example.com/guide?q=rust#install",
            "https://localhost:3004/help",
            "mailto:support@example.com?subject=Help",
        ] {
            assert_eq!(validate_external_url(safe), Ok(safe));
        }
        for unsafe_url in [
            "http://example.com",
            "javascript:alert(1)",
            "https://user:secret@example.com",
            "https://bad..example.com",
            "https://example.com:99999",
            "https://example.com\\@evil.test",
            "https://example.com\n--args",
            "mailto:support@example.com%0d%0aBcc:evil@example.com",
            "mailto:not-an-address",
        ] {
            assert_eq!(
                validate_external_url(unsafe_url),
                Err(ExternalActionError::UnsafeUrl)
            );
        }
    }
}
