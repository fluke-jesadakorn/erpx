use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_crm::{
    CommandUnitOfWork, CrmAggregate, CrmCommit, CrmLead, CrmLeadId, CrmOpportunity, OpportunityId,
    PortError, WriteRepository,
    ports::{CommandFingerprint, CommandIdentity, StoredCommit},
};
use erpx_integration_contracts::IntegrationEvent;
use erpx_kernel::{AggregateId, CommandId, EventId, TenantScope};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    CONTEXT_NAME, EVENT_VERSION, LEAD_AGGREGATE_TYPE, LEAD_SNAPSHOT_EVENT,
    OPPORTUNITY_AGGREGATE_TYPE, OPPORTUNITY_SNAPSHOT_EVENT, lead_partition, opportunity_partition,
    projection::{CrmOutboxPayload, CrmSnapshot, pipeline_category_name, stage_name},
};

#[derive(Debug, Clone)]
pub struct CrmWriteRepository {
    store: CommandStore,
}
impl CrmWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
    async fn load_aggregate(
        &self,
        scope: &TenantScope,
        partition: &str,
        kind: &str,
        id: Uuid,
    ) -> Result<Option<CrmAggregate>, PortError> {
        let aggregate_id = AggregateId::try_from_uuid(id).map_err(|_| PortError::Internal)?;
        let snapshot = self
            .store
            .load_snapshot::<CrmAggregate>(CONTEXT_NAME, partition, kind, aggregate_id)
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        if snapshot.version != snapshot.state.version()
            || snapshot.state.id_uuid() != id
            || snapshot.state.aggregate_type() != kind
            || snapshot.state.validate().is_err()
        {
            return Err(PortError::Internal);
        }
        if !scope.contains(snapshot.state.scope()) {
            return Err(PortError::Denied);
        }
        Ok(Some(snapshot.state))
    }
}
#[async_trait]
impl WriteRepository for CrmWriteRepository {
    async fn load_lead(
        &self,
        scope: &TenantScope,
        id: CrmLeadId,
    ) -> Result<Option<CrmLead>, PortError> {
        match self
            .load_aggregate(
                scope,
                &lead_partition(id),
                LEAD_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(CrmAggregate::Lead(value)) => Ok(Some(value)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }
    async fn load_opportunity(
        &self,
        scope: &TenantScope,
        id: OpportunityId,
    ) -> Result<Option<CrmOpportunity>, PortError> {
        match self
            .load_aggregate(
                scope,
                &opportunity_partition(id),
                OPPORTUNITY_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(CrmAggregate::Opportunity(value)) => Ok(Some(value)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CrmCommandAdapter {
    store: CommandStore,
}
impl CrmCommandAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}
#[async_trait]
impl CommandUnitOfWork for CrmCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        let (partition, kind) = fingerprint_target(&identity.fingerprint);
        self.store
            .find_receipt::<CrmAggregate>(
                CONTEXT_NAME,
                &partition,
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?
            .map(|value| receipt_to_stored(value, kind))
            .transpose()
    }
    async fn commit(&self, request: CrmCommit) -> Result<StoredCommit, PortError> {
        validate_commit(&request)?;
        let id = request.aggregate.id_uuid();
        let kind = request.aggregate.aggregate_type();
        let partition = aggregate_partition(&request.aggregate);
        let snapshot_event = match &request.aggregate {
            CrmAggregate::Lead(_) => LEAD_SNAPSHOT_EVENT,
            CrmAggregate::Opportunity(_) => OPPORTUNITY_SNAPSHOT_EVENT,
        };
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
                partition: partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: CrmOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, snapshot_event, events.len()),
            event_type: snapshot_event.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: CrmOutboxPayload::Snapshot(CrmSnapshot::from(&request.aggregate)),
        });
        let guard = CrmAtomicGuard {
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
            aggregate_type: kind.into(),
            aggregate_id: AggregateId::try_from_uuid(id).map_err(|_| PortError::Internal)?,
            expected_version: request.expected_version,
            aggregate_state: serde_json::to_value(&aggregate).map_err(|_| PortError::Internal)?,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"crm-audit-v1"),
                action: audit_action(&request.identity.fingerprint).into(),
                occurred_at,
                details: json!({"aggregate_id":id,"aggregate_type":kind,"expected_version":request.expected_version}),
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
        let mut stored = receipt_to_stored(receipt, kind)?;
        stored.replayed = replayed;
        Ok(stored)
    }
}

struct CrmAtomicGuard {
    aggregate: CrmAggregate,
}
#[async_trait]
impl AtomicCommandGuard for CrmAtomicGuard {
    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        match &self.aggregate {
            CrmAggregate::Lead(lead) => {
                let row=sqlx::query("SELECT lead_id,aggregate_version FROM erpx_crm_write.lead_guards WHERE lead_id=$1 OR marketing_candidate_id=$2 ORDER BY lead_id FOR UPDATE").bind(lead.id().as_uuid()).bind(lead.marketing_candidate_id()).fetch_optional(&mut **transaction).await?;
                match (context.expected_version, row) {
                    (None, Some(row)) => {
                        let existing: Uuid = row.try_get("lead_id")?;
                        if existing != lead.id().as_uuid() {
                            Err(CqrsSqlxError::GuardRejected(
                                "Marketing candidate was already accepted",
                            ))
                        } else {
                            Err(CqrsSqlxError::GuardRejected("CRM lead already exists"))
                        }
                    }
                    (Some(_), None) => {
                        Err(CqrsSqlxError::GuardRejected("CRM lead guard is missing"))
                    }
                    _ => Ok(()),
                }
            }
            CrmAggregate::Opportunity(opportunity) => {
                let current:Option<i64>=sqlx::query_scalar("SELECT aggregate_version FROM erpx_crm_write.opportunity_guards WHERE opportunity_id=$1 FOR UPDATE").bind(opportunity.id().as_uuid()).fetch_optional(&mut **transaction).await?;
                match (context.expected_version, current) {
                    (None, Some(_)) => {
                        return Err(CqrsSqlxError::GuardRejected(
                            "CRM opportunity already exists",
                        ));
                    }
                    (Some(_), None) => {
                        return Err(CqrsSqlxError::GuardRejected(
                            "CRM opportunity guard is missing",
                        ));
                    }
                    _ => {}
                }
                let lead=sqlx::query("SELECT scope_key,customer_id FROM erpx_crm_write.lead_guards WHERE lead_id=$1 FOR SHARE").bind(opportunity.accepted_lead_id().as_uuid()).fetch_optional(&mut **transaction).await?;
                let Some(lead) = lead else {
                    return Err(CqrsSqlxError::GuardRejected(
                        "accepted CRM lead guard is missing",
                    ));
                };
                let lead_scope: String = lead.try_get("scope_key")?;
                let customer: Option<Uuid> = lead.try_get("customer_id")?;
                if lead_scope != scope_key(opportunity.scope())?
                    || customer.is_some_and(|id| id != opportunity.customer_id())
                {
                    return Err(CqrsSqlxError::GuardRejected(
                        "opportunity does not match accepted lead",
                    ));
                }
                Ok(())
            }
        }
    }
    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        if context.context != CONTEXT_NAME
            || context.aggregate_type != self.aggregate.aggregate_type()
            || context.aggregate_id.as_uuid() != self.aggregate.id_uuid()
            || context.partition != aggregate_partition(&self.aggregate)
            || context.aggregate_version != self.aggregate.version()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "CRM applied guard metadata mismatch",
            ));
        }
        match &self.aggregate {
            CrmAggregate::Lead(lead) => {
                sqlx::query("INSERT INTO erpx_crm_write.lead_guards (lead_id,scope_key,marketing_candidate_id,customer_id,aggregate_version,accepted_at) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (lead_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,marketing_candidate_id=EXCLUDED.marketing_candidate_id,customer_id=EXCLUDED.customer_id,aggregate_version=EXCLUDED.aggregate_version,accepted_at=EXCLUDED.accepted_at").bind(lead.id().as_uuid()).bind(scope_key(lead.scope())?).bind(lead.marketing_candidate_id()).bind(lead.customer_id()).bind(to_i64(context.aggregate_version)?).bind(lead.accepted_at()).execute(&mut **transaction).await?;
            }
            CrmAggregate::Opportunity(value) => {
                sqlx::query("INSERT INTO erpx_crm_write.opportunity_guards (opportunity_id,scope_key,accepted_lead_id,customer_id,opportunity_stage,account_id,owner_ref,pipeline_category,probability_basis_points,expected_close_on,originating_campaign_id,aggregate_version,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT (opportunity_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,accepted_lead_id=EXCLUDED.accepted_lead_id,customer_id=EXCLUDED.customer_id,opportunity_stage=EXCLUDED.opportunity_stage,account_id=EXCLUDED.account_id,owner_ref=EXCLUDED.owner_ref,pipeline_category=EXCLUDED.pipeline_category,probability_basis_points=EXCLUDED.probability_basis_points,expected_close_on=EXCLUDED.expected_close_on,originating_campaign_id=EXCLUDED.originating_campaign_id,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at").bind(value.id().as_uuid()).bind(scope_key(value.scope())?).bind(value.accepted_lead_id().as_uuid()).bind(value.customer_id()).bind(stage_name(value.stage())).bind(value.account().map(|v|v.as_uuid())).bind(value.owner()).bind(value.pipeline_category().map(pipeline_category_name)).bind(value.probability_basis_points().map(i32::from)).bind(value.expected_close_on()).bind(value.originating_campaign().map(|v|v.as_uuid())).bind(to_i64(context.aggregate_version)?).bind(value.updated_at()).execute(&mut **transaction).await?;
            }
        }
        Ok(())
    }
}

fn validate_guard_context(
    value: &CrmAggregate,
    context: &GuardCommandContext<'_>,
) -> Result<(), CqrsSqlxError> {
    if context.context != CONTEXT_NAME
        || context.aggregate_type != value.aggregate_type()
        || context.aggregate_id.as_uuid() != value.id_uuid()
        || context.partition != aggregate_partition(value)
    {
        Err(CqrsSqlxError::GuardRejected("CRM guard metadata mismatch"))
    } else {
        Ok(())
    }
}
fn validate_commit(request: &CrmCommit) -> Result<(), PortError> {
    let (id, kind) = fingerprint_target(&request.identity.fingerprint);
    if request.aggregate.validate().is_err()
        || id != aggregate_partition(&request.aggregate)
        || kind != request.aggregate.aggregate_type()
        || !request.authority_scope.contains(request.aggregate.scope())
    {
        Err(PortError::Internal)
    } else {
        Ok(())
    }
}
fn fingerprint_target(value: &CommandFingerprint) -> (String, &'static str) {
    match value {
        CommandFingerprint::AcceptLead { lead_id, .. } => {
            (lead_partition(*lead_id), LEAD_AGGREGATE_TYPE)
        }
        CommandFingerprint::CreateOpportunity { opportunity_id, .. }
        | CommandFingerprint::TransitionOpportunity { opportunity_id, .. }
        | CommandFingerprint::UpdateOpportunityForecast { opportunity_id, .. } => (
            opportunity_partition(*opportunity_id),
            OPPORTUNITY_AGGREGATE_TYPE,
        ),
    }
}
fn aggregate_partition(value: &CrmAggregate) -> String {
    match value {
        CrmAggregate::Lead(v) => lead_partition(v.id()),
        CrmAggregate::Opportunity(v) => opportunity_partition(v.id()),
    }
}
fn aggregate_updated_at(value: &CrmAggregate) -> chrono::DateTime<chrono::Utc> {
    match value {
        CrmAggregate::Lead(v) => v.accepted_at(),
        CrmAggregate::Opportunity(v) => v.updated_at(),
    }
}
fn audit_action(value: &CommandFingerprint) -> &'static str {
    match value {
        CommandFingerprint::AcceptLead { .. } => "crm.lead.accept",
        CommandFingerprint::CreateOpportunity { .. } => "crm.opportunity.create",
        CommandFingerprint::TransitionOpportunity { .. } => "crm.opportunity.transition",
        CommandFingerprint::UpdateOpportunityForecast { .. } => "crm.opportunity.forecast.update",
    }
}
fn request_hash(value: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let mut digest = Sha256::new();
    digest.update(b"erpx.crm.command.v1\0");
    digest.update(serde_json::to_vec(value).map_err(|_| PortError::Internal)?);
    Ok(RequestHash::new(digest.finalize().into()))
}
fn deterministic_event_id(command: CommandId, kind: &str, index: usize) -> EventId {
    let mut input = kind.as_bytes().to_vec();
    input.extend_from_slice(&index.to_be_bytes());
    EventId::try_from_uuid(deterministic_uuid(command.as_uuid(), &input)).expect("derived id")
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
        _ => Err(CqrsSqlxError::InvalidInput("CRM scope must be exact")),
    }
}
fn to_i64(value: u64) -> Result<i64, CqrsSqlxError> {
    i64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}
fn receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<CrmAggregate>,
    kind: &str,
) -> Result<StoredCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != kind
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
    fn fingerprint_routes_to_owner_aggregate() {
        let lead = CrmLeadId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        assert_eq!(
            fingerprint_target(&CommandFingerprint::AcceptLead {
                lead_id: lead,
                marketing_candidate_id: Uuid::from_u128(2)
            })
            .1,
            LEAD_AGGREGATE_TYPE
        );
    }
    #[test]
    fn stable_hash_is_payload_sensitive() {
        let id = OpportunityId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let a = CommandFingerprint::TransitionOpportunity {
            opportunity_id: id,
            stage: erpx_crm::OpportunityStage::Won,
        };
        let b = CommandFingerprint::TransitionOpportunity {
            opportunity_id: id,
            stage: erpx_crm::OpportunityStage::Lost,
        };
        assert_ne!(request_hash(&a).unwrap(), request_hash(&b).unwrap());
    }
}
