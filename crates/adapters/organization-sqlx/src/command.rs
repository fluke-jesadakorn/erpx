use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, IntegrationEvent,
    ItemDescriptorDraft, ItemDescriptorDraftV2, RecordTypeId, StableRecordRef, catalog::ContextId,
    organization, record_policy_id,
};
use erpx_kernel::{AggregateId, AuthorityScope, CommandId, DataClassification, EventId, RecordId};
use erpx_organization::{
    CommandFingerprint, CommandIdentity, CommandUnitOfWork, HierarchyGuard, HierarchyPort,
    OrganizationCommit, OrganizationNode, OrganizationNodeId, OrganizationNodeKind,
    OrganizationNodeState, OrganizationNodeStatus, OrganizationPlacement, PortError, StoredCommit,
    WriteRepository,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    AGGREGATE_TYPE, CONTEXT_NAME, NODE_SNAPSHOT_EVENT_VERSION, SNAPSHOT_EVENT_TYPE,
    canonical_node_scope, node_partition,
    projection::{OrganizationOutboxPayload, OrganizationSnapshot, kind_name, status_name},
};

const REPARENT_BLOCKED_BY_CHILDREN_GUARD: &str =
    "organization node has children and cannot be reparented";
const NORMALIZED_VERSION_CONFLICT_GUARD: &str =
    "organization normalized version did not advance exactly once";
const ACTIVE_ANCESTOR_QUERY: &str = "WITH requested(node_id, ordinal) AS (
         SELECT * FROM UNNEST($1::uuid[]) WITH ORDINALITY
     )
     SELECT authority.node_id,authority.node_kind,authority.node_status,
            authority.parent_node_id,authority.lineage_node_ids,authority.holding_id,
            authority.company_id,authority.branch_id,authority.department_id
     FROM requested
     JOIN erpx_organization_write.organization_node_authority authority
       ON authority.node_id=requested.node_id
      AND authority.holding_id=erpx_cqrs_write.current_holding_id()
     ORDER BY requested.ordinal
     FOR SHARE OF authority";

#[derive(Debug, Clone)]
pub struct OrganizationWriteRepository {
    pool: WritePool,
}

impl OrganizationWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self { pool }
    }
}

#[derive(Debug)]
struct AuthoritySqlScope {
    mode: &'static str,
    holding_id: Option<Uuid>,
    company_id: Option<Uuid>,
    branch_id: Option<Uuid>,
    subtree_node_id: Option<Uuid>,
}

impl AuthoritySqlScope {
    fn try_from_scope(scope: &AuthorityScope) -> Result<Self, PortError> {
        scope.validate().map_err(|_| PortError::Denied)?;
        Ok(match scope {
            AuthorityScope::Platform => Self {
                mode: "platform",
                holding_id: None,
                company_id: None,
                branch_id: None,
                subtree_node_id: None,
            },
            AuthorityScope::Holding { holding_id } => Self {
                mode: "holding",
                holding_id: Some(*holding_id),
                company_id: None,
                branch_id: None,
                subtree_node_id: None,
            },
            AuthorityScope::Company {
                holding_id,
                company_id,
            } => Self {
                mode: "company",
                holding_id: Some(*holding_id),
                company_id: Some(*company_id),
                branch_id: None,
                subtree_node_id: None,
            },
            AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Self {
                mode: "branch",
                holding_id: Some(*holding_id),
                company_id: Some(*company_id),
                branch_id: Some(*branch_id),
                subtree_node_id: None,
            },
            AuthorityScope::Subtree {
                holding_id,
                organization_node_id,
            } => Self {
                mode: "subtree",
                holding_id: Some(*holding_id),
                company_id: None,
                branch_id: None,
                subtree_node_id: Some(*organization_node_id),
            },
        })
    }
}

#[async_trait]
impl WriteRepository for OrganizationWriteRepository {
    async fn load(
        &self,
        authority_scope: &AuthorityScope,
        node_id: OrganizationNodeId,
    ) -> Result<Option<OrganizationNode>, PortError> {
        let authority = AuthoritySqlScope::try_from_scope(authority_scope)?;
        let mut transaction = self.pool.begin().await.map_err(map_store_error)?;
        let row = sqlx::query(
            "SELECT aggregate_version,node_kind,holding_id,company_id,branch_id,
                    department_id,node_code,node_name,node_status,created_at,updated_at
             FROM erpx_organization_write.organization_node_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND node_id=$1
               AND (
                 $2='platform'
                 OR ($2='holding' AND holding_id=$3)
                 OR ($2='company' AND holding_id=$3 AND company_id=$4)
                 OR ($2='branch' AND holding_id=$3 AND company_id=$4 AND branch_id=$5)
                 OR ($2='subtree' AND holding_id=$3
                     AND ($6=node_id OR $6=ANY(lineage_node_ids)))
               )",
        )
        .bind(node_id.as_uuid())
        .bind(authority.mode)
        .bind(authority.holding_id)
        .bind(authority.company_id)
        .bind(authority.branch_id)
        .bind(authority.subtree_node_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| PortError::Unavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let kind = parse_kind(
            &row.try_get::<String, _>("node_kind")
                .map_err(|_| PortError::Internal)?,
        )?;
        let holding_id = row.try_get("holding_id").map_err(|_| PortError::Internal)?;
        let company_id = row.try_get("company_id").map_err(|_| PortError::Internal)?;
        let branch_id = row.try_get("branch_id").map_err(|_| PortError::Internal)?;
        let department_id = row
            .try_get("department_id")
            .map_err(|_| PortError::Internal)?;
        let state = OrganizationNodeState {
            id: node_id,
            version: from_i64(
                row.try_get("aggregate_version")
                    .map_err(|_| PortError::Internal)?,
            )?,
            code: row.try_get("node_code").map_err(|_| PortError::Internal)?,
            name: row.try_get("node_name").map_err(|_| PortError::Internal)?,
            placement: placement_from_columns(
                node_id,
                kind,
                holding_id,
                company_id,
                branch_id,
                department_id,
            )?,
            status: parse_status(
                &row.try_get::<String, _>("node_status")
                    .map_err(|_| PortError::Internal)?,
            )?,
            created_at: row.try_get("created_at").map_err(|_| PortError::Internal)?,
            updated_at: row.try_get("updated_at").map_err(|_| PortError::Internal)?,
        };
        let aggregate = OrganizationNode::try_rehydrate(state).map_err(|_| PortError::Internal)?;
        if !aggregate
            .placement()
            .is_within_scope(aggregate.id(), authority_scope)
            .map_err(|_| PortError::Internal)?
        {
            return Err(PortError::Denied);
        }
        Ok(Some(aggregate))
    }
}

/// Local placement preflight. Database-backed hierarchy authority lives in
/// `OrganizationAtomicGuard`, inside the command transaction.
#[derive(Debug, Clone, Copy, Default)]
pub struct OrganizationHierarchyRepository;

#[async_trait]
impl HierarchyPort for OrganizationHierarchyRepository {
    async fn validate_placement(
        &self,
        authority_scope: &AuthorityScope,
        node_id: OrganizationNodeId,
        placement: &OrganizationPlacement,
    ) -> Result<(), PortError> {
        if !placement
            .is_within_scope(node_id, authority_scope)
            .map_err(|_| PortError::InvalidParent)?
        {
            return Err(PortError::Denied);
        }
        Ok(())
    }

    async fn ensure_can_archive(
        &self,
        _authority_scope: &AuthorityScope,
        _node_id: OrganizationNodeId,
    ) -> Result<(), PortError> {
        // The authoritative child lock/check is deliberately deferred to the
        // atomic guard so this preflight cannot race another command.
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct OrganizationCommandAdapter {
    store: CommandStore,
}

impl OrganizationCommandAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl CommandUnitOfWork for OrganizationCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        let node_id = fingerprint_node_id(&identity.fingerprint);
        let receipt = self
            .store
            .find_receipt::<OrganizationNode>(
                CONTEXT_NAME,
                &node_partition(node_id),
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?;
        receipt.map(receipt_to_stored).transpose()
    }

    async fn commit(&self, request: OrganizationCommit) -> Result<StoredCommit, PortError> {
        validate_commit(&request)?;
        let node_id = request.aggregate.id();
        let partition = node_partition(node_id);
        let occurred_at = request.aggregate.updated_at();
        let command_id = request.identity.command_id;
        let request_hash = request_hash(&request.identity.fingerprint)?;
        let audit_action = audit_action(&request.identity.fingerprint);
        let idempotency_key = request.identity.idempotency_key.clone();
        let integration_events =
            integration_events_with_lineage(&request.integration_events, &request.aggregate)?;
        let mut events = integration_events
            .iter()
            .enumerate()
            .map(|(index, event)| OutboxEventWrite {
                event_id: deterministic_event_id(command_id, event.event_type(), index),
                event_type: event.event_type().into(),
                event_version: u32::from(event.schema_version()),
                partition: partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: OrganizationOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, SNAPSHOT_EVENT_TYPE, events.len()),
            event_type: SNAPSHOT_EVENT_TYPE.into(),
            event_version: NODE_SNAPSHOT_EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: OrganizationOutboxPayload::Snapshot(OrganizationSnapshot {
                node: (&request.aggregate).into(),
            }),
        });
        if let Some(directory_draft) = organization_directory_draft(&request.aggregate)? {
            events.push(OutboxEventWrite {
                event_id: deterministic_event_id(
                    command_id,
                    ITEM_DIRECTORY_OWNER_EVENT_TYPE,
                    events.len(),
                ),
                event_type: ITEM_DIRECTORY_OWNER_EVENT_TYPE.into(),
                event_version: ITEM_DIRECTORY_FACT_VERSION,
                partition: partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: OrganizationOutboxPayload::Directory(directory_draft),
            });
        }

        let atomic_guard = OrganizationAtomicGuard {
            aggregate: request.aggregate.clone(),
            requirement: request.hierarchy_guard,
        };
        let aggregate_id = aggregate_id(node_id)?;
        let active_scope = request.aggregate.scope().clone();
        let aggregate = request.aggregate;
        let command = AtomicCommand {
            context: CONTEXT_NAME.into(),
            partition,
            command_id,
            correlation_id: request.correlation_id,
            idempotency_key,
            request_hash,
            active_scope,
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: AGGREGATE_TYPE.into(),
            aggregate_id,
            expected_version: request.expected_version,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"organization-audit-v1"),
                action: audit_action.into(),
                occurred_at,
                details: json!({
                    "node_id": node_id.as_uuid(),
                    "node_kind": kind_name(atomic_guard.aggregate.kind()),
                    "expected_version": request.expected_version,
                }),
            },
            events,
        };
        let outcome = self
            .store
            .commit_normalized(command, &atomic_guard)
            .await
            .map_err(map_store_error)?;
        let replayed = matches!(outcome, AtomicCommandOutcome::Replayed(_));
        let receipt = match outcome {
            AtomicCommandOutcome::Committed(receipt) | AtomicCommandOutcome::Replayed(receipt) => {
                receipt
            }
        };
        let mut stored = receipt_to_stored(receipt)?;
        stored.replayed = replayed;
        Ok(stored)
    }
}

fn organization_directory_draft(
    aggregate: &OrganizationNode,
) -> Result<Option<ItemDescriptorDraftV2>, PortError> {
    let record_type = match aggregate.kind() {
        erpx_organization::OrganizationNodeKind::Holding => "organization.holding",
        erpx_organization::OrganizationNodeKind::Company => "organization.company",
        erpx_organization::OrganizationNodeKind::Branch => "organization.branch",
        // Headquarters, departments, and teams are hierarchy internals in the
        // current catalog, anchored through their owning company or branch.
        erpx_organization::OrganizationNodeKind::Headquarters
        | erpx_organization::OrganizationNodeKind::Department
        | erpx_organization::OrganizationNodeKind::Team => return Ok(None),
    };
    let record_id =
        RecordId::try_from_uuid(aggregate.id().as_uuid()).map_err(|_| PortError::Internal)?;
    let record = StableRecordRef::try_new(
        record_id,
        ContextId::Organization,
        RecordTypeId::try_new(record_type).map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    let value = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
        record,
        scope: aggregate.scope().clone(),
        lifecycle_state: status_name(aggregate.status()).into(),
        classification: DataClassification::Confidential,
        descriptor_version: aggregate.version(),
        authorized_search_text: format!("{} {}", aggregate.code(), aggregate.name()),
    };
    ItemDescriptorDraftV2::try_new(value, format!("{} {}", aggregate.code(), aggregate.name()))
        .map(Some)
        .map_err(|_| PortError::Internal)
}

/// The SQL unit of work owns this publication so every committed aggregate
/// state emits exactly one current lineage witness, including lifecycle-only
/// changes and archive commands whose legacy integration fact is `Retired`.
fn integration_events_with_lineage(
    source: &[organization::Event],
    aggregate: &OrganizationNode,
) -> Result<Vec<organization::Event>, PortError> {
    if source
        .iter()
        .any(|event| matches!(event, organization::Event::OrganizationLineageChanged(_)))
    {
        return Err(PortError::Internal);
    }
    let mut events = Vec::with_capacity(source.len() + 1);
    events.extend_from_slice(source);
    events.push(organization_lineage_event(aggregate)?);
    Ok(events)
}

fn organization_lineage_event(
    aggregate: &OrganizationNode,
) -> Result<organization::Event, PortError> {
    let node_id = aggregate.id();
    let placement = aggregate.placement();
    let key = organization::OrganizationLineageKey::try_new(
        placement.holding_id(node_id),
        node_id.as_uuid(),
    )
    .map_err(|_| PortError::Internal)?;
    let witness = organization::OrganizationLineageWitness::try_new(
        key,
        integration_node_kind(aggregate.kind()),
        integration_node_status(aggregate.status()),
        placement.parent_id(node_id).map(|parent| parent.as_uuid()),
        lineage_ids(node_id, placement),
        aggregate.version(),
    )
    .map_err(|_| PortError::Internal)?;
    let event = organization::Event::OrganizationLineageChanged(witness);
    event.validate().map_err(|_| PortError::Internal)?;
    Ok(event)
}

const fn integration_node_kind(kind: OrganizationNodeKind) -> organization::OrganizationNodeKind {
    match kind {
        OrganizationNodeKind::Holding => organization::OrganizationNodeKind::Holding,
        OrganizationNodeKind::Company => organization::OrganizationNodeKind::Company,
        OrganizationNodeKind::Headquarters => organization::OrganizationNodeKind::Headquarters,
        OrganizationNodeKind::Branch => organization::OrganizationNodeKind::Branch,
        OrganizationNodeKind::Department => organization::OrganizationNodeKind::Department,
        OrganizationNodeKind::Team => organization::OrganizationNodeKind::Team,
    }
}

const fn integration_node_status(
    status: OrganizationNodeStatus,
) -> organization::OrganizationNodeStatus {
    match status {
        OrganizationNodeStatus::Active => organization::OrganizationNodeStatus::Active,
        OrganizationNodeStatus::Suspended => organization::OrganizationNodeStatus::Suspended,
        OrganizationNodeStatus::Archived => organization::OrganizationNodeStatus::Archived,
    }
}

struct OrganizationAtomicGuard {
    aggregate: OrganizationNode,
    requirement: Option<HierarchyGuard>,
}

#[derive(Debug)]
struct GuardRow {
    kind: String,
    status: String,
    lineage: Vec<Uuid>,
    holding_id: Uuid,
    company_id: Option<Uuid>,
    branch_id: Option<Uuid>,
    department_id: Option<Uuid>,
}

#[derive(Debug)]
struct LockedAncestor {
    node_id: Uuid,
    parent_node_id: Option<Uuid>,
    authority: GuardRow,
}

#[async_trait]
impl AtomicCommandGuard for OrganizationAtomicGuard {
    async fn authoritative_version(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT aggregate_version
             FROM erpx_organization_write.organization_node_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND node_id=$1
             FOR UPDATE",
        )
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        current.map(from_i64_store).transpose()
    }

    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT aggregate_version
             FROM erpx_organization_write.organization_node_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND node_id=$1
             FOR UPDATE",
        )
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        match (context.expected_version, current) {
            (None, Some(_)) => {
                return Err(CqrsSqlxError::GuardRejected(
                    "organization node already exists",
                ));
            }
            (Some(_), None) => {
                return Err(CqrsSqlxError::GuardRejected(
                    "organization hierarchy guard is missing",
                ));
            }
            _ => {}
        }

        match &self.requirement {
            Some(HierarchyGuard::Placement(placement)) => {
                if placement != self.aggregate.placement() {
                    return Err(CqrsSqlxError::GuardRejected(
                        "organization placement guard does not match aggregate",
                    ));
                }
            }
            Some(HierarchyGuard::ReparentPlacement(placement)) => {
                if placement != self.aggregate.placement() {
                    return Err(CqrsSqlxError::GuardRejected(
                        "organization reparent guard does not match aggregate",
                    ));
                }
                let child: Option<Uuid> = sqlx::query_scalar(
                    "SELECT node_id
                     FROM erpx_organization_write.organization_node_authority
                     WHERE holding_id=erpx_cqrs_write.current_holding_id()
                       AND parent_node_id = $1
                     ORDER BY node_id LIMIT 1 FOR UPDATE",
                )
                .bind(self.aggregate.id().as_uuid())
                .fetch_optional(&mut **transaction)
                .await?;
                if child.is_some() {
                    return Err(CqrsSqlxError::GuardRejected(
                        REPARENT_BLOCKED_BY_CHILDREN_GUARD,
                    ));
                }
            }
            Some(HierarchyGuard::NoActiveChildren) => {
                let child: Option<Uuid> = sqlx::query_scalar(
                    "SELECT node_id
                     FROM erpx_organization_write.organization_node_authority
                     WHERE holding_id=erpx_cqrs_write.current_holding_id()
                       AND parent_node_id = $1 AND node_status = 'active'
                     ORDER BY node_id LIMIT 1 FOR UPDATE",
                )
                .bind(self.aggregate.id().as_uuid())
                .fetch_optional(&mut **transaction)
                .await?;
                if child.is_some() {
                    return Err(CqrsSqlxError::GuardRejected(
                        "organization node has active children",
                    ));
                }
            }
            None => {}
        }

        // Any command that would leave this node active must pin its complete
        // root-to-parent lineage in this same transaction. This applies to
        // ordinary edits as well as create/reparent/activate, so a concurrent
        // ancestor suspend, archive, or reparent cannot race the commit.
        if self.aggregate.status() == OrganizationNodeStatus::Active {
            validate_active_ancestor_chain(
                transaction,
                self.aggregate.id(),
                self.aggregate.placement(),
            )
            .await?;
        }

        let parent_id = self.aggregate.placement().parent_id(self.aggregate.id());
        let duplicate: Option<Uuid> = sqlx::query_scalar(
            "SELECT node_id
             FROM erpx_organization_write.organization_node_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id()
               AND COALESCE(parent_node_id, holding_id) =
                   COALESCE($1, erpx_cqrs_write.current_holding_id())
               AND node_code = $2 AND node_status <> 'archived' AND node_id <> $3
             ORDER BY node_id LIMIT 1 FOR UPDATE",
        )
        .bind(parent_id.map(|id| id.as_uuid()))
        .bind(self.aggregate.code())
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        if duplicate.is_some() {
            return Err(CqrsSqlxError::GuardRejected(
                "organization sibling code already exists",
            ));
        }
        Ok(())
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        if context.context != CONTEXT_NAME
            || context.aggregate_type != AGGREGATE_TYPE
            || context.aggregate_id.as_uuid() != self.aggregate.id().as_uuid()
            || context.partition != node_partition(self.aggregate.id())
            || context.aggregate_version != self.aggregate.version()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "organization applied guard context mismatch",
            ));
        }
        let placement = self.aggregate.placement();
        let node_id = self.aggregate.id();
        let result = sqlx::query(
            "INSERT INTO erpx_organization_write.organization_node_authority
                (holding_id,node_id,node_kind,parent_node_id,lineage_node_ids,
                 company_id,branch_id,department_id,node_code,node_name,node_status,
                 aggregate_version,created_at,updated_at)
             VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,
                     $8,$9,$10,$11,$12,$13)
             ON CONFLICT (holding_id,node_id) DO UPDATE SET
                node_kind = EXCLUDED.node_kind,
                parent_node_id = EXCLUDED.parent_node_id,
                lineage_node_ids = EXCLUDED.lineage_node_ids,
                company_id = EXCLUDED.company_id,
                branch_id = EXCLUDED.branch_id,
                department_id = EXCLUDED.department_id,
                node_code = EXCLUDED.node_code,
                node_name = EXCLUDED.node_name,
                node_status = EXCLUDED.node_status,
                aggregate_version = EXCLUDED.aggregate_version,
                created_at = EXCLUDED.created_at,
                updated_at = EXCLUDED.updated_at
             WHERE erpx_organization_write.organization_node_authority.holding_id=
                       erpx_cqrs_write.current_holding_id()
               AND erpx_organization_write.organization_node_authority.aggregate_version + 1=
                       EXCLUDED.aggregate_version",
        )
        .bind(node_id.as_uuid())
        .bind(kind_name(self.aggregate.kind()))
        .bind(placement.parent_id(node_id).map(|id| id.as_uuid()))
        .bind(lineage_ids(node_id, placement))
        .bind(placement.company_id(node_id))
        .bind(placement.branch_id(node_id))
        .bind(placement.department_id(node_id))
        .bind(self.aggregate.code())
        .bind(self.aggregate.name())
        .bind(status_name(self.aggregate.status()))
        .bind(to_i64(context.aggregate_version)?)
        .bind(self.aggregate.created_at())
        .bind(self.aggregate.updated_at())
        .execute(&mut **transaction)
        .await?;
        if result.rows_affected() != 1 {
            return Err(CqrsSqlxError::GuardRejected(
                NORMALIZED_VERSION_CONFLICT_GUARD,
            ));
        }
        Ok(())
    }
}

async fn validate_active_ancestor_chain(
    transaction: &mut Transaction<'_, Postgres>,
    node_id: OrganizationNodeId,
    placement: &OrganizationPlacement,
) -> Result<(), CqrsSqlxError> {
    placement
        .validate_for(node_id)
        .map_err(|_| CqrsSqlxError::GuardRejected("organization placement is invalid"))?;
    let lineage = lineage_ids(node_id, placement);
    if lineage.is_empty() {
        return Ok(());
    }
    let rows = sqlx::query(ACTIVE_ANCESTOR_QUERY)
        .bind(&lineage)
        .fetch_all(&mut **transaction)
        .await?;
    let locked = rows
        .into_iter()
        .map(|row| {
            Ok(LockedAncestor {
                node_id: row.try_get("node_id")?,
                parent_node_id: row.try_get("parent_node_id")?,
                authority: GuardRow {
                    kind: row.try_get("node_kind")?,
                    status: row.try_get("node_status")?,
                    lineage: row.try_get("lineage_node_ids")?,
                    holding_id: row.try_get("holding_id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    department_id: row.try_get("department_id")?,
                },
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    if !active_ancestor_chain_matches(node_id, placement, &locked) {
        return Err(CqrsSqlxError::GuardRejected(
            "organization ancestor placement is invalid",
        ));
    }
    Ok(())
}

fn active_ancestor_chain_matches(
    node_id: OrganizationNodeId,
    placement: &OrganizationPlacement,
    rows: &[LockedAncestor],
) -> bool {
    let lineage = lineage_ids(node_id, placement);
    let kinds = expected_active_ancestor_kinds(placement);
    if rows.len() != lineage.len() || kinds.len() != lineage.len() {
        return false;
    }
    for (index, ((row, expected_node_id), expected_kind)) in
        rows.iter().zip(&lineage).zip(kinds).enumerate()
    {
        let expected_parent = index.checked_sub(1).map(|parent| lineage[parent]);
        if row.node_id != *expected_node_id
            || row.parent_node_id != expected_parent
            || row.authority.kind != *expected_kind
            || row.authority.status != "active"
            || row.authority.lineage != lineage[..index]
            || !normalized_ancestor_matches(&row.authority, row.node_id, expected_kind)
        {
            return false;
        }
    }
    rows.last()
        .is_some_and(|parent| parent_matches(placement, &parent.authority))
}

fn expected_active_ancestor_kinds(placement: &OrganizationPlacement) -> &'static [&'static str] {
    match placement {
        OrganizationPlacement::Holding => &[],
        OrganizationPlacement::Company { .. } => &["holding"],
        OrganizationPlacement::Headquarters { .. } | OrganizationPlacement::Branch { .. } => {
            &["holding", "company"]
        }
        OrganizationPlacement::Department {
            branch_id: None, ..
        } => &["holding", "company"],
        OrganizationPlacement::Department {
            branch_id: Some(_), ..
        } => &["holding", "company", "branch"],
        OrganizationPlacement::Team {
            branch_id: None, ..
        } => &["holding", "company", "department"],
        OrganizationPlacement::Team {
            branch_id: Some(_), ..
        } => &["holding", "company", "branch", "department"],
    }
}

fn normalized_ancestor_matches(parent: &GuardRow, node_id: Uuid, kind: &str) -> bool {
    let lineage = &parent.lineage;
    match kind {
        "holding" => {
            parent.holding_id == node_id
                && parent.company_id.is_none()
                && parent.branch_id.is_none()
                && parent.department_id.is_none()
        }
        "company" => {
            lineage
                .first()
                .is_some_and(|holding| *holding == parent.holding_id)
                && parent.company_id == Some(node_id)
                && parent.branch_id.is_none()
                && parent.department_id.is_none()
        }
        "branch" => {
            lineage
                .first()
                .is_some_and(|holding| *holding == parent.holding_id)
                && lineage.get(1).copied() == parent.company_id
                && parent.branch_id == Some(node_id)
                && parent.department_id.is_none()
        }
        "department" => {
            lineage
                .first()
                .is_some_and(|holding| *holding == parent.holding_id)
                && lineage.get(1).copied() == parent.company_id
                && match lineage.len() {
                    2 => parent.branch_id.is_none(),
                    3 => lineage.get(2).copied() == parent.branch_id,
                    _ => false,
                }
                && parent.department_id == Some(node_id)
        }
        _ => false,
    }
}

fn parent_matches(placement: &OrganizationPlacement, parent: &GuardRow) -> bool {
    match placement {
        OrganizationPlacement::Holding => false,
        OrganizationPlacement::Company { holding_id } => {
            parent.kind == "holding" && parent.holding_id == *holding_id
        }
        OrganizationPlacement::Headquarters {
            holding_id,
            company_id,
        }
        | OrganizationPlacement::Branch {
            holding_id,
            company_id,
        } => {
            parent.kind == "company"
                && parent.holding_id == *holding_id
                && parent.company_id == Some(*company_id)
        }
        OrganizationPlacement::Department {
            holding_id,
            company_id,
            branch_id,
        } => {
            parent.holding_id == *holding_id
                && parent.company_id == Some(*company_id)
                && branch_id.map_or_else(
                    || parent.kind == "company",
                    |branch_id| parent.kind == "branch" && parent.branch_id == Some(branch_id),
                )
        }
        OrganizationPlacement::Team {
            holding_id,
            company_id,
            branch_id,
            department_id,
        } => {
            parent.kind == "department"
                && parent.holding_id == *holding_id
                && parent.company_id == Some(*company_id)
                && parent.branch_id == *branch_id
                && parent.department_id == Some(*department_id)
        }
    }
}

fn lineage_ids(node_id: OrganizationNodeId, placement: &OrganizationPlacement) -> Vec<Uuid> {
    match placement {
        OrganizationPlacement::Holding => Vec::new(),
        OrganizationPlacement::Company { holding_id } => vec![*holding_id],
        OrganizationPlacement::Headquarters {
            holding_id,
            company_id,
        }
        | OrganizationPlacement::Branch {
            holding_id,
            company_id,
        } => vec![*holding_id, *company_id],
        OrganizationPlacement::Department {
            holding_id,
            company_id,
            branch_id,
        } => {
            let mut values = vec![*holding_id, *company_id];
            if let Some(branch_id) = branch_id {
                values.push(*branch_id);
            }
            values
        }
        OrganizationPlacement::Team {
            holding_id,
            company_id,
            branch_id,
            department_id,
        } => {
            let mut values = vec![*holding_id, *company_id];
            if let Some(branch_id) = branch_id {
                values.push(*branch_id);
            }
            values.push(*department_id);
            values
        }
    }
    .into_iter()
    .filter(|id| *id != node_id.as_uuid())
    .collect()
}

fn validate_guard_context(
    aggregate: &OrganizationNode,
    context: &GuardCommandContext<'_>,
) -> Result<(), CqrsSqlxError> {
    if context.context != CONTEXT_NAME
        || context.aggregate_type != AGGREGATE_TYPE
        || context.aggregate_id.as_uuid() != aggregate.id().as_uuid()
        || context.partition != node_partition(aggregate.id())
    {
        return Err(CqrsSqlxError::GuardRejected(
            "organization guard context mismatch",
        ));
    }
    Ok(())
}

fn validate_commit(request: &OrganizationCommit) -> Result<(), PortError> {
    let next = request
        .expected_version
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(PortError::Internal)?;
    if request.aggregate.validate().is_err()
        || fingerprint_node_id(&request.identity.fingerprint) != request.aggregate.id()
        || !request
            .aggregate
            .placement()
            .is_within_scope(request.aggregate.id(), &request.authority_scope)
            .map_err(|_| PortError::Internal)?
        || !hierarchy_guard_matches_command(
            &request.identity.fingerprint,
            request.hierarchy_guard.as_ref(),
            request.aggregate.placement(),
        )
        || request.aggregate.version() != next
    {
        return Err(PortError::Internal);
    }
    Ok(())
}

fn hierarchy_guard_matches_command(
    fingerprint: &CommandFingerprint,
    guard: Option<&HierarchyGuard>,
    current_placement: &OrganizationPlacement,
) -> bool {
    match (fingerprint, guard) {
        (
            CommandFingerprint::Create { placement, .. },
            Some(HierarchyGuard::Placement(guarded)),
        ) => placement == current_placement && guarded == current_placement,
        (
            CommandFingerprint::Reparent { placement, .. },
            Some(HierarchyGuard::ReparentPlacement(guarded)),
        ) => placement == current_placement && guarded == current_placement,
        (CommandFingerprint::Activate { .. }, Some(HierarchyGuard::Placement(guarded))) => {
            guarded == current_placement
        }
        (CommandFingerprint::Archive { .. }, Some(HierarchyGuard::NoActiveChildren)) => true,
        (
            CommandFingerprint::Rename { .. }
            | CommandFingerprint::ChangeCode { .. }
            | CommandFingerprint::Suspend { .. },
            None,
        ) => true,
        _ => false,
    }
}

fn aggregate_id(node_id: OrganizationNodeId) -> Result<AggregateId, PortError> {
    AggregateId::try_from_uuid(node_id.as_uuid()).map_err(|_| PortError::Internal)
}

fn fingerprint_node_id(fingerprint: &CommandFingerprint) -> OrganizationNodeId {
    match fingerprint {
        CommandFingerprint::Create { node_id, .. }
        | CommandFingerprint::Rename { node_id, .. }
        | CommandFingerprint::ChangeCode { node_id, .. }
        | CommandFingerprint::Reparent { node_id, .. }
        | CommandFingerprint::Suspend { node_id }
        | CommandFingerprint::Activate { node_id }
        | CommandFingerprint::Archive { node_id } => *node_id,
    }
}

fn request_hash(fingerprint: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let bytes = serde_json::to_vec(fingerprint).map_err(|_| PortError::Internal)?;
    let mut digest = Sha256::new();
    digest.update(b"erpx.organization.command-fingerprint.v1\0");
    digest.update(bytes);
    Ok(RequestHash::new(digest.finalize().into()))
}

fn deterministic_event_id(command_id: CommandId, event_type: &str, index: usize) -> EventId {
    let mut input = Vec::with_capacity(event_type.len() + 16);
    input.extend_from_slice(event_type.as_bytes());
    input.extend_from_slice(&index.to_be_bytes());
    EventId::try_from_uuid(deterministic_uuid(command_id.as_uuid(), &input))
        .expect("a SHA-256-derived UUID is non-nil")
}

fn deterministic_uuid(seed: Uuid, label: &[u8]) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(seed.as_bytes());
    digest.update(label);
    let digest = digest.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn audit_action(fingerprint: &CommandFingerprint) -> &'static str {
    match fingerprint {
        CommandFingerprint::Create { .. } => "organization.node.create",
        CommandFingerprint::Rename { .. } => "organization.node.rename",
        CommandFingerprint::ChangeCode { .. } => "organization.node.change_code",
        CommandFingerprint::Reparent { .. } => "organization.node.reparent",
        CommandFingerprint::Suspend { .. } => "organization.node.suspend",
        CommandFingerprint::Activate { .. } => "organization.node.activate",
        CommandFingerprint::Archive { .. } => "organization.node.archive",
    }
}

fn receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<OrganizationNode>,
) -> Result<StoredCommit, PortError> {
    // Receipts written before exact Department/Team scopes persist the old
    // company/branch scope witness inside the aggregate JSON. Rehydrate from
    // the stable typed state so replay upgrades that witness deterministically
    // without changing the receipt identity or aggregate version.
    let response = normalize_receipt_node(&receipt.response)?;
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != AGGREGATE_TYPE
        || receipt.aggregate.aggregate_id.as_uuid() != response.id().as_uuid()
        || receipt.aggregate.version != response.version()
    {
        return Err(PortError::Internal);
    }
    Ok(StoredCommit {
        command_id: receipt.command_id,
        correlation_id: receipt.correlation_id,
        aggregate: response,
        committed_events: receipt.committed_events,
        committed_at: receipt.committed_at,
        replayed: receipt.replayed,
    })
}

fn normalize_receipt_node(value: &OrganizationNode) -> Result<OrganizationNode, PortError> {
    canonical_node_scope(value.placement(), value.id(), value.scope())
        .map_err(|_| PortError::Internal)?;
    OrganizationNode::try_rehydrate(OrganizationNodeState {
        id: value.id(),
        version: value.version(),
        code: value.code().into(),
        name: value.name().into(),
        placement: value.placement().clone(),
        status: value.status(),
        created_at: value.created_at(),
        updated_at: value.updated_at(),
    })
    .map_err(|_| PortError::Internal)
}

fn map_store_error(error: CqrsSqlxError) -> PortError {
    match error {
        CqrsSqlxError::IdempotencyConflict | CqrsSqlxError::CommandReplayConflict => {
            PortError::IdempotencyConflict
        }
        CqrsSqlxError::OptimisticConcurrency {
            expected: Some(expected),
            actual,
            ..
        } => PortError::Conflict {
            expected,
            actual: actual.unwrap_or(0),
        },
        CqrsSqlxError::GuardRejected("organization node has active children") => {
            PortError::ActiveChildren
        }
        CqrsSqlxError::GuardRejected(REPARENT_BLOCKED_BY_CHILDREN_GUARD) => {
            PortError::ReparentBlockedByChildren
        }
        CqrsSqlxError::GuardRejected("organization parent lineage is invalid") => {
            PortError::HierarchyCycle
        }
        CqrsSqlxError::GuardRejected(message)
            if message.contains("parent") || message.contains("placement") =>
        {
            PortError::InvalidParent
        }
        CqrsSqlxError::GuardRejected("organization node already exists")
        | CqrsSqlxError::GuardRejected("organization sibling code already exists")
        | CqrsSqlxError::GuardRejected(NORMALIZED_VERSION_CONFLICT_GUARD) => PortError::Conflict {
            expected: 0,
            actual: 0,
        },
        CqrsSqlxError::GuardRejected(_) => PortError::Internal,
        CqrsSqlxError::Database(error)
            if sibling_code_constraint(
                error
                    .as_database_error()
                    .and_then(|error| error.constraint()),
            ) =>
        {
            PortError::Conflict {
                expected: 0,
                actual: 0,
            }
        }
        CqrsSqlxError::Database(_) => PortError::Unavailable,
        _ => PortError::Internal,
    }
}

fn sibling_code_constraint(constraint: Option<&str>) -> bool {
    matches!(
        constraint,
        Some("organization_sibling_code" | "organization_node_authority_sibling_code")
    )
}

fn to_i64(value: u64) -> Result<i64, CqrsSqlxError> {
    i64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}

fn from_i64(value: i64) -> Result<u64, PortError> {
    u64::try_from(value).map_err(|_| PortError::Internal)
}

fn from_i64_store(value: i64) -> Result<u64, CqrsSqlxError> {
    u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}

fn parse_kind(value: &str) -> Result<OrganizationNodeKind, PortError> {
    match value {
        "holding" => Ok(OrganizationNodeKind::Holding),
        "company" => Ok(OrganizationNodeKind::Company),
        "headquarters" => Ok(OrganizationNodeKind::Headquarters),
        "branch" => Ok(OrganizationNodeKind::Branch),
        "department" => Ok(OrganizationNodeKind::Department),
        "team" => Ok(OrganizationNodeKind::Team),
        _ => Err(PortError::Internal),
    }
}

fn parse_status(value: &str) -> Result<OrganizationNodeStatus, PortError> {
    match value {
        "active" => Ok(OrganizationNodeStatus::Active),
        "suspended" => Ok(OrganizationNodeStatus::Suspended),
        "archived" => Ok(OrganizationNodeStatus::Archived),
        _ => Err(PortError::Internal),
    }
}

fn placement_from_columns(
    node_id: OrganizationNodeId,
    kind: OrganizationNodeKind,
    holding_id: Uuid,
    company_id: Option<Uuid>,
    branch_id: Option<Uuid>,
    department_id: Option<Uuid>,
) -> Result<OrganizationPlacement, PortError> {
    match (kind, company_id, branch_id, department_id) {
        (OrganizationNodeKind::Holding, None, None, None) if holding_id == node_id.as_uuid() => {
            Ok(OrganizationPlacement::Holding)
        }
        (OrganizationNodeKind::Company, Some(company_id), None, None)
            if company_id == node_id.as_uuid() =>
        {
            Ok(OrganizationPlacement::Company { holding_id })
        }
        (OrganizationNodeKind::Headquarters, Some(company_id), None, None) => {
            Ok(OrganizationPlacement::Headquarters {
                holding_id,
                company_id,
            })
        }
        (OrganizationNodeKind::Branch, Some(company_id), Some(branch_id), None)
            if branch_id == node_id.as_uuid() =>
        {
            Ok(OrganizationPlacement::Branch {
                holding_id,
                company_id,
            })
        }
        (OrganizationNodeKind::Department, Some(company_id), branch_id, Some(department_id))
            if department_id == node_id.as_uuid() =>
        {
            Ok(OrganizationPlacement::Department {
                holding_id,
                company_id,
                branch_id,
            })
        }
        (OrganizationNodeKind::Team, Some(company_id), branch_id, Some(department_id)) => {
            Ok(OrganizationPlacement::Team {
                holding_id,
                company_id,
                branch_id,
                department_id,
            })
        }
        _ => Err(PortError::Internal),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node_id(value: u128) -> OrganizationNodeId {
        OrganizationNodeId::try_from_uuid(Uuid::from_u128(value)).unwrap()
    }

    #[test]
    fn fingerprint_hash_is_stable_and_payload_sensitive() {
        let first = CommandFingerprint::Rename {
            node_id: node_id(1),
            name: "First".into(),
        };
        let changed = CommandFingerprint::Rename {
            node_id: node_id(1),
            name: "Second".into(),
        };
        assert_eq!(request_hash(&first).unwrap(), request_hash(&first).unwrap());
        assert_ne!(
            request_hash(&first).unwrap(),
            request_hash(&changed).unwrap()
        );
    }

    #[test]
    fn reparent_children_guard_maps_to_the_typed_conflict() {
        assert_eq!(
            map_store_error(CqrsSqlxError::GuardRejected(
                REPARENT_BLOCKED_BY_CHILDREN_GUARD,
            )),
            PortError::ReparentBlockedByChildren
        );
    }

    #[test]
    fn inactive_parent_guard_maps_to_invalid_parent() {
        assert_eq!(
            map_store_error(CqrsSqlxError::GuardRejected(
                "organization parent is inactive",
            )),
            PortError::InvalidParent
        );
    }

    #[test]
    fn normalized_create_races_and_both_sibling_constraints_map_to_conflict() {
        assert_eq!(
            map_store_error(CqrsSqlxError::GuardRejected(
                NORMALIZED_VERSION_CONFLICT_GUARD,
            )),
            PortError::Conflict {
                expected: 0,
                actual: 0,
            }
        );
        assert!(sibling_code_constraint(Some("organization_sibling_code")));
        assert!(sibling_code_constraint(Some(
            "organization_node_authority_sibling_code"
        )));
        assert!(!sibling_code_constraint(Some("unrelated_constraint")));
    }

    #[test]
    fn activation_requires_an_atomic_guard_for_its_current_placement() {
        let root = OrganizationPlacement::Holding;
        let branch = OrganizationPlacement::Branch {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        };
        let activate = CommandFingerprint::Activate {
            node_id: node_id(3),
        };

        assert!(hierarchy_guard_matches_command(
            &activate,
            Some(&HierarchyGuard::Placement(branch.clone())),
            &branch,
        ));
        assert!(hierarchy_guard_matches_command(
            &activate,
            Some(&HierarchyGuard::Placement(root.clone())),
            &root,
        ));
        assert!(!hierarchy_guard_matches_command(&activate, None, &branch,));
        assert!(!hierarchy_guard_matches_command(
            &activate,
            Some(&HierarchyGuard::Placement(root)),
            &branch,
        ));
    }

    #[test]
    fn lineage_is_derived_from_typed_placement() {
        let placement = OrganizationPlacement::Team {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
            branch_id: Some(Uuid::from_u128(3)),
            department_id: Uuid::from_u128(4),
        };
        assert_eq!(
            lineage_ids(node_id(5), &placement),
            vec![
                Uuid::from_u128(1),
                Uuid::from_u128(2),
                Uuid::from_u128(3),
                Uuid::from_u128(4)
            ]
        );
    }

    fn locked_ancestor(
        node: u128,
        parent: Option<u128>,
        kind: &str,
        status: &str,
        lineage: &[u128],
        normalized: (Option<u128>, Option<u128>, Option<u128>),
    ) -> LockedAncestor {
        LockedAncestor {
            node_id: Uuid::from_u128(node),
            parent_node_id: parent.map(Uuid::from_u128),
            authority: GuardRow {
                kind: kind.into(),
                status: status.into(),
                lineage: lineage.iter().copied().map(Uuid::from_u128).collect(),
                holding_id: Uuid::from_u128(1),
                company_id: normalized.0.map(Uuid::from_u128),
                branch_id: normalized.1.map(Uuid::from_u128),
                department_id: normalized.2.map(Uuid::from_u128),
            },
        }
    }

    #[test]
    fn active_team_commit_rejects_a_suspended_branch_or_department_ancestor() {
        let placement = OrganizationPlacement::Team {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
            branch_id: Some(Uuid::from_u128(3)),
            department_id: Uuid::from_u128(4),
        };
        let mut ancestors = vec![
            locked_ancestor(1, None, "holding", "active", &[], (None, None, None)),
            locked_ancestor(2, Some(1), "company", "active", &[1], (Some(2), None, None)),
            locked_ancestor(
                3,
                Some(2),
                "branch",
                "active",
                &[1, 2],
                (Some(2), Some(3), None),
            ),
            locked_ancestor(
                4,
                Some(3),
                "department",
                "active",
                &[1, 2, 3],
                (Some(2), Some(3), Some(4)),
            ),
        ];
        assert!(active_ancestor_chain_matches(
            node_id(5),
            &placement,
            &ancestors,
        ));

        ancestors[2].authority.status = "suspended".into();
        assert!(!active_ancestor_chain_matches(
            node_id(5),
            &placement,
            &ancestors,
        ));
        ancestors[2].authority.status = "active".into();
        ancestors[3].authority.status = "suspended".into();
        assert!(!active_ancestor_chain_matches(
            node_id(5),
            &placement,
            &ancestors,
        ));
    }

    #[test]
    fn every_resulting_active_command_uses_one_ordered_share_locked_ancestor_query() {
        assert!(ACTIVE_ANCESTOR_QUERY.contains("UNNEST($1::uuid[]) WITH ORDINALITY"));
        assert!(ACTIVE_ANCESTOR_QUERY.contains("ORDER BY requested.ordinal"));
        assert!(ACTIVE_ANCESTOR_QUERY.contains("FOR SHARE OF authority"));

        let source = include_str!("command.rs");
        let requirement_match = source
            .find("match &self.requirement")
            .expect("the optional command-specific guard remains explicit");
        let active_chain = source
            .find("if self.aggregate.status() == OrganizationNodeStatus::Active")
            .expect("all active outcomes validate their lineage");
        let sibling_check = source
            .find("let duplicate: Option<Uuid>")
            .expect("the sibling-code guard remains after lineage validation");
        assert!(requirement_match < active_chain && active_chain < sibling_check);
    }

    #[test]
    fn lineage_event_maps_exact_department_and_team_paths() {
        let at = chrono::Utc::now();
        let department = OrganizationNode::create(
            node_id(4),
            OrganizationPlacement::Department {
                holding_id: Uuid::from_u128(1),
                company_id: Uuid::from_u128(2),
                branch_id: Some(Uuid::from_u128(3)),
            },
            "OPS".into(),
            "Operations".into(),
            at,
        )
        .unwrap();
        let team = OrganizationNode::create(
            node_id(5),
            OrganizationPlacement::Team {
                holding_id: Uuid::from_u128(1),
                company_id: Uuid::from_u128(2),
                branch_id: Some(Uuid::from_u128(3)),
                department_id: Uuid::from_u128(4),
            },
            "AP".into(),
            "Accounts Payable".into(),
            at,
        )
        .unwrap();

        let organization::Event::OrganizationLineageChanged(department_witness) =
            organization_lineage_event(&department).unwrap()
        else {
            panic!("department must emit a lineage witness")
        };
        assert_eq!(
            department_witness.kind(),
            organization::OrganizationNodeKind::Department
        );
        assert_eq!(
            department_witness.key().node_id().as_uuid(),
            department.id().as_uuid()
        );
        assert_eq!(
            department_witness.key().holding_id().as_uuid(),
            Uuid::from_u128(1)
        );
        assert_eq!(
            department_witness
                .lineage_node_ids()
                .iter()
                .map(|node| node.as_uuid())
                .collect::<Vec<_>>(),
            vec![Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3),]
        );
        assert_eq!(department_witness.aggregate_version(), department.version());

        let organization::Event::OrganizationLineageChanged(team_witness) =
            organization_lineage_event(&team).unwrap()
        else {
            panic!("team must emit a lineage witness")
        };
        assert_eq!(
            team_witness.kind(),
            organization::OrganizationNodeKind::Team
        );
        assert_eq!(
            team_witness
                .lineage_node_ids()
                .iter()
                .map(|node| node.as_uuid())
                .collect::<Vec<_>>(),
            vec![
                Uuid::from_u128(1),
                Uuid::from_u128(2),
                Uuid::from_u128(3),
                Uuid::from_u128(4),
            ]
        );
        assert_eq!(team_witness.aggregate_version(), team.version());
    }

    #[test]
    fn archived_state_emits_current_lineage_after_the_legacy_fact() {
        let at = chrono::Utc::now();
        let mut department = OrganizationNode::create(
            node_id(4),
            OrganizationPlacement::Department {
                holding_id: Uuid::from_u128(1),
                company_id: Uuid::from_u128(2),
                branch_id: None,
            },
            "OPS".into(),
            "Operations".into(),
            at,
        )
        .unwrap();
        department
            .archive(at + chrono::Duration::minutes(1))
            .unwrap();
        let legacy = organization::Event::OrganizationUnitRetired {
            business_unit: organization::BusinessUnitRef::try_from_uuid(department.id().as_uuid())
                .unwrap(),
        };
        let events =
            integration_events_with_lineage(std::slice::from_ref(&legacy), &department).unwrap();

        assert_eq!(events.first(), Some(&legacy));
        let Some(organization::Event::OrganizationLineageChanged(witness)) = events.last() else {
            panic!("current lineage must be the final integration event")
        };
        assert_eq!(events.len(), 2);
        assert_eq!(
            witness.status(),
            organization::OrganizationNodeStatus::Archived
        );
        assert_eq!(witness.key().node_id().as_uuid(), department.id().as_uuid());
        assert_eq!(witness.aggregate_version(), department.version());
        assert_eq!(
            events.last().unwrap().event_type(),
            organization::ORGANIZATION_LINEAGE_CHANGED_EVENT_TYPE
        );
        assert_eq!(
            events.last().unwrap().schema_version(),
            organization::ORGANIZATION_LINEAGE_CHANGED_VERSION
        );
    }

    #[test]
    fn adapter_rejects_a_caller_supplied_lineage_witness() {
        let node = OrganizationNode::create(
            node_id(1),
            OrganizationPlacement::Holding,
            "H".into(),
            "Holding".into(),
            chrono::Utc::now(),
        )
        .unwrap();
        let event = organization_lineage_event(&node).unwrap();
        assert_eq!(
            integration_events_with_lineage(&[event], &node),
            Err(PortError::Internal)
        );
    }

    #[test]
    fn subtree_write_filter_uses_authoritative_node_lineage() {
        let node = Uuid::from_u128(4);
        let scope = AuthoritySqlScope::try_from_scope(&AuthorityScope::Subtree {
            holding_id: Uuid::from_u128(1),
            organization_node_id: node,
        })
        .unwrap();
        assert_eq!(scope.mode, "subtree");
        assert_eq!(scope.subtree_node_id, Some(node));
        assert!(scope.company_id.is_none());
        assert!(scope.branch_id.is_none());

        let source = include_str!("command.rs");
        assert!(source.contains("$6=node_id OR $6=ANY(lineage_node_ids)"));
        let obsolete_company_alias = ["company_id", "=ANY($6)"].concat();
        assert!(!source.contains(&obsolete_company_alias));
    }

    #[test]
    fn legacy_department_receipt_scope_is_upcast_from_typed_placement() {
        let department = OrganizationNode::create(
            node_id(4),
            OrganizationPlacement::Department {
                holding_id: Uuid::from_u128(1),
                company_id: Uuid::from_u128(2),
                branch_id: Some(Uuid::from_u128(3)),
            },
            "OPS".into(),
            "Operations".into(),
            chrono::Utc::now(),
        )
        .unwrap();
        let mut legacy = serde_json::to_value(&department).unwrap();
        legacy["scope"] = serde_json::to_value(AuthorityScope::Branch {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
            branch_id: Uuid::from_u128(3),
        })
        .unwrap();
        let legacy: OrganizationNode = serde_json::from_value(legacy).unwrap();
        assert!(legacy.validate().is_err());

        let normalized = normalize_receipt_node(&legacy).unwrap();
        assert_eq!(
            normalized.scope(),
            &AuthorityScope::Subtree {
                holding_id: Uuid::from_u128(1),
                organization_node_id: Uuid::from_u128(4),
            }
        );
        assert_eq!(normalized.version(), legacy.version());

        let mut corrupted = serde_json::to_value(&department).unwrap();
        corrupted["scope"] = serde_json::to_value(AuthorityScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(99),
        })
        .unwrap();
        let corrupted: OrganizationNode = serde_json::from_value(corrupted).unwrap();
        assert_eq!(normalize_receipt_node(&corrupted), Err(PortError::Internal));
    }

    #[test]
    fn parent_kind_matrix_is_fail_closed() {
        let parent = GuardRow {
            kind: "company".into(),
            status: "active".into(),
            lineage: vec![Uuid::from_u128(1)],
            holding_id: Uuid::from_u128(1),
            company_id: Some(Uuid::from_u128(2)),
            branch_id: None,
            department_id: None,
        };
        assert!(parent_matches(
            &OrganizationPlacement::Branch {
                holding_id: Uuid::from_u128(1),
                company_id: Uuid::from_u128(2),
            },
            &parent
        ));
        assert!(!parent_matches(
            &OrganizationPlacement::Team {
                holding_id: Uuid::from_u128(1),
                company_id: Uuid::from_u128(2),
                branch_id: None,
                department_id: Uuid::from_u128(4),
            },
            &parent
        ));
    }

    #[test]
    fn holding_directory_draft_uses_the_canonical_holding_scope() {
        let holding_id = node_id(1);
        let holding = OrganizationNode::create(
            holding_id,
            OrganizationPlacement::Holding,
            "holding-organization-bootstrap".into(),
            "holding-organization-bootstrap".into(),
            chrono::Utc::now(),
        )
        .unwrap();

        let record_id = RecordId::try_from_uuid(holding.id().as_uuid()).expect("record id");
        let record_type =
            RecordTypeId::try_new("organization.holding").expect("record type policy");
        let record = StableRecordRef::try_new(record_id, ContextId::Organization, record_type)
            .expect("stable record");
        record_policy_id(&record).expect("record policy owner");

        let draft = organization_directory_draft(&holding)
            .unwrap()
            .expect("holding directory draft");
        assert_eq!(
            draft.draft.scope,
            AuthorityScope::Holding {
                holding_id: holding_id.as_uuid(),
            }
        );
        draft.validate().unwrap();
    }
}
