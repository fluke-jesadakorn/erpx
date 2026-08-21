#![doc = "PostgreSQL adapters for Agent Orchestration."]
#![forbid(unsafe_code)]

mod command;
mod experience;
mod governance;
mod holding;
mod maintenance;
mod normalized;
mod policy;
mod projection;
mod publication;
mod registry;
mod retention;
mod runtime;
mod suggestion;

pub use experience::{SqlxAgentExperienceQuery, SqlxAgentExperienceStore};
pub use governance::{SqlxAgentGovernanceQuery, SqlxAgentGovernanceRepository};
pub use maintenance::{AgentMaintenanceOutcome, SqlxAgentGovernanceMaintenance};
pub use policy::SqlxProviderPolicyRepository;
pub use projection::AgentProjectionSet;
pub use registry::SqlxAgentRegistry;
pub use retention::SqlxAgentMaterialRetentionRepository;
pub use runtime::{ClaimedAgentNotificationFact, SqlxAgentRuntime, SqlxWorkerQueue};
pub use suggestion::SqlxSuggestionRepository;

pub const CONTEXT_NAME: &str = "agent_orchestration";
pub const ASSIGNMENT_PROJECTION: &str = "agent_orchestration.assignments";

#[cfg(test)]
const WRITE_DDL: &str =
    include_str!("../../../migration/migrations_holding_write/0001_baseline.sql");
#[cfg(test)]
const READ_DDL: &str = include_str!("../../../migration/migrations_holding_read/0001_baseline.sql");

#[cfg(test)]
#[path = "../../schema_test.rs"]
mod schema_test;
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_schema_has_durable_assignment_and_budget_fences() {
        assert!(WRITE_DDL.contains("CREATE TABLE erpx_agent_write.agent_assignments"));
        assert!(
            WRITE_DDL
                .contains("UNIQUE (holding_id, scope_key, root_action_ref, requested_agent_ref)")
        );
        assert!(WRITE_DDL.contains("agent_budget_reservation_layers"));
        assert!(WRITE_DDL.contains("trigger_kind TEXT NOT NULL"));
        for allowed in [
            "'mention'",
            "'assign'",
            "'run'",
            "'compare'",
            "'delegation'",
        ] {
            assert!(WRITE_DDL.contains(allowed));
        }
        assert!(!WRITE_DDL.contains("schedule_trigger"));
        assert!(!WRITE_DDL.contains("event_trigger"));
        assert!(READ_DDL.contains(ASSIGNMENT_PROJECTION));
        assert!(!READ_DDL.contains("prompt_material"));
    }

    #[test]
    fn write_and_read_schema_fail_closed_without_the_exact_holding_setting() {
        for ddl in [WRITE_DDL, READ_DDL] {
            assert!(ddl.contains("nullif(current_setting('app.holding_id', true), '')::uuid"));
            assert!(ddl.contains("ENABLE ROW LEVEL SECURITY"));
            assert!(ddl.contains("FORCE ROW LEVEL SECURITY"));
            assert!(ddl.contains("USING (holding_id = erpx_agent_"));
        }
        assert!(
            WRITE_DDL.contains("WITH CHECK (holding_id = erpx_agent_write.current_holding_id())")
        );
    }

    #[test]
    fn every_agent_table_key_and_parent_reference_is_holding_composite() {
        let write = schema_test::select_schema_statements(WRITE_DDL, &["erpx_agent_write"]);
        let read = schema_test::select_schema_statements(READ_DDL, &["erpx_agent_read"]);
        assert_eq!(write.matches("PRIMARY KEY (holding_id").count(), 30);
        assert_eq!(read.matches("PRIMARY KEY (holding_id").count(), 8);
        let normalized = write.split_whitespace().collect::<Vec<_>>().join(" ");
        for reference in normalized.split("REFERENCES erpx_agent_write.").skip(1) {
            let mut tokens = reference.split_whitespace();
            let table = tokens.next().expect("referenced table");
            let columns = tokens.next().expect("referenced columns");
            assert!(
                columns.starts_with("(holding_id,"),
                "reference to {table} is not holding-composite: {columns}"
            );
        }
    }

    #[test]
    fn all_non_worker_adapters_share_the_verified_holding_boundary() {
        for source in [
            include_str!("command.rs"),
            include_str!("experience.rs"),
            include_str!("normalized.rs"),
            include_str!("policy.rs"),
            include_str!("registry.rs"),
            include_str!("retention.rs"),
            include_str!("runtime.rs"),
        ] {
            assert!(!source.contains(concat!(".pool", ".begin()")));
            assert!(!source.contains("fetch_optional(&self.pool)"));
            assert!(!source.contains("execute(&self.pool)"));
        }
        assert!(include_str!("normalized.rs").contains("WHERE holding_id=$1"));
        assert!(include_str!("command.rs").contains("ON CONFLICT (holding_id,"));
    }

    #[test]
    fn pool_session_and_provenance_are_versioned_authoritative_records() {
        for table in [
            "agent_pools",
            "agent_pool_members",
            "agent_pool_routing_decisions",
            "agent_sessions",
            "agent_session_messages",
            "provider_invocation_evidence",
        ] {
            assert!(WRITE_DDL.contains(&format!("CREATE TABLE erpx_agent_write.{table}")));
        }
        for column in [
            "pool_version BIGINT",
            "session_version BIGINT",
            "message_version BIGINT",
            "assignment_version BIGINT",
            "policy_version BIGINT",
            "output_digest TEXT",
            "evidence_version BIGINT",
        ] {
            assert!(WRITE_DDL.contains(column), "missing {column}");
        }
        for projection in [
            "agent_orchestration.pools",
            "agent_orchestration.pool_routing",
            "agent_orchestration.provider_invocations",
        ] {
            assert!(READ_DDL.contains(projection));
        }
        assert!(!READ_DDL.contains("encrypted_payload_ref"));
        assert!(!READ_DDL.contains("output_material"));
    }
}
