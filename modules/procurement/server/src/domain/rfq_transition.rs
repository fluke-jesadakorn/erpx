use erpx_platform_shared_server::public::{BusinessError, FailureKind};

/// Call only after the retained schema, purchase admission, replay and detail
/// checks, with current protected source facts. The approval callback must
/// enforce current authority; the returned decision is not an authorization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RfqTransitionAction {
    Submit,
    Approve,
    Return,
    Cancel,
}
#[derive(Clone, Debug)]
pub struct RfqCurrentOffer {
    pub id: String,
    pub valid_until: String,
}
#[derive(Clone, Debug)]
pub struct RfqTransitionFacts {
    pub status: String,
    pub source_blocked: bool,
    pub selected_offer_id: Option<String>,
    pub current_offers: Vec<RfqCurrentOffer>,
    pub can_approve: bool,
    pub creator_is_actor: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RfqTransitionDecision {
    pub status: &'static str,
    pub selected_offer_id: Option<String>,
}
fn error(kind: FailureKind, code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(kind, code, message)
}

pub fn decide_rfq_transition(
    facts: &RfqTransitionFacts,
    action: RfqTransitionAction,
    requested_offer: Option<&str>,
    today: &str,
    mut require_approve: impl FnMut() -> Result<(), BusinessError>,
) -> Result<RfqTransitionDecision, BusinessError> {
    use RfqTransitionAction::*;
    if facts.source_blocked && action != Cancel {
        return Err(error(
            FailureKind::Conflict,
            "RFQ_REQUEST_CHANGED",
            "The requisition is no longer approved and ready with the same demand. Resolve the source or cancel this RFQ.",
        ));
    }
    let selected_offer_id = facts.selected_offer_id.clone();
    match action {
        Submit => {
            let chosen = facts
                .current_offers
                .iter()
                .find(|o| Some(o.id.as_str()) == requested_offer);
            if facts.status != "collecting" || chosen.is_none_or(|o| o.valid_until.as_str() < today)
            {
                return Err(error(
                    FailureKind::Conflict,
                    "RFQ_CURRENT_OFFER_REQUIRED",
                    "Select a current unexpired supplier offer while collecting.",
                ));
            }
            Ok(RfqTransitionDecision {
                status: "review",
                selected_offer_id: chosen.map(|o| o.id.clone()),
            })
        }
        Approve => {
            require_approve()?;
            if facts.status != "review" || !facts.can_approve {
                return Err(error(
                    FailureKind::Forbidden,
                    "RFQ_INDEPENDENT_APPROVER_REQUIRED",
                    "A different purchasing manager must review the creator, proposer and selected offer recorder.",
                ));
            }
            let selected = facts
                .current_offers
                .iter()
                .find(|o| Some(&o.id) == selected_offer_id.as_ref());
            if selected.is_none_or(|o| o.valid_until.as_str() < today) {
                return Err(error(
                    FailureKind::Conflict,
                    "RFQ_QUOTE_EXPIRED",
                    "The selected quotation expired. Return it for a replacement.",
                ));
            }
            Ok(RfqTransitionDecision {
                status: "approved",
                selected_offer_id,
            })
        }
        Return => {
            require_approve()?;
            if !matches!(facts.status.as_str(), "review" | "approved") {
                return Err(error(
                    FailureKind::Conflict,
                    "RFQ_RETURN_INVALID",
                    "Only an unconsumed review or award can return to collection.",
                ));
            }
            Ok(RfqTransitionDecision {
                status: "collecting",
                selected_offer_id: None,
            })
        }
        Cancel => {
            if !matches!(facts.status.as_str(), "collecting" | "review" | "approved") {
                return Err(error(
                    FailureKind::Conflict,
                    "RFQ_CANCEL_INVALID",
                    "The purchase order owns cancellation after handoff.",
                ));
            }
            if facts.status == "approved" {
                require_approve()?;
                if facts.creator_is_actor {
                    return Err(error(
                        FailureKind::Forbidden,
                        "RFQ_INDEPENDENT_APPROVER_REQUIRED",
                        "A different manager must cancel an approved award.",
                    ));
                }
            }
            Ok(RfqTransitionDecision {
                status: "cancelled",
                selected_offer_id,
            })
        }
    }
}
