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
    include_str!("../../../migration/migrations_tenant_write/0076_ecommerce_talent.sql");
#[cfg(test)]
const READ_DDL: &str =
    include_str!("../../../migration/migrations_tenant_read/0067_ecommerce_talent_projection.sql");

pub fn aggregate_partition(aggregate_type: &str, id: uuid::Uuid) -> String {
    format!("talent.{aggregate_type}:{}", id.simple())
}

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
    fn all_talent_tables_are_tenant_bound_and_rls_protected() {
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
        assert!(!WRITE_DDL.contains("aggregate_guards"));
        assert!(!WRITE_DDL.contains("state JSONB"));
    }

    #[test]
    fn read_schema_separates_candidate_and_workforce_realms() {
        assert!(READ_DDL.contains(CANDIDATE_PROJECTION));
        assert!(READ_DDL.contains(WORKFORCE_PROJECTION));
        assert!(READ_DDL.contains("CREATE TABLE erpx_talent_read.candidate_projection"));
        assert!(READ_DDL.contains("CREATE TABLE erpx_talent_read.workforce_projection"));
        assert!(!READ_DDL.contains("email TEXT"));
        assert!(!READ_DDL.contains("phone TEXT"));
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
