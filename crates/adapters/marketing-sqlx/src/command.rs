use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::{
    BoundedContext, CurrencyCode, DimensionSet, IntegrationEvent, MoneyAmount, RecordRef,
    ReferenceCode, assets, legal, marketing, operations, organization, people_payroll,
};
use erpx_kernel::{AggregateId, AuthorityScope, CommandId, EventId};
use erpx_marketing::{
    BudgetAllocation, BudgetAllocationId, CampaignBudget, CampaignBudgetId, CampaignBudgetState,
    CampaignBudgetStatus, CampaignId, CommandUnitOfWork, LeadCandidate, LeadCandidateId,
    LeadCandidateState, LeadCandidateStatus, MarketingAggregate, MarketingCommit, PortError,
    SpendRequestId, SpendReservation, SpendReservationStatus, WriteRepository,
    ports::{CommandFingerprint, CommandIdentity, StoredCommit},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    BUDGET_AGGREGATE_TYPE, BUDGET_SNAPSHOT_EVENT_TYPE, CONTEXT_NAME, EVENT_VERSION,
    LEAD_AGGREGATE_TYPE, LEAD_SNAPSHOT_EVENT_TYPE, budget_partition, lead_partition,
    projection::{
        MarketingOutboxPayload, MarketingSnapshot, budget_period_name, budget_status_name,
        lead_status_name,
    },
};

#[derive(Debug, Clone)]
pub struct MarketingWriteRepository {
    pool: WritePool,
}

impl MarketingWriteRepository {
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
impl WriteRepository for MarketingWriteRepository {
    async fn load_lead(
        &self,
        authority_scope: &AuthorityScope,
        id: LeadCandidateId,
    ) -> Result<Option<LeadCandidate>, PortError> {
        let authority = AuthoritySqlScope::try_from_scope(authority_scope)?;
        let mut transaction = self.pool.begin().await.map_err(map_store_error)?;
        let query = format!(
            "SELECT aggregate_version,holding_id,company_id,branch_id,campaign_id,
                    audience_member_id,source,existing_customer_id,
                    qualification_score_basis_points,lead_status,engagement_count,
                    created_at,updated_at
             FROM erpx_marketing_write.lead_candidate_authority
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
        let Some(row) = row else {
            return Ok(None);
        };
        let scope = exact_scope_from_row(&row)?;
        let lead = LeadCandidate::try_rehydrate(LeadCandidateState {
            id,
            version: row_u64(&row, "aggregate_version")?,
            scope,
            campaign_id: CampaignId::try_from_uuid(
                row.try_get("campaign_id")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            audience_member_id: erpx_marketing::AudienceMemberId::try_from_uuid(
                row.try_get("audience_member_id")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            source: row.try_get("source").map_err(|_| PortError::Internal)?,
            existing_customer_id: row
                .try_get("existing_customer_id")
                .map_err(|_| PortError::Internal)?,
            qualification_score_basis_points: row
                .try_get::<Option<i32>, _>("qualification_score_basis_points")
                .map_err(|_| PortError::Internal)?
                .map(u16::try_from)
                .transpose()
                .map_err(|_| PortError::Internal)?,
            status: parse_lead_status(
                row.try_get("lead_status")
                    .map_err(|_| PortError::Internal)?,
            )?,
            engagement_count: row_u64(&row, "engagement_count")?,
            created_at: row.try_get("created_at").map_err(|_| PortError::Internal)?,
            updated_at: row.try_get("updated_at").map_err(|_| PortError::Internal)?,
        })
        .map_err(|_| PortError::Internal)?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Unavailable)?;
        Ok(Some(lead))
    }

    async fn load_budget(
        &self,
        authority_scope: &AuthorityScope,
        id: CampaignBudgetId,
    ) -> Result<Option<CampaignBudget>, PortError> {
        let authority = AuthoritySqlScope::try_from_scope(authority_scope)?;
        let mut transaction = self.pool.begin().await.map_err(map_store_error)?;
        let query = format!(
            "SELECT aggregate_version,holding_id,company_id,branch_id,campaign_id,budget_code,
                    budget_name,budget_period,currency,requested_minor,approved_minor,
                    dimension_business_unit_id,dimension_cost_center_id,dimension_location_id,
                    dimension_project_id,dimension_channel_id,dimension_party_id,
                    dimension_employee_id,dimension_asset_id,dimension_legal_matter_id,
                    dimension_intercompany_partner_id,approval_request_id,
                    approval_subject_version,approval_requested_by,approval_requested_at,
                    budget_status,created_at,updated_at
             FROM erpx_marketing_write.campaign_budget_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND budget_id=$1{AUTHORITY_FILTER}"
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
        let Some(row) = row else {
            return Ok(None);
        };
        let scope = exact_scope_from_row(&row)?;
        let campaign_id = CampaignId::try_from_uuid(
            row.try_get("campaign_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?;
        let currency = CurrencyCode::try_new(
            row.try_get::<String, _>("currency")
                .map_err(|_| PortError::Internal)?
                .as_str(),
        )
        .map_err(|_| PortError::Internal)?;
        let allocations = load_budget_allocations(&mut transaction, id, currency).await?;
        let reservations = load_spend_reservations(&mut transaction, id, currency).await?;
        let company_id = scope_company_id(&scope)?;
        let dimensions = dimensions_from_row(&row, company_id, campaign_id)?;
        let approval_request_id: Option<Uuid> = row
            .try_get("approval_request_id")
            .map_err(|_| PortError::Internal)?;
        let approval_subject_version: Option<i64> = row
            .try_get("approval_subject_version")
            .map_err(|_| PortError::Internal)?;
        let approval_requested_by: Option<String> = row
            .try_get("approval_requested_by")
            .map_err(|_| PortError::Internal)?;
        let approval_requested_at = row
            .try_get("approval_requested_at")
            .map_err(|_| PortError::Internal)?;
        let approval_request = approval_request_id
            .map(operations::ApprovalRequestRef::try_from_uuid)
            .transpose()
            .map_err(|_| PortError::Internal)?;
        let approval_subject = approval_subject_version
            .map(|version| {
                RecordRef::try_new(
                    BoundedContext::Marketing,
                    "campaign_budget",
                    id.as_uuid(),
                    u64::try_from(version).map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)
            })
            .transpose()?;
        let approval_requested_by = approval_requested_by
            .map(ReferenceCode::try_new)
            .transpose()
            .map_err(|_| PortError::Internal)?;
        let requested_minor: i64 = row
            .try_get("requested_minor")
            .map_err(|_| PortError::Internal)?;
        let approved_minor: Option<i64> = row
            .try_get("approved_minor")
            .map_err(|_| PortError::Internal)?;
        let budget = CampaignBudget::try_rehydrate(CampaignBudgetState {
            id,
            version: row_u64(&row, "aggregate_version")?,
            scope,
            campaign_id,
            code: row
                .try_get("budget_code")
                .map_err(|_| PortError::Internal)?,
            name: row
                .try_get("budget_name")
                .map_err(|_| PortError::Internal)?,
            period: parse_budget_period(
                row.try_get("budget_period")
                    .map_err(|_| PortError::Internal)?,
            )?,
            requested_amount: MoneyAmount::new(requested_minor, currency),
            approved_amount: approved_minor.map(|amount| MoneyAmount::new(amount, currency)),
            dimensions,
            allocations,
            reservations,
            approval_request,
            approval_subject,
            approval_requested_by,
            approval_requested_at,
            status: parse_budget_status(
                row.try_get("budget_status")
                    .map_err(|_| PortError::Internal)?,
            )?,
            created_at: row.try_get("created_at").map_err(|_| PortError::Internal)?,
            updated_at: row.try_get("updated_at").map_err(|_| PortError::Internal)?,
        })
        .map_err(|_| PortError::Internal)?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Unavailable)?;
        Ok(Some(budget))
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
            active_scope: aggregate.scope().clone(),
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: target.aggregate_type.into(),
            aggregate_id: aggregate_id(target.id)?,
            expected_version: request.expected_version,
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
            .commit_normalized(command, &guard)
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
    async fn authoritative_version(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        let (table, id_column, id) = match &self.aggregate {
            MarketingAggregate::LeadCandidate(value) => {
                ("lead_candidate_authority", "lead_id", value.id().as_uuid())
            }
            MarketingAggregate::CampaignBudget(value) => (
                "campaign_budget_authority",
                "budget_id",
                value.id().as_uuid(),
            ),
        };
        let query = format!(
            "SELECT aggregate_version FROM erpx_marketing_write.{table}
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND {id_column}=$1 FOR UPDATE"
        );
        let current: Option<i64> = sqlx::query_scalar(&query)
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
        if let MarketingAggregate::CampaignBudget(value) = &self.aggregate {
            let allocation_rows = sqlx::query(
                "SELECT allocation_id FROM erpx_marketing_write.budget_allocation_authority
                 WHERE holding_id=erpx_cqrs_write.current_holding_id() AND budget_id=$1
                 FOR UPDATE",
            )
            .bind(value.id().as_uuid())
            .fetch_all(&mut **transaction)
            .await?;
            if allocation_rows.iter().any(|row| {
                row.try_get::<Uuid, _>("allocation_id")
                    .ok()
                    .and_then(|id| BudgetAllocationId::try_from_uuid(id).ok())
                    .is_none_or(|id| {
                        !value
                            .allocations()
                            .iter()
                            .any(|allocation| allocation.allocation_id == id)
                    })
            }) {
                return Err(CqrsSqlxError::GuardRejected(
                    "Marketing budget allocation history cannot be removed",
                ));
            }
            let rows = sqlx::query(
                "SELECT request_id FROM erpx_marketing_write.spend_reservation_authority
                 WHERE holding_id=erpx_cqrs_write.current_holding_id() AND budget_id=$1
                 FOR UPDATE",
            )
            .bind(value.id().as_uuid())
            .fetch_all(&mut **transaction)
            .await?;
            if rows.iter().any(|row| {
                row.try_get::<Uuid, _>("request_id")
                    .ok()
                    .and_then(|id| SpendRequestId::try_from_uuid(id).ok())
                    .is_none_or(|id| !value.reservations().contains_key(&id))
            }) {
                return Err(CqrsSqlxError::GuardRejected(
                    "Marketing spend reservation history cannot be removed",
                ));
            }
        }
        Ok(())
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_applied_context(&self.aggregate, context)?;
        match &self.aggregate {
            MarketingAggregate::LeadCandidate(value) => {
                let (holding_id, company_id, branch_id) = exact_scope_ids(value.scope())?;
                let result = sqlx::query(
                    "INSERT INTO erpx_marketing_write.lead_candidate_authority
                        (holding_id,lead_id,aggregate_version,holding_id,company_id,branch_id,
                         campaign_id,audience_member_id,source,existing_customer_id,
                         qualification_score_basis_points,lead_status,engagement_count,
                         created_at,updated_at)
                     VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,
                             $10,$11,$12,$13,$14)
                     ON CONFLICT (holding_id,lead_id) DO UPDATE SET
                        aggregate_version=EXCLUDED.aggregate_version,
                        holding_id=EXCLUDED.holding_id,company_id=EXCLUDED.company_id,
                        branch_id=EXCLUDED.branch_id,campaign_id=EXCLUDED.campaign_id,
                        audience_member_id=EXCLUDED.audience_member_id,source=EXCLUDED.source,
                        existing_customer_id=EXCLUDED.existing_customer_id,
                        qualification_score_basis_points=EXCLUDED.qualification_score_basis_points,
                        lead_status=EXCLUDED.lead_status,engagement_count=EXCLUDED.engagement_count,
                        created_at=EXCLUDED.created_at,updated_at=EXCLUDED.updated_at
                     WHERE erpx_marketing_write.lead_candidate_authority.holding_id=
                               erpx_cqrs_write.current_holding_id()
                       AND erpx_marketing_write.lead_candidate_authority.aggregate_version + 1=
                               EXCLUDED.aggregate_version",
                )
                .bind(value.id().as_uuid())
                .bind(to_i64(context.aggregate_version)?)
                .bind(holding_id)
                .bind(company_id)
                .bind(branch_id)
                .bind(value.campaign_id().as_uuid())
                .bind(value.audience_member_id().as_uuid())
                .bind(value.source())
                .bind(value.existing_customer_id())
                .bind(value.score().map(i32::from))
                .bind(lead_status_name(value.status()))
                .bind(to_i64(value.engagement_count())?)
                .bind(value.created_at())
                .bind(value.updated_at())
                .execute(&mut **transaction)
                .await?;
                require_one_row(result.rows_affected(), "Marketing lead version fence")?;
            }
            MarketingAggregate::CampaignBudget(value) => {
                let (holding_id, company_id, branch_id) = exact_scope_ids(value.scope())?;
                let dimensions = value.dimensions();
                let result = sqlx::query(
                    "INSERT INTO erpx_marketing_write.campaign_budget_authority
                        (holding_id,budget_id,aggregate_version,holding_id,company_id,branch_id,
                         campaign_id,budget_code,budget_name,budget_period,currency,
                         requested_minor,approved_minor,dimension_business_unit_id,
                         dimension_cost_center_id,dimension_location_id,dimension_project_id,
                         dimension_channel_id,dimension_party_id,dimension_employee_id,
                         dimension_asset_id,dimension_legal_matter_id,
                         dimension_intercompany_partner_id,approval_request_id,
                         approval_subject_version,approval_requested_by,approval_requested_at,
                         budget_status,created_at,updated_at)
                     VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,
                             $10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,
                             $26,$27,$28,$29)
                     ON CONFLICT (holding_id,budget_id) DO UPDATE SET
                        aggregate_version=EXCLUDED.aggregate_version,
                        holding_id=EXCLUDED.holding_id,company_id=EXCLUDED.company_id,
                        branch_id=EXCLUDED.branch_id,campaign_id=EXCLUDED.campaign_id,
                        budget_code=EXCLUDED.budget_code,budget_name=EXCLUDED.budget_name,
                        budget_period=EXCLUDED.budget_period,currency=EXCLUDED.currency,
                        requested_minor=EXCLUDED.requested_minor,approved_minor=EXCLUDED.approved_minor,
                        dimension_business_unit_id=EXCLUDED.dimension_business_unit_id,
                        dimension_cost_center_id=EXCLUDED.dimension_cost_center_id,
                        dimension_location_id=EXCLUDED.dimension_location_id,
                        dimension_project_id=EXCLUDED.dimension_project_id,
                        dimension_channel_id=EXCLUDED.dimension_channel_id,
                        dimension_party_id=EXCLUDED.dimension_party_id,
                        dimension_employee_id=EXCLUDED.dimension_employee_id,
                        dimension_asset_id=EXCLUDED.dimension_asset_id,
                        dimension_legal_matter_id=EXCLUDED.dimension_legal_matter_id,
                        dimension_intercompany_partner_id=EXCLUDED.dimension_intercompany_partner_id,
                        approval_request_id=EXCLUDED.approval_request_id,
                        approval_subject_version=EXCLUDED.approval_subject_version,
                        approval_requested_by=EXCLUDED.approval_requested_by,
                        approval_requested_at=EXCLUDED.approval_requested_at,
                        budget_status=EXCLUDED.budget_status,created_at=EXCLUDED.created_at,
                        updated_at=EXCLUDED.updated_at
                     WHERE erpx_marketing_write.campaign_budget_authority.holding_id=
                               erpx_cqrs_write.current_holding_id()
                       AND erpx_marketing_write.campaign_budget_authority.aggregate_version + 1=
                               EXCLUDED.aggregate_version",
                )
                    .bind(value.id().as_uuid())
                    .bind(to_i64(context.aggregate_version)?)
                    .bind(holding_id)
                    .bind(company_id)
                    .bind(branch_id)
                    .bind(value.campaign_id().as_uuid())
                    .bind(value.code())
                    .bind(value.name())
                    .bind(budget_period_name(value.period()))
                    .bind(value.requested_amount().currency().as_str())
                    .bind(value.requested_amount().minor_units())
                    .bind(value.approved_amount().map(|amount| amount.minor_units()))
                    .bind(dimensions.business_unit.map(|value| value.as_uuid()))
                    .bind(dimensions.cost_center.map(|value| value.as_uuid()))
                    .bind(dimensions.location.map(|value| value.as_uuid()))
                    .bind(dimensions.project.map(|value| value.as_uuid()))
                    .bind(dimensions.channel.map(|value| value.as_uuid()))
                    .bind(dimensions.party.map(|value| value.as_uuid()))
                    .bind(dimensions.employee.map(|value| value.as_uuid()))
                    .bind(dimensions.asset.map(|value| value.as_uuid()))
                    .bind(dimensions.legal_matter.map(|value| value.as_uuid()))
                    .bind(dimensions.intercompany_partner.map(|value| value.as_uuid()))
                    .bind(value.approval_request().map(|request| request.as_uuid()))
                    .bind(approval_subject_version(value))
                    .bind(approval_requested_by(value))
                    .bind(approval_requested_at(value))
                    .bind(budget_status_name(value.status()))
                    .bind(value.created_at())
                    .bind(value.updated_at())
                    .execute(&mut **transaction)
                    .await?;
                require_one_row(result.rows_affected(), "Marketing budget version fence")?;

                for allocation in value.allocations() {
                    let result = sqlx::query(
                        "INSERT INTO erpx_marketing_write.budget_allocation_authority
                            (holding_id,budget_id,allocation_id,tactic_id,channel_id,currency,
                             planned_minor,budget_aggregate_version)
                         VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7)
                         ON CONFLICT (holding_id,budget_id,allocation_id) DO UPDATE SET
                            budget_aggregate_version=EXCLUDED.budget_aggregate_version
                         WHERE erpx_marketing_write.budget_allocation_authority.tactic_id=
                                   EXCLUDED.tactic_id
                           AND erpx_marketing_write.budget_allocation_authority.channel_id=
                                   EXCLUDED.channel_id
                           AND erpx_marketing_write.budget_allocation_authority.currency=
                                   EXCLUDED.currency
                           AND erpx_marketing_write.budget_allocation_authority.planned_minor=
                                   EXCLUDED.planned_minor",
                    )
                    .bind(value.id().as_uuid())
                    .bind(allocation.allocation_id.as_uuid())
                    .bind(allocation.tactic.as_uuid())
                    .bind(allocation.channel.as_uuid())
                    .bind(allocation.planned_amount.currency().as_str())
                    .bind(allocation.planned_amount.minor_units())
                    .bind(to_i64(context.aggregate_version)?)
                    .execute(&mut **transaction)
                    .await?;
                    require_one_row(
                        result.rows_affected(),
                        "Marketing allocation immutable fields changed",
                    )?;
                }
                for reservation in value.reservations().values() {
                    let status = match reservation.status {
                        SpendReservationStatus::Active => "active",
                        SpendReservationStatus::Released => "released",
                    };
                    let result = sqlx::query("INSERT INTO erpx_marketing_write.spend_reservation_authority (holding_id,request_id,budget_id,allocation_id,currency,amount_minor,expected_outcome,spend_status,requested_at,released_at,budget_aggregate_version) VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT (holding_id,request_id) DO UPDATE SET spend_status=EXCLUDED.spend_status,released_at=EXCLUDED.released_at,budget_aggregate_version=EXCLUDED.budget_aggregate_version WHERE erpx_marketing_write.spend_reservation_authority.budget_id=EXCLUDED.budget_id AND erpx_marketing_write.spend_reservation_authority.allocation_id=EXCLUDED.allocation_id AND erpx_marketing_write.spend_reservation_authority.currency=EXCLUDED.currency AND erpx_marketing_write.spend_reservation_authority.amount_minor=EXCLUDED.amount_minor AND erpx_marketing_write.spend_reservation_authority.expected_outcome=EXCLUDED.expected_outcome AND erpx_marketing_write.spend_reservation_authority.requested_at=EXCLUDED.requested_at AND (erpx_marketing_write.spend_reservation_authority.spend_status=EXCLUDED.spend_status OR (erpx_marketing_write.spend_reservation_authority.spend_status='active' AND EXCLUDED.spend_status='released'))")
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
    digest.update(b"erpx.marketing.command\0");
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

async fn load_budget_allocations(
    transaction: &mut Transaction<'_, Postgres>,
    budget_id: CampaignBudgetId,
    currency: CurrencyCode,
) -> Result<Vec<BudgetAllocation>, PortError> {
    let rows = sqlx::query(
        "SELECT allocation_id,tactic_id,channel_id,currency,planned_minor
         FROM erpx_marketing_write.budget_allocation_authority
         WHERE holding_id=erpx_cqrs_write.current_holding_id() AND budget_id=$1
         ORDER BY allocation_id",
    )
    .bind(budget_id.as_uuid())
    .fetch_all(&mut **transaction)
    .await
    .map_err(|_| PortError::Unavailable)?;
    rows.into_iter()
        .map(|row| {
            let stored_currency = CurrencyCode::try_new(
                row.try_get::<String, _>("currency")
                    .map_err(|_| PortError::Internal)?
                    .as_str(),
            )
            .map_err(|_| PortError::Internal)?;
            if stored_currency != currency {
                return Err(PortError::Internal);
            }
            Ok(BudgetAllocation {
                allocation_id: BudgetAllocationId::try_from_uuid(
                    row.try_get("allocation_id")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                tactic: marketing::TacticRef::try_from_uuid(
                    row.try_get("tactic_id").map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                channel: marketing::ChannelRef::try_from_uuid(
                    row.try_get("channel_id").map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                planned_amount: MoneyAmount::new(
                    row.try_get("planned_minor")
                        .map_err(|_| PortError::Internal)?,
                    currency,
                ),
            })
        })
        .collect()
}

async fn load_spend_reservations(
    transaction: &mut Transaction<'_, Postgres>,
    budget_id: CampaignBudgetId,
    currency: CurrencyCode,
) -> Result<std::collections::BTreeMap<SpendRequestId, SpendReservation>, PortError> {
    let rows = sqlx::query(
        "SELECT request_id,allocation_id,currency,amount_minor,expected_outcome,
                spend_status,requested_at,released_at
         FROM erpx_marketing_write.spend_reservation_authority
         WHERE holding_id=erpx_cqrs_write.current_holding_id() AND budget_id=$1
         ORDER BY request_id",
    )
    .bind(budget_id.as_uuid())
    .fetch_all(&mut **transaction)
    .await
    .map_err(|_| PortError::Unavailable)?;
    let mut reservations = std::collections::BTreeMap::new();
    for row in rows {
        let request_id = SpendRequestId::try_from_uuid(
            row.try_get("request_id").map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?;
        let stored_currency = CurrencyCode::try_new(
            row.try_get::<String, _>("currency")
                .map_err(|_| PortError::Internal)?
                .as_str(),
        )
        .map_err(|_| PortError::Internal)?;
        if stored_currency != currency {
            return Err(PortError::Internal);
        }
        let reservation = SpendReservation {
            request_id,
            allocation_id: BudgetAllocationId::try_from_uuid(
                row.try_get("allocation_id")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            amount: MoneyAmount::new(
                row.try_get("amount_minor")
                    .map_err(|_| PortError::Internal)?,
                currency,
            ),
            expected_outcome: row
                .try_get("expected_outcome")
                .map_err(|_| PortError::Internal)?,
            status: parse_reservation_status(
                row.try_get("spend_status")
                    .map_err(|_| PortError::Internal)?,
            )?,
            requested_at: row
                .try_get("requested_at")
                .map_err(|_| PortError::Internal)?,
            released_at: row
                .try_get("released_at")
                .map_err(|_| PortError::Internal)?,
        };
        if reservations.insert(request_id, reservation).is_some() {
            return Err(PortError::Internal);
        }
    }
    Ok(reservations)
}

fn dimensions_from_row(
    row: &sqlx::postgres::PgRow,
    company_id: Uuid,
    campaign_id: CampaignId,
) -> Result<DimensionSet, PortError> {
    let optional_ref = |column| {
        row.try_get::<Option<Uuid>, _>(column)
            .map_err(|_| PortError::Internal)
    };
    Ok(DimensionSet {
        legal_entity: organization::CompanyRef::try_from_uuid(company_id)
            .map_err(|_| PortError::Internal)?,
        branch: optional_ref("branch_id")?
            .map(organization::BranchRef::try_from_uuid)
            .transpose()
            .map_err(|_| PortError::Internal)?,
        business_unit: optional_ref("dimension_business_unit_id")?
            .map(organization::BusinessUnitRef::try_from_uuid)
            .transpose()
            .map_err(|_| PortError::Internal)?,
        cost_center: optional_ref("dimension_cost_center_id")?
            .map(erpx_integration_contracts::CostCenterRef::try_from_uuid)
            .transpose()
            .map_err(|_| PortError::Internal)?,
        location: optional_ref("dimension_location_id")?
            .map(erpx_integration_contracts::LocationRef::try_from_uuid)
            .transpose()
            .map_err(|_| PortError::Internal)?,
        project: optional_ref("dimension_project_id")?
            .map(erpx_integration_contracts::ProjectRef::try_from_uuid)
            .transpose()
            .map_err(|_| PortError::Internal)?,
        campaign: Some(
            marketing::CampaignRef::try_from_uuid(campaign_id.as_uuid())
                .map_err(|_| PortError::Internal)?,
        ),
        channel: optional_ref("dimension_channel_id")?
            .map(marketing::ChannelRef::try_from_uuid)
            .transpose()
            .map_err(|_| PortError::Internal)?,
        party: optional_ref("dimension_party_id")?
            .map(erpx_integration_contracts::PartyRef::try_from_uuid)
            .transpose()
            .map_err(|_| PortError::Internal)?,
        employee: optional_ref("dimension_employee_id")?
            .map(people_payroll::EmployeeRef::try_from_uuid)
            .transpose()
            .map_err(|_| PortError::Internal)?,
        asset: optional_ref("dimension_asset_id")?
            .map(assets::AssetRef::try_from_uuid)
            .transpose()
            .map_err(|_| PortError::Internal)?,
        legal_matter: optional_ref("dimension_legal_matter_id")?
            .map(legal::LegalMatterRef::try_from_uuid)
            .transpose()
            .map_err(|_| PortError::Internal)?,
        intercompany_partner: optional_ref("dimension_intercompany_partner_id")?
            .map(organization::CompanyRef::try_from_uuid)
            .transpose()
            .map_err(|_| PortError::Internal)?,
    })
}

fn exact_scope_from_row(row: &sqlx::postgres::PgRow) -> Result<AuthorityScope, PortError> {
    let holding_id: Uuid = row.try_get("holding_id").map_err(|_| PortError::Internal)?;
    let company_id: Uuid = row.try_get("company_id").map_err(|_| PortError::Internal)?;
    let branch_id: Option<Uuid> = row.try_get("branch_id").map_err(|_| PortError::Internal)?;
    let scope = match branch_id {
        Some(branch_id) => AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        },
        None => AuthorityScope::Company {
            holding_id,
            company_id,
        },
    };
    scope.validate().map_err(|_| PortError::Internal)?;
    Ok(scope)
}

fn exact_scope_ids(scope: &AuthorityScope) -> Result<(Uuid, Uuid, Option<Uuid>), CqrsSqlxError> {
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
        _ => Err(CqrsSqlxError::InvalidInput(
            "Marketing authority requires an exact scope",
        )),
    }
}

fn scope_company_id(scope: &AuthorityScope) -> Result<Uuid, PortError> {
    match scope {
        AuthorityScope::Company { company_id, .. } | AuthorityScope::Branch { company_id, .. } => {
            Ok(*company_id)
        }
        _ => Err(PortError::Internal),
    }
}

fn parse_lead_status(value: &str) -> Result<LeadCandidateStatus, PortError> {
    match value {
        "open" => Ok(LeadCandidateStatus::Captured),
        "qualified" => Ok(LeadCandidateStatus::Qualified),
        "rejected" => Ok(LeadCandidateStatus::Rejected),
        _ => Err(PortError::Internal),
    }
}

fn parse_budget_status(value: &str) -> Result<CampaignBudgetStatus, PortError> {
    match value {
        "draft" => Ok(CampaignBudgetStatus::Draft),
        "review" => Ok(CampaignBudgetStatus::Review),
        "approved" => Ok(CampaignBudgetStatus::Approved),
        "running" => Ok(CampaignBudgetStatus::Running),
        "completed" => Ok(CampaignBudgetStatus::Completed),
        "cancelled" => Ok(CampaignBudgetStatus::Cancelled),
        _ => Err(PortError::Internal),
    }
}

fn parse_budget_period(value: &str) -> Result<marketing::CampaignBudgetPeriod, PortError> {
    match value {
        "annual" => Ok(marketing::CampaignBudgetPeriod::Annual),
        "quarterly" => Ok(marketing::CampaignBudgetPeriod::Quarterly),
        "campaign" => Ok(marketing::CampaignBudgetPeriod::Campaign),
        _ => Err(PortError::Internal),
    }
}

fn parse_reservation_status(value: &str) -> Result<SpendReservationStatus, PortError> {
    match value {
        "active" => Ok(SpendReservationStatus::Active),
        "released" => Ok(SpendReservationStatus::Released),
        _ => Err(PortError::Internal),
    }
}

fn row_u64(row: &sqlx::postgres::PgRow, column: &str) -> Result<u64, PortError> {
    u64::try_from(
        row.try_get::<i64, _>(column)
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)
}

fn from_i64_store(value: i64) -> Result<u64, CqrsSqlxError> {
    u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}

fn require_one_row(rows: u64, message: &'static str) -> Result<(), CqrsSqlxError> {
    if rows == 1 {
        Ok(())
    } else {
        Err(CqrsSqlxError::GuardRejected(message))
    }
}

fn approval_subject_version(value: &CampaignBudget) -> Option<i64> {
    value
        .approval_subject()
        .and_then(|subject| i64::try_from(subject.aggregate_version).ok())
}

fn approval_requested_by(value: &CampaignBudget) -> Option<&str> {
    value
        .approval_requested_by()
        .map(erpx_integration_contracts::ReferenceCode::as_str)
}

fn approval_requested_at(value: &CampaignBudget) -> Option<chrono::DateTime<chrono::Utc>> {
    value.approval_requested_at()
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
