//! Retained three-step stock-count Flow policy. All participants and actor
//! permissions are owner-sourced facts, not authority granted by this module.
use erpx_server_shared::public::{BusinessError, FailureKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StockCountStep {
    Draft,
    Submitted,
    Approved,
}
impl StockCountStep {
    pub fn key(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Submitted => "submitted",
            Self::Approved => "approved",
        }
    }
    pub fn index(self) -> usize {
        match self {
            Self::Draft => 0,
            Self::Submitted => 1,
            Self::Approved => 2,
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "draft" => Some(Self::Draft),
            "submitted" => Some(Self::Submitted),
            "approved" => Some(Self::Approved),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockCountParticipant {
    pub user_id: String,
    pub name: String,
    pub team_name: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockCountStepFacts {
    pub step: StockCountStep,
    pub candidates: Vec<StockCountParticipant>,
    /// Canonical principal of saved/default/preferred identity, already resolved
    /// by Identity. An ineligible named value deliberately does not fall back.
    pub selected: Option<String>,
    pub actor_role_allowed: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockCountStepDecision {
    pub primary: Option<StockCountParticipant>,
    pub blocker: Option<&'static str>,
}
pub fn resolve_stock_count_step(facts: &StockCountStepFacts) -> StockCountStepDecision {
    let selected = facts
        .selected
        .as_deref()
        .or_else(|| (facts.candidates.len() == 1).then(|| facts.candidates[0].user_id.as_str()));
    let primary = selected
        .and_then(|id| {
            facts
                .candidates
                .iter()
                .find(|candidate| candidate.user_id == id)
        })
        .cloned();
    let blocker = if primary.is_some() {
        None
    } else if selected.is_some() {
        Some("The primary staff member is no longer eligible. Reassign with a reason.")
    } else if facts.candidates.is_empty() {
        Some("No eligible staff member. Configure active staff and role coverage.")
    } else {
        Some("Select a primary staff member from the eligible team.")
    };
    StockCountStepDecision { primary, blocker }
}
pub fn current_stock_count_step(status: &str) -> Option<StockCountStep> {
    match status {
        "draft" | "rejected" => Some(StockCountStep::Draft),
        "submitted" => Some(StockCountStep::Submitted),
        "approved" => Some(StockCountStep::Approved),
        _ => None,
    }
}
pub fn stock_count_terminal(status: &str) -> bool {
    matches!(status, "posted" | "cancelled")
}
/// Cancel is an administrative Flow action; Inventory still applies its stricter
/// preparer, scope, state, reason and purchase policy in the same unit.
pub fn require_stock_count_action(
    facts: &StockCountStepFacts,
    actor: &str,
    action: &str,
) -> Result<(), BusinessError> {
    if action == "cancel" {
        return Ok(());
    }
    let decision = resolve_stock_count_step(facts);
    if let Some(blocker) = decision.blocker {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "FLOW_PRIMARY_REQUIRED",
            blocker,
        ));
    }
    if !facts.actor_role_allowed
        || decision
            .primary
            .as_ref()
            .is_none_or(|person| person.user_id != actor)
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "FLOW_PRIMARY_REQUIRED",
            "This action belongs to the responsible staff member.",
        ));
    }
    Ok(())
}
pub struct StockCountAssignmentPolicy<'a> {
    pub current: Option<StockCountStep>,
    pub step: StockCountStep,
    pub candidates: &'a [StockCountParticipant],
    pub user: &'a str,
    pub expected_flow: i64,
    pub actual_flow: i64,
    pub expected_record: i64,
    pub actual_record: i64,
    pub company_admin: bool,
}
pub fn require_stock_count_assignment(
    input: StockCountAssignmentPolicy<'_>,
) -> Result<StockCountParticipant, BusinessError> {
    let StockCountAssignmentPolicy {
        current,
        step,
        candidates,
        user,
        expected_flow,
        actual_flow,
        expected_record,
        actual_record,
        company_admin,
    } = input;
    if !company_admin {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "FLOW_ADMIN_REQUIRED",
            "A company administrator must assign workflow responsibilities.",
        ));
    }
    if expected_flow != actual_flow || expected_record != actual_record {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "VERSION_CONFLICT",
            "The workflow changed. Refresh before assigning.",
        ));
    }
    if current.is_none_or(|c| step.index() < c.index()) {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "FLOW_STEP_LOCKED",
            "Historical steps cannot be reassigned.",
        ));
    }
    candidates
        .iter()
        .find(|person| person.user_id == user)
        .cloned()
        .ok_or_else(|| {
            BusinessError::invalid(
                "FLOW_STAFF_INELIGIBLE",
                "Choose an active eligible staff member for this step.",
            )
        })
}
