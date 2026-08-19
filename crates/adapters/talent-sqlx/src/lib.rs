#![doc = "PostgreSQL CQRS adapters for Recruiting and Talent."]
#![forbid(unsafe_code)]

mod directory;
mod repository;
mod runner_projection;

pub use repository::{SqlxTalentReadRepository, SqlxTalentWriteRepository};
pub use runner_projection::{TalentCandidateProjector, TalentSnapshot, TalentWorkforceProjector};

pub const CONTEXT_NAME: &str = "talent";
pub const CANDIDATE_PROJECTION: &str = "talent.candidate";
pub const WORKFORCE_PROJECTION: &str = "talent.workforce";
pub const CANDIDATE_SNAPSHOT_EVENT: &str = "talent.candidate.snapshot.v1";
pub const WORKFORCE_SNAPSHOT_EVENT: &str = "talent.workforce.snapshot.v1";
pub const WEB_ONLY: bool = true;
const _: () = assert!(WEB_ONLY);

#[cfg(test)]
const WRITE_DDL: &str =
    include_str!("../../../migration/migrations_holding_write/0001_baseline.sql");
#[cfg(test)]
const READ_DDL: &str = include_str!("../../../migration/migrations_holding_read/0001_baseline.sql");

pub fn aggregate_partition(aggregate_type: &str, id: uuid::Uuid) -> String {
    format!("talent.{aggregate_type}:{}", id.simple())
}

#[cfg(test)]
#[path = "../../schema_test.rs"]
mod schema_test;
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_schema_pins_selection_offer_and_handoff_evidence() {
        assert!(WRITE_DDL.contains("CREATE TABLE erpx_talent_write.applications"));
        assert!(WRITE_DDL.contains("profile_version"));
        assert!(WRITE_DDL.contains("requisition_version"));
        assert!(WRITE_DDL.contains("terms_digest"));
        assert!(WRITE_DDL.contains("offer_version"));
        assert!(WRITE_DDL.contains("employee_ref"));
    }

    #[test]
    fn all_talent_tables_are_holding_bound_and_rls_protected() {
        let owned = schema_test::select_schema_statements(WRITE_DDL, &["erpx_talent_write"]);
        for table in [
            "requisitions",
            "candidates",
            "applications",
            "offers",
            "hire_handoffs",
            "proposals",
            "command_receipts",
            "audit_log",
            "outbox",
        ] {
            assert!(WRITE_DDL.contains(&format!("'erpx_talent_write','{table}'")));
        }
        assert!(WRITE_DDL.contains("FORCE ROW LEVEL SECURITY"));
        assert!(!owned.contains("aggregate_guards"));
        assert!(!owned.contains("state JSONB"));
    }

    #[test]
    fn read_schema_separates_candidate_and_workforce_realms() {
        let owned = schema_test::select_schema_statements(READ_DDL, &["erpx_talent_read"]);
        assert!(owned.contains(CANDIDATE_PROJECTION));
        assert!(owned.contains(WORKFORCE_PROJECTION));
        assert!(owned.contains("CREATE TABLE erpx_talent_read.candidate_projection"));
        assert!(owned.contains("CREATE TABLE erpx_talent_read.workforce_projection"));
        assert!(!owned.contains("email TEXT"));
        assert!(!owned.contains("phone TEXT"));
    }

    #[test]
    fn talent_is_web_only_and_partitions_are_stable() {
        let id = uuid::Uuid::from_u128(2);
        assert_eq!(
            aggregate_partition("talent_offer", id),
            "talent.talent_offer:00000000000000000000000000000002"
        );
    }
}
