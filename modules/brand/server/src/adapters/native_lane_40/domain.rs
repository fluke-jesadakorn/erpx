//! Native lane 40 Brand domain: management authority guard reads.
//!
//! Pure response types, validation and invariants only. No SQL, no transport
//! and no I/O live here. The two guards mirror the retired TypeScript owner
//! reads exactly: an active Branch row (`id`, `display_name`) and an active
//! Company existence flag.

use erpx_platform_shared_server::public::BusinessError;

/// Maximum identifier length of the retired owner guard.
pub const MAX_ID_LEN: usize = 200;
/// Maximum UTF-8 bytes in a Branch display name.
pub const MAX_NAME_BYTES: usize = 2048;

/// One active Branch guard row. Field names mirror the retired owner shape.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveBranch {
    pub id: String,
    pub display_name: String,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

/// Bounded owner identifier: non-empty, length-limited and free of control,
/// path-separator and backslash bytes, exactly as the retired guard checked
/// before binding a value. No caller text ever becomes SQL.
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_LEN
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

/// Validate a management target before it becomes a bind value. The Company
/// scope is optional because the Company existence guard binds only `id`.
pub fn validate_target(id: &str, company_id: Option<&str>) -> Result<(), BusinessError> {
    if !valid_id(id) || !company_id.map(valid_id).unwrap_or(true) {
        return Err(invalid(
            "BRAND_MANAGEMENT_REQUEST_INVALID",
            "Use one bounded Brand management target.",
        ));
    }
    Ok(())
}

/// Project a postgres guard row into the exact retired response shape. Any
/// drift fails closed rather than emitting a partial guard result.
pub fn project_branch(id: &str, display_name: &str) -> Result<ActiveBranch, BusinessError> {
    if !valid_id(id)
        || display_name.is_empty()
        || display_name.as_bytes().len() > MAX_NAME_BYTES
    {
        return Err(invalid(
            "BRAND_MANAGEMENT_RESPONSE_INVALID",
            "The native Brand management reply was invalid.",
        ));
    }
    Ok(ActiveBranch {
        id: id.to_owned(),
        display_name: display_name.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_bounded_and_path_or_control_bytes_are_refused() {
        assert!(valid_id("co-a"));
        assert!(valid_id("branch_1:2"));
        assert!(!valid_id(""));
        assert!(!valid_id("bad id"));
        assert!(!valid_id("br/invalid"));
        assert!(!valid_id("back\\slash"));
        assert!(!valid_id(&"x".repeat(MAX_ID_LEN + 1)));
        assert!(!valid_id("line\nfeed"));
    }

    #[test]
    fn target_validation_requires_a_bounded_company_scope_when_present() {
        assert!(validate_target("br-1", Some("co-1")).is_ok());
        assert!(validate_target("co-1", None).is_ok());
        assert_eq!(
            validate_target("br-1", Some("bad/id")).unwrap_err().code,
            "BRAND_MANAGEMENT_REQUEST_INVALID"
        );
    }

    #[test]
    fn branch_projection_refuses_drifted_rows() {
        let row = project_branch("br-1", "North Branch").unwrap();
        assert_eq!(row.id, "br-1");
        assert_eq!(row.display_name, "North Branch");
        assert_eq!(
            project_branch("", "North Branch").unwrap_err().code,
            "BRAND_MANAGEMENT_RESPONSE_INVALID"
        );
        assert_eq!(
            project_branch("br-1", "").unwrap_err().code,
            "BRAND_MANAGEMENT_RESPONSE_INVALID"
        );
        assert_eq!(
            project_branch("br-1", &"x".repeat(MAX_NAME_BYTES + 1))
                .unwrap_err()
                .code,
            "BRAND_MANAGEMENT_RESPONSE_INVALID"
        );
    }
}
