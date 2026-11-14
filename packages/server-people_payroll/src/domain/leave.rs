//! Native leave requests: draft/create/submit, approval, cancellation review,
//! evidence policy/manifest facts and the scheduled accrual plan.
//!
//! This module validates the exact retained TypeScript leave command/read
//! shapes. It never reads or writes Workflow, Files, Organization, Operations
//! or Calendar rows; those owners are supplied through public ports at
//! composition. Company branch/department placement is carried on the request
//! and checked against the supplied actor scope.
use erpx_server_shared::public::{BusinessError, FailureKind, TimestampMillis};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

pub const LEAVE_CREATE_OPERATION: &str = "leave.create";
pub const LEAVE_SAVE_OPERATION: &str = "leave.save";
pub const LEAVE_SUBMIT_OPERATION: &str = "leave.submit";
pub const LEAVE_APPROVE_OPERATION: &str = "leave.approve";
pub const LEAVE_REJECT_OPERATION: &str = "leave.reject";
pub const LEAVE_CANCELLATION_OPERATION: &str = "leave.cancellation";
pub const LEAVE_CANCELLATION_APPROVE_OPERATION: &str = "leave.cancellation.approve";
pub const LEAVE_CANCELLATION_REJECT_OPERATION: &str = "leave.cancellation.reject";
pub const LEAVE_RECORD_TYPE: &str = "leave_requests";
/// Workflow approval record type. The retained approval table checks this exact
/// value, while audit/outbox keep the People table name.
pub const LEAVE_APPROVAL_RECORD_TYPE: &str = "leave";
pub const LEAVE_CANCELLATION_RECORD_TYPE: &str = "leave_cancellation";

pub const LEAVE_EVIDENCE_PURPOSES: [&str; 2] = ["medical_certificate", "leave_supporting_photo"];

const MAX_NOTE: usize = 2_048;
const MAX_REASON: usize = 2_048;

/// One native leave request row plus the retained leave-type display name.
#[derive(Clone, Debug, Serialize)]
pub struct LeaveRequestRow {
    pub id: String,
    pub number: String,
    pub employee_id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub person_id: Option<String>,
    pub engagement_id: Option<String>,
    pub start_date: String,
    pub end_date: Option<String>,
    pub days: i64,
    pub calculated_units: Option<f64>,
    pub handover_note: String,
    pub leave_type_id: Option<String>,
    pub leave_type_name: Option<String>,
    pub policy_version_id: Option<String>,
    pub state: String,
    pub version: i64,
    pub created_by: String,
    pub prepared_by: Option<String>,
    pub decided_by: Option<String>,
    pub decision_note: Option<String>,
    pub cancellation_state: Option<String>,
    pub evidence_policy_snapshot: Option<Value>,
    pub evidence_due_at: Option<i64>,
    pub evidence_version: i64,
    pub submitted_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl LeaveRequestRow {
    pub fn is_requester(&self, actor: &str) -> bool {
        self.created_by == actor
    }

    pub fn editable(&self) -> bool {
        matches!(self.state.as_str(), "draft" | "rejected")
    }

    pub fn units(&self) -> f64 {
        self.calculated_units.unwrap_or(self.days as f64)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct LeaveCreateInput {
    pub start_date: String,
    pub days: i64,
    pub handover_note: String,
    pub leave_type_id: Option<String>,
    pub end_date: Option<String>,
    pub start_segment: String,
    pub end_segment: String,
    pub calculated_units: Option<f64>,
    pub policy_version_id: Option<String>,
    pub evidence_policy_snapshot: Option<Value>,
    /// `true` creates a submitted request; the retained legacy route creates a
    /// draft. Self-service composition passes its preview result here.
    pub submit: bool,
}

impl LeaveCreateInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let start_date = date(raw, "startDate")?;
        let calculated_units = optional_number(raw, "calculatedUnits", 0.0, 365.0)?;
        let days = if raw.get("days").is_some_and(|value| !value.is_null()) {
            integer(raw, "days", 1, 365)?
        } else {
            calculated_units
                .map(|units| units.ceil() as i64)
                .filter(|days| (1..=365).contains(days))
                .ok_or_else(request_invalid)?
        };
        let end_date = optional_date(raw, "endDate")?;
        if end_date
            .as_deref()
            .is_some_and(|end| end < start_date.as_str())
        {
            return Err(BusinessError::invalid(
                "LEAVE_DATE_RANGE",
                "End date must be on or after the start date.",
            ));
        }
        let start_segment = segment(raw, "startSegment")?;
        let end_segment = segment(raw, "endSegment")?;
        let submit = match raw.get("submit") {
            Some(Value::Bool(value)) => *value,
            Some(_) => return Err(request_invalid()),
            None => match raw.get("saveOnly") {
                Some(Value::Bool(save_only)) => !save_only,
                Some(_) => return Err(request_invalid()),
                // The retained legacy `newLeaveSchema` has no saveOnly field and
                // always creates a draft.
                None => false,
            },
        };
        Ok(Self {
            start_date,
            days,
            handover_note: note(raw, "handoverNote", 0, MAX_NOTE, false)?,
            leave_type_id: optional_identifier(raw, "leaveTypeId")?,
            end_date,
            start_segment,
            end_segment,
            calculated_units,
            policy_version_id: optional_identifier(raw, "policyVersionId")?,
            evidence_policy_snapshot: optional_policy(raw, "evidencePolicy")?,
            submit,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct LeaveDraftInput {
    pub start_date: String,
    pub days: i64,
    pub handover_note: String,
    pub expected_version: i64,
    pub save_only: bool,
    pub leave_type_id: Option<String>,
    pub end_date: Option<String>,
    pub start_segment: String,
    pub end_segment: String,
    pub calculated_units: Option<f64>,
    pub policy_version_id: Option<String>,
    pub evidence_policy_snapshot: Option<Value>,
}

impl LeaveDraftInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let start_date = date(raw, "startDate")?;
        let calculated_units = optional_number(raw, "calculatedUnits", 0.0, 365.0)?;
        let days = if raw.get("days").is_some_and(|value| !value.is_null()) {
            integer(raw, "days", 1, 365)?
        } else {
            calculated_units
                .map(|units| units.ceil() as i64)
                .filter(|days| (1..=365).contains(days))
                .ok_or_else(request_invalid)?
        };
        let end_date = optional_date(raw, "endDate")?;
        if end_date
            .as_deref()
            .is_some_and(|end| end < start_date.as_str())
        {
            return Err(BusinessError::invalid(
                "LEAVE_DATE_RANGE",
                "End date must be on or after the start date.",
            ));
        }
        Ok(Self {
            start_date,
            days,
            handover_note: note(raw, "handoverNote", 0, MAX_NOTE, false)?,
            expected_version: positive(raw, "expectedVersion")?,
            save_only: flag(raw, "saveOnly").unwrap_or(false),
            leave_type_id: optional_identifier(raw, "leaveTypeId")?,
            end_date,
            start_segment: segment(raw, "startSegment")?,
            end_segment: segment(raw, "endSegment")?,
            calculated_units,
            policy_version_id: optional_identifier(raw, "policyVersionId")?,
            evidence_policy_snapshot: optional_policy(raw, "evidencePolicy")?,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct LeaveDecisionInput {
    pub expected_version: i64,
    pub note: String,
    pub override_decision: bool,
}

impl LeaveDecisionInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        Ok(Self {
            expected_version: positive(raw, "expectedVersion")?,
            note: note(raw, "note", 0, MAX_NOTE, false)?,
            override_decision: flag(raw, "override").unwrap_or(false),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaveLifecycleAction {
    Reject,
    Cancel,
}

#[derive(Clone, Debug, Serialize)]
pub struct LeaveLifecycleInput {
    pub action: LeaveLifecycleAction,
    pub expected_version: i64,
    pub note: String,
    pub override_decision: bool,
}

impl LeaveLifecycleInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let action = match text_required(raw, "action")?.as_str() {
            "reject" => LeaveLifecycleAction::Reject,
            "cancel" => LeaveLifecycleAction::Cancel,
            _ => return Err(request_invalid()),
        };
        Ok(Self {
            action,
            expected_version: positive(raw, "expectedVersion")?,
            note: note(raw, "note", 0, MAX_REASON, false)?,
            override_decision: flag(raw, "override").unwrap_or(false),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaveEvidencePolicy {
    pub required: bool,
    pub minimum_units: Option<f64>,
    pub purpose_key: Option<String>,
    pub minimum_count: i64,
    pub due_stage: String,
    pub grace_days: i64,
    pub privacy: String,
    pub note: String,
}

impl LeaveEvidencePolicy {
    pub fn none() -> Self {
        Self {
            required: false,
            minimum_units: None,
            purpose_key: None,
            minimum_count: 0,
            due_stage: "before_final_approval".to_owned(),
            grace_days: 0,
            privacy: "requester_approvers_hr".to_owned(),
            note: String::new(),
        }
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if !["before_submit", "before_final_approval"].contains(&self.due_stage.as_str())
            || self.privacy != "requester_approvers_hr"
            || !(0..=100).contains(&self.minimum_count)
            || !(0..=365).contains(&self.grace_days)
            || self
                .minimum_units
                .is_some_and(|value| !value.is_finite() || value <= 0.0)
            || self
                .purpose_key
                .as_deref()
                .is_some_and(|value| !LEAVE_EVIDENCE_PURPOSES.contains(&value))
        {
            return Err(request_invalid());
        }
        if self.required
            && (self.purpose_key.is_none()
                || self.minimum_units.is_none()
                || self.minimum_count < 1)
        {
            return Err(request_invalid());
        }
        Ok(())
    }
}

/// Parse the retained camelCase policy snapshot stored on the request.
pub fn parse_leave_evidence_policy(value: Option<&Value>) -> Option<LeaveEvidencePolicy> {
    let value = value?;
    let parsed = match value {
        Value::String(text) => serde_json::from_str::<Value>(text).ok()?,
        other => other.clone(),
    };
    let policy: LeaveEvidencePolicy = serde_json::from_value(parsed).ok()?;
    policy.validate().ok()?;
    Some(policy)
}

/// Policy rule facts read from the selected policy version.
#[derive(Clone, Debug)]
pub struct LeaveEvidencePolicyRule {
    pub evidence_min_units: Option<f64>,
    pub evidence_purpose: Option<String>,
    pub evidence_min_count: i64,
    pub evidence_due_stage: String,
    pub evidence_grace_days: i64,
    pub evidence_note: String,
    pub evidence_privacy: String,
}

pub fn evidence_policy(
    row: &LeaveRequestRow,
    rule: Option<&LeaveEvidencePolicyRule>,
) -> LeaveEvidencePolicy {
    evidence_policy_for(row.units(), row.evidence_policy_snapshot.as_ref(), rule)
}

pub fn evidence_policy_for(
    units: f64,
    snapshot: Option<&Value>,
    rule: Option<&LeaveEvidencePolicyRule>,
) -> LeaveEvidencePolicy {
    if let Some(snapshot) = parse_leave_evidence_policy(snapshot) {
        return snapshot;
    }
    let Some(rule) = rule else {
        return LeaveEvidencePolicy::none();
    };
    let minimum_units = rule.evidence_min_units;
    let minimum_count = rule.evidence_min_count;
    let purpose_key = rule.evidence_purpose.clone();
    let required = purpose_key.is_some()
        && minimum_units.is_some_and(|minimum| units >= minimum)
        && minimum_count > 0;
    LeaveEvidencePolicy {
        required,
        minimum_units,
        purpose_key,
        minimum_count,
        due_stage: rule.evidence_due_stage.clone(),
        grace_days: rule.evidence_grace_days,
        privacy: rule.evidence_privacy.clone(),
        note: rule.evidence_note.clone(),
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct LeaveEvidenceFile {
    pub link_id: String,
    pub source_id: String,
    pub purpose_key: String,
    pub checksum_sha256: String,
    pub media_type: String,
    pub bytes: i64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct LeaveEvidenceManifest {
    pub canonical: Vec<LeaveEvidenceFile>,
    pub legacy: Vec<LeaveEvidenceFile>,
    pub legacy_included: bool,
    pub draft_set_ids: Vec<String>,
    pub pending: i64,
}

impl LeaveEvidenceManifest {
    pub fn files(&self) -> Vec<&LeaveEvidenceFile> {
        self.canonical.iter().chain(self.legacy.iter()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.canonical.is_empty() && self.legacy.is_empty()
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct LeaveEvidenceRequirement {
    pub count: i64,
    pub set_ids: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct LeaveEvidenceBindRequest<'a> {
    pub company_id: &'a str,
    pub request_id: &'a str,
    pub approval_instance_id: &'a str,
    pub expected_version: i64,
    pub command_id: &'a str,
    pub draft_set_ids: &'a [String],
}

#[derive(Clone, Debug)]
pub struct LeaveEvidenceManifestWrite<'a> {
    pub id: &'a str,
    pub request_id: &'a str,
    pub company_id: &'a str,
    pub approval_instance_id: &'a str,
    pub decision_command_id: &'a str,
    pub request_version: i64,
    pub policy_version_id: Option<&'a str>,
    pub reviewer_user_id: &'a str,
    pub manifest_json: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct LeaveBalance {
    pub available: f64,
    pub reserved: f64,
    pub used: f64,
    pub accrued: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum LeaveBalanceAction {
    Use,
    Release,
    Reverse,
}

#[derive(Clone, Debug)]
pub struct LeaveBalanceTransition {
    pub kind: &'static str,
    pub multiplier: f64,
    pub note: &'static str,
    pub action: LeaveBalanceAction,
}

#[derive(Clone, Debug)]
pub struct LeaveBalanceWrite<'a> {
    pub id: &'a str,
    pub key: &'a str,
    pub kind: &'a str,
    pub multiplier: f64,
    pub note: &'a str,
    pub actor: &'a str,
    pub now: i64,
    pub request_id: &'a str,
    pub company_id: &'a str,
    pub action: LeaveBalanceAction,
}

/// Retained balance effects. Usage reads the negative reservation before its
/// release is inserted; the adapter computes the grouped units from the ledger.
pub fn balance_transitions(action: LeaveBalanceAction) -> Vec<LeaveBalanceTransition> {
    match action {
        LeaveBalanceAction::Use => vec![
            LeaveBalanceTransition {
                kind: "usage",
                multiplier: 1.0,
                note: "Approved leave",
                action,
            },
            LeaveBalanceTransition {
                kind: "reservation_release",
                multiplier: -1.0,
                note: "Released reservation",
                action,
            },
        ],
        LeaveBalanceAction::Release => vec![LeaveBalanceTransition {
            kind: "reservation_release",
            multiplier: -1.0,
            note: "Released reservation",
            action,
        }],
        LeaveBalanceAction::Reverse => vec![LeaveBalanceTransition {
            kind: "usage_reversal",
            multiplier: -1.0,
            note: "Approved cancellation",
            action,
        }],
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LeaveWriteResult {
    pub id: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancellation_state: Option<String>,
    pub version: i64,
}

#[derive(Clone, Debug)]
pub struct LeaveTransitionPlan {
    pub state: String,
    pub cancellation_state: Option<String>,
    pub version: i64,
}

#[derive(Clone, Debug)]
pub struct LeaveDecisionPlan {
    pub state: String,
    pub version: i64,
    pub final_decision: bool,
    pub apply_balance: Option<LeaveBalanceAction>,
}

#[derive(Clone, Debug)]
pub struct LeaveSubmitPlan {
    pub state: String,
    pub version: i64,
}

/// Retained submit/save rules: only the requester may change a draft/rejected
/// request, the request version must match, and placement must be current.
pub fn plan_submit(
    actor_user_id: &str,
    input: &LeaveDraftInput,
    row: &LeaveRequestRow,
    engagement_id: &str,
    engagement_version: i64,
    expected_engagement_version: Option<i64>,
) -> Result<LeaveSubmitPlan, BusinessError> {
    if !row.is_requester(actor_user_id) {
        return Err(forbidden(
            "REQUESTER_REQUIRED",
            "Only the requester can submit or correct this request.",
        ));
    }
    if row.version != input.expected_version {
        return Err(conflict(
            "LEAVE_CHANGED",
            "This request changed. Reload to review the current version.",
        ));
    }
    if !row.editable() {
        return Err(conflict(
            "LEAVE_CHANGED",
            "This request can no longer be edited.",
        ));
    }
    if row
        .engagement_id
        .as_deref()
        .is_some_and(|current| current != engagement_id)
    {
        return Err(conflict(
            "ENGAGEMENT_CHANGED",
            "Start a new request for your current engagement.",
        ));
    }
    if expected_engagement_version.is_some_and(|expected| expected != engagement_version) {
        return Err(conflict(
            "ENGAGEMENT_CHANGED",
            "Your placement changed. Reload before submitting.",
        ));
    }
    Ok(LeaveSubmitPlan {
        state: if input.save_only {
            "draft".to_owned()
        } else {
            "pending_approval".to_owned()
        },
        version: input.expected_version + 1,
    })
}

/// Retained decision rules. The caller must already hold the current Workflow
/// approval decision; this only maps it to leave state and balance effects.
pub fn plan_decision(
    input: &LeaveDecisionInput,
    row: &LeaveRequestRow,
    approve: bool,
    final_decision: bool,
    policy: &LeaveEvidencePolicy,
    manifest: &LeaveEvidenceManifest,
    available: &LeaveEvidenceRequirement,
) -> Result<LeaveDecisionPlan, BusinessError> {
    if row.version != input.expected_version {
        return Err(conflict(
            "LEAVE_CHANGED",
            "This leave request changed. Reload before deciding.",
        ));
    }
    if row.state != "pending_approval" {
        return Err(conflict(
            "LEAVE_CHANGED",
            "This leave request is no longer awaiting a decision.",
        ));
    }
    let final_decision = approve && final_decision;
    if final_decision {
        if manifest.pending > 0 {
            return Err(conflict(
                "LEAVE_EVIDENCE_SCANNING",
                "Wait for every leave evidence file to finish security scanning before final approval.",
            ));
        }
        if policy.required && policy.purpose_key.is_some() && available.count < policy.minimum_count
        {
            return Err(conflict(
                "LEAVE_EVIDENCE_REQUIRED",
                "Final approval requires the configured clean leave evidence files.",
            ));
        }
    }
    Ok(LeaveDecisionPlan {
        state: if !approve {
            "rejected".to_owned()
        } else if final_decision {
            "approved".to_owned()
        } else {
            "pending_approval".to_owned()
        },
        version: input.expected_version + 1,
        final_decision,
        apply_balance: if final_decision || !approve {
            Some(if final_decision {
                LeaveBalanceAction::Use
            } else {
                LeaveBalanceAction::Release
            })
        } else {
            None
        },
    })
}

/// Retained cancellation rules. An approved request opens a separate
/// cancellation review; every other state cancels directly and closes the
/// open leave approval.
pub fn plan_cancellation(
    actor_user_id: &str,
    input: &LeaveLifecycleInput,
    row: &LeaveRequestRow,
) -> Result<LeaveTransitionPlan, BusinessError> {
    if !row.is_requester(actor_user_id) {
        return Err(forbidden(
            "REQUESTER_REQUIRED",
            "Only the requester can cancel this request.",
        ));
    }
    if row.version != input.expected_version {
        return Err(conflict(
            "LEAVE_CHANGED",
            "This request changed. Reload before cancelling.",
        ));
    }
    if row.state == "approved" {
        if row.cancellation_state.as_deref() == Some("pending_review") {
            return Err(conflict(
                "CANCELLATION_PENDING",
                "Cancellation is already in review.",
            ));
        }
        if input.note.trim().is_empty() {
            return Err(BusinessError::invalid(
                "REASON_REQUIRED",
                "Explain why the approved leave should be cancelled.",
            ));
        }
        return Ok(LeaveTransitionPlan {
            state: "approved".to_owned(),
            cancellation_state: Some("pending_review".to_owned()),
            version: input.expected_version + 1,
        });
    }
    if !matches!(
        row.state.as_str(),
        "draft" | "rejected" | "pending_approval"
    ) {
        return Err(conflict(
            "LEAVE_CHANGED",
            "This leave request cannot be cancelled.",
        ));
    }
    Ok(LeaveTransitionPlan {
        state: "cancelled".to_owned(),
        cancellation_state: row.cancellation_state.clone(),
        version: input.expected_version + 1,
    })
}

pub fn plan_cancellation_decision(
    input: &LeaveDecisionInput,
    row: &LeaveRequestRow,
    approve: bool,
    final_decision: bool,
) -> Result<LeaveDecisionPlan, BusinessError> {
    if row.state != "approved" || row.cancellation_state.as_deref() != Some("pending_review") {
        return Err(not_found());
    }
    if row.version != input.expected_version {
        return Err(conflict(
            "LEAVE_CHANGED",
            "This cancellation review changed. Reload before deciding.",
        ));
    }
    let final_decision = approve && final_decision;
    Ok(LeaveDecisionPlan {
        state: if final_decision {
            "cancelled".to_owned()
        } else {
            "approved".to_owned()
        },
        version: input.expected_version + 1,
        final_decision,
        apply_balance: final_decision.then_some(LeaveBalanceAction::Reverse),
    })
}

/// Due accrual facts from the active/retired policy assignment.
#[derive(Clone, Debug)]
pub struct LeaveAccrualDue {
    pub company_id: String,
    pub engagement_id: String,
    pub leave_type_id: String,
    pub assignment_id: String,
    pub policy_version_id: String,
    pub grant_kind: String,
    pub grant_units: f64,
    pub accrual_units: f64,
    pub accrual_period: Option<String>,
    pub carryover_limit: Option<f64>,
    pub first_day: String,
    pub last_day: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LeaveAccrualEntry {
    pub kind: String,
    pub units: f64,
    pub effective_date: String,
    pub command_key: String,
    pub note: String,
}

#[derive(Clone, Debug)]
pub struct LeaveAccrualWrite<'a> {
    pub id: &'a str,
    pub company_id: &'a str,
    pub engagement_id: &'a str,
    pub leave_type_id: &'a str,
    pub kind: &'a str,
    pub units: f64,
    pub effective_date: &'a str,
    pub command_key: &'a str,
    pub note: &'a str,
    pub now: i64,
}

/// Retained annual/period/accrual/carryover plan. `available_before` returns
/// the prior year's available balance and is supplied by the owner adapter so
/// this stays a pure, replayable computation.
pub fn plan_leave_accruals(
    item: &LeaveAccrualDue,
    available_before: &dyn Fn(i32) -> f64,
) -> Vec<LeaveAccrualEntry> {
    if item.first_day > item.last_day {
        return Vec::new();
    }
    let start_year = year_of(&item.first_day);
    let end_year = year_of(&item.last_day);
    let mut entries = Vec::new();
    let identity = if item.grant_kind == "period" {
        item.assignment_id.as_str()
    } else {
        item.engagement_id.as_str()
    };
    let mut push = |kind: &str, units: f64, date: &str, note: &str| {
        if units == 0.0 || !units.is_finite() {
            return;
        }
        let key_part = if kind == "grant" || kind == "carryover" {
            date.get(..4).unwrap_or(date)
        } else {
            date
        };
        entries.push(LeaveAccrualEntry {
            kind: kind.to_owned(),
            units,
            effective_date: date.to_owned(),
            command_key: format!("leave:{identity}:{kind}:{key_part}"),
            note: note.to_owned(),
        });
    };
    if item.grant_kind == "period" {
        push(
            "grant",
            item.grant_units,
            &item.first_day,
            &format!("Policy {}: grant", item.policy_version_id),
        );
    }
    for year in start_year..=end_year {
        let year_start = format!("{year:04}-01-01");
        let first = if year_start.as_str() < item.first_day.as_str() {
            item.first_day.clone()
        } else {
            year_start.clone()
        };
        if item.grant_kind == "annual" {
            push(
                "grant",
                item.grant_units,
                &first,
                &format!("Policy {}: grant", item.policy_version_id),
            );
        }
        if year_start.as_str() >= item.first_day.as_str()
            && let Some(limit) = item.carryover_limit
        {
            let units = limit.min(available_before(year - 1).max(0.0));
            push(
                "carryover",
                units,
                &year_start,
                &format!("Policy {}: carryover", item.policy_version_id),
            );
        }
        if item.grant_kind == "accrual" {
            for month in 1..=12 {
                let date = format!("{year:04}-{month:02}-01");
                if date.as_str() < item.first_day.as_str() || date.as_str() > item.last_day.as_str()
                {
                    continue;
                }
                let due = match item.accrual_period.as_deref() {
                    Some("monthly") => true,
                    Some("quarterly") => [1, 4, 7, 10].contains(&month),
                    Some("annual") => month == 1,
                    _ => false,
                };
                if due {
                    push(
                        "accrual",
                        item.accrual_units,
                        &date,
                        &format!("Policy {}: accrual", item.policy_version_id),
                    );
                }
            }
        }
    }
    entries
}

pub fn year_of(date: &str) -> i32 {
    date.get(..4)
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(0)
}

pub fn leave_attachment_access_error() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "LEAVE_NOT_FOUND",
        "Leave request not found.",
    )
}

pub fn leave_evidence_policy_port_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LEAVE_EVIDENCE_PORT_UNAVAILABLE",
        "The Files leave-evidence port is unavailable.",
    )
}

pub fn leave_staff_port_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LEAVE_STAFF_PORT_UNAVAILABLE",
        "The Operations active-staff port is unavailable.",
    )
}

pub fn leave_route_port_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LEAVE_ROUTE_PORT_UNAVAILABLE",
        "The Organization approval-route port is unavailable.",
    )
}

pub fn leave_approval_port_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LEAVE_APPROVAL_PORT_UNAVAILABLE",
        "The Workflow approval port is unavailable.",
    )
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid("LEAVE_REQUEST_INVALID", "Review the leave request details.")
}

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "LEAVE_NOT_FOUND",
        "Leave request not found.",
    )
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "This command was already used for a different request. Start a new action.",
    )
}

pub fn command_incomplete() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMAND_INCOMPLETE",
        "This command did not finish. Retry the same action.",
    )
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}

fn text_required(raw: &Value, field: &'static str) -> Result<String, BusinessError> {
    match raw.get(field) {
        Some(Value::String(value)) => Ok(value.trim().to_owned()),
        _ => Err(request_invalid()),
    }
}

fn note(
    raw: &Value,
    field: &'static str,
    min: usize,
    max: usize,
    required: bool,
) -> Result<String, BusinessError> {
    match raw.get(field) {
        None | Some(Value::Null) if !required => Ok(String::new()),
        Some(Value::String(value)) => {
            let trimmed = value.trim();
            let length = trimmed.encode_utf16().count();
            if length < min || length > max {
                return Err(request_invalid());
            }
            Ok(trimmed.to_owned())
        }
        _ => Err(request_invalid()),
    }
}

fn optional_identifier(raw: &Value, field: &'static str) -> Result<Option<String>, BusinessError> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => {
            let trimmed = value.trim();
            if trimmed.is_empty() || trimmed.encode_utf16().count() > 200 {
                return Err(request_invalid());
            }
            Ok(Some(trimmed.to_owned()))
        }
        Some(_) => Err(request_invalid()),
    }
}

fn optional_policy(raw: &Value, field: &'static str) -> Result<Option<Value>, BusinessError> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            let policy = parse_leave_evidence_policy(Some(value)).ok_or_else(request_invalid)?;
            serde_json::to_value(policy)
                .map(Some)
                .map_err(|_| request_invalid())
        }
    }
}

fn segment(raw: &Value, field: &'static str) -> Result<String, BusinessError> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok("full".to_owned()),
        Some(Value::String(value))
            if ["full", "first_half", "second_half"].contains(&value.as_str()) =>
        {
            Ok(value.clone())
        }
        _ => Err(request_invalid()),
    }
}

fn flag(raw: &Value, field: &'static str) -> Result<bool, BusinessError> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(request_invalid()),
    }
}

fn positive(raw: &Value, field: &'static str) -> Result<i64, BusinessError> {
    let Some(Value::Number(value)) = raw.get(field) else {
        return Err(request_invalid());
    };
    value
        .as_i64()
        .filter(|value| *value >= 1)
        .ok_or_else(request_invalid)
}

fn integer(raw: &Value, field: &'static str, min: i64, max: i64) -> Result<i64, BusinessError> {
    let Some(Value::Number(value)) = raw.get(field) else {
        return Err(request_invalid());
    };
    value
        .as_i64()
        .filter(|value| (min..=max).contains(value))
        .ok_or_else(request_invalid)
}

fn optional_number(
    raw: &Value,
    field: &'static str,
    min: f64,
    max: f64,
) -> Result<Option<f64>, BusinessError> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(value)) => {
            let value = value.as_f64().ok_or_else(request_invalid)?;
            if !value.is_finite() || value < min || value > max {
                return Err(request_invalid());
            }
            Ok(Some(value))
        }
        Some(_) => Err(request_invalid()),
    }
}

fn date(raw: &Value, field: &'static str) -> Result<String, BusinessError> {
    let Some(Value::String(value)) = raw.get(field) else {
        return Err(request_invalid());
    };
    if !valid_date(value) {
        return Err(request_invalid());
    }
    Ok(value.clone())
}

fn optional_date(raw: &Value, field: &'static str) -> Result<Option<String>, BusinessError> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if valid_date(value) => Ok(Some(value.clone())),
        Some(_) => Err(request_invalid()),
    }
}

pub fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes[..4].iter().all(u8::is_ascii_digit)
        || !bytes[5..7].iter().all(u8::is_ascii_digit)
        || !bytes[8..].iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    let year = value[..4].parse::<i32>().unwrap_or(0);
    let month = value[5..7].parse::<u32>().unwrap_or(0);
    let day = value[8..].parse::<u32>().unwrap_or(0);
    month >= 1 && month <= 12 && day >= 1 && day <= days_in_month(year, month)
}

fn leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        2 if leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Fresh Account/IAM authority is composed by the caller, exactly as for the
/// native workforce commands. This only checks the supplied current facts.
pub fn require_current(
    actor: &crate::domain::workforce::WorkforceCommandActor,
    now: TimestampMillis,
) -> Result<(), BusinessError> {
    if actor.user_id.is_empty()
        || actor.assignment_id.is_empty()
        || actor.role.is_empty()
        || actor.work_context.len() != 64
        || !actor
            .work_context
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "LEAVE_AUTHORITY_INVALID",
            "The current leave authority is unavailable.",
        ));
    }
    if now.get() >= actor.expires_at {
        return Err(conflict(
            "WORK_CONTEXT_CHANGED",
            "Your selected work context changed. Review the current workspace and role.",
        ));
    }
    Ok(())
}

pub fn has_permission(
    actor: &crate::domain::workforce::WorkforceCommandActor,
    permission: &str,
) -> bool {
    actor.permissions.iter().any(|value| value == permission)
}

pub fn require_self_service(
    actor: &crate::domain::workforce::WorkforceCommandActor,
    now: TimestampMillis,
) -> Result<(), BusinessError> {
    if !has_permission(actor, "leave.self") && !has_permission(actor, "people.manage") {
        return Err(forbidden(
            "PERMISSION_REQUIRED",
            "Your selected role does not grant leave self-service access.",
        ));
    }
    require_current(actor, now)
}

pub fn require_decision(
    actor: &crate::domain::workforce::WorkforceCommandActor,
    now: TimestampMillis,
) -> Result<(), BusinessError> {
    require_current(actor, now)
}

pub fn require_read(
    actor: &crate::domain::workforce::WorkforceCommandActor,
    now: TimestampMillis,
) -> Result<(), BusinessError> {
    if !has_permission(actor, "leave.self")
        && !has_permission(actor, "people.manage")
        && !has_permission(actor, "leave.evidence.read")
    {
        return Err(forbidden(
            "PERMISSION_REQUIRED",
            "Your selected role does not grant leave read access.",
        ));
    }
    require_current(actor, now)
}

/// Participant/authority snapshot required by Files evidence access checks.
#[derive(Clone, Debug, Serialize)]
pub struct LeaveAttachmentAccess {
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub version: i64,
    pub editable: bool,
}

/// Build the retained `evidencePolicy` projection used by the self-service
/// preview response. Kept here so both preview and command paths agree.
pub fn evidence_policy_json(policy: &LeaveEvidencePolicy) -> Result<Value, BusinessError> {
    let mut value = Map::new();
    value.insert("required".into(), json!(policy.required));
    value.insert("minimumUnits".into(), json!(policy.minimum_units));
    value.insert("purposeKey".into(), json!(policy.purpose_key));
    value.insert("minimumCount".into(), json!(policy.minimum_count));
    value.insert("dueStage".into(), json!(policy.due_stage));
    value.insert("graceDays".into(), json!(policy.grace_days));
    value.insert("privacy".into(), json!(policy.privacy));
    value.insert("note".into(), json!(policy.note));
    Ok(Value::Object(value))
}

pub fn purpose_allowed(value: &str) -> bool {
    LEAVE_EVIDENCE_PURPOSES.contains(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn due() -> LeaveAccrualDue {
        LeaveAccrualDue {
            company_id: "company".into(),
            engagement_id: "engagement".into(),
            leave_type_id: "type".into(),
            assignment_id: "assignment".into(),
            policy_version_id: "policy".into(),
            grant_kind: "accrual".into(),
            grant_units: 0.0,
            accrual_units: 1.5,
            accrual_period: Some("monthly".into()),
            carryover_limit: None,
            first_day: "2026-01-01".into(),
            last_day: "2026-12-31".into(),
        }
    }

    #[test]
    fn monthly_accrual_plan_uses_deterministic_command_keys() {
        let entries = plan_leave_accruals(&due(), &|_| 0.0);
        assert_eq!(entries.len(), 12);
        assert_eq!(entries[0].kind, "accrual");
        assert_eq!(entries[0].effective_date, "2026-01-01");
        assert_eq!(
            entries[0].command_key,
            "leave:engagement:accrual:2026-01-01"
        );
    }

    #[test]
    fn quarterly_and_annual_accruals_only_emit_period_starts() {
        let mut item = due();
        item.accrual_period = Some("quarterly".into());
        assert_eq!(plan_leave_accruals(&item, &|_| 0.0).len(), 4);
        item.accrual_period = Some("annual".into());
        let entries = plan_leave_accruals(&item, &|_| 0.0);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].effective_date, "2026-01-01");
    }

    #[test]
    fn carryover_is_limited_and_reads_the_previous_year() {
        let mut item = due();
        item.grant_kind = "annual".into();
        item.grant_units = 10.0;
        item.carryover_limit = Some(5.0);
        let entries = plan_leave_accruals(&item, &|year| if year == 2025 { 8.0 } else { 0.0 });
        assert!(entries.iter().any(|entry| {
            entry.kind == "carryover" && entry.units == 5.0 && entry.effective_date == "2026-01-01"
        }));
    }

    #[test]
    fn period_grant_uses_the_assignment_key() {
        let mut item = due();
        item.grant_kind = "period".into();
        item.grant_units = 3.0;
        let entries = plan_leave_accruals(&item, &|_| 0.0);
        assert_eq!(entries[0].command_key, "leave:assignment:grant:2026");
    }

    #[test]
    fn evidence_policy_snapshot_round_trips_and_rejects_partial_rules() {
        let policy = parse_leave_evidence_policy(Some(&json!({
            "required": true,
            "minimumUnits": 3,
            "purposeKey": "medical_certificate",
            "minimumCount": 1,
            "dueStage": "before_final_approval",
            "graceDays": 0,
            "privacy": "requester_approvers_hr",
            "note": ""
        })))
        .unwrap();
        assert!(policy.required);
        assert!(
            parse_leave_evidence_policy(Some(&json!({
                "required": true,
                "minimumUnits": null,
                "purposeKey": null,
                "minimumCount": 0,
                "dueStage": "before_final_approval",
                "graceDays": 0,
                "privacy": "requester_approvers_hr",
                "note": ""
            })))
            .is_none()
        );
    }

    #[test]
    fn balance_transitions_match_the_retained_order() {
        let use_entries = balance_transitions(LeaveBalanceAction::Use);
        assert_eq!(use_entries[0].kind, "usage");
        assert_eq!(use_entries[1].kind, "reservation_release");
        assert_eq!(
            balance_transitions(LeaveBalanceAction::Release)[0].kind,
            "reservation_release"
        );
        assert_eq!(
            balance_transitions(LeaveBalanceAction::Reverse)[0].kind,
            "usage_reversal"
        );
    }
}
