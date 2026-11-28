//! Pure contract for the native handover-acknowledgement POST command.
//!
//! Owner SQL literals are byte-identical to the retired TypeScript statements:
//! the recipient assertion from `operations-handover-facts.ts` and the insert
//! from `collaboration/server/src/adapters/wf1a7.rs`. No I/O lives here.

use erpx_platform_shared_server::public::{BusinessError, FailureKind};

/// Retired `guard7` assertion body. Ordered binds: post_id, user_id,
/// post_id, user_id, post_version.
pub const RECIPIENT_GUARD_SQL: &str = concat!(
    "EXISTS(SELECT 1 FROM social_post_participants WHERE post_id=? AND user_id=?) ",
    "AND NOT EXISTS(SELECT 1 FROM social_handover_acknowledgements WHERE post_id=? AND user_id=? AND post_version=?)"
);

/// Retired `handover_ack_insert` owner write, rendered for sqlx `$n` binds.
pub const ACK_INSERT_SQL: &str = concat!(
    "INSERT INTO social_handover_acknowledgements(post_id,user_id,post_version,acknowledged_at,command_id) ",
    "VALUES($1,$2,$3,$4,$5)"
);

/// Retired `wf_native_command_receipt_replay` statement.
pub const RECEIPT_SQL: &str =
    "SELECT operation,result_json FROM command_receipts WHERE command_id=?";

pub const ACK_OPERATION: &str = "social.handover.acknowledge";

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

pub fn invalid_key() -> BusinessError {
    invalid(
        "COMMAND_KEY_REQUIRED",
        "Provide a valid confirmation key.",
    )
}

pub fn invalid_version() -> BusinessError {
    invalid(
        "HANDOVER_VERSION_INVALID",
        "Provide the handover version shown on this page.",
    )
}

pub fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Confirm from this workspace only.",
    )
}

pub fn source_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "HANDOVER_SOURCE_CHANGED",
        "This handover changed. Read the current version before confirming receipt.",
    )
}

pub fn recipient_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "HANDOVER_RECIPIENT_REQUIRED",
        "Only an intended recipient can confirm this handover.",
    )
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "This command key belongs to a different request.",
    )
}

/// Exact TS `/^[a-f0-9-]{36}$/i` check: 36 hex digits or hyphens.
pub fn validate_key(value: &str) -> Result<(), BusinessError> {
    if value.len() == 36
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
    {
        Ok(())
    } else {
        Err(invalid_key())
    }
}

/// Exact TS `x-erpx-handover-version` rule: `[1-9][0-9]{0,15}` and safe integer.
pub fn parse_version(raw: Option<&str>) -> Result<Option<i64>, BusinessError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    if raw.len() > 16
        || !raw
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_digit() && *byte != b'0')
        || !raw.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(invalid_version());
    }
    raw.parse::<i64>()
        .ok()
        .map(Some)
        .ok_or_else(invalid_version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_sql_matches_the_retired_statements() {
        assert_eq!(
            RECIPIENT_GUARD_SQL,
            "EXISTS(SELECT 1 FROM social_post_participants WHERE post_id=? AND user_id=?) AND NOT EXISTS(SELECT 1 FROM social_handover_acknowledgements WHERE post_id=? AND user_id=? AND post_version=?)"
        );
        assert!(ACK_INSERT_SQL.contains("VALUES($1,$2,$3,$4,$5)"));
        assert_eq!(
            RECEIPT_SQL,
            "SELECT operation,result_json FROM command_receipts WHERE command_id=?"
        );
    }

    #[test]
    fn command_key_shape_is_exact() {
        assert!(validate_key("01234567-89ab-cdef-0123-456789abcdef").is_ok());
        assert!(validate_key("------------------------------------").is_ok());
        assert!(validate_key("short").is_err());
        assert!(validate_key("01234567-89ab-cdef-0123-456789abcdeg").is_err());
    }

    #[test]
    fn version_header_shape_is_exact() {
        assert_eq!(parse_version(None).unwrap(), None);
        assert_eq!(parse_version(Some("1")).unwrap(), Some(1));
        assert_eq!(parse_version(Some("9007199254740991")).unwrap(), Some(9_007_199_254_740_991));
        assert!(parse_version(Some("0")).is_err());
        assert!(parse_version(Some("01")).is_err());
        assert!(parse_version(Some("99999999999999999")).is_err());
        assert!(parse_version(Some("12x")).is_err());
    }
}
