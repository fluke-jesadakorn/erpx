//! Owner-local plans for identity_access read fragments retired from TypeScript.
//!
//! A plan is a closed, byte-pinned statement fragment owned here rather than in
//! a TypeScript adapter. The first entry is the retired finance refund notice
//! assignment join; its only former TypeScript producer and public re-export
//! were dead (no caller in the repository), so no behavior depends on them now.
//! These fragments are never executed by this module: they record the exact
//! pre-retirement bytes so a consumer can reintroduce them only through an
//! owner-reviewed native read port.
use sha2::{Digest, Sha256};

/// Byte-identical successor of `financeRefundNoticeAssignmentSource()`.
pub const FINANCE_REFUND_NOTICE_ASSIGNMENT_SQL: &str =
    "JOIN (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a";

pub fn plan_sha256(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finance_refund_notice_assignment_plan_pins_the_retired_bytes() {
        assert_eq!(
            plan_sha256(FINANCE_REFUND_NOTICE_ASSIGNMENT_SQL),
            "dc11981c00603befbc6a70880dede5a69363220831ddb77a70c9800ce0edf44f"
        );
        assert!(FINANCE_REFUND_NOTICE_ASSIGNMENT_SQL.starts_with("JOIN (SELECT "));
        assert!(FINANCE_REFUND_NOTICE_ASSIGNMENT_SQL.ends_with(" FROM assignments) a"));
        assert!(!FINANCE_REFUND_NOTICE_ASSIGNMENT_SQL.contains("INSERT INTO"));
        assert!(!FINANCE_REFUND_NOTICE_ASSIGNMENT_SQL.contains("DELETE FROM"));
        assert!(!FINANCE_REFUND_NOTICE_ASSIGNMENT_SQL.contains("UPDATE "));
        assert!(!FINANCE_REFUND_NOTICE_ASSIGNMENT_SQL.contains("FOR UPDATE"));
    }
}
