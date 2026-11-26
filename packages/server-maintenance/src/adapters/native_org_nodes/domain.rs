//! Pure domain for the native Maintenance organization-node option read.
//!
//! The statement is the exact owner-owned projection formerly composed by the
//! TypeScript `nodes` repository branch (`adapters/work.ts`). The inline
//! projection is the verbatim fixed literal exported by Organization, the only
//! bind set is the bounded Company/branch/department from the request context,
//! and no I/O lives in this module.

use erpx_server_shared::public::BusinessError;
use serde::Serialize;

/// Exact SQL executed by the native read. The inline projection is the verbatim
/// `ORG_NODES_SOURCE` fixed literal owned by Organization; the predicate and
/// ordering are the retained TypeScript `nodes` read with `?` binds rewritten
/// as positional `$1`..`$6`.
pub const ORG_NODES_SQL: &str = "SELECT id,kind,display_name,branch_id \
    FROM (SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes) n \
    WHERE company_id=$1 AND status='active' \
    AND kind IN ('branch','headquarters','department') \
    AND ($2::text IS NULL OR id=$3 OR branch_id=$4) \
    AND ($5::text IS NULL OR id=$6) ORDER BY kind,display_name";

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "MAINTENANCE_ORG_NODES_QUERY_INVALID",
        "Use one bounded Maintenance organization-node read.",
    )
}

/// Company/branch/department scope. A missing branch or department means the
/// actor is holding-scoped and is bound as SQL NULL (the TS `?::text IS NULL`
/// branches). Every present identifier is bounded before it reaches the port.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeScope {
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}

impl NodeScope {
    pub fn new(
        company_id: &str,
        branch_id: Option<&str>,
        department_id: Option<&str>,
    ) -> Result<Self, BusinessError> {
        if !valid_id(company_id)
            || branch_id.is_some_and(|value| !valid_id(value))
            || department_id.is_some_and(|value| !valid_id(value))
        {
            return Err(invalid());
        }
        Ok(Self {
            company_id: company_id.to_owned(),
            branch_id: branch_id.map(str::to_owned),
            department_id: department_id.map(str::to_owned),
        })
    }
}

/// Complete row image returned by the TypeScript `nodes` read:
/// `{id,kind,display_name,branch_id}`. `branch_id` is nullable because a
/// branch/headquarters node has no parent branch.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct OrgNode {
    pub id: String,
    pub kind: String,
    pub display_name: String,
    pub branch_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn statement_is_a_single_bounded_owner_local_read() {
        assert!(ORG_NODES_SQL.starts_with("SELECT id,kind,display_name,branch_id FROM ("));
        for bind in ["$1", "$2", "$3", "$4", "$5", "$6"] {
            assert_eq!(ORG_NODES_SQL.matches(bind).count(), 1, "missing {bind}");
        }
        assert!(!ORG_NODES_SQL.contains('?'));
        for forbidden in ["INSERT", "UPDATE ", "DELETE ", "LOCK "] {
            assert!(!ORG_NODES_SQL.contains(forbidden));
        }
        for fragment in [
            "\"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes",
            "company_id=$1 AND status='active'",
            "kind IN ('branch','headquarters','department')",
            "($2::text IS NULL OR id=$3 OR branch_id=$4)",
            "($5::text IS NULL OR id=$6)",
            "ORDER BY kind,display_name",
        ] {
            assert!(ORG_NODES_SQL.contains(fragment), "missing {fragment}");
        }
    }

    #[test]
    fn ids_are_bounded_and_scope_is_closed() {
        assert!(valid_id("node-acme"));
        assert!(!valid_id(""));
        assert!(!valid_id("x/y"));
        let holding = NodeScope::new("company-1", None, None).unwrap();
        assert_eq!(holding.branch_id, None);
        assert_eq!(holding.department_id, None);
        let scoped = NodeScope::new("company-1", Some("branch-2"), Some("dept-3")).unwrap();
        assert_eq!(scoped.branch_id.as_deref(), Some("branch-2"));
        assert_eq!(scoped.department_id.as_deref(), Some("dept-3"));
        assert!(NodeScope::new("' OR 1=1", None, None).is_err());
        assert!(NodeScope::new("company-1", Some("a/b"), None).is_err());
        assert!(NodeScope::new("company-1", None, Some("bad id")).is_err());
    }

    #[test]
    fn node_shape_matches_the_typescript_row() {
        let value = serde_json::to_value(OrgNode {
            id: "node-1".into(),
            kind: "branch".into(),
            display_name: "Bangkok".into(),
            branch_id: None,
        })
        .unwrap();
        assert_eq!(
            value,
            json!({"id":"node-1","kind":"branch","display_name":"Bangkok","branch_id":null})
        );
        assert_eq!(value.as_object().unwrap().len(), 4);
    }
}
