//! Pure domain for the native Maintenance eligible-owner option read.
//!
//! The statement is the exact owner-owned projection formerly composed by the
//! TypeScript `people` repository branch. Both owner projections are closed,
//! fixed literals copied from their owners; the only bind is the bounded
//! Company identifier from the request context. No SQL is built from caller
//! input and no I/O lives in this module.

use erpx_server_shared::public::BusinessError;
use serde::Serialize;

/// Exact SQL executed by the native read. The two inline projections are the
/// verbatim compact forms exported by People Payroll (`hr_people`) and
/// Identity Access (`assignments`); the predicate and ordering are the retained
/// TypeScript `people` read with `?` binds rewritten as `$1`.
pub const OWNER_OPTIONS_SQL: &str = "SELECT DISTINCT p.user_id id,p.legal_name name \
    FROM (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p \
    JOIN (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) x \
    ON x.user_id=p.user_id AND x.company_id=p.company_id \
    WHERE p.company_id=$1 AND p.status='active' AND p.merged_into_id IS NULL \
    AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
    AND iam_role_allows(x.role,x.company_id,'records.write') ORDER BY p.legal_name";

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "MAINTENANCE_OWNER_OPTIONS_QUERY_INVALID",
        "Use one bounded Maintenance owner-option read.",
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnerScope {
    pub company_id: String,
}

impl OwnerScope {
    pub fn new(company_id: &str) -> Result<Self, BusinessError> {
        if !valid_id(company_id) {
            return Err(invalid());
        }
        Ok(Self {
            company_id: company_id.to_owned(),
        })
    }
}

/// Complete row image returned by the TypeScript `people` read: `{id,name}`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct OwnerOption {
    pub id: String,
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn statement_is_a_single_bounded_owner_local_read() {
        assert!(OWNER_OPTIONS_SQL.starts_with("SELECT DISTINCT p.user_id id,p.legal_name name"));
        assert_eq!(OWNER_OPTIONS_SQL.matches("$1").count(), 1);
        assert!(!OWNER_OPTIONS_SQL.contains('?'));
        assert!(!OWNER_OPTIONS_SQL.contains("INSERT"));
        assert!(!OWNER_OPTIONS_SQL.contains("UPDATE"));
        assert!(!OWNER_OPTIONS_SQL.contains("DELETE"));
        assert!(!OWNER_OPTIONS_SQL.contains("LOCK"));
        for fragment in [
            "FROM hr_people",
            "FROM assignments",
            "iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)",
            "iam_role_allows(x.role,x.company_id,'records.write')",
            "ORDER BY p.legal_name",
        ] {
            assert!(OWNER_OPTIONS_SQL.contains(fragment), "missing {fragment}");
        }
    }

    #[test]
    fn ids_are_bounded_and_scope_is_closed() {
        assert!(valid_id("company-acme"));
        assert!(!valid_id(""));
        assert!(!valid_id("x/y"));
        assert!(OwnerScope::new("company-1").is_ok());
        assert!(OwnerScope::new("' OR 1=1").is_err());
    }

    #[test]
    fn option_shape_matches_the_typescript_row() {
        let value = serde_json::to_value(OwnerOption {
            id: "user-1".into(),
            name: "Ada".into(),
        })
        .unwrap();
        assert_eq!(value, json!({"id":"user-1","name":"Ada"}));
        assert_eq!(value.as_object().unwrap().len(), 2);
    }
}
