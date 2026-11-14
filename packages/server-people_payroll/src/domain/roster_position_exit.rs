//! Native People roster Position-exit plan and retained SQL predicates.
//!
//! A retained Organization position save closes the roster effects of every
//! exiting engagement: it bumps the Company roster lock and future week
//! versions, appends the released-position history rows, releases the affected
//! future active assignments, appends request invalidation history and
//! invalidates the affected pending roster requests. Collaboration notices are
//! written between those People phases by composition, so this owner never
//! writes `staff_notifications`.
//!
//! The builders below validate only the bounded command input. Company/role
//! authorization and the current Position decision stay with the caller; no
//! engagement id is inferred from a Person, Position or department here.
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};

pub const ROSTER_POSITION_EXIT_RELEASED_ACTION: &str = "assignment.released.position";
pub const ROSTER_POSITION_EXIT_INVALIDATED_ACTION: &str = "request.invalidated.position";
/// Retained Organization position reason bounds (`min(3).max(1000)`).
pub const ROSTER_POSITION_EXIT_MIN_REASON: usize = 3;
pub const ROSTER_POSITION_EXIT_MAX_REASON: usize = 1_000;
/// Retained `CommandKey` bounds, because the history row ids embed the key.
pub const ROSTER_POSITION_EXIT_MIN_COMMAND: usize = 8;
pub const ROSTER_POSITION_EXIT_MAX_COMMAND: usize = 180;
pub const ROSTER_POSITION_EXIT_MAX_ENGAGEMENTS: usize = 500;

const MAX_ACTOR: usize = 200;
const MAX_ENGAGEMENT_ID: usize = 200;
const MAX_TIMESTAMP_MILLIS: i64 = 9_007_199_254_740_991;

pub fn roster_position_exit_invalid() -> BusinessError {
    BusinessError::invalid(
        "ROSTER_POSITION_EXIT_INVALID",
        "Review the Position change reason and the exiting staff.",
    )
}

/// The retained `affected` predicate. `company`, `ids` and `now` are SQL
/// parameter tokens (`$1`, `$6`, `$5`) chosen by the statement that embeds it.
///
/// `released` assignments, cancelled shifts and shifts that already started are
/// excluded exactly as before; the Company predicate keeps the week scope.
pub fn affected_assignments(company: &str, ids: &str, now: &str) -> String {
    format!(
        "SELECT a.id,a.person_id,a.engagement_id,s.week_id,w.department_id
    FROM people_roster_assignments a JOIN people_roster_shifts s ON s.id=a.shift_id
    JOIN people_roster_weeks w ON w.id=s.week_id
    WHERE w.company_id={company} AND a.engagement_id=ANY({ids}::text[]) AND a.state='assigned' AND s.state='active' AND s.starts_at>{now}"
    )
}

/// The retained `requests` predicate: any pending roster request that names one
/// of the exiting engagements as requester, coworker, source or target.
pub fn pending_position_requests(company: &str, ids: &str) -> String {
    format!(
        "SELECT r.* FROM people_roster_requests r WHERE r.company_id={company} AND r.state IN ('pending_coworker','pending_manager')
    AND (r.requester_person_id IN (SELECT person_id FROM hr_engagements WHERE id=ANY({ids}::text[]))
      OR r.coworker_person_id IN (SELECT person_id FROM hr_engagements WHERE id=ANY({ids}::text[]))
      OR r.source_assignment_id IN (SELECT id FROM people_roster_assignments WHERE engagement_id=ANY({ids}::text[]))
      OR r.target_assignment_id IN (SELECT id FROM people_roster_assignments WHERE engagement_id=ANY({ids}::text[])))"
    )
}

/// One validated roster Position-exit command. Planning never reports a
/// partial engagement list: composition either stages every retained phase for
/// this exact list or nothing at all.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RosterPositionExitPlan {
    pub company_id: CompanyId,
    pub actor_user_id: String,
    pub engagement_ids: Vec<String>,
    pub command_id: String,
    pub reason: String,
    pub now: TimestampMillis,
}

impl RosterPositionExitPlan {
    pub fn new(
        company_id: CompanyId,
        actor_user_id: &str,
        engagement_ids: &[String],
        command_id: &str,
        reason: &str,
        now: TimestampMillis,
    ) -> Result<Self, BusinessError> {
        let plan = Self {
            company_id,
            actor_user_id: actor_user_id.to_owned(),
            engagement_ids: engagement_ids.to_vec(),
            command_id: command_id.to_owned(),
            reason: reason.trim().to_owned(),
            now,
        };
        plan.validate()?;
        Ok(plan)
    }

    /// Fail closed on any bounded input the retained effect builder accepted.
    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.company_id.as_str().is_empty()
            || self.actor_user_id.trim().is_empty()
            || self.actor_user_id.chars().count() > MAX_ACTOR
            || self.engagement_ids.is_empty()
            || self.engagement_ids.len() > ROSTER_POSITION_EXIT_MAX_ENGAGEMENTS
            || self.engagement_ids.iter().any(|id| {
                id.trim().is_empty() || id.chars().count() > MAX_ENGAGEMENT_ID || id != id.trim()
            })
            || self.command_id.len() < ROSTER_POSITION_EXIT_MIN_COMMAND
            || self.command_id.len() > ROSTER_POSITION_EXIT_MAX_COMMAND
            || !self
                .command_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b":_.-".contains(&byte))
            || !(ROSTER_POSITION_EXIT_MIN_REASON..=ROSTER_POSITION_EXIT_MAX_REASON)
                .contains(&self.reason.chars().count())
            || !(0..=MAX_TIMESTAMP_MILLIS).contains(&self.now.get())
        {
            return Err(roster_position_exit_invalid());
        }
        Ok(())
    }
}
