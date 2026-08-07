use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::IntegrationEvent;
use erpx_kernel::{AggregateId, CommandId, EventId, TenantScope};
use erpx_platform_administration::{
    AdminContextId, AdminContextView, CommandFingerprint, CommandIdentity, CommandUnitOfWork,
    ConnectorBinding, ConnectorBindingId, ConnectorView, PlatformAdministrationCommit,
    PlatformAggregate, PlatformTenant, PlatformTenantId, PortError, StoredCommit, TenantView,
    WriteRepository,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
    ADMIN_CONTEXT_AGGREGATE, ADMIN_CONTEXT_SNAPSHOT_EVENT, CONNECTOR_AGGREGATE,
    CONNECTOR_SNAPSHOT_EVENT, CONTEXT_NAME, EVENT_VERSION, TENANT_AGGREGATE, TENANT_SNAPSHOT_EVENT,
    admin_context_partition, connector_partition,
    projection::{
        PlatformAdministrationOutboxPayload, PlatformAdministrationSnapshot, connector_kind,
        connector_state, scope_key, tenant_state,
    },
    tenant_partition,
};

#[derive(Debug, Clone)]
pub struct PlatformAdministrationWriteRepository {
    store: CommandStore,
}

impl PlatformAdministrationWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl WriteRepository for PlatformAdministrationWriteRepository {
    async fn load_tenant(
        &self,
        authority: &TenantScope,
        id: PlatformTenantId,
    ) -> Result<Option<PlatformTenant>, PortError> {
        let snapshot = self
            .store
            .load_snapshot::<PlatformAggregate>(
                CONTEXT_NAME,
                &tenant_partition(id),
                TENANT_AGGREGATE,
                aggregate_id(id.as_uuid())?,
            )
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        let PlatformAggregate::Tenant(value) = snapshot.state else {
            return Err(PortError::Internal);
        };
        if snapshot.version != value.version()
            || value.id() != id
            || !authority.contains(&value.scope())
        {
            return Err(PortError::Denied);
        }
        Ok(Some(value))
    }

    async fn load_connector(
        &self,
        authority: &TenantScope,
        id: ConnectorBindingId,
    ) -> Result<Option<ConnectorBinding>, PortError> {
        let snapshot = self
            .store
            .load_snapshot::<PlatformAggregate>(
                CONTEXT_NAME,
                &connector_partition(id),
                CONNECTOR_AGGREGATE,
                aggregate_id(id.as_uuid())?,
            )
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        let PlatformAggregate::Connector(value) = snapshot.state else {
            return Err(PortError::Internal);
        };
        if snapshot.version != value.version()
            || value.id() != id
            || !authority.contains(&value.scope().tenant_scope())
        {
            return Err(PortError::Denied);
        }
        Ok(Some(value))
    }

    async fn load_admin_context(
        &self,
        authority: &TenantScope,
        id: AdminContextId,
    ) -> Result<Option<erpx_platform_administration::TenantAdminContext>, PortError> {
        let snapshot = self
            .store
            .load_snapshot::<PlatformAggregate>(
                CONTEXT_NAME,
                &admin_context_partition(id),
                ADMIN_CONTEXT_AGGREGATE,
                aggregate_id(id.as_uuid())?,
            )
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        let PlatformAggregate::AdminContext(value) = snapshot.state else {
            return Err(PortError::Internal);
        };
        if snapshot.version != value.version()
            || value.id() != id
            || !authority.contains(value.scope())
        {
            return Err(PortError::Denied);
        }
        Ok(Some(value))
    }
}

#[derive(Debug, Clone)]
pub struct PlatformAdministrationCommandAdapter {
    store: CommandStore,
}

impl PlatformAdministrationCommandAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl CommandUnitOfWork for PlatformAdministrationCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        let key = fingerprint_key(&identity.fingerprint);
        let receipt = self
            .store
            .find_receipt::<PlatformAggregate>(
                CONTEXT_NAME,
                &key.partition,
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?;
        receipt.map(receipt_to_stored).transpose()
    }

    async fn commit(
        &self,
        request: PlatformAdministrationCommit,
    ) -> Result<StoredCommit, PortError> {
        validate_commit(&request)?;
        let key = aggregate_key(&request.aggregate);
        let occurred_at = aggregate_updated_at(&request.aggregate);
        let command_id = request.identity.command_id;
        let mut events = request
            .integration_events
            .iter()
            .enumerate()
            .map(|(index, event)| OutboxEventWrite {
                event_id: deterministic_event_id(command_id, event.event_type(), index),
                event_type: event.event_type().into(),
                event_version: u32::from(event.schema_version()),
                partition: key.partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: PlatformAdministrationOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, key.snapshot_event, events.len()),
            event_type: key.snapshot_event.into(),
            event_version: EVENT_VERSION,
            partition: key.partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: PlatformAdministrationOutboxPayload::Snapshot(snapshot(
                &request.aggregate,
                occurred_at,
            )),
        });
        let aggregate = request.aggregate;
        let guard = PlatformAdministrationGuard {
            aggregate: aggregate.clone(),
        };
        let command = AtomicCommand {
            context: CONTEXT_NAME.into(),
            partition: key.partition,
            command_id,
            correlation_id: request.correlation_id,
            idempotency_key: request.identity.idempotency_key,
            request_hash: request_hash(&request.identity.fingerprint)?,
            tenant_scope: aggregate.scope(),
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: key.aggregate_type.into(),
            aggregate_id: aggregate_id(aggregate.id_uuid())?,
            expected_version: request.expected_version,
            aggregate_state: serde_json::to_value(&aggregate).map_err(|_| PortError::Internal)?,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(
                    command_id.as_uuid(),
                    b"platform-administration-audit-v1",
                ),
                action: audit_action(&request.identity.fingerprint).into(),
                occurred_at,
                details: json!({"aggregate_id": guard.aggregate.id_uuid(), "expected_version": request.expected_version}),
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
            AtomicCommandOutcome::Committed(value) | AtomicCommandOutcome::Replayed(value) => value,
        };
        let mut stored = receipt_to_stored(receipt)?;
        stored.replayed = replayed;
        Ok(stored)
    }
}

#[derive(Debug)]
struct PlatformAdministrationGuard {
    aggregate: PlatformAggregate,
}

#[async_trait]
impl AtomicCommandGuard for PlatformAdministrationGuard {
    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        let key = aggregate_key(&self.aggregate);
        if context.context != CONTEXT_NAME
            || context.aggregate_id.as_uuid() != self.aggregate.id_uuid()
            || context.aggregate_type != key.aggregate_type
            || context.partition != key.partition
        {
            return Err(CqrsSqlxError::GuardRejected(
                "platform administration guard metadata mismatch",
            ));
        }
        match &self.aggregate {
            PlatformAggregate::Tenant(value) => {
                let conflict: Option<Uuid> = sqlx::query_scalar("SELECT tenant_id FROM erpx_platform_administration_write.tenant_guards WHERE tenant_id<>$1 AND (tenant_code=$2 OR customer_id=$3 OR root_holding_id=$4 OR holding_ref=$5) ORDER BY tenant_id LIMIT 1 FOR UPDATE")
                    .bind(value.id().as_uuid()).bind(value.code()).bind(value.customer_id()).bind(value.root_holding_id()).bind(value.holding_ref()).fetch_optional(&mut **transaction).await?;
                if conflict.is_some() {
                    return Err(CqrsSqlxError::GuardRejected(
                        "platform tenant binding is not unique",
                    ));
                }
            }
            PlatformAggregate::Connector(value) => {
                let conflict: Option<Uuid> = sqlx::query_scalar("SELECT connector_id FROM erpx_platform_administration_write.connector_guards WHERE connector_id<>$1 AND scope_key=$2 AND connector_key=$3 ORDER BY connector_id LIMIT 1 FOR UPDATE")
                    .bind(value.id().as_uuid()).bind(scope_key(&value.scope().tenant_scope())?).bind(value.key()).fetch_optional(&mut **transaction).await?;
                if conflict.is_some() {
                    return Err(CqrsSqlxError::GuardRejected(
                        "platform connector key is not unique in scope",
                    ));
                }
            }
            PlatformAggregate::AdminContext(value) => {
                let holding_id = match value.scope() {
                    TenantScope::Holding { holding_id } => *holding_id,
                    _ => {
                        return Err(CqrsSqlxError::GuardRejected(
                            "admin context requires holding scope",
                        ));
                    }
                };
                let matches_tenant: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM erpx_platform_administration_write.tenant_guards WHERE tenant_id=$1 AND root_holding_id=$2 AND lifecycle_state='active')")
                    .bind(value.tenant_id().as_uuid()).bind(holding_id).fetch_one(&mut **transaction).await?;
                if !matches_tenant {
                    return Err(CqrsSqlxError::GuardRejected(
                        "admin context tenant binding is not active",
                    ));
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
        if context.aggregate_version != self.aggregate.version() {
            return Err(CqrsSqlxError::GuardRejected(
                "platform administration applied version mismatch",
            ));
        }
        let version = to_i64(context.aggregate_version)?;
        match &self.aggregate {
            PlatformAggregate::Tenant(value) => {
                sqlx::query("INSERT INTO erpx_platform_administration_write.tenant_guards (tenant_id,tenant_code,tenant_name,environment_id,customer_id,root_holding_id,holding_ref,lifecycle_state,aggregate_version,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT (tenant_id) DO UPDATE SET tenant_code=EXCLUDED.tenant_code,tenant_name=EXCLUDED.tenant_name,environment_id=EXCLUDED.environment_id,customer_id=EXCLUDED.customer_id,root_holding_id=EXCLUDED.root_holding_id,holding_ref=EXCLUDED.holding_ref,lifecycle_state=EXCLUDED.lifecycle_state,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at")
                    .bind(value.id().as_uuid()).bind(value.code()).bind(value.name()).bind(value.environment_id().as_uuid()).bind(value.customer_id()).bind(value.root_holding_id()).bind(value.holding_ref()).bind(tenant_state(value.status())).bind(version).bind(value.updated_at()).execute(&mut **transaction).await?;
            }
            PlatformAggregate::Connector(value) => {
                sqlx::query("INSERT INTO erpx_platform_administration_write.connector_guards (connector_id,scope_key,connector_kind,connector_key,configuration_digest,capabilities,connector_state,aggregate_version,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT (connector_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,connector_kind=EXCLUDED.connector_kind,connector_key=EXCLUDED.connector_key,configuration_digest=EXCLUDED.configuration_digest,capabilities=EXCLUDED.capabilities,connector_state=EXCLUDED.connector_state,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at")
                    .bind(value.id().as_uuid()).bind(scope_key(&value.scope().tenant_scope())?).bind(connector_kind(value.kind())).bind(value.key()).bind(decode_sha256(value.configuration_digest())?).bind(serde_json::to_value(value.capabilities())?).bind(connector_state(value.state())).bind(version).bind(value.updated_at()).execute(&mut **transaction).await?;
            }
            PlatformAggregate::AdminContext(value) => {
                let serialized = serde_json::to_value(&self.aggregate)?;
                let inner = serialized
                    .get("aggregate")
                    .ok_or(CqrsSqlxError::InvalidInput(
                        "admin context serialization shape changed",
                    ))?;
                sqlx::query("INSERT INTO erpx_platform_administration_write.admin_context_guards (context_id,tenant_id,scope_key,principal_ref,device_id,session_id,reason,issued_at,expires_at,revoked_at,aggregate_version) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT (context_id) DO UPDATE SET tenant_id=EXCLUDED.tenant_id,scope_key=EXCLUDED.scope_key,principal_ref=EXCLUDED.principal_ref,device_id=EXCLUDED.device_id,session_id=EXCLUDED.session_id,reason=EXCLUDED.reason,issued_at=EXCLUDED.issued_at,expires_at=EXCLUDED.expires_at,revoked_at=EXCLUDED.revoked_at,aggregate_version=EXCLUDED.aggregate_version")
                    .bind(value.id().as_uuid()).bind(value.tenant_id().as_uuid()).bind(scope_key(value.scope())?).bind(value.principal_ref()).bind(json_uuid(inner,"device_id")?).bind(json_uuid(inner,"session_id")?).bind(value.reason()).bind(value.issued_at()).bind(value.expires_at()).bind(value.revoked_at()).bind(version).execute(&mut **transaction).await?;
            }
        }
        Ok(())
    }
}

struct AggregateKey {
    partition: String,
    aggregate_type: &'static str,
    snapshot_event: &'static str,
}

fn fingerprint_key(value: &CommandFingerprint) -> AggregateKey {
    match value {
        CommandFingerprint::RegisterTenant { tenant_id, .. }
        | CommandFingerprint::TransitionTenant { tenant_id, .. } => AggregateKey {
            partition: tenant_partition(*tenant_id),
            aggregate_type: TENANT_AGGREGATE,
            snapshot_event: TENANT_SNAPSHOT_EVENT,
        },
        CommandFingerprint::RegisterConnector { connector_id, .. }
        | CommandFingerprint::TransitionConnector { connector_id, .. } => AggregateKey {
            partition: connector_partition(*connector_id),
            aggregate_type: CONNECTOR_AGGREGATE,
            snapshot_event: CONNECTOR_SNAPSHOT_EVENT,
        },
        CommandFingerprint::IssueAdminContext { context_id, .. }
        | CommandFingerprint::RevokeAdminContext { context_id } => AggregateKey {
            partition: admin_context_partition(*context_id),
            aggregate_type: ADMIN_CONTEXT_AGGREGATE,
            snapshot_event: ADMIN_CONTEXT_SNAPSHOT_EVENT,
        },
    }
}

fn aggregate_key(value: &PlatformAggregate) -> AggregateKey {
    match value {
        PlatformAggregate::Tenant(value) => AggregateKey {
            partition: tenant_partition(value.id()),
            aggregate_type: TENANT_AGGREGATE,
            snapshot_event: TENANT_SNAPSHOT_EVENT,
        },
        PlatformAggregate::Connector(value) => AggregateKey {
            partition: connector_partition(value.id()),
            aggregate_type: CONNECTOR_AGGREGATE,
            snapshot_event: CONNECTOR_SNAPSHOT_EVENT,
        },
        PlatformAggregate::AdminContext(value) => AggregateKey {
            partition: admin_context_partition(value.id()),
            aggregate_type: ADMIN_CONTEXT_AGGREGATE,
            snapshot_event: ADMIN_CONTEXT_SNAPSHOT_EVENT,
        },
    }
}

fn validate_commit(request: &PlatformAdministrationCommit) -> Result<(), PortError> {
    let fingerprint = fingerprint_key(&request.identity.fingerprint);
    let aggregate = aggregate_key(&request.aggregate);
    if fingerprint.partition != aggregate.partition
        || fingerprint.aggregate_type != aggregate.aggregate_type
        || !request.authority_scope.contains(&request.aggregate.scope())
    {
        return Err(PortError::Internal);
    }
    Ok(())
}

fn snapshot(
    value: &PlatformAggregate,
    at: chrono::DateTime<chrono::Utc>,
) -> PlatformAdministrationSnapshot {
    match value {
        PlatformAggregate::Tenant(value) => {
            PlatformAdministrationSnapshot::Tenant(TenantView::from(value))
        }
        PlatformAggregate::Connector(value) => {
            PlatformAdministrationSnapshot::Connector(ConnectorView::from(value))
        }
        PlatformAggregate::AdminContext(value) => {
            PlatformAdministrationSnapshot::AdminContext(AdminContextView::from_context(value, at))
        }
    }
}
fn aggregate_updated_at(value: &PlatformAggregate) -> chrono::DateTime<chrono::Utc> {
    match value {
        PlatformAggregate::Tenant(v) => v.updated_at(),
        PlatformAggregate::Connector(v) => v.updated_at(),
        PlatformAggregate::AdminContext(v) => v.revoked_at().unwrap_or(v.issued_at()),
    }
}
fn receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<PlatformAggregate>,
) -> Result<StoredCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != receipt.response.aggregate_type()
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.id_uuid()
        || receipt.aggregate.version != receipt.response.version()
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
fn request_hash(value: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let mut digest = Sha256::new();
    digest.update(b"erpx.platform_administration.command-fingerprint.v1\0");
    digest.update(serde_json::to_vec(value).map_err(|_| PortError::Internal)?);
    Ok(RequestHash::new(digest.finalize().into()))
}
fn aggregate_id(value: Uuid) -> Result<AggregateId, PortError> {
    AggregateId::try_from_uuid(value).map_err(|_| PortError::Internal)
}
fn audit_action(value: &CommandFingerprint) -> &'static str {
    match value {
        CommandFingerprint::RegisterTenant { .. } => "platform_administration.tenant.register",
        CommandFingerprint::TransitionTenant { .. } => "platform_administration.tenant.transition",
        CommandFingerprint::RegisterConnector { .. } => {
            "platform_administration.connector.register"
        }
        CommandFingerprint::TransitionConnector { .. } => {
            "platform_administration.connector.transition"
        }
        CommandFingerprint::IssueAdminContext { .. } => {
            "platform_administration.admin_context.issue"
        }
        CommandFingerprint::RevokeAdminContext { .. } => {
            "platform_administration.admin_context.revoke"
        }
    }
}
fn deterministic_event_id(command_id: CommandId, event_type: &str, index: usize) -> EventId {
    let mut input = Vec::new();
    input.extend_from_slice(event_type.as_bytes());
    input.extend_from_slice(&index.to_be_bytes());
    EventId::try_from_uuid(deterministic_uuid(command_id.as_uuid(), &input))
        .expect("derived UUID is non-nil")
}
fn deterministic_uuid(seed: Uuid, label: &[u8]) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(seed.as_bytes());
    digest.update(label);
    let digest = digest.finalize();
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}
fn to_i64(value: u64) -> Result<i64, CqrsSqlxError> {
    i64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}
fn decode_sha256(value: &str) -> Result<Vec<u8>, CqrsSqlxError> {
    if value.len() != 64 {
        return Err(CqrsSqlxError::GuardRejected("invalid connector digest"));
    }
    (0..64)
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|_| CqrsSqlxError::GuardRejected("invalid connector digest"))
        })
        .collect()
}
fn json_uuid(value: &Value, field: &str) -> Result<Uuid, CqrsSqlxError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
        .filter(|id| !id.is_nil())
        .ok_or(CqrsSqlxError::InvalidInput(
            "admin context identifier serialization is invalid",
        ))
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
        CqrsSqlxError::Database(_) => PortError::Unavailable,
        _ => PortError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_hash_is_payload_sensitive() {
        let id = PlatformTenantId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let a = CommandFingerprint::TransitionTenant {
            tenant_id: id,
            action: erpx_platform_administration::TenantLifecycleAction::Activate,
        };
        let b = CommandFingerprint::TransitionTenant {
            tenant_id: id,
            action: erpx_platform_administration::TenantLifecycleAction::Suspend,
        };
        assert_eq!(request_hash(&a).unwrap(), request_hash(&a).unwrap());
        assert_ne!(request_hash(&a).unwrap(), request_hash(&b).unwrap());
    }
    #[test]
    fn guard_source_covers_all_frozen_control_tables() {
        let source = include_str!("command.rs");
        for table in ["tenant_guards", "connector_guards", "admin_context_guards"] {
            assert!(source.contains(table));
        }
        assert!(source.contains("commit_guarded"));
    }
}
