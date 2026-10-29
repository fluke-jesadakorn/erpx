//! Original Field Service parts policy calculations. Plain facts/results are
//! data only; they cannot construct a current actor, owner source or DB proof.
use erpx_server_shared::public::{BusinessError, FailureKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldServicePartsActionKind {
    Reserve,
    Issue,
}
impl FieldServicePartsActionKind {
    pub fn operation(self) -> &'static str {
        match self {
            Self::Reserve => "field.order.reserve-part",
            Self::Issue => "field.order.issue-part",
        }
    }
    pub fn allows_state(self, state: &str) -> bool {
        match self {
            Self::Reserve => matches!(state, "open" | "scheduled"),
            Self::Issue => state == "in_visit",
        }
    }
}
pub struct FieldServicePartsAuthorityFacts<'a> {
    pub technician_user_id: &'a str,
    pub actor_user_id: &'a str,
    pub technician_eligible: bool,
    pub reviewer_eligible: bool,
    pub selected_can_review: bool,
    pub state: &'a str,
}
#[derive(Debug, Eq, PartialEq)]
pub struct FieldServicePartsAuthorityDecision {
    pub own: bool,
    pub manage: bool,
}
pub fn require_parts_authority(
    facts: &FieldServicePartsAuthorityFacts<'_>,
    action: FieldServicePartsActionKind,
) -> Result<FieldServicePartsAuthorityDecision, BusinessError> {
    // Exact original user identity, not a same-Person alias, selects own work.
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
    Ok(FieldServicePartsAuthorityDecision { own, manage })
}
pub fn require_future_reservation(
    expires_at: i64,
    application_at: i64,
) -> Result<(), BusinessError> {
    if expires_at <= application_at {
        return Err(BusinessError::invalid(
            "FIELD_SERVICE_PART_EXPIRY",
            "Part reservation must expire in the future.",
        ));
    }
    Ok(())
}
/// Purchase data, never a purchase witness. The caller must inspect/revalidate
/// both actual Company capabilities through Commercial in the same unit.
pub fn parts_capabilities() -> [&'static str; 2] {
    ["service.orders", "service.parts-assets"]
}
