//! Native Quality read and cancellation use cases.
//!
//! The repository owns the current Holding transaction and SQL effects; this
//! layer owns current permission, Company/branch/department scope, the
//! retained decision order and the Qualified error codes. It never mints IAM,
//! purchase or Workflow authority.
use crate::domain::quality::{
    CancellationAction, CancellationPlan, HoldStatus, InspectionResult, QualityDecision,
    QualityHoldFacts, QualityMetric, evaluate_measurements, next_action,
    quality_decision_with_same_person,
    validate_reason,
};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, RecordId, UserId,
};
use std::future::Future;
use std::pin::Pin;

pub type QualityFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct QualityActor {
    pub user_id: UserId,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub permissions: Vec<String>,
}

impl QualityActor {
    pub fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    pub fn can(&self, permission: &str) -> bool {
        self.has(permission)
    }
}

fn scope_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "QUALITY_SCOPE_REQUIRED",
        "Use a Company or Branch quality assignment.",
    )
}

fn access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "QUALITY_ACCESS_CHANGED",
        "Choose a current authorized Quality assignment.",
    )
}

fn hold_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "QUALITY_HOLD_NOT_FOUND",
        "Quality hold not found.",
    )
}

fn require_read(actor: &QualityActor) -> Result<(), BusinessError> {
    if actor.department_id.is_some() {
        return Err(scope_required());
    }
    if !actor.has("quality.read") {
        return Err(access_changed());
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityHold {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub lot_id: Option<String>,
    pub reason: String,
    pub due_date: String,
    pub status: HoldStatus,
    pub created_by: String,
    pub last_inspector: Option<String>,
    pub last_result: Option<InspectionResult>,
    pub released_by: Option<String>,
    pub release_reason: Option<String>,
    pub released_at: Option<i64>,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub spec_id: Option<String>,
    pub spec_version: Option<i64>,
    pub quantity_scope: String,
    pub held_quantity_milli: Option<i64>,
    pub source_type: Option<String>,
    pub source_id: Option<String>,
    pub cancel_requested_by: Option<String>,
    pub cancel_reason: Option<String>,
    pub cancelled_by: Option<String>,
    pub cancelled_at: Option<i64>,
    pub disposition_kind: Option<String>,
    pub disposition_reason: Option<String>,
    pub disposition_requested_by: Option<String>,
    pub disposition_count_id: Option<String>,
    pub disposition_quantity_milli: Option<i64>,
    pub disposed_by: Option<String>,
    pub disposed_at: Option<i64>,
    pub inspection_count: i64,
}

impl QualityHold {
    pub fn facts(&self) -> QualityHoldFacts {
        QualityHoldFacts {
            id: self.id.clone(),
            status: self.status,
            version: self.version,
            created_by: self.created_by.clone(),
            cancel_requested_by: self.cancel_requested_by.clone(),
            last_inspector: self.last_inspector.clone(),
            last_result: self.last_result,
            has_inspections: self.inspection_count > 0,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityInspection {
    pub id: String,
    pub hold_id: String,
    pub reviewed_version: i64,
    pub result: InspectionResult,
    pub evidence: String,
    pub inspected_by: String,
    pub created_at: i64,
    pub spec_id: Option<String>,
    pub spec_version: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityMeasurement {
    pub id: String,
    pub inspection_id: String,
    pub metric_id: String,
    pub code: String,
    pub label: String,
    pub unit: String,
    pub minimum: Option<String>,
    pub maximum: Option<String>,
    pub measured_value: String,
    pub passed: bool,
    pub ordinal: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityCancellation {
    pub id: String,
    pub hold_id: String,
    pub version: i64,
    pub action: CancellationAction,
    pub reason: String,
    pub actor_id: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityDisposition {
    pub id: String,
    pub hold_id: String,
    pub count_id: String,
    pub action: String,
    pub actor_id: String,
    pub reviewed_version: i64,
    pub reason: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityAlert {
    pub id: String,
    pub hold_id: String,
    pub phase: String,
    pub source_version: i64,
    pub due_date: String,
    pub detected_at: i64,
    pub version: i64,
    pub acknowledged_by: Option<String>,
    pub acknowledged_at: Option<i64>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityWorkspaceData {
    pub holds: Vec<QualityHold>,
    pub specification_metrics: Vec<QualityMetric>,
    pub alerts: Vec<QualityAlert>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityDetailData {
    pub hold: QualityHold,
    pub inspections: Vec<QualityInspection>,
    pub measurements: Vec<QualityMeasurement>,
    pub cancellations: Vec<QualityCancellation>,
    pub dispositions: Vec<QualityDisposition>,
    pub spec_metrics: Vec<QualityMetric>,
    pub alerts: Vec<QualityAlert>,
}

/// Detail source facts excluding cancellation history, which is fetched via
/// an explicit complete-history page contract rather than an empty array.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityDetailCoreData {
    pub hold: QualityHold,
    pub inspections: Vec<QualityInspection>,
    pub measurements: Vec<QualityMeasurement>,
    pub dispositions: Vec<QualityDisposition>,
    pub spec_metrics: Vec<QualityMetric>,
    pub alerts: Vec<QualityAlert>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityHoldView {
    pub hold: QualityHold,
    pub decision: QualityDecision,
    pub next_action: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityWorkspace {
    pub holds: Vec<QualityHoldView>,
    pub can_hold: bool,
    pub can_manage_specs: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityDetail {
    pub hold: QualityHoldView,
    pub inspections: Vec<QualityInspection>,
    pub measurements: Vec<QualityMeasurement>,
    pub cancellations: Vec<QualityCancellation>,
    pub dispositions: Vec<QualityDisposition>,
    pub spec_metrics: Vec<QualityMetric>,
    pub alerts: Vec<QualityAlert>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityDetailCore {
    pub hold: QualityHoldView,
    pub inspections: Vec<QualityInspection>,
    pub measurements: Vec<QualityMeasurement>,
    pub dispositions: Vec<QualityDisposition>,
    pub spec_metrics: Vec<QualityMetric>,
    pub alerts: Vec<QualityAlert>,
}

pub fn quality_detail_core_from_source(
    actor: &QualityActor,
    data: QualityDetailCoreData,
) -> Result<QualityDetailCore, BusinessError> {
    quality_detail_core_with_same_person(actor, data, &|user| actor.user_id.as_str() == user)
}

/// The predicate is sourced from the selected actor's current Identity facts.
pub fn quality_detail_core_with_same_person(
    actor: &QualityActor,
    data: QualityDetailCoreData,
    same_person: &dyn Fn(&str) -> bool,
) -> Result<QualityDetailCore, BusinessError> {
    require_read(actor)?;
    if data.hold.company_id != actor.company_id.as_str()
        || actor
            .branch_id
            .as_ref()
            .is_some_and(|branch| data.hold.branch_id != branch.as_str())
    {
        return Err(hold_not_found());
    }
    let facts = crate::domain::quality::QualityActorFacts {
        user_id: actor.user_id.as_str().to_owned(),
        can_hold: actor.can("quality.hold"),
        can_inspect: actor.can("quality.inspect"),
        can_release: actor.can("quality.release"),
        company_scope: actor.branch_id.is_none(),
    };
    let decision = quality_decision_with_same_person(&data.hold.facts(), &facts, same_person);
    let next_action = next_action(data.hold.status, data.hold.last_result, decision);
    Ok(QualityDetailCore {
        hold: QualityHoldView {
            hold: data.hold,
            decision,
            next_action,
        },
        inspections: data.inspections,
        measurements: data.measurements,
        dispositions: data.dispositions,
        spec_metrics: data.spec_metrics,
        alerts: data.alerts,
    })
}

pub trait QualityReadRepository: Send + Sync {
    fn workspace<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch: Option<&'a RecordId>,
    ) -> QualityFuture<'a, QualityWorkspaceData>;

    fn detail<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch: Option<&'a RecordId>,
        id: &'a str,
    ) -> QualityFuture<'a, Option<QualityDetailData>>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityCancellationCommand {
    pub command_key: CommandKey,
    pub hold_id: RecordId,
    pub expected_version: i64,
    pub action: CancellationAction,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityCancellationReceipt {
    pub id: String,
    pub hold_id: String,
    pub cancellation_id: String,
    pub status: HoldStatus,
    pub version: i64,
}

/// Normalize before constructing a Workflow request so its fingerprint and
/// the owner effect both use the same retained command input.
pub fn normalize_quality_cancellation(
    command: &QualityCancellationCommand,
) -> Result<QualityCancellationCommand, BusinessError> {
    let reason = validate_reason(
        &command.reason,
        "QUALITY_CANCELLATION_REASON",
        "Enter a cancellation reason between 3 and 2000 characters.",
    )?;
    Ok(QualityCancellationCommand {
        reason,
        ..command.clone()
    })
}

pub trait QualityCancellationRepository: Send + Sync {
    fn cancel<'a>(
        &'a mut self,
        command: &'a QualityCancellationCommand,
        company: &'a CompanyId,
        branch: Option<&'a RecordId>,
        actor_id: &'a UserId,
        now: i64,
    ) -> QualityFuture<'a, QualityCancellationReceipt>;
}

pub fn quality_workspace<'a, R: QualityReadRepository>(
    actor: &'a QualityActor,
    repository: &'a mut R,
) -> QualityFuture<'a, QualityWorkspace> {
    quality_workspace_with_same_person(actor, repository, move |user: &str| actor.user_id.as_str() == user)
}

/// The predicate must be bound to retained Identity facts in the caller's unit.
pub fn quality_workspace_with_same_person<'a, R: QualityReadRepository, P: Fn(&str) -> bool + Send + Sync + 'a>(
    actor: &'a QualityActor,
    repository: &'a mut R,
    same_person: P,
) -> QualityFuture<'a, QualityWorkspace> {
    Box::pin(async move {
        require_read(actor)?;
        let data = repository
            .workspace(&actor.company_id, actor.branch_id.as_ref())
            .await?;
        let can_hold = actor.can("quality.hold");
        let can_manage_specs = actor.branch_id.is_none() && actor.can("quality.release");
        let actor_facts = crate::domain::quality::QualityActorFacts {
            user_id: actor.user_id.as_str().to_owned(),
            can_hold,
            can_inspect: actor.can("quality.inspect"),
            can_release: actor.can("quality.release"),
            company_scope: actor.branch_id.is_none(),
        };
        let holds = data
            .holds
            .into_iter()
            .map(|hold| {
                let decision = quality_decision_with_same_person(&hold.facts(), &actor_facts, &same_person);
                let next_action = next_action(hold.status, hold.last_result, decision);
                QualityHoldView {
                    hold,
                    decision,
                    next_action,
                }
            })
            .collect();
        Ok(QualityWorkspace {
            holds,
            can_hold,
            can_manage_specs,
        })
    })
}

pub fn quality_detail<'a, R: QualityReadRepository>(
    actor: &'a QualityActor,
    id: &'a str,
    repository: &'a mut R,
) -> QualityFuture<'a, QualityDetail> {
    quality_detail_with_same_person(actor, id, repository, move |user: &str| actor.user_id.as_str() == user)
}

/// The predicate must be bound to retained Identity facts in the caller's unit.
pub fn quality_detail_with_same_person<'a, R: QualityReadRepository, P: Fn(&str) -> bool + Send + Sync + 'a>(
    actor: &'a QualityActor,
    id: &'a str,
    repository: &'a mut R,
    same_person: P,
) -> QualityFuture<'a, QualityDetail> {
    Box::pin(async move {
        require_read(actor)?;
        let id = RecordId::new(id)?.as_str().to_owned();
        let data = repository
            .detail(&actor.company_id, actor.branch_id.as_ref(), &id)
            .await?
            .ok_or_else(hold_not_found)?;
        let actor_facts = crate::domain::quality::QualityActorFacts {
            user_id: actor.user_id.as_str().to_owned(),
            can_hold: actor.can("quality.hold"),
            can_inspect: actor.can("quality.inspect"),
            can_release: actor.can("quality.release"),
            company_scope: actor.branch_id.is_none(),
        };
        let decision = quality_decision_with_same_person(&data.hold.facts(), &actor_facts, &same_person);
        let next_action = next_action(data.hold.status, data.hold.last_result, decision);
        Ok(QualityDetail {
            hold: QualityHoldView {
                hold: data.hold,
                decision,
                next_action,
            },
            inspections: data.inspections,
            measurements: data.measurements,
            cancellations: data.cancellations,
            dispositions: data.dispositions,
            spec_metrics: data.spec_metrics,
            alerts: data.alerts,
        })
    })
}

pub fn cancel_quality_hold<'a, R: QualityCancellationRepository>(
    actor: &'a QualityActor,
    command: QualityCancellationCommand,
    repository: &'a mut R,
    clock: &'a (dyn Clock + Sync),
) -> QualityFuture<'a, QualityCancellationReceipt> {
    Box::pin(async move {
        require_read(actor)?;
        if !actor.can(command.action.permission()) {
            return Err(access_changed());
        }
        let reason = validate_reason(
            &command.reason,
            "QUALITY_CANCELLATION_REASON",
            "Enter a cancellation reason between 3 and 2000 characters.",
        )?;
        let command = QualityCancellationCommand { reason, ..command };
        repository
            .cancel(
                &command,
                &actor.company_id,
                actor.branch_id.as_ref(),
                &actor.user_id,
                clock.now().get(),
            )
            .await
    })
}

// Re-export the pure plan helper for adapters and tests that hold the locked
// row directly.
pub use crate::domain::quality::plan_cancellation as plan_quality_cancellation;

pub fn plan_for_locked_hold(
    hold: &QualityHold,
    actor_id: &str,
    action: CancellationAction,
    expected_version: i64,
) -> Result<CancellationPlan, BusinessError> {
    plan_quality_cancellation(&hold.facts(), actor_id, action, expected_version)
}

pub fn evaluate_inspection(
    metrics: &[QualityMetric],
    measurements: &[crate::domain::quality::MeasurementInput],
    requested: Option<InspectionResult>,
) -> Result<crate::domain::quality::MeasurementEvaluation, BusinessError> {
    evaluate_measurements(metrics, measurements, requested)
}
