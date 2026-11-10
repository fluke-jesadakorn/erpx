//! Preventive-plan state policy. These facts and rules are pure owner business
//! rules; current scope, People/IAM eligibility, asset lifecycle, the calendar
//! port, command receipt and effects remain the adapter's responsibility.
use erpx_server_shared::public::{BusinessError, FailureKind};

/// Largest integer that JavaScript can represent exactly.
pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
const DAY_MS: i64 = 86_400_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreventivePlanFacts {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub asset_id: String,
    pub title: String,
    pub priority: String,
    pub owner_id: String,
    pub interval_days: i32,
    pub time_zone: String,
    pub first_due_local: String,
    pub next_due_local: String,
    pub next_due_at: i64,
    pub lead_days: i32,
    pub status: String,
    pub version: i64,
    pub created_by: String,
    pub edited_by: String,
    pub approved_by: Option<String>,
    pub approved_at: Option<i64>,
    pub updated_at: i64,
    /// Current People/IAM eligibility of the assigned maintenance owner.
    pub owner_eligible: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreventivePlanActor {
    pub user_id: String,
    pub can_review: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreventivePlanDefinition {
    pub asset_id: String,
    pub asset_version: i64,
    pub title: String,
    pub priority: String,
    pub owner_id: String,
    pub interval_days: i32,
    pub first_due_at: i64,
    pub time_zone: String,
    pub lead_days: i32,
    pub expected_version: Option<i64>,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreventivePlanAction {
    Activate,
    Pause,
    Cancel,
    Generate,
}

impl PreventivePlanAction {
    pub fn action(&self) -> &'static str {
        match self {
            Self::Activate => "activate",
            Self::Pause => "pause",
            Self::Cancel => "cancel",
            Self::Generate => "generate",
        }
    }
}

/// Save state after the domain admission rules: version and status only. The
/// application resolves the wall-clock minute and instant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreventiveSavePolicy {
    pub next_version: i64,
    pub status: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreventiveActionPolicy {
    pub status: &'static str,
    pub next_version: i64,
    /// Record the independent reviewer on activation.
    pub approve: bool,
}

fn conflict(message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, "CONFLICT", message)
}

/// Retained SQL assertion failure text for the owner source/version/owner
/// guards, so HTTP callers keep the same refusal contract.
pub fn source_conflict() -> BusinessError {
    conflict(
        "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
    )
}

fn next_version(current: i64) -> Result<i64, BusinessError> {
    current
        .checked_add(1)
        .filter(|version| *version <= MAX_SAFE_INTEGER)
        .ok_or_else(|| conflict("The preventive plan changed. Reload it before continuing."))
}

/// Revision admission: only a draft or paused plan of the same asset may be
/// revised, by its requester or an eligible reviewer. The version assertion
/// remains a transactional guard, exactly as the retained application.
pub fn require_revision(
    actor: &PreventivePlanActor,
    current: &PreventivePlanFacts,
    definition: &PreventivePlanDefinition,
) -> Result<(), BusinessError> {
    if !matches!(current.status.as_str(), "draft" | "paused")
        || current.asset_id != definition.asset_id
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "MAINTENANCE_PLAN_PAUSE_REQUIRED",
            "Pause the same asset plan before revising its definition.",
        ));
    }
    if current.created_by != actor.user_id && !actor.can_review {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "MAINTENANCE_PLAN_REVIEW_REQUIRED",
            "The plan requester or an eligible reviewer must revise the definition.",
        ));
    }
    Ok(())
}

/// Decide a create/revise admission. Order matches the retained application:
/// state and requester admission, then the required current version.
pub fn require_save(
    actor: &PreventivePlanActor,
    current: Option<&PreventivePlanFacts>,
    definition: &PreventivePlanDefinition,
) -> Result<PreventiveSavePolicy, BusinessError> {
    match current {
        // A create ignores `expectedVersion`, as the retained application does;
        // the canonical receipt bytes still preserve it when supplied.
        None => Ok(PreventiveSavePolicy {
            next_version: 1,
            status: "draft",
        }),
        Some(current) => {
            require_revision(actor, current, definition)?;
            let Some(expected) = definition.expected_version else {
                return Err(BusinessError::invalid(
                    "VERSION_REQUIRED",
                    "Use the current plan version.",
                ));
            };
            if expected != current.version {
                return Err(source_conflict());
            }
            let status = match current.status.as_str() {
                "draft" => "draft",
                "paused" => "paused",
                _ => return Err(source_conflict()),
            };
            Ok(PreventiveSavePolicy {
                next_version: next_version(current.version)?,
                status,
            })
        }
    }
}

fn require_review(actor: &PreventivePlanActor) -> Result<(), BusinessError> {
    if actor.can_review {
        Ok(())
    } else {
        Err(BusinessError::new(
            FailureKind::Forbidden,
            "PERMISSION_REQUIRED",
            "Your selected role does not grant this permission.",
        ))
    }
}

/// Decide an action admission. Refusal order preserves the retained
/// application: current version, selected review permission, plan state,
/// independent activation, generate authority and the planning window.
pub fn require_action(
    actor: &PreventivePlanActor,
    plan: &PreventivePlanFacts,
    action: &PreventivePlanAction,
    expected_version: i64,
    now: i64,
) -> Result<PreventiveActionPolicy, BusinessError> {
    if expected_version != plan.version {
        return Err(conflict(
            "The preventive plan changed. Reload it before continuing.",
        ));
    }
    let version = next_version(plan.version)?;
    match action {
        PreventivePlanAction::Activate => {
            require_review(actor)?;
            if !matches!(plan.status.as_str(), "draft" | "paused") {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "MAINTENANCE_PLAN_STATE",
                    "Review a draft or paused plan.",
                ));
            }
            if plan.edited_by == actor.user_id {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "INDEPENDENT_PLAN_REVIEW_REQUIRED",
                    "A different reviewer must activate the current preventive definition.",
                ));
            }
            if !plan.owner_eligible {
                return Err(source_conflict());
            }
            Ok(PreventiveActionPolicy {
                status: "active",
                next_version: version,
                approve: true,
            })
        }
        PreventivePlanAction::Generate => {
            if plan.status != "active" {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "MAINTENANCE_PLAN_INACTIVE",
                    "Activate this plan before generating its due work.",
                ));
            }
            // The retained application admits the assigned owner by identity,
            // then the transactional owner-source assertion proves eligibility.
            let own = plan.owner_id == actor.user_id;
            if !own && !actor.can_review {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "MAINTENANCE_PLAN_OWNER_REQUIRED",
                    "The assigned owner or a reviewer must generate due work.",
                ));
            }
            let window = i64::from(plan.lead_days).saturating_mul(DAY_MS);
            if plan.next_due_at > now.saturating_add(window) {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "MAINTENANCE_PLAN_NOT_DUE",
                    "This occurrence is outside the approved planning window.",
                ));
            }
            if !plan.owner_eligible {
                return Err(source_conflict());
            }
            Ok(PreventiveActionPolicy {
                status: "active",
                next_version: version,
                approve: false,
            })
        }
        PreventivePlanAction::Pause => {
            require_review(actor)?;
            if plan.status != "active" {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "MAINTENANCE_PLAN_STATE",
                    "Reload an active plan before changing its schedule.",
                ));
            }
            Ok(PreventiveActionPolicy {
                status: "paused",
                next_version: version,
                approve: false,
            })
        }
        PreventivePlanAction::Cancel => {
            require_review(actor)?;
            if plan.status == "cancelled" {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "MAINTENANCE_PLAN_STATE",
                    "Reload an active plan before changing its schedule.",
                ));
            }
            Ok(PreventiveActionPolicy {
                status: "cancelled",
                next_version: version,
                approve: false,
            })
        }
    }
}

/// A revision may not move the next due date behind an occurrence that was
/// already generated for this plan.
pub fn retained_occurrence_conflict(
    retained_earliest_due_local: Option<&str>,
    next_local: &str,
) -> bool {
    retained_earliest_due_local.is_some_and(|due| due >= next_local)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(review: bool) -> PreventivePlanActor {
        PreventivePlanActor {
            user_id: "actor".into(),
            can_review: review,
        }
    }

    fn plan() -> PreventivePlanFacts {
        PreventivePlanFacts {
            id: "plan".into(),
            company_id: "company".into(),
            branch_id: "branch".into(),
            department_id: None,
            asset_id: "asset".into(),
            title: "Daily safety inspection".into(),
            priority: "normal".into(),
            owner_id: "owner".into(),
            interval_days: 1,
            time_zone: "America/New_York".into(),
            first_due_local: "2026-03-07T02:30".into(),
            next_due_local: "2026-03-07T02:30".into(),
            next_due_at: 1_772_868_600_000,
            lead_days: 0,
            status: "draft".into(),
            version: 1,
            created_by: "actor".into(),
            edited_by: "actor".into(),
            approved_by: None,
            approved_at: None,
            updated_at: 0,
            owner_eligible: true,
        }
    }

    fn definition() -> PreventivePlanDefinition {
        PreventivePlanDefinition {
            asset_id: "asset".into(),
            asset_version: 1,
            title: "Daily safety inspection".into(),
            priority: "normal".into(),
            owner_id: "owner".into(),
            interval_days: 1,
            first_due_at: 1_772_868_659_000,
            time_zone: "America/New_York".into(),
            lead_days: 0,
            expected_version: None,
            reason: "Reviewed preventive plan".into(),
        }
    }

    #[test]
    fn create_admits_only_a_new_draft_and_ignores_expected_version() {
        let save = require_save(&actor(false), None, &definition()).unwrap();
        assert_eq!(save.next_version, 1);
        assert_eq!(save.status, "draft");
        let mut with_expected = definition();
        with_expected.expected_version = Some(9);
        assert_eq!(
            require_save(&actor(false), None, &with_expected)
                .unwrap()
                .next_version,
            1
        );
        assert_eq!(
            require_save(&actor(false), Some(&plan()), &definition())
                .unwrap_err()
                .code,
            "VERSION_REQUIRED"
        );
    }

    #[test]
    fn revision_requires_a_paused_or_draft_same_asset_plan_and_current_version() {
        let mut expected = definition();
        expected.expected_version = Some(1);
        assert_eq!(
            require_save(&actor(false), Some(&plan()), &expected)
                .unwrap()
                .next_version,
            2
        );
        let mut wrong_asset = expected.clone();
        wrong_asset.asset_id = "other".into();
        assert_eq!(
            require_save(&actor(false), Some(&plan()), &wrong_asset)
                .unwrap_err()
                .code,
            "MAINTENANCE_PLAN_PAUSE_REQUIRED"
        );
        let mut active = plan();
        active.status = "active".into();
        assert_eq!(
            require_save(&actor(true), Some(&active), &expected)
                .unwrap_err()
                .code,
            "MAINTENANCE_PLAN_PAUSE_REQUIRED"
        );
        let mut other_creator = plan();
        other_creator.created_by = "requester".into();
        assert_eq!(
            require_save(&actor(false), Some(&other_creator), &expected)
                .unwrap_err()
                .code,
            "MAINTENANCE_PLAN_REVIEW_REQUIRED"
        );
        assert_eq!(
            require_save(&actor(true), Some(&other_creator), &expected)
                .unwrap()
                .next_version,
            2
        );
        let mut stale_version = expected.clone();
        stale_version.expected_version = Some(4);
        assert_eq!(
            require_save(&actor(false), Some(&plan()), &stale_version)
                .unwrap_err()
                .code,
            "CONFLICT"
        );
    }

    #[test]
    fn activation_requires_an_independent_reviewer_and_an_eligible_owner() {
        let mut current = plan();
        current.status = "draft".into();
        assert_eq!(
            require_action(&actor(false), &current, &PreventivePlanAction::Activate, 1, 0)
                .unwrap_err()
                .code,
            "PERMISSION_REQUIRED"
        );
        assert_eq!(
            require_action(&actor(true), &current, &PreventivePlanAction::Activate, 1, 0)
                .unwrap_err()
                .code,
            "INDEPENDENT_PLAN_REVIEW_REQUIRED"
        );
        current.edited_by = "editor".into();
        let policy =
            require_action(&actor(true), &current, &PreventivePlanAction::Activate, 1, 0).unwrap();
        assert_eq!(policy.status, "active");
        assert_eq!(policy.next_version, 2);
        assert!(policy.approve);
        let mut ineligible = current.clone();
        ineligible.owner_eligible = false;
        assert_eq!(
            require_action(&actor(true), &ineligible, &PreventivePlanAction::Activate, 1, 0)
                .unwrap_err()
                .code,
            "CONFLICT"
        );
    }

    #[test]
    fn generation_requires_an_active_plan_due_window_and_owner_or_reviewer() {
        let mut current = plan();
        current.status = "active".into();
        assert_eq!(
            require_action(&actor(false), &current, &PreventivePlanAction::Generate, 1, 0)
                .unwrap_err()
                .code,
            "MAINTENANCE_PLAN_OWNER_REQUIRED"
        );
        let mut owner = current.clone();
        owner.owner_id = "actor".into();
        let now = owner.next_due_at + DAY_MS;
        let policy =
            require_action(&actor(false), &owner, &PreventivePlanAction::Generate, 1, now).unwrap();
        assert_eq!(policy.status, "active");
        assert_eq!(policy.next_version, 2);
        assert_eq!(
            require_action(&actor(false), &owner, &PreventivePlanAction::Generate, 1, 0)
                .unwrap_err()
                .code,
            "MAINTENANCE_PLAN_NOT_DUE"
        );
        assert_eq!(
            require_action(&actor(false), &current, &PreventivePlanAction::Generate, 2, 0)
                .unwrap_err()
                .code,
            "CONFLICT"
        );
        let mut draft = owner.clone();
        draft.status = "draft".into();
        assert_eq!(
            require_action(&actor(true), &draft, &PreventivePlanAction::Generate, 1, now)
                .unwrap_err()
                .code,
            "MAINTENANCE_PLAN_INACTIVE"
        );
    }

    #[test]
    fn pause_and_cancel_preserve_the_retained_state_rules() {
        let mut current = plan();
        current.status = "active".into();
        assert_eq!(
            require_action(&actor(false), &current, &PreventivePlanAction::Pause, 1, 0)
                .unwrap_err()
                .code,
            "PERMISSION_REQUIRED"
        );
        assert_eq!(
            require_action(&actor(true), &current, &PreventivePlanAction::Pause, 1, 0)
                .unwrap()
                .status,
            "paused"
        );
        let mut draft = current.clone();
        draft.status = "draft".into();
        assert_eq!(
            require_action(&actor(true), &draft, &PreventivePlanAction::Pause, 1, 0)
                .unwrap_err()
                .code,
            "MAINTENANCE_PLAN_STATE"
        );
        assert_eq!(
            require_action(&actor(true), &draft, &PreventivePlanAction::Cancel, 1, 0)
                .unwrap()
                .status,
            "cancelled"
        );
        let mut cancelled = current.clone();
        cancelled.status = "cancelled".into();
        assert_eq!(
            require_action(&actor(true), &cancelled, &PreventivePlanAction::Cancel, 1, 0)
                .unwrap_err()
                .code,
            "MAINTENANCE_PLAN_STATE"
        );
    }

    #[test]
    fn retained_occurrence_guard_compares_stored_local_minutes() {
        assert!(retained_occurrence_conflict(
            Some("2026-03-08T02:30"),
            "2026-03-07T02:30"
        ));
        assert!(retained_occurrence_conflict(
            Some("2026-03-08T02:30"),
            "2026-03-08T02:30"
        ));
        assert!(!retained_occurrence_conflict(
            Some("2026-03-07T02:30"),
            "2026-03-08T02:30"
        ));
        assert!(!retained_occurrence_conflict(None, "2026-03-08T02:30"));
    }
}
