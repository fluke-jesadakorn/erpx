use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, IntegrationEvent,
    ItemDescriptorDraft, RecordTypeId, StableRecordRef, catalog::ContextId, record_policy_id,
};
use erpx_kernel::{AggregateId, CommandId, DataClassification, EventId, RecordId, TenantScope};
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
    AGGREGATE_TYPE, CONTEXT_NAME, EVENT_VERSION, SNAPSHOT_EVENT_TYPE, node_partition,
    projection::{OrganizationOutboxPayload, OrganizationSnapshot, kind_name, status_name},
};

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
    company_ids: Vec<Uuid>,
    branch_ids: Vec<Uuid>,
}

impl AuthoritySqlScope {
    fn try_from_scope(scope: &TenantScope) -> Result<Self, PortError> {
        scope.validate().map_err(|_| PortError::Denied)?;
        Ok(match scope {
            TenantScope::Platform => Self {
                mode: "platform",
                holding_id: None,
                company_id: None,
                branch_id: None,
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            TenantScope::Holding { holding_id } => Self {
                mode: "holding",
                holding_id: Some(*holding_id),
                company_id: None,
                branch_id: None,
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            TenantScope::Company {
                holding_id,
                company_id,
            } => Self {
                mode: "company",
                holding_id: Some(*holding_id),
                company_id: Some(*company_id),
                branch_id: None,
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            TenantScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Self {
                mode: "branch",
                holding_id: Some(*holding_id),
                company_id: Some(*company_id),
                branch_id: Some(*branch_id),
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            TenantScope::Subtree {
                holding_id,
                company_ids,
                branch_ids,
            } => Self {
                mode: "subtree",
                holding_id: Some(*holding_id),
                company_id: None,
                branch_id: None,
                company_ids: company_ids.clone(),
                branch_ids: branch_ids.clone(),
            },
        })
    }
}

#[async_trait]
impl WriteRepository for OrganizationWriteRepository {
    async fn load(
        &self,
        authority_scope: &TenantScope,
        node_id: OrganizationNodeId,
    ) -> Result<Option<OrganizationNode>, PortError> {
        let authority = AuthoritySqlScope::try_from_scope(authority_scope)?;
        let mut transaction = self.pool.begin().await.map_err(map_store_error)?;
        let row = sqlx::query(
            "SELECT aggregate_version,node_kind,holding_id,company_id,branch_id,
                    department_id,node_code,node_name,node_status,created_at,updated_at
             FROM erpx_organization_write.organization_node_authority
             WHERE tenant_id=erpx_cqrs_write.current_tenant_id() AND node_id=$1
               AND (
                 $2='platform'
                 OR ($2='holding' AND holding_id=$3)
                 OR ($2='company' AND holding_id=$3 AND company_id=$4)
                 OR ($2='branch' AND holding_id=$3 AND company_id=$4 AND branch_id=$5)
                 OR ($2='subtree' AND holding_id=$3 AND company_id=ANY($6)
                     AND ((branch_id IS NULL AND cardinality($7::uuid[])=0)
                          OR branch_id=ANY($7)))
               )",
        )
        .bind(node_id.as_uuid())
        .bind(authority.mode)
        .bind(authority.holding_id)
        .bind(authority.company_id)
        .bind(authority.branch_id)
        .bind(&authority.company_ids)
        .bind(&authority.branch_ids)
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
        if !authority_scope.contains(aggregate.scope()) {
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
        authority_scope: &TenantScope,
        node_id: OrganizationNodeId,
        placement: &OrganizationPlacement,
    ) -> Result<(), PortError> {
        let scope = placement
            .tenant_scope(node_id)
            .map_err(|_| PortError::InvalidParent)?;
        if !authority_scope.contains(&scope) {
            return Err(PortError::Denied);
        }
        Ok(())
    }

    async fn ensure_can_archive(
        &self,
        _authority_scope: &TenantScope,
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
        let mut events = request
            .integration_events
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
            event_version: EVENT_VERSION,
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
        let tenant_scope = request.aggregate.scope().clone();
        let aggregate = request.aggregate;
        let command = AtomicCommand {
            context: CONTEXT_NAME.into(),
            partition,
            command_id,
            correlation_id: request.correlation_id,
            idempotency_key,
            request_hash,
            tenant_scope,
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
) -> Result<Option<ItemDescriptorDraft>, PortError> {
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
    value.validate().map_err(|_| PortError::Internal)?;
    Ok(Some(value))
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
             WHERE tenant_id=erpx_cqrs_write.current_tenant_id() AND node_id=$1
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
             WHERE tenant_id=erpx_cqrs_write.current_tenant_id() AND node_id=$1
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
                validate_parent(transaction, self.aggregate.id(), placement).await?;
            }
            Some(HierarchyGuard::NoActiveChildren) => {
                let child: Option<Uuid> = sqlx::query_scalar(
                    "SELECT node_id
                     FROM erpx_organization_write.organization_node_authority
                     WHERE tenant_id=erpx_cqrs_write.current_tenant_id()
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

        let parent_id = self.aggregate.placement().parent_id(self.aggregate.id());
        let duplicate: Option<Uuid> = sqlx::query_scalar(
            "SELECT node_id
             FROM erpx_organization_write.organization_node_authority
             WHERE tenant_id=erpx_cqrs_write.current_tenant_id() AND holding_id = $1
               AND COALESCE(parent_node_id, holding_id) = COALESCE($2, $1)
               AND node_code = $3 AND node_status <> 'archived' AND node_id <> $4
             ORDER BY node_id LIMIT 1 FOR UPDATE",
        )
        .bind(self.aggregate.placement().holding_id(self.aggregate.id()))
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
                (tenant_id,node_id,node_kind,parent_node_id,lineage_node_ids,holding_id,
                 company_id,branch_id,department_id,node_code,node_name,node_status,
                 aggregate_version,created_at,updated_at)
             VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8,
                     $9,$10,$11,$12,$13,$14)
             ON CONFLICT (tenant_id,node_id) DO UPDATE SET
                node_kind = EXCLUDED.node_kind,
                parent_node_id = EXCLUDED.parent_node_id,
                lineage_node_ids = EXCLUDED.lineage_node_ids,
                holding_id = EXCLUDED.holding_id,
                company_id = EXCLUDED.company_id,
                branch_id = EXCLUDED.branch_id,
                department_id = EXCLUDED.department_id,
                node_code = EXCLUDED.node_code,
                node_name = EXCLUDED.node_name,
                node_status = EXCLUDED.node_status,
                aggregate_version = EXCLUDED.aggregate_version,
                created_at = EXCLUDED.created_at,
                updated_at = EXCLUDED.updated_at
             WHERE erpx_organization_write.organization_node_authority.tenant_id=
                       erpx_cqrs_write.current_tenant_id()
               AND erpx_organization_write.organization_node_authority.aggregate_version + 1=
                       EXCLUDED.aggregate_version",
        )
        .bind(node_id.as_uuid())
        .bind(kind_name(self.aggregate.kind()))
        .bind(placement.parent_id(node_id).map(|id| id.as_uuid()))
        .bind(lineage_ids(node_id, placement))
        .bind(placement.holding_id(node_id))
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
                "organization normalized version did not advance exactly once",
            ));
        }
        Ok(())
    }
}

async fn validate_parent(
    transaction: &mut Transaction<'_, Postgres>,
    node_id: OrganizationNodeId,
    placement: &OrganizationPlacement,
) -> Result<(), CqrsSqlxError> {
    placement
        .validate_for(node_id)
        .map_err(|_| CqrsSqlxError::GuardRejected("organization placement is invalid"))?;
    let Some(parent_id) = placement.parent_id(node_id) else {
        return Ok(());
    };
    let row = sqlx::query(
        "SELECT node_kind, node_status, lineage_node_ids, holding_id,
                company_id, branch_id, department_id
         FROM erpx_organization_write.organization_node_authority
         WHERE tenant_id=erpx_cqrs_write.current_tenant_id() AND node_id=$1
         FOR UPDATE",
    )
    .bind(parent_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await?;
    let Some(row) = row else {
        return Err(CqrsSqlxError::GuardRejected(
            "organization parent does not exist",
        ));
    };
    let parent = GuardRow {
        kind: row.try_get("node_kind")?,
        status: row.try_get("node_status")?,
        lineage: row.try_get("lineage_node_ids")?,
        holding_id: row.try_get("holding_id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        department_id: row.try_get("department_id")?,
    };
    if parent.status != "active"
        || parent.lineage.contains(&node_id.as_uuid())
        || !parent_matches(placement, &parent)
    {
        return Err(CqrsSqlxError::GuardRejected(
            "organization parent lineage is invalid",
        ));
    }
    Ok(())
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
        || !request.authority_scope.contains(request.aggregate.scope())
        || request.aggregate.version() != next
    {
        return Err(PortError::Internal);
    }
    Ok(())
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
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != AGGREGATE_TYPE
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.id().as_uuid()
        || receipt.aggregate.version != receipt.response.version()
        || receipt.response.validate().is_err()
    {
        return Err(PortError::Internal);
    }
    Ok(StoredCommit {
        command_id: receipt.command_id,
        correlation_id: receipt.correlation_id,
        aggregate: receipt.response,
        committed_events: receipt.committed_events,
        committed_at: receipt.committed_at,
        replayed: receipt.replayed,
    })
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
        CqrsSqlxError::GuardRejected("organization parent lineage is invalid") => {
            PortError::HierarchyCycle
        }
        CqrsSqlxError::GuardRejected(message)
            if message.contains("parent") || message.contains("placement") =>
        {
            PortError::InvalidParent
        }
        CqrsSqlxError::GuardRejected("organization node already exists")
        | CqrsSqlxError::GuardRejected("organization sibling code already exists") => {
            PortError::Conflict {
                expected: 0,
                actual: 0,
            }
        }
        CqrsSqlxError::GuardRejected(_) => PortError::Internal,
        CqrsSqlxError::Database(error)
            if error
                .as_database_error()
                .and_then(|error| error.constraint())
                == Some("organization_sibling_code") =>
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
}
