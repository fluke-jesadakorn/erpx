use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::IntegrationEvent;
use erpx_kernel::{AggregateId, CommandId, EventId, TenantScope};
use erpx_organization::{
    CommandFingerprint, CommandIdentity, CommandUnitOfWork, HierarchyGuard, HierarchyPort,
    OrganizationCommit, OrganizationNode, OrganizationNodeId, OrganizationPlacement, PortError,
    StoredCommit, WriteRepository,
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
    store: CommandStore,
}

impl OrganizationWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl WriteRepository for OrganizationWriteRepository {
    async fn load(
        &self,
        authority_scope: &TenantScope,
        node_id: OrganizationNodeId,
    ) -> Result<Option<OrganizationNode>, PortError> {
        let snapshot = self
            .store
            .load_snapshot::<OrganizationNode>(
                CONTEXT_NAME,
                &node_partition(node_id),
                AGGREGATE_TYPE,
                aggregate_id(node_id)?,
            )
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        if snapshot.version != snapshot.state.version()
            || snapshot.state.id() != node_id
            || snapshot.state.validate().is_err()
        {
            return Err(PortError::Internal);
        }
        if !authority_scope.contains(snapshot.state.scope()) {
            return Err(PortError::Denied);
        }
        Ok(Some(snapshot.state))
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
            aggregate_state: serde_json::to_value(&aggregate).map_err(|_| PortError::Internal)?,
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
            .commit_guarded(command, &atomic_guard)
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
    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT aggregate_version
             FROM erpx_organization_write.hierarchy_guards
             WHERE node_id = $1 FOR UPDATE",
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
                     FROM erpx_organization_write.hierarchy_guards
                     WHERE parent_node_id = $1 AND node_status = 'active'
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
             FROM erpx_organization_write.hierarchy_guards
             WHERE holding_id = $1
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
        sqlx::query(
            "INSERT INTO erpx_organization_write.hierarchy_guards
                (node_id, node_kind, parent_node_id, lineage_node_ids, holding_id,
                 company_id, branch_id, department_id, node_code, node_status,
                 aggregate_version, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
             ON CONFLICT (node_id) DO UPDATE SET
                node_kind = EXCLUDED.node_kind,
                parent_node_id = EXCLUDED.parent_node_id,
                lineage_node_ids = EXCLUDED.lineage_node_ids,
                holding_id = EXCLUDED.holding_id,
                company_id = EXCLUDED.company_id,
                branch_id = EXCLUDED.branch_id,
                department_id = EXCLUDED.department_id,
                node_code = EXCLUDED.node_code,
                node_status = EXCLUDED.node_status,
                aggregate_version = EXCLUDED.aggregate_version,
                updated_at = EXCLUDED.updated_at",
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
        .bind(status_name(self.aggregate.status()))
        .bind(to_i64(context.aggregate_version)?)
        .bind(self.aggregate.updated_at())
        .execute(&mut **transaction)
        .await?;
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
         FROM erpx_organization_write.hierarchy_guards
         WHERE node_id = $1 FOR UPDATE",
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
    if request.aggregate.validate().is_err()
        || fingerprint_node_id(&request.identity.fingerprint) != request.aggregate.id()
        || !request.authority_scope.contains(request.aggregate.scope())
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
