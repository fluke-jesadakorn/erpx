use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_identity_access::{
    AggregateKey, CommandFingerprint, CommandIdentity, CommandUnitOfWork, IdentityAccessAggregate,
    IdentityAccessCommit, PortError, PrincipalView, RiskTier, RoleView, StoredCommit,
    WriteRepository,
};
use erpx_integration_contracts::IntegrationEvent;
use erpx_kernel::{AggregateId, CommandId, EventId, TenantScope};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
    CONTEXT_NAME, EVENT_VERSION, PRINCIPAL_AGGREGATE_TYPE, PRINCIPAL_SNAPSHOT_EVENT_TYPE,
    ROLE_AGGREGATE_TYPE, ROLE_SNAPSHOT_EVENT_TYPE, principal_partition,
    projection::{IdentityAccessOutboxPayload, IdentityAccessSnapshot},
    role_partition,
};

#[derive(Debug, Clone)]
pub struct IdentityAccessWriteRepository {
    store: CommandStore,
}

impl IdentityAccessWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl WriteRepository for IdentityAccessWriteRepository {
    async fn load(
        &self,
        authority_scope: &TenantScope,
        key: AggregateKey,
    ) -> Result<Option<IdentityAccessAggregate>, PortError> {
        let (partition, aggregate_type, aggregate_id) = key_parts(key)?;
        let snapshot = self
            .store
            .load_snapshot::<IdentityAccessAggregate>(
                CONTEXT_NAME,
                &partition,
                aggregate_type,
                aggregate_id,
            )
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        if snapshot.version != snapshot.state.version()
            || snapshot.state.aggregate_type() != aggregate_type
            || snapshot.state.aggregate_uuid() != aggregate_id.as_uuid()
        {
            return Err(PortError::Internal);
        }
        validate_aggregate(&snapshot.state)?;
        if !authority_scope.contains(snapshot.state.scope()) {
            return Err(PortError::Denied);
        }
        Ok(Some(snapshot.state))
    }
}

#[derive(Debug, Clone)]
pub struct IdentityAccessCommandAdapter {
    store: CommandStore,
}

impl IdentityAccessCommandAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl CommandUnitOfWork for IdentityAccessCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        let key = fingerprint_key(&identity.fingerprint);
        let (partition, _, _) = key_parts(key)?;
        let receipt = self
            .store
            .find_receipt::<IdentityAccessAggregate>(
                CONTEXT_NAME,
                &partition,
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?;
        receipt.map(receipt_to_stored).transpose()
    }

    async fn commit(&self, request: IdentityAccessCommit) -> Result<StoredCommit, PortError> {
        validate_commit(&request)?;
        let key = fingerprint_key(&request.identity.fingerprint);
        let (partition, aggregate_type, aggregate_id) = key_parts(key)?;
        let occurred_at = aggregate_updated_at(&request.aggregate);
        let command_id = request.identity.command_id;
        let request_hash = request_hash(&request.identity.fingerprint)?;
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
                payload: IdentityAccessOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        let (snapshot_type, snapshot) = match &request.aggregate {
            IdentityAccessAggregate::Principal(value) => (
                PRINCIPAL_SNAPSHOT_EVENT_TYPE,
                IdentityAccessSnapshot::Principal(PrincipalView::from(value)),
            ),
            IdentityAccessAggregate::Role(value) => (
                ROLE_SNAPSHOT_EVENT_TYPE,
                IdentityAccessSnapshot::Role(RoleView::from(value)),
            ),
        };
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, snapshot_type, events.len()),
            event_type: snapshot_type.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: IdentityAccessOutboxPayload::Snapshot(snapshot),
        });

        let tenant_scope = request.aggregate.scope().clone();
        let aggregate = request.aggregate;
        let guard = IdentityAccessGuard {
            aggregate: aggregate.clone(),
        };
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
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            expected_version: request.expected_version,
            aggregate_state: serde_json::to_value(&aggregate).map_err(|_| PortError::Internal)?,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"identity-access-audit-v1"),
                action: audit_action(&request.identity.fingerprint).into(),
                occurred_at,
                details: json!({
                    "aggregate_id": aggregate_id.as_uuid(),
                    "expected_version": request.expected_version,
                }),
            },
            events,
        };
        let outcome = self
            .store
            .commit_guarded(command, &guard)
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

#[derive(Debug, Clone)]
struct IdentityAccessGuard {
    aggregate: IdentityAccessAggregate,
}

#[async_trait]
impl AtomicCommandGuard for IdentityAccessGuard {
    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(
            context.context,
            context.aggregate_type,
            context.aggregate_id,
            &self.aggregate,
        )?;
        match &self.aggregate {
            IdentityAccessAggregate::Role(role) => {
                let owner: Option<Uuid> = sqlx::query_scalar(
                    "SELECT role_id
                     FROM erpx_identity_access_write.role_code_guards
                     WHERE scope_key = $1 AND role_code = $2
                     FOR UPDATE",
                )
                .bind(scope_key(role.scope()))
                .bind(role.code().as_str())
                .fetch_optional(&mut **transaction)
                .await?;
                if owner.is_some_and(|owner| owner != role.id().as_uuid()) {
                    return Err(CqrsSqlxError::GuardRejected(
                        "role code is already owned in this scope",
                    ));
                }
            }
            IdentityAccessAggregate::Principal(principal) => {
                for assignment in principal.assignments() {
                    let owner: Option<Uuid> = sqlx::query_scalar(
                        "SELECT principal_id
                         FROM erpx_identity_access_write.assignment_guards
                         WHERE assignment_id = $1
                         FOR UPDATE",
                    )
                    .bind(assignment.id().as_uuid())
                    .fetch_optional(&mut **transaction)
                    .await?;
                    if owner.is_some_and(|owner| owner != principal.id().as_uuid()) {
                        return Err(CqrsSqlxError::GuardRejected(
                            "assignment id belongs to another principal",
                        ));
                    }
                    let duplicate: Option<Uuid> = sqlx::query_scalar(
                        "SELECT assignment_id
                         FROM erpx_identity_access_write.assignment_guards
                         WHERE principal_id = $1 AND role_id = $2 AND scope_key = $3
                           AND revoked_at IS NULL AND assignment_id <> $4
                         FOR UPDATE",
                    )
                    .bind(principal.id().as_uuid())
                    .bind(assignment.role_id().as_uuid())
                    .bind(scope_key(assignment.scope()))
                    .bind(assignment.id().as_uuid())
                    .fetch_optional(&mut **transaction)
                    .await?;
                    if duplicate.is_some() {
                        return Err(CqrsSqlxError::GuardRejected(
                            "an active role assignment already exists",
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(
            context.context,
            context.aggregate_type,
            context.aggregate_id,
            &self.aggregate,
        )?;
        if context.aggregate_version != self.aggregate.version() {
            return Err(CqrsSqlxError::GuardRejected(
                "guard aggregate version does not match snapshot",
            ));
        }
        match &self.aggregate {
            IdentityAccessAggregate::Role(role) => {
                sqlx::query(
                    "INSERT INTO erpx_identity_access_write.role_code_guards
                        (role_id, scope_key, role_code, aggregate_version, updated_at)
                     VALUES ($1, $2, $3, $4, $5)
                     ON CONFLICT (role_id) DO UPDATE
                     SET scope_key = EXCLUDED.scope_key, role_code = EXCLUDED.role_code,
                         aggregate_version = EXCLUDED.aggregate_version,
                         updated_at = EXCLUDED.updated_at",
                )
                .bind(role.id().as_uuid())
                .bind(scope_key(role.scope()))
                .bind(role.code().as_str())
                .bind(to_i64(context.aggregate_version)?)
                .bind(role.updated_at())
                .execute(&mut **transaction)
                .await?;
            }
            IdentityAccessAggregate::Principal(principal) => {
                for assignment in principal.assignments() {
                    let applied = sqlx::query(
                        "INSERT INTO erpx_identity_access_write.assignment_guards
                            (assignment_id, principal_id, role_id, scope_key, risk_tier,
                             valid_from, valid_until, revoked_at, principal_version, updated_at)
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                         ON CONFLICT (assignment_id) DO UPDATE
                         SET role_id = EXCLUDED.role_id, scope_key = EXCLUDED.scope_key,
                             risk_tier = EXCLUDED.risk_tier,
                             valid_from = EXCLUDED.valid_from,
                             valid_until = EXCLUDED.valid_until,
                             revoked_at = EXCLUDED.revoked_at,
                             principal_version = EXCLUDED.principal_version,
                             updated_at = EXCLUDED.updated_at
                         WHERE erpx_identity_access_write.assignment_guards.principal_id
                               = EXCLUDED.principal_id",
                    )
                    .bind(assignment.id().as_uuid())
                    .bind(principal.id().as_uuid())
                    .bind(assignment.role_id().as_uuid())
                    .bind(scope_key(assignment.scope()))
                    .bind(risk_name(assignment.risk_tier()))
                    .bind(assignment.valid_from())
                    .bind(assignment.valid_until())
                    .bind(assignment.revoked_at())
                    .bind(to_i64(context.aggregate_version)?)
                    .bind(principal.updated_at())
                    .execute(&mut **transaction)
                    .await?;
                    if applied.rows_affected() != 1 {
                        return Err(CqrsSqlxError::GuardRejected(
                            "assignment guard owner changed",
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

fn validate_guard_context(
    context: &str,
    aggregate_type: &str,
    aggregate_id: AggregateId,
    aggregate: &IdentityAccessAggregate,
) -> Result<(), CqrsSqlxError> {
    if context != CONTEXT_NAME
        || aggregate_type != aggregate.aggregate_type()
        || aggregate_id.as_uuid() != aggregate.aggregate_uuid()
    {
        return Err(CqrsSqlxError::GuardRejected(
            "guard received mismatched aggregate metadata",
        ));
    }
    Ok(())
}

fn validate_commit(request: &IdentityAccessCommit) -> Result<(), PortError> {
    validate_aggregate(&request.aggregate)?;
    let key = fingerprint_key(&request.identity.fingerprint);
    if key.as_uuid() != request.aggregate.aggregate_uuid()
        || key.aggregate_type() != request.aggregate.aggregate_type()
        || !request.authority_scope.contains(request.aggregate.scope())
    {
        return Err(PortError::Internal);
    }
    Ok(())
}

fn validate_aggregate(aggregate: &IdentityAccessAggregate) -> Result<(), PortError> {
    aggregate
        .scope()
        .validate()
        .map_err(|_| PortError::Internal)?;
    match aggregate {
        IdentityAccessAggregate::Principal(principal) => {
            if principal.version() == 0 || principal.display_name().trim().is_empty() {
                return Err(PortError::Internal);
            }
            for assignment in principal.assignments() {
                if !principal.scope().contains(assignment.scope())
                    || assignment.valid_until() <= assignment.valid_from()
                {
                    return Err(PortError::Internal);
                }
            }
        }
        IdentityAccessAggregate::Role(role) => {
            if role.version() == 0
                || role.display_name().trim().is_empty()
                || role.permissions().is_empty()
            {
                return Err(PortError::Internal);
            }
        }
    }
    Ok(())
}

fn key_parts(key: AggregateKey) -> Result<(String, &'static str, AggregateId), PortError> {
    let (partition, aggregate_type, uuid) = match key {
        AggregateKey::Principal(id) => (
            principal_partition(id),
            PRINCIPAL_AGGREGATE_TYPE,
            id.as_uuid(),
        ),
        AggregateKey::Role(id) => (role_partition(id), ROLE_AGGREGATE_TYPE, id.as_uuid()),
    };
    Ok((
        partition,
        aggregate_type,
        AggregateId::try_from_uuid(uuid).map_err(|_| PortError::Internal)?,
    ))
}

fn fingerprint_key(fingerprint: &CommandFingerprint) -> AggregateKey {
    match fingerprint {
        CommandFingerprint::CreatePrincipal { principal_id, .. }
        | CommandFingerprint::ActivatePrincipal { principal_id }
        | CommandFingerprint::SuspendPrincipal { principal_id }
        | CommandFingerprint::ReactivatePrincipal { principal_id }
        | CommandFingerprint::DisablePrincipal { principal_id }
        | CommandFingerprint::AssignRole { principal_id, .. }
        | CommandFingerprint::RevokeRole { principal_id, .. } => {
            AggregateKey::Principal(*principal_id)
        }
        CommandFingerprint::DefineRole { role_id, .. }
        | CommandFingerprint::SetRolePermissions { role_id, .. } => AggregateKey::Role(*role_id),
    }
}

fn request_hash(fingerprint: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let bytes = serde_json::to_vec(fingerprint).map_err(|_| PortError::Internal)?;
    let mut digest = Sha256::new();
    digest.update(b"erpx.identity-access.command-fingerprint.v1\0");
    digest.update(bytes);
    Ok(RequestHash::new(digest.finalize().into()))
}

fn deterministic_event_id(command_id: CommandId, event_type: &str, index: usize) -> EventId {
    let mut label = event_type.as_bytes().to_vec();
    label.extend_from_slice(&index.to_be_bytes());
    EventId::try_from_uuid(deterministic_uuid(command_id.as_uuid(), &label))
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
        CommandFingerprint::CreatePrincipal { .. } => "identity_access.principal.create",
        CommandFingerprint::ActivatePrincipal { .. } => "identity_access.principal.activate",
        CommandFingerprint::SuspendPrincipal { .. } => "identity_access.principal.suspend",
        CommandFingerprint::ReactivatePrincipal { .. } => "identity_access.principal.reactivate",
        CommandFingerprint::DisablePrincipal { .. } => "identity_access.principal.disable",
        CommandFingerprint::AssignRole { .. } => "identity_access.principal.assign_role",
        CommandFingerprint::RevokeRole { .. } => "identity_access.principal.revoke_role",
        CommandFingerprint::DefineRole { .. } => "identity_access.role.define",
        CommandFingerprint::SetRolePermissions { .. } => "identity_access.role.set_permissions",
    }
}

fn aggregate_updated_at(aggregate: &IdentityAccessAggregate) -> chrono::DateTime<chrono::Utc> {
    match aggregate {
        IdentityAccessAggregate::Principal(value) => value.updated_at(),
        IdentityAccessAggregate::Role(value) => value.updated_at(),
    }
}

pub(crate) fn scope_key(scope: &TenantScope) -> String {
    match scope {
        TenantScope::Platform => "platform".into(),
        TenantScope::Holding { holding_id } => format!("holding:{holding_id}"),
        TenantScope::Company {
            holding_id,
            company_id,
        } => format!("company:{holding_id}:{company_id}"),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => format!("branch:{holding_id}:{company_id}:{branch_id}"),
        TenantScope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        } => {
            let mut companies = company_ids
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            let mut branches = branch_ids
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            companies.sort_unstable();
            branches.sort_unstable();
            format!(
                "subtree:{holding_id}:companies={}:branches={}",
                companies.join(","),
                branches.join(",")
            )
        }
    }
}

pub(crate) const fn risk_name(risk: RiskTier) -> &'static str {
    match risk {
        RiskTier::PlatformRoot => "platform_root",
        RiskTier::Administrator => "administrator",
        RiskTier::High => "high",
        RiskTier::Standard => "standard",
        RiskTier::Contractor => "contractor",
        RiskTier::SupportDiagnostic => "support_diagnostic",
        RiskTier::SupportMutation => "support_mutation",
        RiskTier::Emergency => "emergency",
    }
}

fn receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<IdentityAccessAggregate>,
) -> Result<StoredCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != receipt.response.aggregate_type()
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.aggregate_uuid()
        || receipt.aggregate.version != receipt.response.version()
    {
        return Err(PortError::Internal);
    }
    validate_aggregate(&receipt.response)?;
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
        CqrsSqlxError::GuardRejected(_) => PortError::Conflict {
            expected: 0,
            actual: 0,
        },
        CqrsSqlxError::Database(error)
            if error
                .as_database_error()
                .and_then(|value| value.code())
                .is_some_and(|code| code == "23505") =>
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
    use erpx_identity_access::PrincipalId;

    use super::*;

    #[test]
    fn fingerprints_are_stable_and_payload_sensitive() {
        let principal_id = PrincipalId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let first = CommandFingerprint::CreatePrincipal {
            principal_id,
            scope: TenantScope::Platform,
            display_name: "Ada".into(),
        };
        let changed = CommandFingerprint::CreatePrincipal {
            principal_id,
            scope: TenantScope::Platform,
            display_name: "Grace".into(),
        };
        assert_eq!(request_hash(&first).unwrap(), request_hash(&first).unwrap());
        assert_ne!(
            request_hash(&first).unwrap(),
            request_hash(&changed).unwrap()
        );
    }

    #[test]
    fn canonical_scope_keys_are_order_insensitive_for_subtrees() {
        let left = TenantScope::Subtree {
            holding_id: Uuid::from_u128(1),
            company_ids: vec![Uuid::from_u128(3), Uuid::from_u128(2)],
            branch_ids: vec![Uuid::from_u128(5), Uuid::from_u128(4)],
        };
        let right = TenantScope::Subtree {
            holding_id: Uuid::from_u128(1),
            company_ids: vec![Uuid::from_u128(2), Uuid::from_u128(3)],
            branch_ids: vec![Uuid::from_u128(4), Uuid::from_u128(5)],
        };
        assert_eq!(scope_key(&left), scope_key(&right));
    }

    #[test]
    fn deterministic_ids_are_namespaced() {
        let command = CommandId::try_from_uuid(Uuid::from_u128(9)).unwrap();
        assert_eq!(
            deterministic_event_id(command, PRINCIPAL_SNAPSHOT_EVENT_TYPE, 0),
            deterministic_event_id(command, PRINCIPAL_SNAPSHOT_EVENT_TYPE, 0)
        );
        assert_ne!(
            deterministic_event_id(command, PRINCIPAL_SNAPSHOT_EVENT_TYPE, 0),
            deterministic_event_id(command, ROLE_SNAPSHOT_EVENT_TYPE, 0)
        );
    }
}
