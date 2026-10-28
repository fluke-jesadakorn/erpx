//! Pure journal reviewer selection; owner-sourced participants and current work
//! authority must be supplied by the same transaction's composition.
use erpx_server_shared::public::{BusinessError, FailureKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalFlowCandidate {
    pub user_id: String,
    pub name: String,
    pub team_name: String,
}

/// The saved route takes precedence over defaults, and an ineligible named
/// primary never falls through to another reviewer. Only a single candidate is
/// selected automatically when neither route nor default names a primary.
pub fn select_journal_primary<'a>(
    candidates: &'a [JournalFlowCandidate],
    saved: Option<&str>,
    default: Option<&str>,
    actor: &str,
    actor_can_participate: bool,
) -> Result<&'a JournalFlowCandidate, BusinessError> {
    let selected = saved
        .or(default)
        .or_else(|| (candidates.len() == 1).then(|| candidates[0].user_id.as_str()));
    let primary =
        selected.and_then(|id| candidates.iter().find(|candidate| candidate.user_id == id));
    let Some(primary) = primary else {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "FLOW_PRIMARY_REQUIRED",
            "An eligible primary reviewer is required.",
        ));
    };
    if primary.user_id != actor || !actor_can_participate {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "FLOW_PRIMARY_REQUIRED",
            "This action belongs to the current primary reviewer.",
        ));
    }
    Ok(primary)
}
