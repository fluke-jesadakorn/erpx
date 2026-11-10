//! Quality owner policy for holds, inspections, release gates and cancellation.
//!
//! These are portable facts and decisions. Current IAM authority, the database
//! unit and cross-owner presentation facts are supplied by the composition.
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;

pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HoldStatus {
    Held,
    AwaitingRelease,
    Released,
    CancelPending,
    Cancelled,
    DispositionPending,
    Disposed,
}

impl HoldStatus {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "held" => Some(Self::Held),
            "awaiting_release" => Some(Self::AwaitingRelease),
            "released" => Some(Self::Released),
            "cancel_pending" => Some(Self::CancelPending),
            "cancelled" => Some(Self::Cancelled),
            "disposition_pending" => Some(Self::DispositionPending),
            "disposed" => Some(Self::Disposed),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Held => "held",
            Self::AwaitingRelease => "awaiting_release",
            Self::Released => "released",
            Self::CancelPending => "cancel_pending",
            Self::Cancelled => "cancelled",
            Self::DispositionPending => "disposition_pending",
            Self::Disposed => "disposed",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Released | Self::Cancelled | Self::Disposed)
    }

    pub fn is_inspectable(self) -> bool {
        matches!(self, Self::Held | Self::AwaitingRelease)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpecStatus {
    Draft,
    Active,
    Superseded,
    Cancelled,
}

impl SpecStatus {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "draft" => Some(Self::Draft),
            "active" => Some(Self::Active),
            "superseded" => Some(Self::Superseded),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::Superseded => "superseded",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InspectionResult {
    Pass,
    Fail,
}

impl InspectionResult {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pass" => Some(Self::Pass),
            "fail" => Some(Self::Fail),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancellationAction {
    Request,
    Approve,
    Reject,
}

impl CancellationAction {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "request" => Some(Self::Request),
            "approve" => Some(Self::Approve),
            "reject" => Some(Self::Reject),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Approve => "approve",
            Self::Reject => "reject",
        }
    }

    pub fn permission(self) -> &'static str {
        match self {
            Self::Request => "quality.hold",
            Self::Approve | Self::Reject => "quality.release",
        }
    }
}

/// A versioned acceptance limit captured on the inspection specification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityMetric {
    pub id: String,
    pub spec_id: Option<String>,
    pub code: String,
    pub label: String,
    pub unit: String,
    pub minimum: Option<String>,
    pub maximum: Option<String>,
    pub ordinal: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MeasurementInput {
    pub metric_id: String,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MeasurementEvaluation {
    pub provided: BTreeMap<String, String>,
    pub result: InspectionResult,
}

fn measured_number(value: &str) -> Option<f64> {
    if value.is_empty() || value.len() > 32 {
        return None;
    }
    let number = value.parse::<f64>().ok()?;
    number.is_finite().then_some(number)
}

pub fn quality_measurement_passed(metric: &QualityMetric, value: &str) -> bool {
    let Some(measured) = measured_number(value) else {
        return false;
    };
    // An invalid stored bound is not an absent bound. Treating it as absent
    // could incorrectly release stock if the captured specification is corrupt.
    metric
        .minimum
        .as_deref()
        .is_none_or(|bound| measured_number(bound).is_some_and(|minimum| measured >= minimum))
        && metric
            .maximum
            .as_deref()
            .is_none_or(|bound| measured_number(bound).is_some_and(|maximum| measured <= maximum))
}

/// Retained inspection derivation: every configured metric is required, the
/// result is calculated from the captured limits and an explicitly conflicting
/// result is rejected. Holds without a specification require an explicit
/// pass/fail result and may not send measurements.
pub fn evaluate_measurements(
    metrics: &[QualityMetric],
    measurements: &[MeasurementInput],
    requested: Option<InspectionResult>,
) -> Result<MeasurementEvaluation, BusinessError> {
    let mut provided = BTreeMap::new();
    for measurement in measurements {
        if measurement.metric_id.is_empty()
            || provided
                .insert(measurement.metric_id.clone(), measurement.value.clone())
                .is_some()
        {
            return Err(BusinessError::invalid(
                "QUALITY_MEASUREMENTS_REQUIRED",
                "Record one value for every measurement in the held specification.",
            ));
        }
    }
    if metrics.is_empty() {
        if !measurements.is_empty() {
            return Err(BusinessError::invalid(
                "QUALITY_SPEC_REQUIRED",
                "This hold has no measurement specification.",
            ));
        }
        let Some(result) = requested else {
            return Err(BusinessError::invalid(
                "QUALITY_RESULT_REQUIRED",
                "Choose pass or fail for a hold without a structured specification.",
            ));
        };
        return Ok(MeasurementEvaluation { provided, result });
    }
    if provided.len() != measurements.len()
        || provided.len() != metrics.len()
        || metrics
            .iter()
            .any(|metric| !provided.contains_key(&metric.id))
    {
        return Err(BusinessError::invalid(
            "QUALITY_MEASUREMENTS_REQUIRED",
            "Record one value for every measurement in the held specification.",
        ));
    }
    let result = if metrics.iter().all(|metric| {
        quality_measurement_passed(
            metric,
            provided.get(&metric.id).map(String::as_str).unwrap_or(""),
        )
    }) {
        InspectionResult::Pass
    } else {
        InspectionResult::Fail
    };
    if requested.is_some_and(|requested| requested != result) {
        return Err(BusinessError::invalid(
            "QUALITY_RESULT_DERIVED",
            "The inspection result is calculated from the recorded limits.",
        ));
    }
    Ok(MeasurementEvaluation { provided, result })
}

/// A retained Quality hold decision. `None` timestamps are intentional and
/// distinguish an uninspected hold from a cancelled or released one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityHoldFacts {
    pub id: String,
    pub status: HoldStatus,
    pub version: i64,
    pub created_by: String,
    pub cancel_requested_by: Option<String>,
    pub last_inspector: Option<String>,
    pub last_result: Option<InspectionResult>,
    pub has_inspections: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QualityDecision {
    pub can_inspect: bool,
    pub can_release: bool,
    pub can_cancel: bool,
    pub can_review_cancellation: bool,
    pub can_disposition: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityActorFacts {
    pub user_id: String,
    pub can_hold: bool,
    pub can_inspect: bool,
    pub can_release: bool,
    pub company_scope: bool,
}

fn same_person(left: &str, right: &str) -> bool {
    left == right
}

fn same_optional(left: &str, right: &Option<String>) -> bool {
    right.as_deref().is_some_and(|right| same_person(left, right))
}

/// Mirrors `qualityDetail` after the caller has already enforced `quality.read`
/// and rejected a department-scoped assignment.
pub fn quality_decision(hold: &QualityHoldFacts, actor: &QualityActorFacts) -> QualityDecision {
    quality_decision_with_same_person(hold, actor, &|user| same_person(&actor.user_id, user))
}

/// The predicate must come from the current selected actor's retained Identity
/// facts. This projection does not establish authority or authorize writes.
pub fn quality_decision_with_same_person(
    hold: &QualityHoldFacts,
    actor: &QualityActorFacts,
    same_person: &dyn Fn(&str) -> bool,
) -> QualityDecision {
    let status = hold.status;
    let self_created = same_person(&hold.created_by);
    let self_inspected = hold.last_inspector.as_deref().is_some_and(same_person);
    let self_requested = hold.cancel_requested_by.as_deref().is_some_and(same_person);
    QualityDecision {
        can_inspect: status.is_inspectable() && actor.can_inspect && !self_created,
        can_release: status == HoldStatus::AwaitingRelease
            && hold.last_result == Some(InspectionResult::Pass)
            && actor.can_release
            && !self_created
            && !self_inspected,
        can_cancel: status == HoldStatus::Held && hold.last_result.is_none() && actor.can_hold,
        can_review_cancellation: status == HoldStatus::CancelPending
            && actor.can_release
            && !self_created
            && !self_requested,
        can_disposition: status == HoldStatus::Held
            && hold.last_result == Some(InspectionResult::Fail)
            && actor.can_release
            && !self_created
            && !self_inspected,
    }
}

/// The retained next-action copy in the Quality detail presenter.
pub fn next_action(
    status: HoldStatus,
    last_result: Option<InspectionResult>,
    decision: QualityDecision,
) -> &'static str {
    if decision.can_disposition {
        "Request controlled scrap disposal"
    } else if decision.can_release {
        "Approve release of the passing inspection"
    } else if decision.can_review_cancellation {
        "Review cancellation of the incorrect hold"
    } else if decision.can_inspect {
        "Record an independent inspection"
    } else if decision.can_cancel {
        "Request cancellation of the incorrect hold"
    } else if status.is_terminal() {
        "Review the recorded outcome"
    } else if status == HoldStatus::DispositionPending {
        "Wait for Company Finance to review the controlled scrap"
    } else if status == HoldStatus::Held && last_result == Some(InspectionResult::Fail) {
        "Wait for an independent Quality manager"
    } else {
        "Wait for an eligible independent Quality reviewer"
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CancellationPlan {
    pub request: bool,
    pub target: HoldStatus,
    pub version: i64,
}

fn cancellation_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CONFLICT",
        "The Quality hold changed. Reload and review before submitting.",
    )
}

fn uninspected_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "UNINSPECTED_HOLD_REQUIRED",
        "Only an uninspected hold can be cancelled. Failed or passing inspections must follow quality disposition.",
    )
}

fn independent_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "INDEPENDENT_CANCELLATION_REQUIRED",
        "A different Quality manager must review cancellation.",
    )
}

/// Cancellation is a reviewed correction of an uninspected hold, never a
/// failed-inspection bypass. The expected version is the version the reviewer
/// actually saw; the first write always advances it by exactly one.
pub fn plan_cancellation(
    hold: &QualityHoldFacts,
    actor_id: &str,
    action: CancellationAction,
    expected_version: i64,
) -> Result<CancellationPlan, BusinessError> {
    if !(1..=MAX_SAFE_INTEGER).contains(&expected_version) || hold.version != expected_version {
        return Err(cancellation_changed());
    }
    if hold.last_result.is_some() || hold.has_inspections {
        return Err(uninspected_required());
    }
    match action {
        CancellationAction::Request => {
            if hold.status != HoldStatus::Held {
                return Err(uninspected_required());
            }
            Ok(CancellationPlan {
                request: true,
                target: HoldStatus::CancelPending,
                version: expected_version + 1,
            })
        }
        CancellationAction::Approve | CancellationAction::Reject => {
            if hold.status != HoldStatus::CancelPending {
                return Err(uninspected_required());
            }
            if same_person(actor_id, &hold.created_by)
                || same_optional(actor_id, &hold.cancel_requested_by)
            {
                return Err(independent_required());
            }
            Ok(CancellationPlan {
                request: false,
                target: if action == CancellationAction::Approve {
                    HoldStatus::Cancelled
                } else {
                    HoldStatus::Held
                },
                version: expected_version + 1,
            })
        }
    }
}

/// Zod-compatible trimmed text length in UTF-16 code units.
pub fn validate_text(
    value: &str,
    minimum: usize,
    maximum: usize,
    code: &'static str,
    message: &'static str,
) -> Result<String, BusinessError> {
    let value = value.trim_matches(|c: char| matches!(c,
        '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' |
        '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' |
        '\u{205f}' | '\u{3000}' | '\u{feff}'));
    let length = value.encode_utf16().count();
    if !(minimum..=maximum).contains(&length) {
        return Err(BusinessError::invalid(code, message));
    }
    Ok(value.to_owned())
}

pub fn validate_reason(
    value: &str,
    code: &'static str,
    message: &'static str,
) -> Result<String, BusinessError> {
    validate_text(value, 3, 2000, code, message)
}
