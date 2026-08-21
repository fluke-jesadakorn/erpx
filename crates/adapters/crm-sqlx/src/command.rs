use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_crm::{
    CommandUnitOfWork, CrmAggregate, CrmCommit, CrmLead, CrmLeadId, CrmLeadState, CrmOpportunity,
    CrmOpportunityState, OpportunityId, OpportunityStage, PortError, WriteRepository,
    ports::{CommandFingerprint, CommandIdentity, StoredCommit},
};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, IntegrationEvent,
    ItemDescriptorDraft, ItemDescriptorDraftV2, RecordTypeId, StableRecordRef, catalog::ContextId,
    record_policy_id,
};
use erpx_kernel::{AggregateId, AuthorityScope, CommandId, DataClassification, EventId, RecordId};
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
    pool: WritePool,
}
impl CrmWriteRepository {
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
    fn try_from_scope(scope: &AuthorityScope) -> Result<Self, PortError> {
        scope.validate().map_err(|_| PortError::Denied)?;
        Ok(match scope {
            AuthorityScope::Platform => Self {
                mode: "platform",
                holding_id: None,
                company_id: None,
                branch_id: None,
                company_ids: vec![],
                branch_ids: vec![],
            },
            AuthorityScope::Holding { holding_id } => Self {
                mode: "holding",
                holding_id: Some(*holding_id),
                company_id: None,
                branch_id: None,
                company_ids: vec![],
                branch_ids: vec![],
            },
            AuthorityScope::Company {
                holding_id,
                company_id,
            } => Self {
                mode: "company",
                holding_id: Some(*holding_id),
                company_id: Some(*company_id),
                branch_id: None,
                company_ids: vec![],
                branch_ids: vec![],
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
                company_ids: vec![],
                branch_ids: vec![],
            },
            AuthorityScope::Subtree {
                holding_id,
                organization_node_id,
            } => Self {
                mode: "subtree",
                holding_id: Some(*holding_id),
                company_id: None,
                branch_id: None,
                company_ids: vec![*organization_node_id],
                branch_ids: Vec::new(),
            },
        })
    }
}

const AUTHORITY_FILTER: &str = "
 AND ($2='platform' OR ($2='holding' AND holding_id=$3)
   OR ($2='company' AND holding_id=$3 AND company_id=$4)
   OR ($2='branch' AND holding_id=$3 AND company_id=$4 AND branch_id=$5)
   OR ($2='subtree' AND holding_id=$3 AND company_id=ANY($6)
       AND ((branch_id IS NULL AND cardinality($7::uuid[])=0) OR branch_id=ANY($7))))";
#[async_trait]
impl WriteRepository for CrmWriteRepository {
    async fn load_lead(
        &self,
        scope: &AuthorityScope,
        id: CrmLeadId,
    ) -> Result<Option<CrmLead>, PortError> {
        let authority = AuthoritySqlScope::try_from_scope(scope)?;
        let mut transaction = self.pool.begin().await.map_err(map_store_error)?;
        let query = format!(
            "SELECT aggregate_version,holding_id,company_id,branch_id,marketing_candidate_id,
                    campaign_id,audience_member_id,marketing_source_version,customer_id,source,
                    qualification_score_basis_points,accepted_at
             FROM erpx_crm_write.lead_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND lead_id=$1{AUTHORITY_FILTER}"
        );
        let row = sqlx::query(&query)
            .bind(id.as_uuid())
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
        let aggregate = CrmLead::try_rehydrate(CrmLeadState {
            id,
            version: from_i64(
                row.try_get("aggregate_version")
                    .map_err(|_| PortError::Internal)?,
            )?,
            scope: scope_from_row(&row)?,
            marketing_candidate_id: row
                .try_get("marketing_candidate_id")
                .map_err(|_| PortError::Internal)?,
            campaign_id: row
                .try_get("campaign_id")
                .map_err(|_| PortError::Internal)?,
            audience_member_id: row
                .try_get("audience_member_id")
                .map_err(|_| PortError::Internal)?,
            marketing_source_version: from_i64(
                row.try_get("marketing_source_version")
                    .map_err(|_| PortError::Internal)?,
            )?,
            customer_id: row
                .try_get("customer_id")
                .map_err(|_| PortError::Internal)?,
            source: row.try_get("source").map_err(|_| PortError::Internal)?,
            qualification_score_basis_points: u16::try_from(
                row.try_get::<i32, _>("qualification_score_basis_points")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            accepted_at: row
                .try_get("accepted_at")
                .map_err(|_| PortError::Internal)?,
        })
        .map_err(|_| PortError::Internal)?;
        Ok(Some(aggregate))
    }
    async fn load_opportunity(
        &self,
        scope: &AuthorityScope,
        id: OpportunityId,
    ) -> Result<Option<CrmOpportunity>, PortError> {
        let authority = AuthoritySqlScope::try_from_scope(scope)?;
        let mut transaction = self.pool.begin().await.map_err(map_store_error)?;
        let query = format!(
            "SELECT aggregate_version,holding_id,company_id,branch_id,accepted_lead_id,
                    customer_id,opportunity_name,expected_value_minor,currency,opportunity_stage,
                    account_id,owner_ref,pipeline_category,probability_basis_points,
                    expected_close_on,originating_campaign_id,created_at,updated_at
             FROM erpx_crm_write.opportunity_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND opportunity_id=$1{AUTHORITY_FILTER}"
        );
        let row = sqlx::query(&query)
            .bind(id.as_uuid())
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
        let account_id: Option<Uuid> =
            row.try_get("account_id").map_err(|_| PortError::Internal)?;
        let campaign_id: Option<Uuid> = row
            .try_get("originating_campaign_id")
            .map_err(|_| PortError::Internal)?;
        let probability: Option<i32> = row
            .try_get("probability_basis_points")
            .map_err(|_| PortError::Internal)?;
        let aggregate = CrmOpportunity::try_rehydrate(CrmOpportunityState {
            id,
            version: from_i64(
                row.try_get("aggregate_version")
                    .map_err(|_| PortError::Internal)?,
            )?,
            scope: scope_from_row(&row)?,
            accepted_lead_id: CrmLeadId::try_from_uuid(
                row.try_get("accepted_lead_id")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            customer_id: row
                .try_get("customer_id")
                .map_err(|_| PortError::Internal)?,
            name: row
                .try_get("opportunity_name")
                .map_err(|_| PortError::Internal)?,
            expected_value_minor: row
                .try_get("expected_value_minor")
                .map_err(|_| PortError::Internal)?,
            currency: row.try_get("currency").map_err(|_| PortError::Internal)?,
            stage: parse_stage(
                &row.try_get::<String, _>("opportunity_stage")
                    .map_err(|_| PortError::Internal)?,
            )?,
            account: account_id
                .map(|id| {
                    erpx_integration_contracts::crm::CrmAccountRef::try_from_uuid(id)
                        .map_err(|_| PortError::Internal)
                })
                .transpose()?,
            owner: row.try_get("owner_ref").map_err(|_| PortError::Internal)?,
            pipeline_category: row
                .try_get::<Option<String>, _>("pipeline_category")
                .map_err(|_| PortError::Internal)?
                .map(|value| parse_pipeline_category(&value))
                .transpose()?,
            probability_basis_points: probability
                .map(|value| u16::try_from(value).map_err(|_| PortError::Internal))
                .transpose()?,
            expected_close_on: row
                .try_get("expected_close_on")
                .map_err(|_| PortError::Internal)?,
            originating_campaign: campaign_id
                .map(|id| {
                    erpx_integration_contracts::marketing::CampaignRef::try_from_uuid(id)
                        .map_err(|_| PortError::Internal)
                })
                .transpose()?,
            created_at: row.try_get("created_at").map_err(|_| PortError::Internal)?,
            updated_at: row.try_get("updated_at").map_err(|_| PortError::Internal)?,
        })
        .map_err(|_| PortError::Internal)?;
        Ok(Some(aggregate))
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
            payload: CrmOutboxPayload::Directory(crm_directory_draft(&request.aggregate)?),
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
            active_scope: aggregate.scope().clone(),
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: kind.into(),
            aggregate_id: AggregateId::try_from_uuid(id).map_err(|_| PortError::Internal)?,
            expected_version: request.expected_version,
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
            .commit_normalized(command, &guard)
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

fn crm_directory_draft(aggregate: &CrmAggregate) -> Result<ItemDescriptorDraftV2, PortError> {
    let (record_type, lifecycle_state, display_label) = match aggregate {
        CrmAggregate::Lead(value) => (
            "crm.lead",
            "accepted".to_owned(),
            format!("{} {}", value.source(), value.marketing_candidate_id()),
        ),
        CrmAggregate::Opportunity(value) => (
            "crm.opportunity",
            stage_name(value.stage()).to_owned(),
            value.name().to_owned(),
        ),
    };
    let record_id =
        RecordId::try_from_uuid(aggregate.id_uuid()).map_err(|_| PortError::Internal)?;
    let record = StableRecordRef::try_new(
        record_id,
        ContextId::Crm,
        RecordTypeId::try_new(record_type).map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    let value = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
        record,
        scope: aggregate.scope().clone(),
        lifecycle_state,
        classification: DataClassification::Confidential,
        descriptor_version: aggregate.version(),
        authorized_search_text: display_label.clone(),
    };
    ItemDescriptorDraftV2::try_new(value, display_label).map_err(|_| PortError::Internal)
}

struct CrmAtomicGuard {
    aggregate: CrmAggregate,
}
#[async_trait]
impl AtomicCommandGuard for CrmAtomicGuard {
    async fn authoritative_version(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        let (table, id) = match &self.aggregate {
            CrmAggregate::Lead(value) => ("lead_authority", value.id().as_uuid()),
            CrmAggregate::Opportunity(value) => ("opportunity_authority", value.id().as_uuid()),
        };
        let sql = format!(
            "SELECT aggregate_version FROM erpx_crm_write.{table}
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND {}=$1 FOR UPDATE",
            if table == "lead_authority" {
                "lead_id"
            } else {
                "opportunity_id"
            }
        );
        let current: Option<i64> = sqlx::query_scalar(&sql)
            .bind(id)
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
        match &self.aggregate {
            CrmAggregate::Lead(lead) => {
                let row=sqlx::query("SELECT lead_id,aggregate_version FROM erpx_crm_write.lead_authority WHERE holding_id=erpx_cqrs_write.current_holding_id() AND (lead_id=$1 OR marketing_candidate_id=$2) ORDER BY lead_id FOR UPDATE").bind(lead.id().as_uuid()).bind(lead.marketing_candidate_id()).fetch_optional(&mut **transaction).await?;
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
                let current:Option<i64>=sqlx::query_scalar("SELECT aggregate_version FROM erpx_crm_write.opportunity_authority WHERE holding_id=erpx_cqrs_write.current_holding_id() AND opportunity_id=$1 FOR UPDATE").bind(opportunity.id().as_uuid()).fetch_optional(&mut **transaction).await?;
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
                let lead=sqlx::query("SELECT scope_key,customer_id FROM erpx_crm_write.lead_authority WHERE holding_id=erpx_cqrs_write.current_holding_id() AND lead_id=$1 FOR SHARE").bind(opportunity.accepted_lead_id().as_uuid()).fetch_optional(&mut **transaction).await?;
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
                let (holding_id, company_id, branch_id) = exact_scope_columns(lead.scope())?;
                let result=sqlx::query("INSERT INTO erpx_crm_write.lead_authority (holding_id,lead_id,aggregate_version,scope_key,holding_id,company_id,branch_id,marketing_candidate_id,campaign_id,audience_member_id,marketing_source_version,customer_id,source,qualification_score_basis_points,accepted_at) VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) ON CONFLICT (holding_id,lead_id) DO UPDATE SET aggregate_version=EXCLUDED.aggregate_version,scope_key=EXCLUDED.scope_key,holding_id=EXCLUDED.holding_id,company_id=EXCLUDED.company_id,branch_id=EXCLUDED.branch_id,marketing_candidate_id=EXCLUDED.marketing_candidate_id,campaign_id=EXCLUDED.campaign_id,audience_member_id=EXCLUDED.audience_member_id,marketing_source_version=EXCLUDED.marketing_source_version,customer_id=EXCLUDED.customer_id,source=EXCLUDED.source,qualification_score_basis_points=EXCLUDED.qualification_score_basis_points,accepted_at=EXCLUDED.accepted_at WHERE erpx_crm_write.lead_authority.holding_id=erpx_cqrs_write.current_holding_id() AND erpx_crm_write.lead_authority.aggregate_version + 1=EXCLUDED.aggregate_version").bind(lead.id().as_uuid()).bind(to_i64(context.aggregate_version)?).bind(scope_key(lead.scope())?).bind(holding_id).bind(company_id).bind(branch_id).bind(lead.marketing_candidate_id()).bind(lead.campaign_id()).bind(lead.audience_member_id()).bind(to_i64(lead.marketing_source_version())?).bind(lead.customer_id()).bind(lead.source()).bind(i32::from(lead.qualification_score_basis_points())).bind(lead.accepted_at()).execute(&mut **transaction).await?;
                ensure_one(
                    result.rows_affected(),
                    "CRM lead normalized version mismatch",
                )?;
            }
            CrmAggregate::Opportunity(value) => {
                let (holding_id, company_id, branch_id) = exact_scope_columns(value.scope())?;
                let result=sqlx::query("INSERT INTO erpx_crm_write.opportunity_authority (holding_id,opportunity_id,aggregate_version,scope_key,holding_id,company_id,branch_id,accepted_lead_id,customer_id,opportunity_name,expected_value_minor,currency,opportunity_stage,account_id,owner_ref,pipeline_category,probability_basis_points,expected_close_on,originating_campaign_id,created_at,updated_at) VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20) ON CONFLICT (holding_id,opportunity_id) DO UPDATE SET aggregate_version=EXCLUDED.aggregate_version,scope_key=EXCLUDED.scope_key,holding_id=EXCLUDED.holding_id,company_id=EXCLUDED.company_id,branch_id=EXCLUDED.branch_id,accepted_lead_id=EXCLUDED.accepted_lead_id,customer_id=EXCLUDED.customer_id,opportunity_name=EXCLUDED.opportunity_name,expected_value_minor=EXCLUDED.expected_value_minor,currency=EXCLUDED.currency,opportunity_stage=EXCLUDED.opportunity_stage,account_id=EXCLUDED.account_id,owner_ref=EXCLUDED.owner_ref,pipeline_category=EXCLUDED.pipeline_category,probability_basis_points=EXCLUDED.probability_basis_points,expected_close_on=EXCLUDED.expected_close_on,originating_campaign_id=EXCLUDED.originating_campaign_id,created_at=EXCLUDED.created_at,updated_at=EXCLUDED.updated_at WHERE erpx_crm_write.opportunity_authority.holding_id=erpx_cqrs_write.current_holding_id() AND erpx_crm_write.opportunity_authority.aggregate_version + 1=EXCLUDED.aggregate_version").bind(value.id().as_uuid()).bind(to_i64(context.aggregate_version)?).bind(scope_key(value.scope())?).bind(holding_id).bind(company_id).bind(branch_id).bind(value.accepted_lead_id().as_uuid()).bind(value.customer_id()).bind(value.name()).bind(value.expected_value_minor()).bind(value.currency()).bind(stage_name(value.stage())).bind(value.account().map(|v|v.as_uuid())).bind(value.owner()).bind(value.pipeline_category().map(pipeline_category_name)).bind(value.probability_basis_points().map(i32::from)).bind(value.expected_close_on()).bind(value.originating_campaign().map(|v|v.as_uuid())).bind(value.created_at()).bind(value.updated_at()).execute(&mut **transaction).await?;
                ensure_one(
                    result.rows_affected(),
                    "CRM opportunity normalized version mismatch",
                )?;
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
    let next = request
        .expected_version
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(PortError::Internal)?;
    if request.aggregate.validate().is_err()
        || id != aggregate_partition(&request.aggregate)
        || kind != request.aggregate.aggregate_type()
        || !request.authority_scope.contains(request.aggregate.scope())
        || request.aggregate.version() != next
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
    digest.update(b"erpx.crm.command\0");
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
pub(crate) fn scope_key(scope: &AuthorityScope) -> Result<String, CqrsSqlxError> {
    match scope {
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => Ok(format!("h:{holding_id}/c:{company_id}")),
        AuthorityScope::Branch {
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
fn from_i64(value: i64) -> Result<u64, PortError> {
    u64::try_from(value).map_err(|_| PortError::Internal)
}
fn from_i64_store(value: i64) -> Result<u64, CqrsSqlxError> {
    u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}
fn exact_scope_columns(
    scope: &AuthorityScope,
) -> Result<(Uuid, Uuid, Option<Uuid>), CqrsSqlxError> {
    match scope {
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => Ok((*holding_id, *company_id, None)),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok((*holding_id, *company_id, Some(*branch_id))),
        _ => Err(CqrsSqlxError::InvalidInput("CRM scope must be exact")),
    }
}
fn scope_from_row(row: &sqlx::postgres::PgRow) -> Result<AuthorityScope, PortError> {
    let holding_id = row.try_get("holding_id").map_err(|_| PortError::Internal)?;
    let company_id = row.try_get("company_id").map_err(|_| PortError::Internal)?;
    let branch_id: Option<Uuid> = row.try_get("branch_id").map_err(|_| PortError::Internal)?;
    Ok(branch_id.map_or(
        AuthorityScope::Company {
            holding_id,
            company_id,
        },
        |branch_id| AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        },
    ))
}
fn parse_stage(value: &str) -> Result<OpportunityStage, PortError> {
    match value {
        "qualified" => Ok(OpportunityStage::Qualified),
        "discovery" => Ok(OpportunityStage::Discovery),
        "proposal" => Ok(OpportunityStage::Proposal),
        "negotiation" => Ok(OpportunityStage::Negotiation),
        "won" => Ok(OpportunityStage::Won),
        "lost" => Ok(OpportunityStage::Lost),
        _ => Err(PortError::Internal),
    }
}
fn parse_pipeline_category(
    value: &str,
) -> Result<erpx_integration_contracts::crm::PipelineCategory, PortError> {
    match value {
        "pipeline" => Ok(erpx_integration_contracts::crm::PipelineCategory::Pipeline),
        "best_case" => Ok(erpx_integration_contracts::crm::PipelineCategory::BestCase),
        "commit" => Ok(erpx_integration_contracts::crm::PipelineCategory::Commit),
        "closed" => Ok(erpx_integration_contracts::crm::PipelineCategory::Closed),
        _ => Err(PortError::Internal),
    }
}
fn ensure_one(rows: u64, message: &'static str) -> Result<(), CqrsSqlxError> {
    if rows == 1 {
        Ok(())
    } else {
        Err(CqrsSqlxError::GuardRejected(message))
    }
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
