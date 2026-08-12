use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::IntegrationEvent;
use erpx_kernel::{AggregateId, CommandId, EventId, TenantScope};
use erpx_marketing::{
    CampaignBudget, CampaignBudgetId, CommandUnitOfWork, LeadCandidate, LeadCandidateId,
    MarketingAggregate, MarketingCommit, PortError, SpendReservationStatus, WriteRepository,
    ports::{CommandFingerprint, CommandIdentity, StoredCommit},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    BUDGET_AGGREGATE_TYPE, BUDGET_SNAPSHOT_EVENT_TYPE, CONTEXT_NAME, EVENT_VERSION,
    LEAD_AGGREGATE_TYPE, LEAD_SNAPSHOT_EVENT_TYPE, budget_partition, lead_partition,
    projection::{MarketingOutboxPayload, MarketingSnapshot, budget_status_name, lead_status_name},
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
    async fn load_lead(
        &self,
        authority_scope: &TenantScope,
        id: LeadCandidateId,
    ) -> Result<Option<LeadCandidate>, PortError> {
        let snapshot = self
            .store
            .load_snapshot::<MarketingAggregate>(
                CONTEXT_NAME,
                &lead_partition(id),
                LEAD_AGGREGATE_TYPE,
                aggregate_id(id.as_uuid())?,
            )
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        let MarketingAggregate::LeadCandidate(value) = snapshot.state else {
            return Err(PortError::Internal);
        };
        if snapshot.version != value.version()
            || value.id() != id
            || value.validate().is_err()
            || !authority_scope.contains(value.scope())
        {
            return Err(if authority_scope.contains(value.scope()) {
                PortError::Internal
            } else {
                PortError::Denied
            });
        }
        Ok(Some(*value))
    }

    async fn load_budget(
        &self,
        authority_scope: &TenantScope,
        id: CampaignBudgetId,
    ) -> Result<Option<CampaignBudget>, PortError> {
        let snapshot = self
            .store
            .load_snapshot::<MarketingAggregate>(
                CONTEXT_NAME,
                &budget_partition(id),
                BUDGET_AGGREGATE_TYPE,
                aggregate_id(id.as_uuid())?,
            )
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        let MarketingAggregate::CampaignBudget(value) = snapshot.state else {
            return Err(PortError::Internal);
        };
        if snapshot.version != value.version()
            || value.id() != id
            || value.validate().is_err()
            || !authority_scope.contains(value.scope())
        {
            return Err(if authority_scope.contains(value.scope()) {
                PortError::Internal
            } else {
                PortError::Denied
            });
        }
        Ok(Some(*value))
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
        let target = fingerprint_target(&identity.fingerprint);
        self.store
            .find_receipt::<MarketingAggregate>(
                CONTEXT_NAME,
                &target.partition,
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
        let target = aggregate_target(&request.aggregate);
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
                partition: target.partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: MarketingOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, target.snapshot_event, events.len()),
            event_type: target.snapshot_event.into(),
            event_version: EVENT_VERSION,
            partition: target.partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: MarketingOutboxPayload::Snapshot(MarketingSnapshot::from(&request.aggregate)),
        });
        let guard = MarketingAtomicGuard {
            aggregate: request.aggregate.clone(),
        };
        let aggregate = request.aggregate;
        let command = AtomicCommand {
            context: CONTEXT_NAME.into(),
            partition: target.partition,
            command_id,
            correlation_id: request.correlation_id,
            idempotency_key: request.identity.idempotency_key,
            request_hash: request_hash(&request.identity.fingerprint)?,
            tenant_scope: aggregate.scope().clone(),
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: target.aggregate_type.into(),
            aggregate_id: aggregate_id(target.id)?,
            expected_version: request.expected_version,
            aggregate_state: serde_json::to_value(&aggregate).map_err(|_| PortError::Internal)?,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"marketing-audit-v2"),
                action: audit_action(&request.identity.fingerprint).into(),
                occurred_at,
                details: json!({
                    "aggregate_type": target.aggregate_type,
                    "aggregate_id": target.id,
                    "expected_version": request.expected_version
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
            AtomicCommandOutcome::Committed(value) | AtomicCommandOutcome::Replayed(value) => value,
        };
        let mut stored = receipt_to_stored(receipt)?;
        stored.replayed = replayed;
        Ok(stored)
    }
}

struct MarketingAtomicGuard {
    aggregate: MarketingAggregate,
}

#[async_trait]
impl AtomicCommandGuard for MarketingAtomicGuard {
    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        let current = match &self.aggregate {
            MarketingAggregate::LeadCandidate(value) => {
                sqlx::query_scalar::<_, i64>(
                    "SELECT aggregate_version FROM erpx_marketing_write.lead_candidate_guards WHERE lead_id=$1 FOR UPDATE",
                )
                .bind(value.id().as_uuid())
                .fetch_optional(&mut **transaction)
                .await?
            }
            MarketingAggregate::CampaignBudget(value) => {
                let current = sqlx::query_scalar::<_, i64>(
                    "SELECT aggregate_version FROM erpx_marketing_write.campaign_budget_guards WHERE budget_id=$1 FOR UPDATE",
                )
                .bind(value.id().as_uuid())
                .fetch_optional(&mut **transaction)
                .await?;
                if current.is_some() {
                    let rows = sqlx::query(
                        "SELECT request_id FROM erpx_marketing_write.campaign_spend_request_guards WHERE budget_id=$1 FOR UPDATE",
                    )
                    .bind(value.id().as_uuid())
                    .fetch_all(&mut **transaction)
                    .await?;
                    if rows.iter().any(|row| {
                        row.try_get::<Uuid, _>("request_id")
                            .ok()
                            .and_then(|id| erpx_marketing::SpendRequestId::try_from_uuid(id).ok())
                            .is_none_or(|id| !value.reservations().contains_key(&id))
                    }) {
                        return Err(CqrsSqlxError::GuardRejected(
                            "Marketing spend request history cannot be removed",
                        ));
                    }
                }
                current
            }
        };
        match (context.expected_version, current) {
            (None, Some(_)) => Err(CqrsSqlxError::GuardRejected(
                "Marketing aggregate already exists",
            )),
            (Some(_), None) => Err(CqrsSqlxError::GuardRejected(
                "Marketing aggregate guard is missing",
            )),
            _ => Ok(()),
        }
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_applied_context(&self.aggregate, context)?;
        match &self.aggregate {
            MarketingAggregate::LeadCandidate(value) => {
                sqlx::query("INSERT INTO erpx_marketing_write.lead_candidate_guards (lead_id,scope_key,campaign_id,audience_member_id,lead_status,aggregate_version,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (lead_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,campaign_id=EXCLUDED.campaign_id,audience_member_id=EXCLUDED.audience_member_id,lead_status=EXCLUDED.lead_status,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at")
                    .bind(value.id().as_uuid())
                    .bind(scope_key(value.scope())?)
                    .bind(value.campaign_id().as_uuid())
                    .bind(value.audience_member_id().as_uuid())
                    .bind(lead_status_name(value.status()))
                    .bind(to_i64(context.aggregate_version)?)
                    .bind(value.updated_at())
                    .execute(&mut **transaction)
                    .await?;
            }
            MarketingAggregate::CampaignBudget(value) => {
                sqlx::query("INSERT INTO erpx_marketing_write.campaign_budget_guards (budget_id,scope_key,campaign_id,budget_code,budget_status,currency,requested_minor,approved_minor,reserved_minor,aggregate_version,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT (budget_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,campaign_id=EXCLUDED.campaign_id,budget_code=EXCLUDED.budget_code,budget_status=EXCLUDED.budget_status,currency=EXCLUDED.currency,requested_minor=EXCLUDED.requested_minor,approved_minor=EXCLUDED.approved_minor,reserved_minor=EXCLUDED.reserved_minor,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at")
                    .bind(value.id().as_uuid())
                    .bind(scope_key(value.scope())?)
                    .bind(value.campaign_id().as_uuid())
                    .bind(value.code())
                    .bind(budget_status_name(value.status()))
                    .bind(value.requested_amount().currency().as_str())
                    .bind(value.requested_amount().minor_units())
                    .bind(value.approved_amount().map(|amount| amount.minor_units()))
                    .bind(value.reserved_minor().map_err(|_| CqrsSqlxError::NumericRange)?)
                    .bind(to_i64(context.aggregate_version)?)
                    .bind(value.updated_at())
                    .execute(&mut **transaction)
                    .await?;
                for reservation in value.reservations().values() {
                    let status = match reservation.status {
                        SpendReservationStatus::Active => "active",
                        SpendReservationStatus::Released => "released",
                    };
                    let result = sqlx::query("INSERT INTO erpx_marketing_write.campaign_spend_request_guards (request_id,budget_id,allocation_id,currency,amount_minor,expected_outcome,spend_status,requested_at,released_at,budget_aggregate_version) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT (request_id) DO UPDATE SET spend_status=EXCLUDED.spend_status,released_at=EXCLUDED.released_at,budget_aggregate_version=EXCLUDED.budget_aggregate_version WHERE erpx_marketing_write.campaign_spend_request_guards.budget_id=EXCLUDED.budget_id AND erpx_marketing_write.campaign_spend_request_guards.allocation_id=EXCLUDED.allocation_id AND erpx_marketing_write.campaign_spend_request_guards.currency=EXCLUDED.currency AND erpx_marketing_write.campaign_spend_request_guards.amount_minor=EXCLUDED.amount_minor AND erpx_marketing_write.campaign_spend_request_guards.expected_outcome=EXCLUDED.expected_outcome AND erpx_marketing_write.campaign_spend_request_guards.requested_at=EXCLUDED.requested_at AND (erpx_marketing_write.campaign_spend_request_guards.spend_status=EXCLUDED.spend_status OR (erpx_marketing_write.campaign_spend_request_guards.spend_status='active' AND EXCLUDED.spend_status='released'))")
                        .bind(reservation.request_id.as_uuid())
                        .bind(value.id().as_uuid())
                        .bind(reservation.allocation_id.as_uuid())
                        .bind(reservation.amount.currency().as_str())
                        .bind(reservation.amount.minor_units())
                        .bind(&reservation.expected_outcome)
                        .bind(status)
                        .bind(reservation.requested_at)
                        .bind(reservation.released_at)
                        .bind(to_i64(context.aggregate_version)?)
                        .execute(&mut **transaction)
                        .await?;
                    if result.rows_affected() != 1 {
                        return Err(CqrsSqlxError::GuardRejected(
                            "Marketing spend request immutable fields changed",
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

fn validate_guard_context(
    value: &MarketingAggregate,
    context: &GuardCommandContext<'_>,
) -> Result<(), CqrsSqlxError> {
    let target = aggregate_target(value);
    if context.context != CONTEXT_NAME
        || context.aggregate_type != target.aggregate_type
        || context.aggregate_id.as_uuid() != target.id
        || context.partition != target.partition
    {
        Err(CqrsSqlxError::GuardRejected(
            "Marketing guard metadata mismatch",
        ))
    } else {
        Ok(())
    }
}

fn validate_applied_context(
    value: &MarketingAggregate,
    context: &AppliedGuardContext<'_>,
) -> Result<(), CqrsSqlxError> {
    let target = aggregate_target(value);
    if context.context != CONTEXT_NAME
        || context.aggregate_type != target.aggregate_type
        || context.aggregate_id.as_uuid() != target.id
        || context.partition != target.partition
        || context.aggregate_version != value.version()
    {
        Err(CqrsSqlxError::GuardRejected(
            "Marketing applied guard metadata mismatch",
        ))
    } else {
        Ok(())
    }
}

fn validate_commit(request: &MarketingCommit) -> Result<(), PortError> {
    let fingerprint = fingerprint_target(&request.identity.fingerprint);
    let aggregate = aggregate_target(&request.aggregate);
    if request.aggregate.validate().is_err()
        || fingerprint.id != aggregate.id
        || fingerprint.aggregate_type != aggregate.aggregate_type
        || fingerprint.partition != aggregate.partition
        || !request.authority_scope.contains(request.aggregate.scope())
    {
        Err(PortError::Internal)
    } else {
        Ok(())
    }
}

struct AggregateTarget {
    id: Uuid,
    aggregate_type: &'static str,
    partition: String,
    snapshot_event: &'static str,
}

fn aggregate_target(value: &MarketingAggregate) -> AggregateTarget {
    match value {
        MarketingAggregate::LeadCandidate(value) => AggregateTarget {
            id: value.id().as_uuid(),
            aggregate_type: LEAD_AGGREGATE_TYPE,
            partition: lead_partition(value.id()),
            snapshot_event: LEAD_SNAPSHOT_EVENT_TYPE,
        },
        MarketingAggregate::CampaignBudget(value) => AggregateTarget {
            id: value.id().as_uuid(),
            aggregate_type: BUDGET_AGGREGATE_TYPE,
            partition: budget_partition(value.id()),
            snapshot_event: BUDGET_SNAPSHOT_EVENT_TYPE,
        },
    }
}

fn fingerprint_target(value: &CommandFingerprint) -> AggregateTarget {
    match value {
        CommandFingerprint::CreateLead { lead_id, .. }
        | CommandFingerprint::CaptureEngagement { lead_id, .. }
        | CommandFingerprint::Qualify { lead_id, .. }
        | CommandFingerprint::Reject { lead_id } => AggregateTarget {
            id: lead_id.as_uuid(),
            aggregate_type: LEAD_AGGREGATE_TYPE,
            partition: lead_partition(*lead_id),
            snapshot_event: LEAD_SNAPSHOT_EVENT_TYPE,
        },
        CommandFingerprint::CreateBudget { budget_id, .. }
        | CommandFingerprint::SubmitBudget { budget_id, .. }
        | CommandFingerprint::ApplyBudgetDecision { budget_id, .. }
        | CommandFingerprint::StartBudget { budget_id }
        | CommandFingerprint::CompleteBudget { budget_id }
        | CommandFingerprint::CancelBudget { budget_id }
        | CommandFingerprint::ReserveSpend { budget_id, .. }
        | CommandFingerprint::ReleaseSpend { budget_id, .. } => AggregateTarget {
            id: budget_id.as_uuid(),
            aggregate_type: BUDGET_AGGREGATE_TYPE,
            partition: budget_partition(*budget_id),
            snapshot_event: BUDGET_SNAPSHOT_EVENT_TYPE,
        },
    }
}

fn aggregate_updated_at(value: &MarketingAggregate) -> chrono::DateTime<chrono::Utc> {
    match value {
        MarketingAggregate::LeadCandidate(value) => value.updated_at(),
        MarketingAggregate::CampaignBudget(value) => value.updated_at(),
    }
}

fn audit_action(value: &CommandFingerprint) -> &'static str {
    match value {
        CommandFingerprint::CreateLead { .. } => "marketing.lead.create",
        CommandFingerprint::CaptureEngagement { .. } => "marketing.engagement.capture",
        CommandFingerprint::Qualify { .. } => "marketing.lead.qualify",
        CommandFingerprint::Reject { .. } => "marketing.lead.reject",
        CommandFingerprint::CreateBudget { .. } => "marketing.budget.create",
        CommandFingerprint::SubmitBudget { .. } => "marketing.budget.submit",
        CommandFingerprint::ApplyBudgetDecision { .. } => "marketing.budget.apply_decision",
        CommandFingerprint::StartBudget { .. } => "marketing.budget.start",
        CommandFingerprint::CompleteBudget { .. } => "marketing.budget.complete",
        CommandFingerprint::CancelBudget { .. } => "marketing.budget.cancel",
        CommandFingerprint::ReserveSpend { .. } => "marketing.spend.reserve",
        CommandFingerprint::ReleaseSpend { .. } => "marketing.spend.release",
    }
}

fn aggregate_id(id: Uuid) -> Result<AggregateId, PortError> {
    AggregateId::try_from_uuid(id).map_err(|_| PortError::Internal)
}

fn request_hash(value: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let mut digest = Sha256::new();
    digest.update(b"erpx.marketing.command.v2\0");
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
    receipt: erpx_cqrs::CommandReceipt<MarketingAggregate>,
) -> Result<StoredCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != receipt.response.aggregate_type()
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.id_uuid()
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

    #[test]
    fn fingerprint_targets_are_aggregate_specific() {
        let lead = LeadCandidateId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let budget = CampaignBudgetId::try_from_uuid(Uuid::from_u128(2)).unwrap();
        let lead_target = fingerprint_target(&CommandFingerprint::Reject { lead_id: lead });
        let budget_target =
            fingerprint_target(&CommandFingerprint::StartBudget { budget_id: budget });
        assert_eq!(lead_target.aggregate_type, LEAD_AGGREGATE_TYPE);
        assert_eq!(budget_target.aggregate_type, BUDGET_AGGREGATE_TYPE);
        assert_ne!(lead_target.partition, budget_target.partition);
    }

    #[test]
    fn stable_hash_is_payload_sensitive() {
        let budget = CampaignBudgetId::try_from_uuid(Uuid::from_u128(2)).unwrap();
        let first = request_hash(&CommandFingerprint::StartBudget { budget_id: budget }).unwrap();
        let second = request_hash(&CommandFingerprint::CancelBudget { budget_id: budget }).unwrap();
        assert_ne!(first, second);
    }
}
