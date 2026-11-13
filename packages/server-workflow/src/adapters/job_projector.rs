//! Incremental native Workflow projection over the caller's transaction.
//!
//! Exact port of `packages/server-work_projection/src/adapters/workflow-projector.ts`
//! for the owner families that can be projected natively today: task/routine,
//! leave, overtime and payroll. Every statement below is the retained statement
//! and runs on the caller's `&mut sqlx::PgConnection`; this adapter never opens
//! an independent transaction and never commits.
//!
//! Owner facts are resolved through [`ProjectionOwnerSources`]. Composition
//! implements that trait over the Operations and People/Payroll read ports on
//! the selected Holding's writer connection; a missing fact is
//! `PROJECTION_SOURCE_UNAVAILABLE` and must never become an empty work item.
//! Other event families are reported as [`ProjectionResult::Unhandled`] so the
//! composition handler can leave them to the retained TypeScript supervisor
//! instead of acknowledging them.
use crate::domain::job_projector::{
    EventMessage, ProjectionSideEffect, ProjectionWork, event_work, projection_side_effects,
    projection_work,
};
use erpx_server_runtime::public::postgres::PersistenceError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::PgConnection;
use std::{
    future::Future,
    pin::Pin,
};

pub type ProjectionSourceFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<ProjectionSource>, BusinessError>> + Send + 'a>>;

/// The owner row facts retained by the TypeScript `ProjectionSource` shape.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectionSource {
    pub state: String,
    pub version: i64,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}

impl ProjectionSource {
    pub fn new(
        state: impl Into<String>,
        version: i64,
        company_id: impl Into<String>,
        branch_id: Option<String>,
        department_id: Option<String>,
    ) -> Self {
        Self {
            state: state.into(),
            version,
            company_id: company_id.into(),
            branch_id,
            department_id,
        }
    }
}

/// Owner read ports used to refresh a delayed or version-only event.
///
/// Composition binds the selected Holding writer connection inside its
/// implementation. The projection connection passed to [`project_event`]
/// carries only the three retained projection statements.
pub trait ProjectionOwnerSources: Send {
    fn task<'a>(&'a mut self, id: &'a str) -> ProjectionSourceFuture<'a>;
    fn leave<'a>(&'a mut self, id: &'a str) -> ProjectionSourceFuture<'a>;
    fn overtime<'a>(&'a mut self, id: &'a str) -> ProjectionSourceFuture<'a>;
    fn payroll<'a>(&'a mut self, id: &'a str) -> ProjectionSourceFuture<'a>;
}

/// Whether this projector acknowledged the event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionResult {
    /// The receipt, optional work item and checkpoint were written.
    Projected,
    /// Another family (or an excluded policy/eligibility event) owns the event.
    /// No receipt is written and the caller must not acknowledge it natively.
    Unhandled,
}

pub type SideEffectFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), BusinessError>> + Send + 'a>>;

/// Owner side-effect drains required before projection.
///
/// Composition implements this trait by forwarding each call to the owning
/// crate's public drain on the same Holding writer. The native orchestrator
/// owns only the routing/order; every owner keeps its own authority,
/// transaction and retry policy. A missing implementation must keep the event
/// with the retained supervisor instead of silently acknowledging it.
pub trait ProjectionSideEffects: Send {
    /// Resolve the notice id for a queued fulfillment, then deliver it.
    fn customer_notification_queue<'a>(
        &'a mut self,
        fulfillment_id: &'a str,
    ) -> SideEffectFuture<'a>;

    /// Deliver one retried customer notice by its durable id.
    fn customer_notification_retry<'a>(
        &'a mut self,
        notification_id: &'a str,
    ) -> SideEffectFuture<'a>;

    fn legal_ai<'a>(&'a mut self) -> SideEffectFuture<'a>;

    fn executive_generation<'a>(&'a mut self) -> SideEffectFuture<'a>;

    fn social_agent<'a>(&'a mut self, post_id: &'a str) -> SideEffectFuture<'a>;

    fn expense_ai<'a>(&'a mut self, claim_id: &'a str) -> SideEffectFuture<'a>;

    fn finance_ai<'a>(&'a mut self) -> SideEffectFuture<'a>;

    fn request_ai<'a>(&'a mut self, request_id: &'a str) -> SideEffectFuture<'a>;
}

/// Run the retained owner side effects for one event in the retained order.
///
/// This performs no projection write and opens no transaction; each effect is
/// the owner's own bounded drain. A failure stops before projection so the
/// durable event is retried instead of being acknowledged without its effect.
pub async fn run_side_effects(
    effects: &mut impl ProjectionSideEffects,
    message: &EventMessage,
    production: bool,
) -> Result<(), BusinessError> {
    for effect in projection_side_effects(&message.event_type, production) {
        match effect {
            ProjectionSideEffect::CustomerNotificationQueue => {
                effects
                    .customer_notification_queue(&message.aggregate_id)
                    .await?;
            }
            ProjectionSideEffect::CustomerNotificationRetry => {
                effects
                    .customer_notification_retry(&message.aggregate_id)
                    .await?;
            }
            ProjectionSideEffect::LegalAiQueue => effects.legal_ai().await?,
            ProjectionSideEffect::ExecutiveGenerationQueue => {
                effects.executive_generation().await?;
            }
            ProjectionSideEffect::SocialAgent => {
                effects.social_agent(&message.aggregate_id).await?;
            }
            ProjectionSideEffect::ExpenseAiPrepare => {
                effects.expense_ai(&message.aggregate_id).await?;
            }
            ProjectionSideEffect::FinanceAiPrepare => effects.finance_ai().await?,
            ProjectionSideEffect::RequestAiPrepare => {
                effects.request_ai(&message.aggregate_id).await?;
            }
        }
    }
    Ok(())
}

/// Full native projector entry: Holding check, owner side effects, then the
/// incremental projection on the caller's transaction.
///
/// `known_holding` is resolved by composition from the registered Holding
/// slots; an unknown Holding is never projected. The caller owns the single
/// commit/rollback of the projection unit exactly like the retained flow.
pub async fn project_message(
    connection: &mut PgConnection,
    sources: &mut impl ProjectionOwnerSources,
    effects: &mut impl ProjectionSideEffects,
    message: &EventMessage,
    known_holding: bool,
    production: bool,
    now: i64,
) -> Result<ProjectionResult, BusinessError> {
    if !known_holding {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "INVALID_HOLDING",
            "The Holding is not registered for Workflow projection.",
        ));
    }
    run_side_effects(effects, message, production).await?;
    project_event(connection, sources, message, now).await
}

/// Project one durable event on the caller's transaction.
///
/// The task family always re-reads the owner, and a People event without an
/// explicit payload state does too, matching the retained freshness rule. The
/// receipt always uses the durable aggregate version, while the work item uses
/// the refreshed owner version.
pub async fn project_event(
    connection: &mut PgConnection,
    sources: &mut impl ProjectionOwnerSources,
    message: &EventMessage,
    now: i64,
) -> Result<ProjectionResult, BusinessError> {
    let flags = event_work(message);
    if !flags.any() {
        return Ok(ProjectionResult::Unhandled);
    }

    let mut state = message.payload.state.clone();
    let mut version = message.aggregate_version;
    let mut company_id = message.payload.company_id.clone();
    let mut branch_id = message.payload.branch_id.clone();
    let mut department_id = message.payload.department_id.clone();

    let refresh_owner = flags.is_task
        || (state.is_none() && (flags.is_leave || flags.is_overtime || flags.is_payroll));
    if refresh_owner {
        let source = if flags.is_task {
            sources.task(&message.aggregate_id).await?
        } else if flags.is_overtime {
            sources.overtime(&message.aggregate_id).await?
        } else if flags.is_leave {
            sources.leave(&message.aggregate_id).await?
        } else {
            sources.payroll(&message.aggregate_id).await?
        }
        .ok_or_else(source_unavailable)?;
        state = Some(source.state);
        version = source.version;
        company_id = source.company_id;
        branch_id = source.branch_id;
        department_id = source.department_id;
    }

    let work = projection_work(flags, state.as_deref());

    sqlx::query(
        "INSERT INTO projection_receipts(event_id,aggregate_id,aggregate_version,applied_at) \
         VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING",
    )
    .bind(&message.id)
    .bind(&message.aggregate_id)
    .bind(message.aggregate_version)
    .bind(now)
    .execute(&mut *connection)
    .await
    .map_err(projection_write_failed)?;

    if let Some(work) = work {
        commit_work(
            connection,
            message,
            work,
            &ProjectionWorkFacts {
                version,
                state: state.as_deref(),
                company_id: &company_id,
                branch_id: branch_id.as_deref(),
                department_id: department_id.as_deref(),
                now,
            },
        )
        .await?;
    }

    sqlx::query(
        "INSERT INTO projection_checkpoints(projector,last_event_id,last_event_at,updated_at) \
         VALUES('work-items',$1,$2,$3) ON CONFLICT(projector) DO UPDATE SET \
         last_event_id=excluded.last_event_id,last_event_at=excluded.last_event_at,\
         updated_at=excluded.updated_at",
    )
    .bind(&message.id)
    .bind(now)
    .bind(now)
    .execute(&mut *connection)
    .await
    .map_err(projection_write_failed)?;

    Ok(ProjectionResult::Projected)
}

struct ProjectionWorkFacts<'a> {
    version: i64,
    state: Option<&'a str>,
    company_id: &'a str,
    branch_id: Option<&'a str>,
    department_id: Option<&'a str>,
    now: i64,
}

async fn commit_work(
    connection: &mut PgConnection,
    message: &EventMessage,
    work: ProjectionWork,
    facts: &ProjectionWorkFacts<'_>,
) -> Result<(), BusinessError> {
    sqlx::query(
        "INSERT INTO work_items(id,source_type,source_id,source_version,title,subtitle,queue,\
         company_id,branch_id,department_id,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) \
         ON CONFLICT(source_type,source_id) DO UPDATE SET \
         source_version=excluded.source_version,subtitle=excluded.subtitle,queue=excluded.queue,\
         company_id=excluded.company_id,branch_id=excluded.branch_id,\
         department_id=excluded.department_id,updated_at=excluded.updated_at \
         WHERE work_items.source_version<excluded.source_version",
    )
    .bind(format!("work_{}", message.aggregate_id))
    .bind(work.source_type)
    .bind(&message.aggregate_id)
    .bind(facts.version)
    .bind(work.title)
    .bind(facts.state)
    .bind(work.queue.as_str())
    .bind(facts.company_id)
    .bind(facts.branch_id)
    .bind(facts.department_id)
    .bind(facts.now)
    .execute(&mut *connection)
    .await
    .map_err(projection_write_failed)?;
    Ok(())
}

fn source_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROJECTION_SOURCE_UNAVAILABLE",
        "The Workflow projection owner source is unavailable.",
    )
}

/// Preserve serialization/deadlock classification for a caller retry while
/// keeping every other persistence failure sanitized.
fn projection_write_failed(error: sqlx::Error) -> BusinessError {
    let persistence = PersistenceError::from(error);
    if persistence.retryable() {
        BusinessError::new(
            FailureKind::Conflict,
            "PROJECTION_RETRY_REQUIRED",
            "The Workflow projection must be retried.",
        )
    } else {
        BusinessError::new(
            FailureKind::Unavailable,
            "PROJECTION_WRITE_FAILED",
            "The Workflow projection could not be written.",
        )
    }
}
