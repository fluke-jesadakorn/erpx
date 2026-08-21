//! Typed owner-fact consumption for the holding-local Item Directory.
//!
//! This projector never examines an aggregate snapshot. Descriptor content is
//! accepted only from the shared, validated owner draft contract and is fenced
//! by the immutable holding capability supplied by routing composition.

use async_trait::async_trait;
use chrono::Utc;
use erpx_collaboration_sqlx::{DirectoryAdapterError, ItemDirectoryProjectionAdapter};
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionPool, ProjectionSource, WritePool};
use erpx_infrastructure::{
    ProjectionClaimRecord, ProjectionJobKind, RoutedDatabaseSide, RoutedHoldingDatabase,
};
use erpx_integration_contracts::{
    AUTHORITY_EVIDENCE_EVENT_TYPE, HoldingItemDirectoryFact, ITEM_DIRECTORY_FACT_VERSION,
    ITEM_DIRECTORY_OWNER_EVENT_TYPE, ITEM_DIRECTORY_PROJECTION_NAME, ItemDescriptorDraft,
    organization::ORGANIZATION_LINEAGE_CHANGED_EVENT_TYPE,
};
use erpx_kernel::HoldingId;
use erpx_organization_sqlx::SNAPSHOT_EVENT_TYPE as ORGANIZATION_SNAPSHOT_EVENT_TYPE;
use serde::Deserialize;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    HoldingAuthorityEvidenceProjection, HoldingProjectionExecutorError,
    HoldingProjectionRoutedExecutor,
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
pub struct HoldingItemDirectoryProjector {
    holding_id: HoldingId,
    directory: ItemDirectoryProjectionAdapter,
}

impl HoldingItemDirectoryProjector {
    pub const fn new(holding_id: HoldingId, directory: ItemDirectoryProjectionAdapter) -> Self {
        Self {
            holding_id,
            directory,
        }
    }

    pub async fn apply(&self, event: &ProjectionEvent) -> Result<(), ItemDirectoryProjectionError> {
        let publication = decode_item_directory_owner_event(self.holding_id, event)?;
        self.directory
            .apply_fact(&publication, Utc::now())
            .await
            .map_err(ItemDirectoryProjectionError::Projection)
    }
}

pub fn decode_item_directory_owner_event(
    holding_id: HoldingId,
    event: &ProjectionEvent,
) -> Result<HoldingItemDirectoryFact, ItemDirectoryProjectionError> {
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
    HoldingItemDirectoryFact::try_new(holding_id, fact)
        .map_err(|_| ItemDirectoryProjectionError::MalformedOwnerFact)
}

pub fn decode_item_directory_event(
    holding_id: HoldingId,
    event: &ProjectionEvent,
) -> Result<HoldingItemDirectoryFact, ItemDirectoryProjectionError> {
    // No live write-side owner is authorized to publish the reserved final-fact
    // transport. Final policy facts are produced and applied inside this worker,
    // while descriptors cross the outbox only as owner-bound drafts. Keep this
    // gate closed until a variant-specific envelope contract can bind every
    // mutation to its authoritative owner identity.
    if event.event_type != ITEM_DIRECTORY_OWNER_EVENT_TYPE {
        return Err(ItemDirectoryProjectionError::UnsupportedContract);
    }
    decode_item_directory_owner_event(holding_id, event)
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

#[derive(Debug, Clone, Copy)]
pub struct ItemDirectoryHoldingProjectionExecutor {
    page_size: u32,
}

impl ItemDirectoryHoldingProjectionExecutor {
    pub fn new(page_size: u32) -> Result<Self, ItemDirectoryProjectionError> {
        if page_size == 0 || page_size > ITEM_DIRECTORY_PROJECTION_MAX_PAGE_SIZE {
            return Err(ItemDirectoryProjectionError::InvalidConfiguration);
        }
        Ok(Self { page_size })
    }

    async fn execute_store(
        &self,
        claim: &ProjectionClaimRecord,
        write: &RoutedHoldingDatabase,
        read: &RoutedHoldingDatabase,
    ) -> Result<(), HoldingProjectionExecutorError> {
        validate_routed_claim(claim, write, read)?;
        let holding_uuid = Uuid::parse_str(&claim.job.binding.holding_id)
            .map_err(|_| HoldingProjectionExecutorError::Permanent)?;
        let holding_id = HoldingId::try_from_uuid(holding_uuid)
            .map_err(|_| HoldingProjectionExecutorError::Permanent)?;
        let write_pool = WritePool::for_holding(write.database_pool().clone(), holding_uuid)
            .map_err(|_| HoldingProjectionExecutorError::Permanent)?;
        let authority_write = write_pool.clone();
        let source = ProjectionSource::new(write_pool);
        let directory = ItemDirectoryProjectionAdapter::new(read.database_pool().clone());
        let authority_pool =
            ProjectionPool::for_holding(read.database_pool().clone(), holding_uuid)
                .map_err(|_| HoldingProjectionExecutorError::Permanent)?;
        let authority =
            HoldingAuthorityEvidenceProjection::new(holding_id, authority_pool, authority_write);
        let event_types = vec![
            ITEM_DIRECTORY_OWNER_EVENT_TYPE.to_owned(),
            AUTHORITY_EVIDENCE_EVENT_TYPE.to_owned(),
            ORGANIZATION_LINEAGE_CHANGED_EVENT_TYPE.to_owned(),
            ORGANIZATION_SNAPSHOT_EVENT_TYPE.to_owned(),
        ];
        let (generation, source_high_watermark, mut cursor) = match claim.job.kind {
            ProjectionJobKind::Rebuild => {
                let (generation, source_high_watermark) = rebuild_fence(claim)?;
                let cursor = directory
                    .begin_rebuild(holding_id, generation, source_high_watermark, Utc::now())
                    .await
                    .map_err(|_| HoldingProjectionExecutorError::Retryable)?;
                (generation, source_high_watermark, cursor)
            }
            ProjectionJobKind::Incremental => {
                let source_high_watermark = source
                    .durable_high_watermark(&event_types)
                    .await
                    .map_err(|_| HoldingProjectionExecutorError::Retryable)?;
                let checkpoint = directory
                    .begin_incremental(holding_id, source_high_watermark, Utc::now())
                    .await
                    .map_err(|_| HoldingProjectionExecutorError::Retryable)?;
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
                .page_durable_between(
                    cursor,
                    Some(source_high_watermark),
                    &event_types,
                    self.page_size,
                )
                .await
                .map_err(|_| HoldingProjectionExecutorError::Retryable)?;
            if page.events.is_empty() {
                return Err(HoldingProjectionExecutorError::Retryable);
            }
            events_seen = events_seen.saturating_add(page.events.len() as u64);
            if events_seen > MAX_ITEM_DIRECTORY_EVENTS_PER_PASS {
                return Err(HoldingProjectionExecutorError::Permanent);
            }
            let previous = cursor;
            for event in page.events {
                if event.event_type == AUTHORITY_EVIDENCE_EVENT_TYPE {
                    authority.apply(&event).await.map_err(|error| match error {
                        crate::AuthorityProjectionError::Unavailable => {
                            HoldingProjectionExecutorError::Retryable
                        }
                        _ => HoldingProjectionExecutorError::Permanent,
                    })?;
                } else if event.event_type == ORGANIZATION_LINEAGE_CHANGED_EVENT_TYPE {
                    authority.apply_organization_lineage(&event).await.map_err(
                        |error| match error {
                            crate::AuthorityProjectionError::Unavailable => {
                                HoldingProjectionExecutorError::Retryable
                            }
                            _ => HoldingProjectionExecutorError::Permanent,
                        },
                    )?;
                } else if event.event_type == ORGANIZATION_SNAPSHOT_EVENT_TYPE {
                    authority
                        .apply_organization_snapshot_lineage(&event)
                        .await
                        .map_err(|error| match error {
                            crate::AuthorityProjectionError::Unavailable => {
                                HoldingProjectionExecutorError::Retryable
                            }
                            _ => HoldingProjectionExecutorError::Permanent,
                        })?;
                } else {
                    let fact = decode_item_directory_event(holding_id, &event)
                        .map_err(|_| HoldingProjectionExecutorError::Permanent)?;
                    directory
                        .apply_fact(&fact, Utc::now())
                        .await
                        .map_err(|_| HoldingProjectionExecutorError::Retryable)?;
                }
                cursor = event.global_sequence;
            }
            directory
                .advance_checkpoint(holding_id, generation, previous, cursor, Utc::now())
                .await
                .map_err(|_| HoldingProjectionExecutorError::Retryable)?;
        }
        authority
            .reconcile_grants(&directory, source_high_watermark, Utc::now())
            .await
            .map_err(|error| match error {
                crate::AuthorityProjectionError::Unavailable => {
                    HoldingProjectionExecutorError::Retryable
                }
                _ => HoldingProjectionExecutorError::Permanent,
            })?;
        directory
            .complete_rebuild(holding_id, generation, source_high_watermark, Utc::now())
            .await
            .map_err(|_| HoldingProjectionExecutorError::Retryable)
    }
}

#[async_trait]
impl HoldingProjectionRoutedExecutor for ItemDirectoryHoldingProjectionExecutor {
    fn projection_name(&self) -> &str {
        ITEM_DIRECTORY_PROJECTION_NAME
    }

    async fn execute_routed(
        &self,
        claim: &ProjectionClaimRecord,
        write: &RoutedHoldingDatabase,
        read: &RoutedHoldingDatabase,
    ) -> Result<(), HoldingProjectionExecutorError> {
        self.execute_store(claim, write, read).await
    }
}

fn validate_routed_claim(
    claim: &ProjectionClaimRecord,
    write: &RoutedHoldingDatabase,
    read: &RoutedHoldingDatabase,
) -> Result<(), HoldingProjectionExecutorError> {
    if claim.job.projection_name != ITEM_DIRECTORY_PROJECTION_NAME
        || write.side != RoutedDatabaseSide::Write
        || read.side != RoutedDatabaseSide::Read
        || write.holding_id != claim.job.binding.holding_id
        || read.holding_id != claim.job.binding.holding_id
        || write.data_profile_key != claim.job.binding.data_profile_key
        || read.data_profile_key != claim.job.binding.data_profile_key
    {
        return Err(HoldingProjectionExecutorError::Permanent);
    }
    Ok(())
}

fn rebuild_fence(
    claim: &ProjectionClaimRecord,
) -> Result<(u64, u64), HoldingProjectionExecutorError> {
    let payload = claim
        .job
        .payload
        .as_object()
        .ok_or(HoldingProjectionExecutorError::Permanent)?;
    let generation = payload
        .get("generation")
        .and_then(serde_json::Value::as_u64)
        .filter(|value| *value > 0)
        .ok_or(HoldingProjectionExecutorError::Permanent)?;
    let source = payload
        .get("source_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(HoldingProjectionExecutorError::Permanent)?;
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
        EffectiveGrantChanged, ITEM_DIRECTORY_FACT_EVENT_TYPE, ItemDirectoryFact, ItemTombstoned,
        StableRecordRef, catalog::ContextId, reference::RecordTypeId,
    };
    use erpx_kernel::{
        ActorAttribution, ActorKind, AggregateId, AuthorityScope, CommandId, CorrelationId,
        DataClassification, EventId, PrincipalId, RecordId,
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
            scope: AuthorityScope::Platform,
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
            active_scope: AuthorityScope::Platform,
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
        let holding_id = HoldingId::try_from_uuid(id()).unwrap();
        let publication = decode_item_directory_owner_event(holding_id, &event()).unwrap();
        assert_eq!(publication.fact.source_event_position(), 81);
    }

    #[test]
    fn snapshot_like_json_and_owner_mismatch_are_rejected() {
        let holding_id = HoldingId::try_from_uuid(id()).unwrap();
        let mut value = event();
        value.payload = serde_json::json!({"aggregate": "invoice", "state": {}});
        assert!(matches!(
            decode_item_directory_owner_event(holding_id, &value),
            Err(ItemDirectoryProjectionError::MalformedOwnerFact)
        ));

        let mut value = event();
        value.context = "sales".into();
        assert!(matches!(
            decode_item_directory_owner_event(holding_id, &value),
            Err(ItemDirectoryProjectionError::OwnerEnvelopeMismatch)
        ));
    }

    #[test]
    fn tagged_owner_payload_is_decoded_without_snapshot_inference() {
        let holding_id = HoldingId::try_from_uuid(id()).unwrap();
        let mut value = event();
        value.payload = serde_json::json!({
            "payload_kind": "directory",
            "payload": value.payload,
        });
        assert_eq!(
            decode_item_directory_owner_event(holding_id, &value)
                .unwrap()
                .fact
                .source_event_position(),
            81
        );
    }

    #[test]
    fn forged_final_descriptor_grant_and_tombstone_are_rejected_before_projection_mutation() {
        let holding_id = HoldingId::try_from_uuid(id()).unwrap();
        let owner_event = event();
        let descriptor = decode_item_directory_owner_event(holding_id, &owner_event)
            .unwrap()
            .fact;
        let invoice = StableRecordRef::try_new(
            RecordId::try_from_uuid(owner_event.aggregate_id.as_uuid()).unwrap(),
            ContextId::Finance,
            RecordTypeId::try_new("finance.invoice").unwrap(),
        )
        .unwrap();
        let principal_id = PrincipalId::try_from_uuid(id()).unwrap();
        let definition = erpx_integration_contracts::record_policy("finance.invoice").unwrap();
        let effective_at = Utc::now();
        let grant = ItemDirectoryFact::EffectiveGrantChanged(EffectiveGrantChanged {
            record: invoice.clone(),
            principal_id,
            authorization_policy: definition.policy_id(),
            policy_version: definition.version,
            effective_at,
            expires_at: effective_at + chrono::Duration::minutes(30),
            source_event_position: 80,
        });
        let tombstone = ItemDirectoryFact::Tombstoned(ItemTombstoned {
            record_id: invoice.id(),
            descriptor_version: 5,
            source_event_position: 80,
        });
        for fact in [&descriptor, &grant, &tombstone] {
            fact.validate().unwrap();
        }

        let mut wrong_context_descriptor = owner_event.clone();
        wrong_context_descriptor.event_type = ITEM_DIRECTORY_FACT_EVENT_TYPE.into();
        wrong_context_descriptor.context = "sales".into();
        wrong_context_descriptor.payload = serde_json::to_value(descriptor).unwrap();

        let mut wrong_aggregate_grant = owner_event.clone();
        wrong_aggregate_grant.event_type = ITEM_DIRECTORY_FACT_EVENT_TYPE.into();
        wrong_aggregate_grant.aggregate_id = AggregateId::try_from_uuid(id()).unwrap();
        wrong_aggregate_grant.payload = serde_json::to_value(grant).unwrap();

        let mut wrong_context_and_aggregate_tombstone = owner_event;
        wrong_context_and_aggregate_tombstone.event_type = ITEM_DIRECTORY_FACT_EVENT_TYPE.into();
        wrong_context_and_aggregate_tombstone.context = "sales".into();
        wrong_context_and_aggregate_tombstone.aggregate_id =
            AggregateId::try_from_uuid(id()).unwrap();
        wrong_context_and_aggregate_tombstone.actor = ActorAttribution {
            principal_ref: "principal:forged-publisher".into(),
            kind: ActorKind::PlatformUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        };
        wrong_context_and_aggregate_tombstone.payload = serde_json::to_value(tombstone).unwrap();

        let forged_events = [
            wrong_context_descriptor,
            wrong_aggregate_grant,
            wrong_context_and_aggregate_tombstone,
        ];
        for forged in forged_events {
            assert!(matches!(
                decode_item_directory_event(holding_id, &forged),
                Err(ItemDirectoryProjectionError::UnsupportedContract)
            ));
        }
    }

    #[test]
    fn production_composition_is_routed_and_never_reads_a_holding_from_environment() {
        assert!(ItemDirectoryHoldingProjectionExecutor::new(1).is_ok());
        assert!(ItemDirectoryHoldingProjectionExecutor::new(0).is_err());
        assert!(
            ItemDirectoryHoldingProjectionExecutor::new(
                ITEM_DIRECTORY_PROJECTION_MAX_PAGE_SIZE + 1
            )
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
                .contains("handler_registry_with_control_plane_multi_holding_projection_scheduler")
        );
        assert!(!branch.contains("ERPX_HOLDING_ID"));
        assert!(!branch.contains("ERPX_DATA_PROFILE_KEY"));

        let projector = include_str!("item_directory_projection.rs");
        assert!(projector.contains("AUTHORITY_EVIDENCE_EVENT_TYPE.to_owned()"));
        assert!(projector.contains("ORGANIZATION_LINEAGE_CHANGED_EVENT_TYPE.to_owned()"));
        assert!(projector.contains("ORGANIZATION_SNAPSHOT_EVENT_TYPE.to_owned()"));
        assert!(projector.contains("apply_organization_lineage(&event)"));
        assert!(projector.contains("apply_organization_snapshot_lineage(&event)"));
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
            assert!(
                !source.contains("ITEM_DIRECTORY_FACT_EVENT_TYPE")
                    && !source.contains("erpx.item-directory.fact"),
                "live owners may emit only owner-bound directory drafts"
            );
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
            assert!(!source.contains("ITEM_DIRECTORY_FACT_EVENT_TYPE"));
            assert!(!source.contains("erpx.item-directory.fact"));
        }
    }
}
