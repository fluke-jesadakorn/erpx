//! Pure Work Projection capability facts.
//!
//! Mirrors `getWorkCapabilities` in the TypeScript work-summary adapter. The
//! caller supplies already-authorized probes; this module only decides the
//! exact creation kinds, team-queue and roster flags, and bounded unavailable
//! labels. It performs no I/O and grants no authority.

use crate::domain::work_card_availability::WorkCapabilitiesFact;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityProbe<T> {
    Value(T),
    Unavailable,
}

/// Inputs from the caller's owner transaction.
///
/// `active_staff` is `Some(roster_required)` for an active staff record and
/// `None` when the member has no active staff record. A rejected probe is
/// `Unavailable`; the caller maps an authority 409 to `Value(None)` before
/// calling this function, exactly like the TypeScript adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkCapabilityFacts {
    pub beta: bool,
    pub active_staff: CapabilityProbe<Option<bool>>,
    pub covered_team: CapabilityProbe<bool>,
    pub can_prepare_counts: bool,
    pub can_prepare_transfers: bool,
}

fn creation_kinds(facts: &WorkCapabilityFacts, staff: bool) -> Vec<String> {
    let mut kinds = vec![
        "expense".to_owned(),
        "service_request".to_owned(),
        "equipment_request".to_owned(),
        "purchase_request".to_owned(),
    ];
    if !facts.beta {
        kinds.push("leave".to_owned());
    }
    if facts.beta {
        kinds.push("conversation".to_owned());
    } else if staff {
        kinds.push("task".to_owned());
        kinds.push("overtime".to_owned());
    }
    if !facts.beta && facts.can_prepare_counts {
        kinds.push("stock_count".to_owned());
    }
    if !facts.beta && facts.can_prepare_transfers {
        kinds.push("transfer".to_owned());
    }
    kinds
}

/// Returns the exact capability projection used by the work summary and card
/// availability. Unavailable labels keep the TypeScript order: employee setup
/// first, then team coverage.
pub fn work_capabilities(facts: &WorkCapabilityFacts) -> WorkCapabilitiesFact {
    let mut unavailable = Vec::new();
    let (staff, roster_access) = match facts.active_staff {
        CapabilityProbe::Value(Some(roster_required)) => (true, !facts.beta && roster_required),
        CapabilityProbe::Value(None) => (false, false),
        CapabilityProbe::Unavailable => {
            unavailable.push("Employee setup".to_owned());
            (false, false)
        }
    };
    let team_queue = match facts.covered_team {
        CapabilityProbe::Value(value) => value,
        CapabilityProbe::Unavailable => {
            unavailable.push("Team coverage".to_owned());
            false
        }
    };
    WorkCapabilitiesFact {
        creation: creation_kinds(facts, staff),
        team_queue,
        roster_access,
        unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> WorkCapabilityFacts {
        WorkCapabilityFacts {
            beta: false,
            active_staff: CapabilityProbe::Value(Some(true)),
            covered_team: CapabilityProbe::Value(true),
            can_prepare_counts: true,
            can_prepare_transfers: true,
        }
    }

    #[test]
    fn staff_member_gets_every_creation_kind_in_oracle_order() {
        let result = work_capabilities(&facts());
        assert_eq!(
            result.creation,
            [
                "expense",
                "service_request",
                "equipment_request",
                "purchase_request",
                "leave",
                "task",
                "overtime",
                "stock_count",
                "transfer"
            ]
        );
        assert!(result.team_queue);
        assert!(result.roster_access);
        assert!(result.unavailable.is_empty());
    }

    #[test]
    fn beta_members_get_conversation_and_never_roster_or_counts() {
        let mut value = facts();
        value.beta = true;
        value.active_staff = CapabilityProbe::Value(Some(true));
        let result = work_capabilities(&value);
        assert_eq!(
            result.creation,
            [
                "expense",
                "service_request",
                "equipment_request",
                "purchase_request",
                "conversation"
            ]
        );
        assert!(!result.roster_access);
    }

    #[test]
    fn no_staff_record_keeps_leave_but_drops_task_overtime_and_roster() {
        let mut value = facts();
        value.active_staff = CapabilityProbe::Value(None);
        value.can_prepare_counts = false;
        value.can_prepare_transfers = false;
        let result = work_capabilities(&value);
        assert_eq!(
            result.creation,
            [
                "expense",
                "service_request",
                "equipment_request",
                "purchase_request",
                "leave"
            ]
        );
        assert!(!result.roster_access);
        assert!(result.team_queue);
    }

    #[test]
    fn rejected_probes_publish_the_exact_unavailable_labels_in_order() {
        let mut value = facts();
        value.active_staff = CapabilityProbe::Unavailable;
        value.covered_team = CapabilityProbe::Unavailable;
        let result = work_capabilities(&value);
        assert_eq!(result.unavailable, ["Employee setup", "Team coverage"]);
        assert!(!result.roster_access);
        assert!(!result.team_queue);
        assert!(!result.creation.iter().any(|kind| kind == "task"));
    }
}
