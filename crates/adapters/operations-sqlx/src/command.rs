use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::IntegrationEvent;
use erpx_kernel::{AggregateId, CommandId, EventId, TenantScope};
use erpx_operations::{
    CommandUnitOfWork, ExceptionId, OperationsCommit, OperationsException, PortError,
    WorkQueuePolicy, WorkQueuePolicyId, WorkQueuePolicyUnitOfWork, WriteRepository,
    ports::{
        CommandFingerprint, CommandIdentity, StoredCommit, StoredWorkQueuePolicyCommit,
        WorkQueuePolicyCommandIdentity, WorkQueuePolicyCommit,
    },
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
    CONTEXT_NAME, EVENT_VERSION, EXCEPTION_AGGREGATE_TYPE, EXCEPTION_SNAPSHOT_EVENT,
    POLICY_AGGREGATE_TYPE, POLICY_SNAPSHOT_EVENT, exception_partition, policy_partition,
    projection::{
        OperationsOutboxPayload, OperationsSnapshot, exception_status_name, policy_status_name,
    },
};

#[derive(Debug, Clone)]
pub struct OperationsWriteRepository {
    store: CommandStore,
}

impl OperationsWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }

    async fn load<T: serde::de::DeserializeOwned>(
        &self,
        authority: &TenantScope,
        partition: &str,
        aggregate_type: &str,
        id: Uuid,
        scope: impl Fn(&T) -> &TenantScope,
        version: impl Fn(&T) -> u64,
    ) -> Result<Option<T>, PortError> {
        let aggregate_id = AggregateId::try_from_uuid(id).map_err(|_| PortError::Internal)?;
        let snapshot = self
            .store
            .load_snapshot::<T>(CONTEXT_NAME, partition, aggregate_type, aggregate_id)
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        if snapshot.version != version(&snapshot.state) {
            return Err(PortError::Internal);
        }
        if !authority.contains(scope(&snapshot.state)) {
            return Err(PortError::Denied);
        }
        Ok(Some(snapshot.state))
    }
}

#[async_trait]
impl WriteRepository for OperationsWriteRepository {
    async fn load_exception(
        &self,
        authority: &TenantScope,
        id: ExceptionId,
    ) -> Result<Option<OperationsException>, PortError> {
        let value = self
            .load(
                authority,
                &exception_partition(id),
                EXCEPTION_AGGREGATE_TYPE,
                id.as_uuid(),
                OperationsException::scope,
                OperationsException::version,
            )
            .await?;
        if value
            .as_ref()
            .is_some_and(|v| v.id() != id || v.validate().is_err())
        {
            Err(PortError::Internal)
        } else {
            Ok(value)
        }
    }

    async fn load_work_queue_policy(
        &self,
        authority: &TenantScope,
        id: WorkQueuePolicyId,
    ) -> Result<Option<WorkQueuePolicy>, PortError> {
        let value = self
            .load(
                authority,
                &policy_partition(id),
                POLICY_AGGREGATE_TYPE,
                id.as_uuid(),
                WorkQueuePolicy::scope,
                WorkQueuePolicy::version,
            )
            .await?;
        if value
            .as_ref()
            .is_some_and(|v| v.id() != id || v.validate().is_err())
        {
            Err(PortError::Internal)
        } else {
            Ok(value)
        }
    }
}

#[derive(Debug, Clone)]
pub struct OperationsCommandAdapter {
    store: CommandStore,
}

impl OperationsCommandAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl CommandUnitOfWork for OperationsCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        let id = exception_target(&identity.fingerprint);
        self.store
            .find_receipt::<OperationsException>(
                CONTEXT_NAME,
                &exception_partition(id),
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?
            .map(exception_receipt)
            .transpose()
    }

    async fn commit(&self, request: OperationsCommit) -> Result<StoredCommit, PortError> {
        let aggregate = request.aggregate;
        aggregate.validate().map_err(|_| PortError::Internal)?;
        if aggregate.version() != request.expected_version + 1 {
            return Err(PortError::Conflict {
                expected: request.expected_version,
                actual: aggregate.version(),
            });
        }
        let partition = exception_partition(aggregate.id());
        let command_id = request.identity.command_id;
        let mut events = integration_events(
            command_id,
            &partition,
            request.causation_id,
            aggregate.updated_at(),
            &request.integration_events,
        );
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, EXCEPTION_SNAPSHOT_EVENT, events.len()),
            event_type: EXCEPTION_SNAPSHOT_EVENT.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at: aggregate.updated_at(),
            payload: OperationsOutboxPayload::Snapshot(OperationsSnapshot::exception(
                &aggregate, 1, None,
            )),
        });
        let guard = OperationsAtomicGuard::Exception(Box::new(aggregate.clone()));
        let aggregate_id = AggregateId::try_from_uuid(aggregate.id().as_uuid())
            .map_err(|_| PortError::Internal)?;
        let command = AtomicCommand {
            context: CONTEXT_NAME.into(),
            partition,
            command_id,
            correlation_id: request.correlation_id,
            idempotency_key: request.identity.idempotency_key,
            request_hash: request_hash(&request.identity.fingerprint)?,
            tenant_scope: aggregate.scope().clone(),
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: EXCEPTION_AGGREGATE_TYPE.into(),
            aggregate_id,
            expected_version: Some(request.expected_version),
            aggregate_state: serde_json::to_value(&aggregate).map_err(|_| PortError::Internal)?,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(
                    command_id.as_uuid(),
                    b"operations-exception-audit-v1",
                ),
                action: exception_audit_action(&request.identity.fingerprint).into(),
                occurred_at: guard.occurred_at(),
                details: json!({"exception_id": aggregate_id.as_uuid(), "expected_version": request.expected_version}),
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
            AtomicCommandOutcome::Committed(v) | AtomicCommandOutcome::Replayed(v) => v,
        };
        let mut stored = exception_receipt(receipt)?;
        stored.replayed = replayed;
        Ok(stored)
    }
}

#[async_trait]
impl WorkQueuePolicyUnitOfWork for OperationsCommandAdapter {
    async fn find_policy_receipt(
        &self,
        identity: &WorkQueuePolicyCommandIdentity,
    ) -> Result<Option<StoredWorkQueuePolicyCommit>, PortError> {
        self.store
            .find_receipt::<WorkQueuePolicy>(
                CONTEXT_NAME,
                &policy_partition(identity.fingerprint.policy_id),
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?
            .map(policy_receipt)
            .transpose()
    }

    async fn commit_policy(
        &self,
        request: WorkQueuePolicyCommit,
    ) -> Result<StoredWorkQueuePolicyCommit, PortError> {
        let aggregate = request.aggregate;
        aggregate.validate().map_err(|_| PortError::Internal)?;
        let expected_next = request.expected_version.map_or(1, |value| value + 1);
        if aggregate.version() != expected_next {
            return Err(PortError::Conflict {
                expected: request.expected_version.unwrap_or(0),
                actual: aggregate.version(),
            });
        }
        let partition = policy_partition(aggregate.id());
        let command_id = request.identity.command_id;
        let mut events = integration_events(
            command_id,
            &partition,
            request.causation_id,
            aggregate.updated_at(),
            &request.integration_events,
        );
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, POLICY_SNAPSHOT_EVENT, events.len()),
            event_type: POLICY_SNAPSHOT_EVENT.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at: aggregate.updated_at(),
            payload: OperationsOutboxPayload::Snapshot(OperationsSnapshot::policy(&aggregate)),
        });
        let guard = OperationsAtomicGuard::Policy(aggregate.clone());
        let aggregate_id = AggregateId::try_from_uuid(aggregate.id().as_uuid())
            .map_err(|_| PortError::Internal)?;
        let command = AtomicCommand {
            context: CONTEXT_NAME.into(),
            partition,
            command_id,
            correlation_id: request.correlation_id,
            idempotency_key: request.identity.idempotency_key,
            request_hash: request_hash(&request.identity.fingerprint)?,
            tenant_scope: aggregate.scope().clone(),
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: POLICY_AGGREGATE_TYPE.into(),
            aggregate_id,
            expected_version: request.expected_version,
            aggregate_state: serde_json::to_value(&aggregate).map_err(|_| PortError::Internal)?,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"operations-policy-audit-v1"),
                action: "operations.work_queue_policy.publish".into(),
                occurred_at: guard.occurred_at(),
                details: json!({"policy_id": aggregate_id.as_uuid(), "expected_version": request.expected_version}),
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
            AtomicCommandOutcome::Committed(v) | AtomicCommandOutcome::Replayed(v) => v,
        };
        let mut stored = policy_receipt(receipt)?;
        stored.replayed = replayed;
        Ok(stored)
    }
}

enum OperationsAtomicGuard {
    Exception(Box<OperationsException>),
    Policy(WorkQueuePolicy),
}

impl OperationsAtomicGuard {
    const fn occurred_at(&self) -> chrono::DateTime<chrono::Utc> {
        match self {
            Self::Exception(value) => value.updated_at(),
            Self::Policy(value) => value.updated_at(),
        }
    }
}

#[async_trait]
impl AtomicCommandGuard for OperationsAtomicGuard {
    async fn validate(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        match self {
            Self::Exception(value) => {
                validate_context(
                    context,
                    EXCEPTION_AGGREGATE_TYPE,
                    value.id().as_uuid(),
                    &exception_partition(value.id()),
                )?;
                let current: Option<i64> = sqlx::query_scalar(
                    "SELECT aggregate_version FROM erpx_operations_write.exception_guards WHERE exception_id=$1 FOR UPDATE",
                )
                .bind(value.id().as_uuid())
                .fetch_optional(&mut **tx)
                .await?;
                if current.and_then(|v| u64::try_from(v).ok()) != context.expected_version {
                    return Err(CqrsSqlxError::GuardRejected(
                        "Operations exception guard version changed",
                    ));
                }
            }
            Self::Policy(value) => {
                validate_context(
                    context,
                    POLICY_AGGREGATE_TYPE,
                    value.id().as_uuid(),
                    &policy_partition(value.id()),
                )?;
                let current: Option<i64> = sqlx::query_scalar(
                    "SELECT aggregate_version FROM erpx_operations_write.work_queue_policy_guards WHERE policy_id=$1 FOR UPDATE",
                )
                .bind(value.id().as_uuid())
                .fetch_optional(&mut **tx)
                .await?;
                if current.and_then(|v| u64::try_from(v).ok()) != context.expected_version {
                    return Err(CqrsSqlxError::GuardRejected(
                        "Operations policy guard version changed",
                    ));
                }
                let policy_name_lock = format!(
                    "operations-policy-name:{}:{}",
                    scope_key(value.scope())?,
                    value.name()
                );
                sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0::bigint))")
                    .bind(policy_name_lock)
                    .execute(&mut **tx)
                    .await?;
                let collision: Option<Uuid> = sqlx::query_scalar(
                    "SELECT policy_id FROM erpx_operations_write.work_queue_policy_guards WHERE scope_key=$1 AND policy_name=$2 AND policy_id<>$3 FOR UPDATE",
                )
                .bind(scope_key(value.scope())?)
                .bind(value.name())
                .bind(value.id().as_uuid())
                .fetch_optional(&mut **tx)
                .await?;
                if collision.is_some() {
                    return Err(CqrsSqlxError::GuardRejected(
                        "Operations policy name already exists",
                    ));
                }
            }
        }
        Ok(())
    }

    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        match self {
            Self::Exception(value) => {
                validate_applied(
                    context,
                    EXCEPTION_AGGREGATE_TYPE,
                    value.id().as_uuid(),
                    value.version(),
                )?;
                let result = sqlx::query("UPDATE erpx_operations_write.exception_guards SET tenant_scope=$2,scope_key=$3,category=$4,severity=$5,policy_ref=$6,assignee_id=$7,manager_id=$8,assignment_authority_ref=$9,assignment_valid_from=$10,assignment_valid_until=$11,due_at=$12,escalation_after_minutes=$13,exception_status=$14,aggregate_version=$15,updated_at=$16 WHERE exception_id=$1")
                    .bind(value.id().as_uuid()).bind(serde_json::to_value(value.scope())?)
                    .bind(scope_key(value.scope())?).bind(value.category()).bind(value.severity()).bind(value.policy_ref())
                    .bind(value.assignment().assignee.as_uuid().to_string()).bind(value.assignment().manager.as_uuid().to_string())
                    .bind(&value.assignment().authority_ref).bind(value.assignment().valid_from).bind(value.assignment().valid_until)
                    .bind(value.due_at()).bind(i32::try_from(value.escalation_after_minutes()).map_err(|_| CqrsSqlxError::NumericRange)?)
                    .bind(exception_status_name(value.status())).bind(i64::try_from(value.version()).map_err(|_| CqrsSqlxError::NumericRange)?)
                    .bind(value.updated_at()).execute(&mut **tx).await?;
                if result.rows_affected() != 1 {
                    return Err(CqrsSqlxError::GuardRejected(
                        "Operations exception guard missing",
                    ));
                }
            }
            Self::Policy(value) => {
                validate_applied(
                    context,
                    POLICY_AGGREGATE_TYPE,
                    value.id().as_uuid(),
                    value.version(),
                )?;
                sqlx::query("INSERT INTO erpx_operations_write.work_queue_policy_guards (policy_id,scope_key,tenant_scope,policy_name,allowed_source_contexts,escalation_after_minutes,policy_status,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT (policy_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,tenant_scope=EXCLUDED.tenant_scope,policy_name=EXCLUDED.policy_name,allowed_source_contexts=EXCLUDED.allowed_source_contexts,escalation_after_minutes=EXCLUDED.escalation_after_minutes,policy_status=EXCLUDED.policy_status,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at")
                    .bind(value.id().as_uuid()).bind(scope_key(value.scope())?).bind(serde_json::to_value(value.scope())?)
                    .bind(value.name()).bind(serde_json::to_value(value.allowed_sources())?)
                    .bind(i32::try_from(value.escalation_after_minutes()).map_err(|_| CqrsSqlxError::NumericRange)?)
                    .bind(policy_status_name(value.status())).bind(i64::try_from(value.version()).map_err(|_| CqrsSqlxError::NumericRange)?)
                    .bind(value.created_at()).bind(value.updated_at()).execute(&mut **tx).await?;
            }
        }
        Ok(())
    }
}

fn validate_context(
    context: &GuardCommandContext<'_>,
    aggregate_type: &str,
    id: Uuid,
    partition: &str,
) -> Result<(), CqrsSqlxError> {
    if context.context != CONTEXT_NAME
        || context.aggregate_type != aggregate_type
        || context.aggregate_id.as_uuid() != id
        || context.partition != partition
    {
        Err(CqrsSqlxError::GuardRejected(
            "Operations command metadata mismatch",
        ))
    } else {
        Ok(())
    }
}

fn validate_applied(
    context: &AppliedGuardContext<'_>,
    aggregate_type: &str,
    id: Uuid,
    version: u64,
) -> Result<(), CqrsSqlxError> {
    if context.context != CONTEXT_NAME
        || context.aggregate_type != aggregate_type
        || context.aggregate_id.as_uuid() != id
        || context.aggregate_version != version
    {
        Err(CqrsSqlxError::GuardRejected(
            "Operations applied metadata mismatch",
        ))
    } else {
        Ok(())
    }
}

fn integration_events(
    command_id: CommandId,
    partition: &str,
    causation_id: Option<EventId>,
    occurred_at: chrono::DateTime<chrono::Utc>,
    events: &[erpx_integration_contracts::operations::Event],
) -> Vec<OutboxEventWrite<OperationsOutboxPayload>> {
    events
        .iter()
        .enumerate()
        .map(|(index, event)| OutboxEventWrite {
            event_id: deterministic_event_id(command_id, event.event_type(), index),
            event_type: event.event_type().into(),
            event_version: u32::from(event.schema_version()),
            partition: partition.into(),
            causation_id,
            occurred_at,
            payload: OperationsOutboxPayload::Integration(event.clone()),
        })
        .collect()
}

fn exception_target(value: &CommandFingerprint) -> ExceptionId {
    match value {
        CommandFingerprint::Reassign { exception_id, .. }
        | CommandFingerprint::Acknowledge { exception_id }
        | CommandFingerprint::Escalate { exception_id, .. }
        | CommandFingerprint::DecideApproval { exception_id, .. }
        | CommandFingerprint::Resolve { exception_id }
        | CommandFingerprint::Close { exception_id } => *exception_id,
    }
}

fn exception_audit_action(value: &CommandFingerprint) -> &'static str {
    match value {
        CommandFingerprint::Reassign { .. } => "operations.exception.reassign",
        CommandFingerprint::Acknowledge { .. } => "operations.exception.acknowledge",
        CommandFingerprint::Escalate { .. } => "operations.exception.escalate",
        CommandFingerprint::DecideApproval { .. } => "operations.approval.decide",
        CommandFingerprint::Resolve { .. } => "operations.exception.resolve",
        CommandFingerprint::Close { .. } => "operations.exception.close",
    }
}

fn request_hash(value: &impl serde::Serialize) -> Result<RequestHash, PortError> {
    let bytes = serde_json::to_vec(value).map_err(|_| PortError::Internal)?;
    Ok(RequestHash::new(Sha256::digest(bytes).into()))
}

fn exception_receipt(
    receipt: erpx_cqrs::CommandReceipt<OperationsException>,
) -> Result<StoredCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != EXCEPTION_AGGREGATE_TYPE
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.id().as_uuid()
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

fn policy_receipt(
    receipt: erpx_cqrs::CommandReceipt<WorkQueuePolicy>,
) -> Result<StoredWorkQueuePolicyCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != POLICY_AGGREGATE_TYPE
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.id().as_uuid()
        || receipt.aggregate.version != receipt.response.version()
    {
        return Err(PortError::Internal);
    }
    Ok(StoredWorkQueuePolicyCommit {
        command_id: receipt.command_id,
        correlation_id: receipt.correlation_id,
        aggregate: receipt.response,
        committed_events: receipt.committed_events,
        committed_at: receipt.committed_at,
        replayed: receipt.replayed,
    })
}

pub(crate) fn scope_key(scope: &TenantScope) -> Result<String, CqrsSqlxError> {
    scope
        .validate()
        .map_err(|_| CqrsSqlxError::InvalidInput("invalid scope"))?;
    Ok(match scope {
        TenantScope::Platform => "platform".into(),
        TenantScope::Holding { holding_id } => format!("holding:{holding_id}"),
        TenantScope::Company {
            holding_id,
            company_id,
        } => format!("holding:{holding_id}/company:{company_id}"),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => format!("holding:{holding_id}/company:{company_id}/branch:{branch_id}"),
        TenantScope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        } => {
            format!(
                "subtree:{holding_id}:{}:{}",
                company_ids.len(),
                branch_ids.len()
            )
        }
    })
}

fn deterministic_event_id(command: CommandId, event_type: &str, index: usize) -> EventId {
    EventId::try_from_uuid(deterministic_uuid(
        command.as_uuid(),
        format!("operations-event-v1:{event_type}:{index}").as_bytes(),
    ))
    .expect("deterministic event identifier is non-nil")
}

pub(crate) fn deterministic_uuid(namespace: Uuid, label: &[u8]) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(namespace.as_bytes());
    hasher.update(label);
    let mut bytes: [u8; 16] = hasher.finalize()[..16].try_into().expect("fixed digest");
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

pub(crate) fn map_store_error(error: CqrsSqlxError) -> PortError {
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
    #[test]
    fn command_guard_updates_context_tables_inside_shared_transaction() {
        let source = include_str!("command.rs");
        assert!(source.contains("commit_guarded"));
        assert!(source.contains("impl AtomicCommandGuard for OperationsAtomicGuard"));
        assert!(source.contains("exception_guards"));
        assert!(source.contains("work_queue_policy_guards"));
        assert!(source.contains("operations-policy-name:"));
        assert!(source.contains("pg_advisory_xact_lock"));
    }

    #[test]
    fn all_human_actions_have_distinct_audit_names() {
        let source = include_str!("command.rs");
        for action in ["reassign", "acknowledge", "escalate", "resolve", "close"] {
            assert!(source.contains(&format!("operations.exception.{action}")));
        }
        assert!(source.contains("operations.approval.decide"));
    }
}
