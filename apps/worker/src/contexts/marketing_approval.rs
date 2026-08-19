//! Idempotent Operations approval-decision delivery into Marketing.

use async_trait::async_trait;
use erpx_cqrs::{CONTRACT_VERSION, CommandEnvelope, CommandHandler, IdempotencyKey};
use erpx_cqrs_sqlx::WritePool;
use erpx_infrastructure::{HoldingDatabaseRouter, RoutedDatabaseSide, SqlxHoldingStoreRegistry};
use erpx_integration_contracts::{BoundedContext, IntegrationEvent, operations};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, AuthorityScope, CommandId, EventId, SystemClock,
};
use erpx_marketing::{
    ApplyCampaignBudgetDecision, AuthorizationPort, CampaignBudgetId, EntitlementPort,
    MarketingAction, MarketingCommand, MarketingCommandHandler, PortError, WriteRepository,
    ports::AuthorizationRequest,
};
use erpx_marketing_sqlx::{MarketingCommandAdapter, MarketingWriteRepository};
use erpx_operations_sqlx::OperationsOutboxPayload;
use erpx_worker::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub const MARKETING_APPROVAL_DECISIONS_HANDLER_NAME: &str = "marketing_approval_decisions";
pub const MARKETING_APPROVAL_DECISIONS_MAX_PAGE_SIZE: u32 = 500;
pub const MARKETING_APPROVAL_DECISIONS_MAX_ACTIVE_HOLDINGS: usize = 1_000;

const CONSUMER_NAME: &str = "marketing.operations-approval-decisions.v1";

#[derive(Debug, Clone)]
pub struct MarketingApprovalDecisionHandler {
    registry: SqlxHoldingStoreRegistry,
    router: HoldingDatabaseRouter,
    page_size: u32,
    max_active_holdings: usize,
}

impl MarketingApprovalDecisionHandler {
    pub fn new(
        registry: SqlxHoldingStoreRegistry,
        router: HoldingDatabaseRouter,
        page_size: u32,
        max_active_holdings: usize,
    ) -> Result<Self, WorkerRuntimeError> {
        if page_size == 0
            || page_size > MARKETING_APPROVAL_DECISIONS_MAX_PAGE_SIZE
            || max_active_holdings == 0
            || max_active_holdings > MARKETING_APPROVAL_DECISIONS_MAX_ACTIVE_HOLDINGS
        {
            return Err(configuration(
                "Marketing approval-decision worker bounds are invalid",
            ));
        }
        Ok(Self {
            registry,
            router,
            page_size,
            max_active_holdings,
        })
    }

    async fn process_holding(
        &self,
        holding_id: &str,
        data_profile_key: &str,
        worker_id: Uuid,
    ) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let routed = self
            .router
            .route(holding_id, data_profile_key, RoutedDatabaseSide::Write)
            .await
            .map_err(|_| configuration("Marketing approval holding route is unavailable"))?;
        let pool = routed.database_pool().clone();
        let after = checkpoint(&pool).await?;
        let rows = source_page(&pool, after, self.page_size).await?;
        let mut report = WorkerHandlerReport {
            claimed: u64::try_from(rows.len())
                .map_err(|_| configuration("Marketing approval page is too large"))?,
            has_work: !rows.is_empty(),
            ..WorkerHandlerReport::default()
        };
        let mut expected_checkpoint = after;
        for row in rows {
            self.apply_decision(holding_id, &pool, worker_id, &row)
                .await?;
            advance_checkpoint(&pool, expected_checkpoint, &row).await?;
            expected_checkpoint = row.global_sequence;
            report.processed = report.processed.saturating_add(1);
            report.published = report.published.saturating_add(1);
        }
        Ok(report)
    }

    async fn apply_decision(
        &self,
        holding_id: &str,
        pool: &PgPool,
        worker_id: Uuid,
        row: &ApprovalDecisionRow,
    ) -> Result<(), WorkerRuntimeError> {
        let budget_id = CampaignBudgetId::try_from_uuid(row.decision.subject.aggregate_id)
            .map_err(|_| configuration("Operations approval subject is invalid"))?;
        let holding_id = Uuid::parse_str(holding_id)
            .map_err(|_| configuration("Marketing holding identity must be a non-nil UUID"))?;
        let write_pool = WritePool::for_holding(pool.clone(), holding_id)
            .map_err(|_| configuration("Marketing holding identity must be a non-nil UUID"))?;
        let write = MarketingWriteRepository::new(write_pool.clone());
        let budget = write
            .load_budget(&row.scope, budget_id)
            .await
            .map_err(map_marketing_error)?
            .ok_or_else(|| configuration("Marketing approval subject was not found"))?;
        if budget.scope() != &row.scope {
            return Err(configuration(
                "Operations approval scope does not match Marketing",
            ));
        }
        let actor = ActorAttribution {
            principal_ref: format!("system-worker:marketing-approval-decisions:{worker_id}"),
            kind: ActorKind::SystemWorker,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        };
        let access = ApprovalWorkerAccess;
        MarketingCommandHandler::new(
            write,
            MarketingCommandAdapter::new(write_pool),
            access,
            access,
            SystemClock,
        )
        .handle(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            command_id: CommandId::try_from_uuid(row.event_id)
                .map_err(|_| configuration("Operations approval event id is invalid"))?,
            idempotency_key: IdempotencyKey::try_new(format!(
                "operations-approval:{}",
                row.event_id
            ))
            .map_err(|_| configuration("Operations approval idempotency key is invalid"))?,
            correlation_id: row.correlation_id,
            causation_id: Some(
                EventId::try_from_uuid(row.event_id)
                    .map_err(|_| configuration("Operations approval event id is invalid"))?,
            ),
            actor,
            active_scope: row.scope.clone(),
            audit: AuditContext {
                purpose: "apply Operations approval decision to Marketing campaign budget".into(),
                customer_visible: false,
                module: Some("marketing".into()),
            },
            expected_aggregate_version: Some(budget.version()),
            command: MarketingCommand::ApplyBudgetDecision(ApplyCampaignBudgetDecision {
                budget_id,
                decision: row.decision.clone(),
            }),
        })
        .await
        .map_err(|_| configuration("Marketing rejected an Operations approval decision"))?;
        Ok(())
    }
}

#[async_trait]
impl NamedWorkerHandler for MarketingApprovalDecisionHandler {
    fn name(&self) -> &str {
        MARKETING_APPROVAL_DECISIONS_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if worker_id.is_nil() {
            return Err(configuration(
                "Marketing approval-decision worker identity is required",
            ));
        }
        let holdings = self
            .registry
            .list_active_bounded(self.max_active_holdings)
            .await
            .map_err(|_| configuration("Active holding registry is unavailable"))?;
        let mut total = WorkerHandlerReport::default();
        for holding in holdings {
            let report = self
                .process_holding(&holding.holding_id, &holding.data_profile_key, worker_id)
                .await?;
            total.claimed = total.claimed.saturating_add(report.claimed);
            total.processed = total.processed.saturating_add(report.processed);
            total.published = total.published.saturating_add(report.published);
            total.has_work |= report.has_work;
        }
        Ok(total)
    }
}

#[derive(Debug, Clone, Copy)]
struct ApprovalWorkerAccess;

#[async_trait]
impl AuthorizationPort for ApprovalWorkerAccess {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError> {
        if request.actor.kind == ActorKind::SystemWorker
            && request
                .actor
                .principal_ref
                .starts_with("system-worker:marketing-approval-decisions:")
            && request.action == MarketingAction::ApplyBudgetDecision
            && request.authority_scope == request.target_scope
        {
            Ok(())
        } else {
            Err(PortError::Denied)
        }
    }
}

#[async_trait]
impl EntitlementPort for ApprovalWorkerAccess {
    async fn ensure_marketing_enabled(
        &self,
        scope: &AuthorityScope,
        write: bool,
    ) -> Result<(), PortError> {
        if write
            && !matches!(
                scope,
                AuthorityScope::Platform | AuthorityScope::Subtree { .. }
            )
        {
            Ok(())
        } else {
            Err(PortError::EntitlementDenied)
        }
    }
}

#[derive(Debug, Clone)]
struct ApprovalDecisionRow {
    event_id: Uuid,
    global_sequence: u64,
    correlation_id: erpx_kernel::CorrelationId,
    scope: AuthorityScope,
    decision: operations::ApprovalDecisionFact,
}

async fn checkpoint(pool: &PgPool) -> Result<u64, WorkerRuntimeError> {
    let value: Option<i64> = sqlx::query_scalar(
        "SELECT last_global_sequence FROM erpx_marketing_write.approval_decision_checkpoints WHERE consumer_name=$1",
    )
    .bind(CONSUMER_NAME)
    .fetch_optional(pool)
    .await?;
    value
        .map(u64::try_from)
        .transpose()
        .map_err(|_| configuration("Marketing approval checkpoint is invalid"))
        .map(|value| value.unwrap_or(0))
}

async fn source_page(
    pool: &PgPool,
    after: u64,
    page_size: u32,
) -> Result<Vec<ApprovalDecisionRow>, WorkerRuntimeError> {
    let rows = sqlx::query(
        "SELECT event_id,global_sequence,context_name,event_type,event_version,aggregate_type,aggregate_version,active_scope,correlation_id,payload,occurred_at
         FROM erpx_cqrs_write.outbox_events
         WHERE global_sequence > $1 AND available_at <= now()
           AND event_type='operations.approval_decided'
         ORDER BY global_sequence
         LIMIT $2",
    )
    .bind(i64::try_from(after).map_err(|_| configuration("Marketing approval cursor is invalid"))?)
    .bind(i64::from(page_size))
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            let event_id: Uuid = row.try_get("event_id")?;
            let global_sequence = u64::try_from(row.try_get::<i64, _>("global_sequence")?)
                .map_err(|_| configuration("Operations approval sequence is invalid"))?;
            let context: String = row.try_get("context_name")?;
            let event_type: String = row.try_get("event_type")?;
            let event_version: i32 = row.try_get("event_version")?;
            let aggregate_type: String = row.try_get("aggregate_type")?;
            let aggregate_version: i64 = row.try_get("aggregate_version")?;
            let scope: AuthorityScope = serde_json::from_value(row.try_get("active_scope")?)
                .map_err(|_| configuration("Operations approval scope is invalid"))?;
            let correlation_id =
                erpx_kernel::CorrelationId::try_from_uuid(row.try_get("correlation_id")?)
                    .map_err(|_| configuration("Operations approval correlation is invalid"))?;
            let occurred_at: chrono::DateTime<chrono::Utc> = row.try_get("occurred_at")?;
            let payload: OperationsOutboxPayload = serde_json::from_value(row.try_get("payload")?)
                .map_err(|_| configuration("Operations approval payload is invalid"))?;
            let OperationsOutboxPayload::Integration(operations::Event::ApprovalDecided(decision)) =
                payload
            else {
                return Err(configuration("Operations approval payload type is invalid"));
            };
            if event_id.is_nil()
                || context != "operations"
                || event_type != "operations.approval_decided"
                || event_version != 1
                || aggregate_type != "operations_exception"
                || aggregate_version <= 0
                || decision.subject.context != BoundedContext::Marketing
                || decision.subject.aggregate_kind.as_str() != "campaign_budget"
                || decision.decided_at != occurred_at
                || operations::Event::ApprovalDecided(decision.clone())
                    .validate()
                    .is_err()
                || scope.validate().is_err()
            {
                return Err(configuration(
                    "Operations approval event contract is invalid",
                ));
            }
            Ok(ApprovalDecisionRow {
                event_id,
                global_sequence,
                correlation_id,
                scope,
                decision,
            })
        })
        .collect()
}

async fn advance_checkpoint(
    pool: &PgPool,
    expected: u64,
    row: &ApprovalDecisionRow,
) -> Result<(), WorkerRuntimeError> {
    let mut tx = pool.begin().await?;
    let current: Option<i64> = sqlx::query_scalar(
        "SELECT last_global_sequence FROM erpx_marketing_write.approval_decision_checkpoints WHERE consumer_name=$1 FOR UPDATE",
    )
    .bind(CONSUMER_NAME)
    .fetch_optional(&mut *tx)
    .await?;
    let current = current
        .map(u64::try_from)
        .transpose()
        .map_err(|_| configuration("Marketing approval checkpoint is invalid"))?
        .unwrap_or(0);
    if current != expected || row.global_sequence <= current {
        return Err(configuration(
            "Marketing approval checkpoint moved concurrently",
        ));
    }
    sqlx::query(
        "INSERT INTO erpx_marketing_write.approval_decision_checkpoints (consumer_name,last_global_sequence,last_event_id,updated_at)
         VALUES ($1,$2,$3,now())
         ON CONFLICT (consumer_name) DO UPDATE SET last_global_sequence=EXCLUDED.last_global_sequence,last_event_id=EXCLUDED.last_event_id,updated_at=EXCLUDED.updated_at",
    )
    .bind(CONSUMER_NAME)
    .bind(i64::try_from(row.global_sequence).map_err(|_| configuration("Marketing approval sequence is invalid"))?)
    .bind(row.event_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

fn map_marketing_error(error: PortError) -> WorkerRuntimeError {
    match error {
        PortError::Unavailable => configuration("Marketing approval storage is unavailable"),
        _ => configuration("Marketing approval subject could not be loaded"),
    }
}

fn configuration(message: &'static str) -> WorkerRuntimeError {
    WorkerRuntimeError::Configuration(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handler_contract_is_bounded_and_source_specific() {
        assert_eq!(
            MARKETING_APPROVAL_DECISIONS_HANDLER_NAME,
            "marketing_approval_decisions"
        );
        assert_eq!(MARKETING_APPROVAL_DECISIONS_MAX_PAGE_SIZE, 500);
        assert!(CONSUMER_NAME.contains("operations-approval-decisions"));
    }

    #[test]
    fn worker_source_has_idempotent_command_and_checkpoint_fences() {
        let source = include_str!("marketing_approval.rs");
        assert!(source.contains("operations.approval_decided"));
        assert!(source.contains("operations-approval:"));
        assert!(source.contains("FOR UPDATE"));
        assert!(source.contains("MarketingCommand::ApplyBudgetDecision"));
    }
}
