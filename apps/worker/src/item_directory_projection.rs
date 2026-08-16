//! Typed owner-fact consumption for the tenant-local Item Directory.
//!
//! This projector never examines an aggregate snapshot. Descriptor content is
//! accepted only from the shared, validated owner draft contract and is fenced
//! by the immutable tenant capability supplied by routing composition.

use async_trait::async_trait;
use chrono::Utc;
use erpx_collaboration_sqlx::{DirectoryAdapterError, ItemDirectoryProjectionAdapter};
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionPool, ProjectionSource, WritePool};
use erpx_infrastructure::{
    ProjectionClaimRecord, ProjectionJobKind, RoutedDatabaseSide, RoutedTenantDatabase,
};
use erpx_integration_contracts::{
    AUTHORITY_EVIDENCE_EVENT_TYPE, ITEM_DIRECTORY_FACT_EVENT_TYPE, ITEM_DIRECTORY_FACT_VERSION,
    ITEM_DIRECTORY_OWNER_EVENT_TYPE, ITEM_DIRECTORY_PROJECTION_NAME, ItemDescriptorDraft,
    ItemDirectoryFact, TenantItemDirectoryFact,
};
use erpx_kernel::TenantId;
use serde::Deserialize;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    TenantAuthorityEvidenceProjection, TenantProjectionExecutorError,
    TenantProjectionRoutedExecutor,
};

pub const ITEM_DIRECTORY_PROJECTION_MAX_PAGE_SIZE: u32 = 1_000;
const MAX_ITEM_DIRECTORY_EVENTS_PER_PASS: u64 = 1_000_000;

pub const FUTURE_LIVE_RECORD_FAMILIES: &[&str] = &[
    "manufacturing.specification",
    "manufacturing.order",
    "manufacturing.routing",
    "manufacturing.batch",
    "manufacturing.result",
    "planning.scenario",
    "planning.recommendation",
    "planning.demand-plan",
    "planning.supply-plan",
    "planning.capacity-plan",
    "project.project",
    "project.phase",
    "project.item",
    "project.template",
    "project.handoff",
    "commerce.catalog-publication",
    "commerce.cart",
    "commerce.order",
    "commerce.return",
    "commerce.fulfillment-request",
    "talent.requisition",
    "talent.candidate",
    "talent.application",
    "talent.offer",
    "talent.proposal",
    "talent.interview",
    "talent.review-cycle",
    "learning.course",
    "learning.enrollment",
    "learning.assignment",
    "learning.completion",
    "benefits.plan",
    "benefits.election",
    "benefits.window",
    "benefits.handoff",
    "field-service.agreement",
    "field-service.request",
    "field-service.dispatch",
    "field-service.visit",
    "field-service.handoff",
    "collaboration.thread",
    "collaboration.moderation-case",
    "collaboration.promotion",
    "agent.definition",
    "agent.version",
    "agent.assignment",
    "agent.session",
    "agent.delegation-tree",
    "agent.budget-decision",
];

pub const FUTURE_DESCRIPTOR_GAPS: &[&str] = &[];

pub const UNROUTED_FUTURE_AUTHORITY: &[&str] = &[
    "planning.run",
    "ecommerce.channel",
    "talent.hire-handoff",
    "field-service.order",
    "projects.dependency",
    "agent.pool",
    "agent.pool-routing-decision",
    "agent.provider-invocation",
    "agent.action-card",
];

#[derive(Debug, Clone)]
pub struct TenantItemDirectoryProjector {
    tenant_id: TenantId,
    directory: ItemDirectoryProjectionAdapter,
}

impl TenantItemDirectoryProjector {
    pub const fn new(tenant_id: TenantId, directory: ItemDirectoryProjectionAdapter) -> Self {
        Self {
            tenant_id,
            directory,
        }
    }

    pub async fn apply(&self, event: &ProjectionEvent) -> Result<(), ItemDirectoryProjectionError> {
        let publication = decode_item_directory_owner_event(self.tenant_id, event)?;
        self.directory
            .apply_fact(&publication, Utc::now())
            .await
            .map_err(ItemDirectoryProjectionError::Projection)
    }
}

pub fn decode_item_directory_owner_event(
    tenant_id: TenantId,
    event: &ProjectionEvent,
) -> Result<TenantItemDirectoryFact, ItemDirectoryProjectionError> {
    if event.event_type != ITEM_DIRECTORY_OWNER_EVENT_TYPE
        || event.event_version != ITEM_DIRECTORY_FACT_VERSION
    {
        return Err(ItemDirectoryProjectionError::UnsupportedContract);
    }
    let draft = decode_draft(event.payload.clone())?;
    draft
        .validate()
        .map_err(|_| ItemDirectoryProjectionError::MalformedOwnerFact)?;
    if draft.record.id().as_uuid() != event.aggregate_id.as_uuid()
        || draft.record.owner_context().slug() != event.context.replace('_', "-")
        || draft.descriptor_version != event.aggregate_version
    {
        return Err(ItemDirectoryProjectionError::OwnerEnvelopeMismatch);
    }
    let fact = draft
        .materialize(event.global_sequence)
        .map_err(|_| ItemDirectoryProjectionError::MalformedOwnerFact)?;
    TenantItemDirectoryFact::try_new(tenant_id, fact)
        .map_err(|_| ItemDirectoryProjectionError::MalformedOwnerFact)
}

pub fn decode_item_directory_event(
    tenant_id: TenantId,
    event: &ProjectionEvent,
) -> Result<TenantItemDirectoryFact, ItemDirectoryProjectionError> {
    if event.event_type == ITEM_DIRECTORY_OWNER_EVENT_TYPE {
        return decode_item_directory_owner_event(tenant_id, event);
    }
    if event.event_type != ITEM_DIRECTORY_FACT_EVENT_TYPE
        || event.event_version != ITEM_DIRECTORY_FACT_VERSION
    {
        return Err(ItemDirectoryProjectionError::UnsupportedContract);
    }
    let fact = decode_final_fact(event.payload.clone())?;
    fact.validate()
        .map_err(|_| ItemDirectoryProjectionError::MalformedOwnerFact)?;
    // A final policy/relation fact may be emitted after the authoritative
    // decision that supplied its source position. It may never claim a future
    // position relative to its durable delivery envelope.
    if fact.source_event_position() > event.global_sequence {
        return Err(ItemDirectoryProjectionError::OwnerEnvelopeMismatch);
    }
    TenantItemDirectoryFact::try_new(tenant_id, fact)
        .map_err(|_| ItemDirectoryProjectionError::MalformedOwnerFact)
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OwnerDraftWire {
    Direct(ItemDescriptorDraft),
    PayloadKind(OwnerDraftPayloadKind),
}

#[derive(Debug, Deserialize)]
struct OwnerDraftPayloadKind {
    payload_kind: OwnerDraftKind,
    payload: ItemDescriptorDraft,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum OwnerDraftKind {
    Directory,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum FinalFactWire {
    Direct(ItemDirectoryFact),
    PayloadKind(FinalFactPayloadKind),
}

#[derive(Debug, Deserialize)]
struct FinalFactPayloadKind {
    payload_kind: FinalFactKind,
    payload: ItemDirectoryFact,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FinalFactKind {
    DirectoryFact,
}

fn decode_draft(
    value: serde_json::Value,
) -> Result<ItemDescriptorDraft, ItemDirectoryProjectionError> {
    match serde_json::from_value::<OwnerDraftWire>(value)
        .map_err(|_| ItemDirectoryProjectionError::MalformedOwnerFact)?
    {
        OwnerDraftWire::Direct(value) => Ok(value),
        OwnerDraftWire::PayloadKind(OwnerDraftPayloadKind {
            payload_kind: OwnerDraftKind::Directory,
            payload,
        }) => Ok(payload),
    }
}

fn decode_final_fact(
    value: serde_json::Value,
) -> Result<ItemDirectoryFact, ItemDirectoryProjectionError> {
    match serde_json::from_value::<FinalFactWire>(value)
        .map_err(|_| ItemDirectoryProjectionError::MalformedOwnerFact)?
    {
        FinalFactWire::Direct(value) => Ok(value),
        FinalFactWire::PayloadKind(FinalFactPayloadKind {
            payload_kind: FinalFactKind::DirectoryFact,
            payload,
        }) => Ok(payload),
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ItemDirectoryTenantProjectionExecutor {
    page_size: u32,
}

impl ItemDirectoryTenantProjectionExecutor {
    pub fn new(page_size: u32) -> Result<Self, ItemDirectoryProjectionError> {
        if page_size == 0 || page_size > ITEM_DIRECTORY_PROJECTION_MAX_PAGE_SIZE {
            return Err(ItemDirectoryProjectionError::InvalidConfiguration);
        }
        Ok(Self { page_size })
    }

    async fn execute_store(
        &self,
        claim: &ProjectionClaimRecord,
        write: &RoutedTenantDatabase,
        read: &RoutedTenantDatabase,
    ) -> Result<(), TenantProjectionExecutorError> {
        validate_routed_claim(claim, write, read)?;
        let tenant_uuid = Uuid::parse_str(&claim.job.binding.tenant_id)
            .map_err(|_| TenantProjectionExecutorError::Permanent)?;
        let tenant_id = TenantId::try_from_uuid(tenant_uuid)
            .map_err(|_| TenantProjectionExecutorError::Permanent)?;
        let write_pool = WritePool::for_tenant(write.database_pool().clone(), tenant_uuid)
            .map_err(|_| TenantProjectionExecutorError::Permanent)?;
        let authority_write = write_pool.clone();
        let source = ProjectionSource::new(write_pool);
        let directory = ItemDirectoryProjectionAdapter::new(read.database_pool().clone());
        let authority_pool = ProjectionPool::for_tenant(read.database_pool().clone(), tenant_uuid)
            .map_err(|_| TenantProjectionExecutorError::Permanent)?;
        let authority =
            TenantAuthorityEvidenceProjection::new(tenant_id, authority_pool, authority_write);
        let event_types = vec![
            ITEM_DIRECTORY_OWNER_EVENT_TYPE.to_owned(),
            ITEM_DIRECTORY_FACT_EVENT_TYPE.to_owned(),
            AUTHORITY_EVIDENCE_EVENT_TYPE.to_owned(),
        ];
        let (generation, source_high_watermark, mut cursor) = match claim.job.kind {
            ProjectionJobKind::Rebuild => {
                let (generation, source_high_watermark) = rebuild_fence(claim)?;
                let cursor = directory
                    .begin_rebuild(tenant_id, generation, source_high_watermark, Utc::now())
                    .await
                    .map_err(|_| TenantProjectionExecutorError::Retryable)?;
                (generation, source_high_watermark, cursor)
            }
            ProjectionJobKind::Incremental => {
                let source_high_watermark = source
                    .high_watermark(&event_types)
                    .await
                    .map_err(|_| TenantProjectionExecutorError::Retryable)?;
                let checkpoint = directory
                    .begin_incremental(tenant_id, source_high_watermark, Utc::now())
                    .await
                    .map_err(|_| TenantProjectionExecutorError::Retryable)?;
                (
                    checkpoint.generation,
                    checkpoint.source_high_watermark,
                    checkpoint.applied_position,
                )
            }
        };
        let mut events_seen = 0_u64;
        while cursor < source_high_watermark {
            let page = source
                .page_between(
                    cursor,
                    Some(source_high_watermark),
                    &event_types,
                    self.page_size,
                )
                .await
                .map_err(|_| TenantProjectionExecutorError::Retryable)?;
            if page.events.is_empty() {
                return Err(TenantProjectionExecutorError::Retryable);
            }
            events_seen = events_seen.saturating_add(page.events.len() as u64);
            if events_seen > MAX_ITEM_DIRECTORY_EVENTS_PER_PASS {
                return Err(TenantProjectionExecutorError::Permanent);
            }
            let previous = cursor;
            for event in page.events {
                if event.event_type == AUTHORITY_EVIDENCE_EVENT_TYPE {
                    authority.apply(&event).await.map_err(|error| match error {
                        crate::AuthorityProjectionError::Unavailable => {
                            TenantProjectionExecutorError::Retryable
                        }
                        _ => TenantProjectionExecutorError::Permanent,
                    })?;
                } else {
                    let fact = decode_item_directory_event(tenant_id, &event)
                        .map_err(|_| TenantProjectionExecutorError::Permanent)?;
                    directory
                        .apply_fact(&fact, Utc::now())
                        .await
                        .map_err(|_| TenantProjectionExecutorError::Retryable)?;
                }
                cursor = event.global_sequence;
            }
            directory
                .advance_checkpoint(tenant_id, generation, previous, cursor, Utc::now())
                .await
                .map_err(|_| TenantProjectionExecutorError::Retryable)?;
        }
        authority
            .reconcile_grants(&directory, source_high_watermark, Utc::now())
            .await
            .map_err(|error| match error {
                crate::AuthorityProjectionError::Unavailable => {
                    TenantProjectionExecutorError::Retryable
                }
                _ => TenantProjectionExecutorError::Permanent,
            })?;
        directory
            .complete_rebuild(tenant_id, generation, source_high_watermark, Utc::now())
            .await
            .map_err(|_| TenantProjectionExecutorError::Retryable)
    }
}

#[async_trait]
impl TenantProjectionRoutedExecutor for ItemDirectoryTenantProjectionExecutor {
    async fn execute_routed(
        &self,
        claim: &ProjectionClaimRecord,
        write: &RoutedTenantDatabase,
        read: &RoutedTenantDatabase,
    ) -> Result<(), TenantProjectionExecutorError> {
        self.execute_store(claim, write, read).await
    }
}

fn validate_routed_claim(
    claim: &ProjectionClaimRecord,
    write: &RoutedTenantDatabase,
    read: &RoutedTenantDatabase,
) -> Result<(), TenantProjectionExecutorError> {
    if claim.job.projection_name != ITEM_DIRECTORY_PROJECTION_NAME
        || write.side != RoutedDatabaseSide::Write
        || read.side != RoutedDatabaseSide::Read
        || write.tenant_id != claim.job.binding.tenant_id
        || read.tenant_id != claim.job.binding.tenant_id
        || write.holding_ref != claim.job.binding.holding_ref
        || read.holding_ref != claim.job.binding.holding_ref
    {
        return Err(TenantProjectionExecutorError::Permanent);
    }
    Ok(())
}

fn rebuild_fence(
    claim: &ProjectionClaimRecord,
) -> Result<(u64, u64), TenantProjectionExecutorError> {
    let payload = claim
        .job
        .payload
        .as_object()
        .ok_or(TenantProjectionExecutorError::Permanent)?;
    let generation = payload
        .get("generation")
        .and_then(serde_json::Value::as_u64)
        .filter(|value| *value > 0)
        .ok_or(TenantProjectionExecutorError::Permanent)?;
    let source = payload
        .get("source_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(TenantProjectionExecutorError::Permanent)?;
    Ok((generation, source))
}

#[derive(Debug, Error)]
pub enum ItemDirectoryProjectionError {
    #[error("invalid Item Directory worker configuration")]
    InvalidConfiguration,
    #[error("unsupported Item Directory owner-fact contract")]
    UnsupportedContract,
    #[error("malformed typed Item Directory owner fact")]
    MalformedOwnerFact,
    #[error("Item Directory owner envelope does not match its typed fact")]
    OwnerEnvelopeMismatch,
    #[error("Item Directory projection is unavailable")]
    Projection(#[source] DirectoryAdapterError),
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use erpx_integration_contracts::{
        CanonicalRouteMoved, EffectiveGrantChanged, EffectiveGrantRevoked, ItemTombstoned,
        ReferenceRelationChanged, ReferenceRelationRevoked, StableRecordRef, catalog::ContextId,
        reference::RecordTypeId,
    };
    use erpx_kernel::{
        ActorAttribution, ActorKind, AggregateId, CommandId, CorrelationId, DataClassification,
        EventId, PrincipalId, RecordId, TenantScope,
    };
    use uuid::Uuid;

    use super::*;

    fn id() -> Uuid {
        Uuid::new_v4()
    }

    fn event() -> ProjectionEvent {
        let aggregate_id = id();
        let record = StableRecordRef::try_new(
            RecordId::try_from_uuid(aggregate_id).unwrap(),
            ContextId::Finance,
            RecordTypeId::try_new("finance.invoice").unwrap(),
        )
        .unwrap();
        let draft = ItemDescriptorDraft {
            record,
            scope: TenantScope::Platform,
            lifecycle_state: "posted".into(),
            authorization_policy: erpx_integration_contracts::record_policy("finance.invoice")
                .unwrap()
                .policy_id(),
            classification: DataClassification::Confidential,
            descriptor_version: 4,
            authorized_search_text: "INV-2026-0042".into(),
        };
        ProjectionEvent {
            event_id: EventId::try_from_uuid(id()).unwrap(),
            global_sequence: 81,
            context: "finance".into(),
            partition: "invoice/test".into(),
            command_id: CommandId::try_from_uuid(id()).unwrap(),
            event_type: ITEM_DIRECTORY_OWNER_EVENT_TYPE.into(),
            event_version: ITEM_DIRECTORY_FACT_VERSION,
            aggregate_type: "customer_invoice".into(),
            aggregate_id: AggregateId::try_from_uuid(aggregate_id).unwrap(),
            aggregate_version: 4,
            tenant_scope: TenantScope::Platform,
            actor: ActorAttribution {
                principal_ref: "worker:item-directory".into(),
                kind: ActorKind::SystemWorker,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            correlation_id: CorrelationId::try_from_uuid(id()).unwrap(),
            causation_id: None,
            payload: serde_json::to_value(draft).unwrap(),
            occurred_at: Utc::now(),
        }
    }

    #[test]
    fn typed_draft_uses_committed_outbox_position() {
        let tenant_id = TenantId::try_from_uuid(id()).unwrap();
        let publication = decode_item_directory_owner_event(tenant_id, &event()).unwrap();
        assert_eq!(publication.fact.source_event_position(), 81);
    }

    #[test]
    fn snapshot_like_json_and_owner_mismatch_are_rejected() {
        let tenant_id = TenantId::try_from_uuid(id()).unwrap();
        let mut value = event();
        value.payload = serde_json::json!({"aggregate": "invoice", "state": {}});
        assert!(matches!(
            decode_item_directory_owner_event(tenant_id, &value),
            Err(ItemDirectoryProjectionError::MalformedOwnerFact)
        ));

        let mut value = event();
        value.context = "sales".into();
        assert!(matches!(
            decode_item_directory_owner_event(tenant_id, &value),
            Err(ItemDirectoryProjectionError::OwnerEnvelopeMismatch)
        ));
    }

    #[test]
    fn tagged_owner_payload_is_decoded_without_snapshot_inference() {
        let tenant_id = TenantId::try_from_uuid(id()).unwrap();
        let mut value = event();
        value.payload = serde_json::json!({
            "payload_kind": "directory",
            "payload": value.payload,
        });
        assert_eq!(
            decode_item_directory_owner_event(tenant_id, &value)
                .unwrap()
                .fact
                .source_event_position(),
            81
        );
    }

    #[test]
    fn final_revocation_facts_accept_committed_past_positions_and_reject_future_claims() {
        let tenant_id = TenantId::try_from_uuid(id()).unwrap();
        let mut value = event();
        value.event_type = ITEM_DIRECTORY_FACT_EVENT_TYPE.into();
        value.payload = serde_json::to_value(ItemDirectoryFact::Tombstoned(ItemTombstoned {
            record_id: RecordId::try_from_uuid(id()).unwrap(),
            descriptor_version: 5,
            source_event_position: 80,
        }))
        .unwrap();
        assert!(matches!(
            decode_item_directory_event(tenant_id, &value).unwrap().fact,
            ItemDirectoryFact::Tombstoned(_)
        ));

        value.payload = serde_json::to_value(ItemDirectoryFact::Tombstoned(ItemTombstoned {
            record_id: RecordId::try_from_uuid(id()).unwrap(),
            descriptor_version: 6,
            source_event_position: 82,
        }))
        .unwrap();
        assert!(matches!(
            decode_item_directory_event(tenant_id, &value),
            Err(ItemDirectoryProjectionError::OwnerEnvelopeMismatch)
        ));
    }

    #[test]
    fn worker_decodes_only_typed_grant_relation_route_and_tombstone_mutations() {
        let tenant_id = TenantId::try_from_uuid(id()).unwrap();
        let invoice = StableRecordRef::try_new(
            RecordId::try_from_uuid(id()).unwrap(),
            ContextId::Finance,
            RecordTypeId::try_new("finance.invoice").unwrap(),
        )
        .unwrap();
        let order = StableRecordRef::try_new(
            RecordId::try_from_uuid(id()).unwrap(),
            ContextId::Sales,
            RecordTypeId::try_new("sales.order").unwrap(),
        )
        .unwrap();
        let principal_id = PrincipalId::try_from_uuid(id()).unwrap();
        let definition = erpx_integration_contracts::record_policy("finance.invoice").unwrap();
        let policy = definition.policy_id();
        let effective_at = Utc::now();
        let facts = [
            ItemDirectoryFact::EffectiveGrantChanged(EffectiveGrantChanged {
                record: invoice.clone(),
                principal_id,
                authorization_policy: policy,
                policy_version: definition.version,
                effective_at,
                expires_at: effective_at + chrono::Duration::minutes(30),
                source_event_position: 80,
            }),
            ItemDirectoryFact::EffectiveGrantRevoked(EffectiveGrantRevoked {
                record_id: invoice.id(),
                principal_id,
                authorization_policy: policy,
                policy_version: 4,
                source_event_position: 80,
            }),
            ItemDirectoryFact::RelationChanged(ReferenceRelationChanged {
                relation_id: id(),
                source: invoice.clone(),
                target: order,
                relation_type: "relates-to".into(),
                relation_version: 1,
                source_event_position: 80,
            }),
            ItemDirectoryFact::RelationRevoked(ReferenceRelationRevoked {
                relation_id: id(),
                source_event_position: 80,
            }),
            ItemDirectoryFact::RouteMoved(CanonicalRouteMoved {
                canonical_route: invoice.canonical_detail_route(),
                record: invoice.clone(),
                descriptor_version: 5,
                source_event_position: 80,
            }),
            ItemDirectoryFact::Tombstoned(ItemTombstoned {
                record_id: invoice.id(),
                descriptor_version: 6,
                source_event_position: 80,
            }),
        ];
        for fact in facts {
            let mut value = event();
            value.event_type = ITEM_DIRECTORY_FACT_EVENT_TYPE.into();
            value.payload = serde_json::to_value(&fact).unwrap();
            assert_eq!(
                decode_item_directory_event(tenant_id, &value).unwrap().fact,
                fact
            );
        }
    }

    #[test]
    fn production_composition_is_routed_and_never_reads_a_tenant_from_environment() {
        assert!(ItemDirectoryTenantProjectionExecutor::new(1).is_ok());
        assert!(ItemDirectoryTenantProjectionExecutor::new(0).is_err());
        assert!(
            ItemDirectoryTenantProjectionExecutor::new(ITEM_DIRECTORY_PROJECTION_MAX_PAGE_SIZE + 1)
                .is_err()
        );

        let app = include_str!("app.rs");
        let branch = app
            .split("\"item_directory\" =>")
            .nth(1)
            .expect("production Item Directory branch")
            .split("\"finance_banking\" =>")
            .next()
            .expect("bounded branch");
        assert!(
            branch
                .contains("handler_registry_with_control_plane_multi_tenant_projection_scheduler")
        );
        assert!(!branch.contains("ERPX_TENANT_ID"));
        assert!(!branch.contains("ERPX_HOLDING_REF"));
    }

    #[test]
    fn every_live_owner_uow_appends_the_typed_fact_inside_atomic_commit() {
        for source in [
            include_str!("../../../crates/adapters/identity-access-sqlx/src/command.rs"),
            include_str!("../../../crates/adapters/brand-sqlx/src/command.rs"),
            include_str!("../../../crates/adapters/commercial-sqlx/src/command.rs"),
            include_str!("../../../crates/adapters/organization-sqlx/src/command.rs"),
            include_str!("../../../crates/adapters/customer-authority-sqlx/src/command.rs"),
            include_str!("../../../crates/adapters/support-sqlx/src/command.rs"),
            include_str!("../../../crates/adapters/finance-sqlx/src/command.rs"),
            include_str!("../../../crates/adapters/sales-sqlx/src/command.rs"),
            include_str!("../../../crates/adapters/pos-sqlx/src/command.rs"),
            include_str!("../../../crates/adapters/procurement-sqlx/src/command.rs"),
            include_str!("../../../crates/adapters/assets-sqlx/src/command.rs"),
            include_str!("../../../crates/adapters/maintenance-sqlx/src/command.rs"),
            include_str!("../../../crates/adapters/quality-sqlx/src/command.rs"),
            include_str!("../../../crates/adapters/operations-sqlx/src/command.rs"),
            include_str!("../../../crates/adapters/operations-sqlx/src/source.rs"),
            include_str!("../../../crates/adapters/people-payroll-sqlx/src/command.rs"),
            include_str!("../../../crates/adapters/legal-sqlx/src/command.rs"),
            include_str!("../../../crates/adapters/crm-sqlx/src/command.rs"),
        ] {
            if source.contains("ItemDescriptorDraft") {
                assert!(
                    source.contains("record_policy_id(&record)"),
                    "a live owner must pin its approved record-policy definition"
                );
                assert!(
                    !source.contains("PolicyId::try_from_uuid(record.id().as_uuid())"),
                    "a record identifier is not an authorization policy"
                );
            }
            let fact = source
                .find("ITEM_DIRECTORY_OWNER_EVENT_TYPE")
                .expect("owner fact event");
            assert!(
                !source.contains("commit_guarded("),
                "live owners must not retain generic snapshot commit authority"
            );
            let commit = source[fact..]
                .find(".commit_normalized(")
                .map(|offset| fact + offset)
                .expect("normalized atomic commit after owner fact");
            assert!(fact < commit);
        }
    }

    #[test]
    fn future_live_and_gap_inventory_exactly_partitions_the_target_catalog() {
        use std::collections::BTreeSet;

        use erpx_integration_contracts::catalog::{ContextLifecycle, RECORD_TYPES};

        let target = RECORD_TYPES
            .iter()
            .filter(|record| record.owner.lifecycle() != ContextLifecycle::Current)
            .map(|record| record.id)
            .collect::<BTreeSet<_>>();
        let live = FUTURE_LIVE_RECORD_FAMILIES
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let gaps = FUTURE_DESCRIPTOR_GAPS
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        assert!(live.is_disjoint(&gaps));
        assert_eq!(live.union(&gaps).copied().collect::<BTreeSet<_>>(), target);
        assert_eq!(live.len(), FUTURE_LIVE_RECORD_FAMILIES.len());
        assert_eq!(gaps.len(), FUTURE_DESCRIPTOR_GAPS.len());
    }

    #[test]
    fn all_ten_future_owners_append_typed_drafts() {
        for source in [
            include_str!("../../../crates/adapters/collaboration-sqlx/src/publication.rs"),
            include_str!("../../../crates/adapters/agent-orchestration-sqlx/src/publication.rs"),
            include_str!("../../../crates/adapters/projects-sqlx/src/directory.rs"),
            include_str!("../../../crates/adapters/manufacturing-sqlx/src/directory.rs"),
            include_str!("../../../crates/adapters/planning-sqlx/src/directory.rs"),
            include_str!("../../../crates/adapters/ecommerce-sqlx/src/directory.rs"),
            include_str!("../../../crates/adapters/talent-sqlx/src/directory.rs"),
            include_str!("../../../crates/adapters/learning-sqlx/src/directory.rs"),
            include_str!("../../../crates/adapters/benefits-sqlx/src/directory.rs"),
            include_str!("../../../crates/adapters/field-service-sqlx/src/directory.rs"),
        ] {
            assert!(source.contains("ITEM_DIRECTORY_OWNER_EVENT_TYPE"));
            assert!(source.contains("ItemDescriptorDraft"));
            assert!(source.contains("record_policy_id(&record)"));
            assert!(source.contains("outbox_events"));
            assert!(!source.contains("ItemDescriptorDraft::from_snapshot"));
        }
    }
}
