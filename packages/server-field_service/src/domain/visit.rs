//! Pure visit transition and authority policy; facts never confer authority.
use erpx_server_shared::public::{BusinessError, FailureKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VisitAction {
    Schedule,
    Dispatch,
    Start,
    Complete,
}
impl VisitAction {
    pub fn operation(self) -> &'static str {
        match self {
            Self::Schedule => "field.order.schedule",
            Self::Dispatch => "field.order.dispatch",
            Self::Start => "field.order.start",
            Self::Complete => "field.order.complete",
        }
    }
    pub fn status(self) -> &'static str {
        match self {
            Self::Schedule => "scheduled",
            Self::Dispatch => "dispatched",
            Self::Start => "in_visit",
            Self::Complete => "completed",
        }
    }
    pub fn allows_state(self, state: &str) -> bool {
        match self {
            Self::Schedule => matches!(state, "open" | "scheduled"),
            Self::Dispatch => state == "scheduled",
            Self::Start => state == "dispatched",
            Self::Complete => state == "in_visit",
        }
    }
}
pub struct VisitAuthorityFacts<'a> {
    pub technician_user_id: &'a str,
    pub actor_user_id: &'a str,
    pub technician_eligible: bool,
    pub reviewer_eligible: bool,
    pub selected_can_review: bool,
    pub state: &'a str,
}
pub fn require_visit_authority(
    facts: &VisitAuthorityFacts<'_>,
    action: VisitAction,
) -> Result<(), BusinessError> {
    let own = facts.technician_user_id == facts.actor_user_id && facts.technician_eligible;
    let manage = facts.selected_can_review && facts.reviewer_eligible;
    if !own && !manage {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "FIELD_SERVICE_OWNER_REQUIRED",
            "The technician or an eligible reviewer must perform this action.",
        ));
    }
    if !action.allows_state(facts.state) {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "FIELD_SERVICE_STATE_CHANGED",
            "Refresh the service order before continuing.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transition_states_and_exact_user_authority() {
        let mut facts = VisitAuthorityFacts {
            technician_user_id: "tech",
            actor_user_id: "tech",
            technician_eligible: true,
            reviewer_eligible: false,
            selected_can_review: false,
            state: "open",
        };
        assert!(require_visit_authority(&facts, VisitAction::Schedule).is_ok());
        assert!(require_visit_authority(&facts, VisitAction::Dispatch).is_err());
        facts.state = "scheduled";
        assert!(require_visit_authority(&facts, VisitAction::Dispatch).is_ok());
        facts.state = "dispatched";
        assert!(require_visit_authority(&facts, VisitAction::Start).is_ok());
        facts.actor_user_id = "person-alias";
        assert!(require_visit_authority(&facts, VisitAction::Start).is_err());
        facts.technician_eligible = false;
        facts.selected_can_review = true;
        facts.reviewer_eligible = true;
        assert!(require_visit_authority(&facts, VisitAction::Start).is_ok());
        facts.reviewer_eligible = false;
        assert!(require_visit_authority(&facts, VisitAction::Start).is_err());
    }
}
