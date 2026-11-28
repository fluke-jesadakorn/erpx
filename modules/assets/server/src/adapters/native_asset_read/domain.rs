//! Pure scope rules for the native Asset register read.
//!
//! No I/O, no SQL and no request types from the transport live here. The
//! literals mirror the retained TypeScript route bounds so the native path
//! rejects exactly the same malformed selectors.
use erpx_platform_shared_server::public::BusinessError;

/// Maximum Asset id length accepted by the retained `/assets?id=` route.
pub const MAX_ASSET_ID_LEN: usize = 200;
/// Maximum workspace slug accepted by the retained Holding route.
pub const MAX_SLUG_LEN: usize = 128;

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

pub fn invalid_request() -> BusinessError {
    invalid(
        "ASSETS_NATIVE_READ_INVALID",
        "Use a bounded Asset register read.",
    )
}

pub fn missing() -> BusinessError {
    BusinessError::new(
        erpx_platform_shared_server::public::FailureKind::NotFound,
        "NOT_FOUND",
        "Not found.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        erpx_platform_shared_server::public::FailureKind::Unavailable,
        "ASSETS_NATIVE_READ_UNAVAILABLE",
        "The native Assets read is unavailable.",
    )
}

/// The retained route rejects empty, oversized and control-character ids.
pub fn validate_id(id: &str) -> Result<(), BusinessError> {
    if id.is_empty() || id.len() > MAX_ASSET_ID_LEN || id.chars().any(char::is_control) {
        return Err(invalid_request());
    }
    Ok(())
}

fn valid_token(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}

pub fn valid_slug(slug: &str) -> bool {
    valid_token(slug, MAX_SLUG_LEN)
}

pub fn valid_host(host: &str) -> bool {
    !host.is_empty() && host.len() <= 512 && !host.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_asset_route_bounds_are_exact() {
        assert!(validate_id("asset-1").is_ok());
        assert!(validate_id(&"a".repeat(MAX_ASSET_ID_LEN)).is_ok());
        for id in ["", "a\n", "a\u{0000}", &"a".repeat(MAX_ASSET_ID_LEN + 1)] {
            assert_eq!(
                validate_id(id).unwrap_err().code,
                "ASSETS_NATIVE_READ_INVALID",
                "id: {id:?}"
            );
        }
        assert!(valid_slug("acme-dev_1"));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug(""));
        assert!(valid_host("acme.example.test"));
        assert!(!valid_host("acme.example.test\n"));
    }

    #[test]
    fn failure_codes_match_the_native_transport_contract() {
        assert_eq!(missing().code, "NOT_FOUND");
        assert_eq!(
            missing().kind,
            erpx_platform_shared_server::public::FailureKind::NotFound
        );
        assert_eq!(
            unavailable().kind,
            erpx_platform_shared_server::public::FailureKind::Unavailable
        );
    }
}
