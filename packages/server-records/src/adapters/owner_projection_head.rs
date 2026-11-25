//! Native-owned static Records owner projection heads.
//!
//! The TypeScript `employeeRequestsProjectionSql` literal was the canonical
//! `employee_requests` column projection. Its exact bytes now live in this
//! owner's JSON contract, bound into Rust with `include_str!` and read by the
//! TypeScript adapter through its native contract import. No statement token
//! remains in TypeScript; consumers keep their own predicates, aliases and
//! binds and execute in their supplied transaction.

/// Native-owned SQL contract for the static Records owner projections.
pub const OWNER_PROJECTION_HEAD_SOURCES_JSON: &str =
    include_str!("owner-projection-head-sources.json");

#[derive(serde::Deserialize)]
struct OwnerProjectionSources {
    #[serde(rename = "employeeRequests")]
    employee_requests: String,
}

static SOURCES: std::sync::OnceLock<OwnerProjectionSources> = std::sync::OnceLock::new();

fn sources() -> &'static OwnerProjectionSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(OWNER_PROJECTION_HEAD_SOURCES_JSON)
            .expect("records owner projection head SQL contract must be valid JSON")
    })
}

/// Frozen `employee_requests` column projection.
pub fn employee_requests_projection() -> &'static str {
    &sources().employee_requests
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPECTED: &str = "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"category\",\"title\",\"description\",\"location\",\"impact\",\"needed_date\",\"items_json\",\"currency\",\"requester_user_id\",\"created_by\",\"parent_request_id\",\"state\",\"team_id\",\"assignee_user_id\",\"routing_json\",\"approval_json\",\"blocked_reason\",\"resolution\",\"purchase_order_id\",\"handover_json\",\"version\",\"evidence_version\",\"created_at\",\"updated_at\",\"engagement_id\" FROM employee_requests";
    const TS_HELPER: &str = include_str!("platform-owner-projection-sources.ts");

    #[test]
    fn contract_is_byte_exact_to_the_retired_typescript_literal() {
        let sql = employee_requests_projection();
        assert_eq!(sql, EXPECTED);
        assert_eq!(sql.len(), 431);
        assert_eq!(sql.matches("SELECT ").count(), 1);
        assert!(sql.starts_with("SELECT \"id\","));
        assert!(sql.ends_with(" FROM employee_requests"));
        for write in ["INSERT INTO", "DELETE FROM", "UPDATE ", "FOR UPDATE", "LOCK TABLE"] {
            assert!(!sql.contains(write));
        }
    }

    #[test]
    fn typescript_helper_carries_no_statement_token() {
        assert!(!TS_HELPER.contains("SELECT "));
        assert!(TS_HELPER.contains("owner-projection-head-sources.json"));
        assert!(TS_HELPER.contains("export const employeeRequestsProjectionSql"));
        assert!(TS_HELPER.contains("export const platformOwnerProjectionSources"));
        assert!(!TS_HELPER.contains(".prepare("));
        assert!(!TS_HELPER.contains(".bind("));
    }

    #[test]
    fn native_rfq_source_states_the_same_projection_bytes() {
        let rfq = include_str!("rfq_request_source.rs");
        assert!(rfq.contains(EXPECTED));
        assert_eq!(rfq.matches(" FROM employee_requests)").count(), 2);
    }
}
