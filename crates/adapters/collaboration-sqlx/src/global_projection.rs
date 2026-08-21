//! Rebuildable control-read projection for Platform-scoped record collaboration.
//!
//! Only typed owner directory drafts from Platform Administration are accepted.
//! Holding-local facts are deliberately handled by the routed holding projector.

use async_trait::async_trait;
use erpx_collaboration::{RelationView, record_relation_graph_thread_id};
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, ItemDescriptorDraftV2,
    ItemDirectoryFact, catalog::ContextId,
};
use erpx_kernel::AuthorityScope;
use sqlx::{Postgres, Transaction};

pub const GLOBAL_COLLABORATION_PROJECTION: &str = "global_collaboration.records";
pub const GLOBAL_RELATION_DRAFT_EVENT_TYPE: &str = "collaboration.global_relation_draft.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlobalRelationDraftKind {
    Added,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GlobalRelationDraft {
    pub relation: RelationView,
    pub kind: GlobalRelationDraftKind,
}

impl GlobalRelationDraft {
    pub fn validate(&self) -> Result<(), ProjectionFailure> {
        self.relation
            .validate()
            .map_err(|_| failure("invalid global relation draft"))?;
        if self.relation.committed_sequence != 0
            || (self.kind == GlobalRelationDraftKind::Added && self.relation.is_revoked())
            || (self.kind == GlobalRelationDraftKind::Revoked && !self.relation.is_revoked())
        {
            return Err(failure("invalid global relation draft state"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct GlobalCollaborationProjector {
    event_types: Vec<String>,
}

impl GlobalCollaborationProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![
                ITEM_DIRECTORY_OWNER_EVENT_TYPE.into(),
                GLOBAL_RELATION_DRAFT_EVENT_TYPE.into(),
            ],
        }
    }
}

impl Default for GlobalCollaborationProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for GlobalCollaborationProjector {
    fn name(&self) -> &str {
        GLOBAL_COLLABORATION_PROJECTION
    }

    fn event_types(&self) -> &[String] {
        &self.event_types
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        _context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        if event.event_type == GLOBAL_RELATION_DRAFT_EVENT_TYPE {
            return apply_relation(transaction, event).await;
        }
        if event.context != ContextId::PlatformAdministration.slug().replace('-', "_") {
            // The control outbox also carries holding-scoped owner drafts for
            // other contexts. Their routed data-plane projector remains the
            // sole consumer of protected descriptor content.
            return Ok(());
        }
        if event.event_type != ITEM_DIRECTORY_OWNER_EVENT_TYPE
            || event.event_version != ITEM_DIRECTORY_FACT_VERSION
            || event.global_sequence == 0
        {
            return Err(failure("invalid global directory owner envelope"));
        }
        let draft = serde_json::from_value::<ItemDescriptorDraftV2>(event.payload.clone())
            .map_err(|_| failure("malformed global directory owner draft"))?;
        draft
            .validate()
            .map_err(|_| failure("invalid global directory owner draft"))?;
        if draft.draft.record.owner_context() != ContextId::PlatformAdministration
            || draft.draft.record.id().as_uuid() != event.aggregate_id.as_uuid()
            || draft.draft.descriptor_version != event.aggregate_version
            || draft.draft.scope != AuthorityScope::Platform
        {
            return Err(failure("global directory owner lineage mismatch"));
        }
        let fact = draft
            .materialize(event.global_sequence)
            .map_err(|_| failure("global directory materialization failed"))?;
        let ItemDirectoryFact::DescriptorChanged(changed) = fact else {
            return Err(failure("global directory accepted a non-descriptor fact"));
        };
        let descriptor = changed.descriptor;
        let record = descriptor.record();
        let record_ref = serde_json::to_value(record).map_err(projection_failure)?;
        let record_scope = serde_json::to_value(descriptor.scope()).map_err(projection_failure)?;
        let descriptor_document = serde_json::to_value(&descriptor).map_err(projection_failure)?;
        let classification = serde_json::to_value(descriptor.classification())
            .map_err(projection_failure)?
            .as_str()
            .ok_or_else(|| failure("invalid descriptor classification"))?
            .to_owned();
        let result = sqlx::query(
            "INSERT INTO erpx_global_collaboration_read.descriptors
             (record_id,owner_context,record_type,record_ref,record_scope,
              authorization_policy_id,authorization_policy_version,descriptor,
              canonical_route,display_label,authorized_search_text,lifecycle_state,
              classification,descriptor_version,source_event_position,tombstoned_at,projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,NULL,now())
             ON CONFLICT (record_id) DO UPDATE SET
               owner_context=EXCLUDED.owner_context,record_type=EXCLUDED.record_type,
               record_ref=EXCLUDED.record_ref,record_scope=EXCLUDED.record_scope,
               authorization_policy_id=EXCLUDED.authorization_policy_id,
               authorization_policy_version=EXCLUDED.authorization_policy_version,
               descriptor=EXCLUDED.descriptor,canonical_route=EXCLUDED.canonical_route,
               display_label=EXCLUDED.display_label,
               authorized_search_text=EXCLUDED.authorized_search_text,
               lifecycle_state=EXCLUDED.lifecycle_state,classification=EXCLUDED.classification,
               descriptor_version=EXCLUDED.descriptor_version,
               source_event_position=EXCLUDED.source_event_position,tombstoned_at=NULL,
               projected_at=now()
             WHERE erpx_global_collaboration_read.descriptors.source_event_position
                   < EXCLUDED.source_event_position
               AND erpx_global_collaboration_read.descriptors.descriptor_version
                   < EXCLUDED.descriptor_version",
        )
        .bind(record.id().as_uuid())
        .bind(record.owner_context().slug())
        .bind(record.record_type().as_str())
        .bind(record_ref)
        .bind(record_scope)
        .bind(descriptor.authorization_policy().as_uuid())
        .bind(to_i64(descriptor.authorization_policy_version())?)
        .bind(descriptor_document)
        .bind(descriptor.canonical_route())
        .bind(descriptor.display_label())
        .bind(changed.authorized_search_text)
        .bind(descriptor.lifecycle_state())
        .bind(classification)
        .bind(to_i64(descriptor.descriptor_version())?)
        .bind(to_i64(event.global_sequence)?)
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;
        if result.rows_affected() > 1 {
            return Err(failure(
                "global descriptor projection affected multiple rows",
            ));
        }
        advance_checkpoint(transaction, event.global_sequence).await
    }

    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        _context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        // Child projections are rebuilt from the same control outbox before
        // descriptors so no dangling authority or protected payload survives.
        for statement in [
            "DELETE FROM erpx_global_collaboration_read.attachments",
            "DELETE FROM erpx_global_collaboration_read.follower_projection",
            "DELETE FROM erpx_global_collaboration_read.reaction_projection",
            "DELETE FROM erpx_global_collaboration_read.comment_projection",
            "DELETE FROM erpx_global_collaboration_read.thread_projection",
            "DELETE FROM erpx_global_collaboration_read.relations",
            "DELETE FROM erpx_global_collaboration_read.effective_grants",
            "DELETE FROM erpx_global_collaboration_read.descriptors",
            "DELETE FROM erpx_global_collaboration_read.projection_state",
        ] {
            sqlx::query(statement)
                .execute(&mut **transaction)
                .await
                .map_err(projection_failure)?;
        }
        Ok(())
    }
}

async fn apply_relation(
    transaction: &mut Transaction<'_, Postgres>,
    event: &ProjectionEvent,
) -> Result<(), ProjectionFailure> {
    if event.context != "collaboration"
        || event.event_version != 1
        || event.active_scope != AuthorityScope::Platform
        || event.global_sequence == 0
    {
        return Err(failure("invalid global relation event envelope"));
    }
    let draft = serde_json::from_value::<GlobalRelationDraft>(event.payload.clone())
        .map_err(|_| failure("malformed global relation draft"))?;
    draft.validate()?;
    if draft.relation.source.id().as_uuid() != event.aggregate_id.as_uuid()
        || draft.relation.version != event.aggregate_version
    {
        return Err(failure("global relation event lineage mismatch"));
    }
    let mut relation = draft.relation;
    relation.committed_sequence = event.global_sequence;
    relation
        .validate()
        .map_err(|_| failure("invalid committed global relation"))?;
    let graph_thread_id = record_relation_graph_thread_id(&relation.source)
        .map_err(|_| failure("invalid global relation graph identity"))?;
    let relation_document = serde_json::to_value(&relation).map_err(projection_failure)?;
    let source_ref = serde_json::to_value(&relation.source).map_err(projection_failure)?;
    let target_ref = serde_json::to_value(&relation.target).map_err(projection_failure)?;
    let relation_kind = relation_kind_name(&relation.kind)?;
    sqlx::query(
        "INSERT INTO erpx_global_collaboration_read.relations
         (relation_id,source_record_id,target_record_id,source_record_ref,target_record_ref,
          graph_thread_id,relation_kind,relation_version,graph_version,relation_document,
          created_by_principal_id,created_at,revoked_by_principal_id,revoked_at,
          committed_sequence,projected_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$8,$9,$10,$11,$12,$13,$14,now())
         ON CONFLICT (relation_id) DO UPDATE SET
           relation_version=EXCLUDED.relation_version,graph_version=EXCLUDED.graph_version,
           relation_document=EXCLUDED.relation_document,
           revoked_by_principal_id=EXCLUDED.revoked_by_principal_id,
           revoked_at=EXCLUDED.revoked_at,committed_sequence=EXCLUDED.committed_sequence,
           projected_at=now()
         WHERE erpx_global_collaboration_read.relations.source_record_ref=EXCLUDED.source_record_ref
           AND erpx_global_collaboration_read.relations.target_record_ref=EXCLUDED.target_record_ref
           AND erpx_global_collaboration_read.relations.relation_kind=EXCLUDED.relation_kind
           AND erpx_global_collaboration_read.relations.relation_version
                 < EXCLUDED.relation_version
           AND erpx_global_collaboration_read.relations.committed_sequence
                 < EXCLUDED.committed_sequence",
    )
    .bind(relation.id.as_uuid())
    .bind(relation.source.id().as_uuid())
    .bind(relation.target.id().as_uuid())
    .bind(source_ref)
    .bind(target_ref)
    .bind(graph_thread_id.as_uuid())
    .bind(relation_kind)
    .bind(to_i64(relation.version)?)
    .bind(relation_document)
    .bind(relation.created_by.id().as_uuid())
    .bind(relation.created_at)
    .bind(
        relation
            .revoked_by
            .map(|principal| principal.id().as_uuid()),
    )
    .bind(relation.revoked_at)
    .bind(to_i64(event.global_sequence)?)
    .execute(&mut **transaction)
    .await
    .map_err(projection_failure)?;
    advance_checkpoint(transaction, event.global_sequence).await
}

fn relation_kind_name(
    kind: &erpx_collaboration::RelationKind,
) -> Result<&'static str, ProjectionFailure> {
    Ok(match kind {
        erpx_collaboration::RelationKind::RelatesTo => "relates_to",
        erpx_collaboration::RelationKind::Blocks => "blocks",
        erpx_collaboration::RelationKind::DependsOn => "depends_on",
        erpx_collaboration::RelationKind::Implements => "implements",
        erpx_collaboration::RelationKind::Supersedes => "supersedes",
        erpx_collaboration::RelationKind::DerivedFrom => "derived_from",
        erpx_collaboration::RelationKind::EvidenceFor => "evidence_for",
        erpx_collaboration::RelationKind::Custom { .. } => {
            return Err(failure("custom global relation is unavailable"));
        }
    })
}

async fn advance_checkpoint(
    transaction: &mut Transaction<'_, Postgres>,
    sequence: u64,
) -> Result<(), ProjectionFailure> {
    let sequence = to_i64(sequence)?;
    sqlx::query(
        "INSERT INTO erpx_global_collaboration_read.projection_state
         (singleton,rebuild_generation,source_high_watermark,applied_position,state,
          last_success_at,updated_at)
         VALUES (TRUE,1,$1,$1,'ready',now(),now())
         ON CONFLICT (singleton) DO UPDATE SET
           source_high_watermark=GREATEST(
             erpx_global_collaboration_read.projection_state.source_high_watermark,$1),
           applied_position=GREATEST(
             erpx_global_collaboration_read.projection_state.applied_position,$1),
           state='ready',last_success_at=now(),updated_at=now()",
    )
    .bind(sequence)
    .execute(&mut **transaction)
    .await
    .map_err(projection_failure)?;
    Ok(())
}

fn to_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value).map_err(|_| failure("global projection sequence overflow"))
}

fn projection_failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

fn failure(message: impl Into<String>) -> ProjectionFailure {
    ProjectionFailure::new(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projector_accepts_only_typed_owner_directory_facts() {
        let projector = GlobalCollaborationProjector::new();
        assert_eq!(projector.name(), GLOBAL_COLLABORATION_PROJECTION);
        assert_eq!(
            projector.event_types(),
            &[
                ITEM_DIRECTORY_OWNER_EVENT_TYPE,
                GLOBAL_RELATION_DRAFT_EVENT_TYPE
            ]
        );
        let source = include_str!("global_projection.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(production.contains("ItemDescriptorDraftV2"));
        assert!(production.contains("AuthorityScope::Platform"));
        assert!(!production.contains("app.holding_id"));
    }
}
