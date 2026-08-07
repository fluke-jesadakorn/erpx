use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::IntegrationEvent;
use erpx_kernel::{AggregateId, CommandId, EventId, TenantScope};
use erpx_marketing::{
    CommandUnitOfWork, LeadCandidate, LeadCandidateId, MarketingCommit, PortError, WriteRepository,
    ports::{CommandFingerprint, CommandIdentity, StoredCommit},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
    AGGREGATE_TYPE, CONTEXT_NAME, EVENT_VERSION, SNAPSHOT_EVENT_TYPE, lead_partition,
    projection::{MarketingOutboxPayload, MarketingSnapshot, status_name},
};

#[derive(Debug, Clone)]
pub struct MarketingWriteRepository {
    store: CommandStore,
}
impl MarketingWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl WriteRepository for MarketingWriteRepository {
    async fn load(
        &self,
        authority_scope: &TenantScope,
        id: LeadCandidateId,
    ) -> Result<Option<LeadCandidate>, PortError> {
        let snapshot = self
            .store
            .load_snapshot::<LeadCandidate>(
                CONTEXT_NAME,
                &lead_partition(id),
                AGGREGATE_TYPE,
                aggregate_id(id)?,
            )
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        if snapshot.version != snapshot.state.version()
            || snapshot.state.id() != id
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

#[derive(Debug, Clone)]
pub struct MarketingCommandAdapter {
    store: CommandStore,
}
impl MarketingCommandAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl CommandUnitOfWork for MarketingCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        let id = fingerprint_id(&identity.fingerprint);
        self.store
            .find_receipt::<LeadCandidate>(
                CONTEXT_NAME,
                &lead_partition(id),
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?
            .map(receipt_to_stored)
            .transpose()
    }

    async fn commit(&self, request: MarketingCommit) -> Result<StoredCommit, PortError> {
        validate_commit(&request)?;
        let id = request.aggregate.id();
        let partition = lead_partition(id);
        let occurred_at = request.aggregate.updated_at();
        let command_id = request.identity.command_id;
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
                payload: MarketingOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, SNAPSHOT_EVENT_TYPE, events.len()),
            event_type: SNAPSHOT_EVENT_TYPE.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: MarketingOutboxPayload::Snapshot(MarketingSnapshot {
                candidate: (&request.aggregate).into(),
            }),
        });
        let guard = MarketingAtomicGuard {
            aggregate: request.aggregate.clone(),
        };
        let aggregate = request.aggregate;
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
            aggregate_type: AGGREGATE_TYPE.into(),
            aggregate_id: aggregate_id(id)?,
            expected_version: request.expected_version,
            aggregate_state: serde_json::to_value(&aggregate).map_err(|_| PortError::Internal)?,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"marketing-audit-v1"),
                action: audit_action(&request.identity.fingerprint).into(),
                occurred_at,
                details: json!({"lead_id": id.as_uuid(), "expected_version": request.expected_version}),
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
        let mut stored = receipt_to_stored(receipt)?;
        stored.replayed = replayed;
        Ok(stored)
    }
}

struct MarketingAtomicGuard {
    aggregate: LeadCandidate,
}
#[async_trait]
impl AtomicCommandGuard for MarketingAtomicGuard {
    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        let current: Option<i64> = sqlx::query_scalar("SELECT aggregate_version FROM erpx_marketing_write.lead_candidate_guards WHERE lead_id=$1 FOR UPDATE").bind(self.aggregate.id().as_uuid()).fetch_optional(&mut **transaction).await?;
        match (context.expected_version, current) {
            (None, Some(_)) => Err(CqrsSqlxError::GuardRejected(
                "Marketing lead already exists",
            )),
            (Some(_), None) => Err(CqrsSqlxError::GuardRejected(
                "Marketing lead guard is missing",
            )),
            _ => Ok(()),
        }
    }
    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        if context.context != CONTEXT_NAME
            || context.aggregate_type != AGGREGATE_TYPE
            || context.aggregate_id.as_uuid() != self.aggregate.id().as_uuid()
            || context.partition != lead_partition(self.aggregate.id())
            || context.aggregate_version != self.aggregate.version()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "Marketing applied guard metadata mismatch",
            ));
        }
        sqlx::query("INSERT INTO erpx_marketing_write.lead_candidate_guards (lead_id,scope_key,campaign_id,audience_member_id,lead_status,aggregate_version,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (lead_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,campaign_id=EXCLUDED.campaign_id,audience_member_id=EXCLUDED.audience_member_id,lead_status=EXCLUDED.lead_status,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at")
            .bind(self.aggregate.id().as_uuid()).bind(scope_key(self.aggregate.scope())?).bind(self.aggregate.campaign_id().as_uuid()).bind(self.aggregate.audience_member_id().as_uuid()).bind(status_name(self.aggregate.status())).bind(to_i64(context.aggregate_version)?).bind(self.aggregate.updated_at()).execute(&mut **transaction).await?;
        Ok(())
    }
}

fn validate_guard_context(
    value: &LeadCandidate,
    context: &GuardCommandContext<'_>,
) -> Result<(), CqrsSqlxError> {
    if context.context != CONTEXT_NAME
        || context.aggregate_type != AGGREGATE_TYPE
        || context.aggregate_id.as_uuid() != value.id().as_uuid()
        || context.partition != lead_partition(value.id())
    {
        Err(CqrsSqlxError::GuardRejected(
            "Marketing guard metadata mismatch",
        ))
    } else {
        Ok(())
    }
}
fn validate_commit(request: &MarketingCommit) -> Result<(), PortError> {
    if request.aggregate.validate().is_err()
        || fingerprint_id(&request.identity.fingerprint) != request.aggregate.id()
        || !request.authority_scope.contains(request.aggregate.scope())
    {
        Err(PortError::Internal)
    } else {
        Ok(())
    }
}
fn fingerprint_id(value: &CommandFingerprint) -> LeadCandidateId {
    match value {
        CommandFingerprint::Create { lead_id, .. }
        | CommandFingerprint::CaptureEngagement { lead_id, .. }
        | CommandFingerprint::Qualify { lead_id, .. }
        | CommandFingerprint::Reject { lead_id } => *lead_id,
    }
}
fn audit_action(value: &CommandFingerprint) -> &'static str {
    match value {
        CommandFingerprint::Create { .. } => "marketing.lead.create",
        CommandFingerprint::CaptureEngagement { .. } => "marketing.engagement.capture",
        CommandFingerprint::Qualify { .. } => "marketing.lead.qualify",
        CommandFingerprint::Reject { .. } => "marketing.lead.reject",
    }
}
fn aggregate_id(id: LeadCandidateId) -> Result<AggregateId, PortError> {
    AggregateId::try_from_uuid(id.as_uuid()).map_err(|_| PortError::Internal)
}
fn request_hash(value: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let mut digest = Sha256::new();
    digest.update(b"erpx.marketing.command.v1\0");
    digest.update(serde_json::to_vec(value).map_err(|_| PortError::Internal)?);
    Ok(RequestHash::new(digest.finalize().into()))
}
fn deterministic_event_id(command: CommandId, kind: &str, index: usize) -> EventId {
    let mut bytes = kind.as_bytes().to_vec();
    bytes.extend_from_slice(&index.to_be_bytes());
    EventId::try_from_uuid(deterministic_uuid(command.as_uuid(), &bytes)).expect("derived id")
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
pub(crate) fn scope_key(scope: &TenantScope) -> Result<String, CqrsSqlxError> {
    match scope {
        TenantScope::Company {
            holding_id,
            company_id,
        } => Ok(format!("h:{holding_id}/c:{company_id}")),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok(format!("h:{holding_id}/c:{company_id}/b:{branch_id}")),
        _ => Err(CqrsSqlxError::InvalidInput("Marketing scope must be exact")),
    }
}
fn to_i64(value: u64) -> Result<i64, CqrsSqlxError> {
    i64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}
fn receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<LeadCandidate>,
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
    use erpx_marketing::LeadCandidateStatus;

    #[test]
    fn stable_hash_and_status_names() {
        let id = LeadCandidateId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let value = CommandFingerprint::Reject { lead_id: id };
        assert_eq!(request_hash(&value).unwrap(), request_hash(&value).unwrap());
        assert_eq!(status_name(LeadCandidateStatus::Captured), "open");
    }
}
