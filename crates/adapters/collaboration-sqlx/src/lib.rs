#![doc = "PostgreSQL write, read, draft, and projection adapters for Collaboration."]
#![forbid(unsafe_code)]

mod attachment;
mod authorization;
mod directory;
mod global;
mod global_projection;
mod projection;
mod publication;
mod read;
mod relation;
mod write;

pub use attachment::{SqlxAttachmentRegistry, attachment_object_key};
pub use authorization::SqlxCollaborationAuthorization;
pub use directory::*;
pub use global::{
    GlobalCollaborationAccess, GlobalCollaborationOutcome, SqlxGlobalCollaborationGateway,
};
pub use global_projection::{GLOBAL_COLLABORATION_PROJECTION, GlobalCollaborationProjector};
pub use projection::{
    CollaborationInboxProjectionFact, CollaborationInboxProjector,
    CollaborationStreamProjectionFact, CollaborationStreamProjector,
    CollaborationThreadProjectionFact, CollaborationThreadProjector,
};
pub use read::SqlxThreadReadRepository;
pub use relation::{
    RECORD_RELATION_DRAFT_EVENT_TYPE, SqlxRecordRelationProjection, SqlxRecordRelationRepository,
};
pub use write::{SqlxDraftRepository, SqlxThreadWriteRepository};

pub const WRITE_SCHEMA: &str = "erpx_collaboration";
pub const READ_SCHEMA: &str = "erpx_collaboration_read";
pub const CONTEXT_NAME: &str = "collaboration";
pub const EVENT_VERSION: u32 = 1;
pub const THREAD_PROJECTION: &str = "collaboration.threads";
pub const STREAM_PROJECTION: &str = "collaboration.stream";
pub const INBOX_PROJECTION: &str = "collaboration.inbox";
pub const THREAD_PROJECTION_FACT_EVENT: &str = "collaboration.thread_projection_fact.v1";
pub const STREAM_PROJECTION_FACT_EVENT: &str = "collaboration.stream_projection_fact.v1";
pub const INBOX_PROJECTION_FACT_EVENT: &str = "collaboration.inbox_projection_fact.v1";

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
    fn write_schema_keeps_body_in_anchor_plane_and_inbox_opaque() {
        let owned = schema_test::select_schema_statements(WRITE_DDL, &["erpx_collaboration"]);
        assert!(owned.contains("threads"));
        assert!(!owned.contains("thread_snapshots"));
        assert!(!owned.contains("snapshot JSONB"));
        assert!(WRITE_DDL.contains("comment_revisions"));
        assert!(WRITE_DDL.contains("encrypted_drafts"));
        assert!(WRITE_DDL.contains("opaque_mention_inbox"));
        let inbox = WRITE_DDL
            .split("CREATE TABLE erpx_collaboration.opaque_mention_inbox")
            .nth(1)
            .expect("opaque inbox table")
            .split("CREATE TABLE")
            .next()
            .unwrap();
        for forbidden in ["comment_body", "plain_text", "record_type", "title"] {
            assert!(
                !inbox.contains(forbidden),
                "opaque inbox leaked {forbidden}"
            );
        }
    }

    #[test]
    fn thread_search_materializes_exact_anchor_authority_before_observable_work() {
        assert!(READ_DDL.contains("audience_allows"));
        assert!(READ_DDL.contains("ENABLE ROW LEVEL SECURITY"));
        let source = include_str!("read.rs");
        let search = source
            .split("async fn search(")
            .nth(1)
            .expect("thread search implementation")
            .split("async fn list_ids_for_record")
            .next()
            .unwrap();
        let freshness = search
            .find("directory_freshness AS MATERIALIZED")
            .expect("freshness gate");
        let candidate = search
            .find("authorized AS MATERIALIZED")
            .expect("authorized candidate set");
        let matching = search
            .find("search_vector @@ websearch_to_tsquery")
            .expect("full-text match");
        let count = search
            .find("SELECT count(*)::BIGINT AS authorized_count FROM matched")
            .expect("authorized count");
        let pagination = search
            .find("LIMIT $9 OFFSET $10")
            .expect("authorized pagination");
        let payload = search
            .find("paged.thread_id AS matched_thread_id, payload.projection")
            .expect("protected payload load");
        assert!(
            freshness < candidate
                && candidate < matching
                && matching < count
                && matching < pagination
                && pagination < payload
        );
        for exact_gate in [
            "item_directory_projection_state",
            "item_directory_descriptors",
            "item_directory_effective_grants",
            "grant_fact.principal_id=$5",
            "grant_fact.authorization_policy_id=descriptor.authorization_policy_id",
            "grant_fact.policy_version=descriptor.authorization_policy_version",
            "grant_fact.source_event_position>=descriptor.source_event_position",
            "descriptor.source_event_position<=freshness.applied_position",
            "grant_fact.source_event_position<=freshness.applied_position",
            "scope_contains($4, descriptor.record_scope)",
            "audience_allows(",
        ] {
            assert!(
                search.contains(exact_gate),
                "missing search gate {exact_gate}"
            );
        }
        assert!(search.contains("FROM (VALUES (TRUE)) AS required(singleton)"));
        assert!(search.contains("LEFT JOIN paged ON freshness.is_fresh"));
        assert!(!search.contains("count(*) OVER()"));
    }

    #[test]
    fn record_thread_candidates_are_opaque_until_exact_thread_authorization() {
        let source = include_str!("read.rs");
        let query = source
            .split("Only opaque IDs cross this candidate query")
            .nth(1)
            .expect("record candidate query")
            .split("async fn stream_after")
            .next()
            .unwrap();
        assert!(query.contains("SELECT thread_id, count(*) OVER() AS authorized_count"));
        assert!(query.contains("anchor_record_id=$4"));
        assert!(query.contains("audience_allows"));
        assert!(!query.contains("SELECT projection"));
        assert!(READ_DDL.contains("actor_realm <> 'internal'"));
        assert!(READ_DDL.contains("audience ->> 'portal' = actor_portal"));
    }

    #[test]
    fn normal_projection_never_serializes_moderation_evidence() {
        let source = include_str!("read.rs");
        assert!(!source.contains("SealedModerationEvidence"));
        assert!(!source.contains("encrypted_object_id"));
    }

    #[test]
    fn cursors_and_unique_identities_are_holding_prefixed() {
        let write = schema_test::select_schema_statements(WRITE_DDL, &["erpx_collaboration"]);
        let read = schema_test::select_schema_statements(READ_DDL, &["erpx_collaboration_read"]);
        for ddl in [&write, &read] {
            assert!(!ddl.contains("BIGSERIAL PRIMARY KEY"));
            assert!(!ddl.contains("UUID NOT NULL UNIQUE"));
        }
        assert!(WRITE_DDL.contains("PRIMARY KEY (holding_id, inbox_cursor)"));
        assert!(WRITE_DDL.contains("UNIQUE (holding_id, event_id)"));
        assert!(
            READ_DDL
                .contains("PRIMARY KEY (holding_id, projection_name, generation, inbox_cursor)")
        );
        assert!(READ_DDL.contains("UNIQUE (holding_id, projection_name, generation, event_id)"));
        assert!(WRITE_DDL.contains("PRIMARY KEY (holding_id, event_cursor)"));
        assert!(WRITE_DDL.contains("(holding_id, thread_id, comment_id, revision)"));
    }

    #[test]
    fn every_rls_binding_verifies_the_exact_holding_returned_by_postgres() {
        for source in [
            include_str!("authorization.rs"),
            include_str!("directory.rs"),
            include_str!("read.rs"),
            include_str!("write.rs"),
        ] {
            assert!(source.contains("query_scalar::<_, String>"));
            assert!(source.contains("bound == expected"));
        }
    }

    #[test]
    fn collaboration_reads_only_the_active_projection_generation() {
        let source = include_str!("read.rs");
        assert!(READ_DDL.contains("REFERENCES erpx_cqrs_read.projection_generations"));
        assert!(source.contains("require_active_generation"));
        assert!(source.contains("status='active'"));
        assert!(source.contains("projection_name=$2 AND generation=$3"));
    }

    #[test]
    fn item_directory_revocation_barriers_are_holding_scoped_and_forced_rls() {
        for table in [
            "item_directory_tombstones",
            "item_directory_grant_tombstones",
            "item_directory_relation_tombstones",
        ] {
            let create = format!("CREATE TABLE erpx_collaboration_read.{table}");
            assert!(READ_DDL.contains(&create), "missing {create}");
            assert!(
                READ_DDL.contains(&format!("'{table}'")),
                "missing RLS inventory for {table}"
            );
        }
        assert!(READ_DDL.contains("FORCE ROW LEVEL SECURITY"));
        assert!(READ_DDL.contains("PRIMARY KEY (holding_id, record_id)"));
        assert!(READ_DDL.contains("PRIMARY KEY (holding_id, record_id, principal_id)"));
        assert!(READ_DDL.contains("PRIMARY KEY (holding_id, relation_id)"));
        assert!(READ_DDL.contains("effective_at TIMESTAMPTZ NOT NULL"));
        assert!(READ_DDL.contains("expires_at TIMESTAMPTZ NOT NULL"));
        assert!(READ_DDL.contains("CHECK (expires_at > effective_at)"));
        assert!(READ_DDL.contains(
            "authorization_policy_version BIGINT NOT NULL CHECK (authorization_policy_version > 0)"
        ));
    }
}
