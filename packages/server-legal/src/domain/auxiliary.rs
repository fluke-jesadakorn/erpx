//! Auxiliary Legal reads and command boundaries.
//!
//! These are the former TypeScript-only workspace/report/knowledge/corpus/
//! governance/retention/reference/trust reads and the retention/governance
//! command rules that do not need a provider. Authorization, Company and
//! record scope, bounded input, replay and the exact original-bytes/trust
//! semantics stay here in the owner; adapters only bind parameters.
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};

/// Maximum accepted free-text search term for knowledge/law reads.
pub const SEARCH_MIN: usize = 2;
pub const SEARCH_MAX: usize = 200;
/// Maximum accepted opaque record/chunk identifier length.
pub const AUXILIARY_ID_MAX: usize = 200;
/// Bounded Retention report row limits that match the retained route.
pub const AUXILIARY_ROW_LIMIT: i64 = 200;
pub const AUXILIARY_JOB_LIMIT: i64 = 100;
pub const AUXILIARY_CHUNK_LIMIT: i64 = 1000;
pub const REASON_MIN: usize = 3;
pub const REASON_MAX: usize = 4000;
pub const RETENTION_DAYS_MAX: i64 = 36_500;

/// Current assignment scope captured by composition from the Account session.
/// It grants nothing by itself; every read re-checks it against owned rows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalAuxiliaryActor {
    pub user_id: UserId,
    pub assignment_id: AssignmentId,
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub role: String,
    pub permissions: Vec<String>,
}

impl LegalAuxiliaryActor {
    pub fn company_scoped(&self) -> bool {
        self.branch_id.is_none() && self.department_id.is_none()
    }
    pub fn is_requester(&self) -> bool {
        self.role == "legal_requester"
    }
    pub fn can(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }
}

/// One bounded auxiliary read. `as_of` is the selected Gregorian date used by
/// the law/knowledge reads; `None` means the current Bangkok date.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegalAuxiliaryRead {
    Workspace,
    Reports,
    Retention { id: Option<String> },
    Governance { id: Option<String> },
    LawCorpus { rule_id: Option<String> },
    Knowledge { query: String, as_of: Option<String> },
    LawCitation { id: String, as_of: Option<String> },
    References { item_id: String },
    Trust,
    SavedViews,
    AiHistory { item_id: String },
}

impl LegalAuxiliaryRead {
    pub const fn operation(&self) -> &'static str {
        match self {
            Self::Workspace => "legal.workspace.read",
            Self::Reports => "legal.reports.read",
            Self::Retention { .. } => "legal.retention.read",
            Self::Governance { .. } => "legal.governance.read",
            Self::LawCorpus { .. } => "legal.law-corpus.read",
            Self::Knowledge { .. } => "legal.knowledge.read",
            Self::LawCitation { .. } => "legal.law-citation.read",
            Self::References { .. } => "legal.references.read",
            Self::Trust => "legal.trust.read",
            Self::SavedViews => "legal.views.read",
            Self::AiHistory { .. } => "legal.ai-history.read",
        }
    }
}

/// Roles that may read the auxiliary Legal surface. The exact permission is
/// still required; this constant only documents the current gate.
pub const READ_PERMISSION: &str = "legal.read";
pub const TRUST_VIEW_PERMISSION: &str = "legal.manage";
pub const RETENTION_POLICY_PERMISSION: &str = "legal.manage";
pub const RETENTION_RECORD_PERMISSION: &str = "legal.destroy";

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
}

fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}

/// Empty identifiers never resolve to a different row; they fail closed.
pub fn parse_identifier(value: &str) -> Result<String, BusinessError> {
    let value = value.trim();
    if value.is_empty() || value.len() > AUXILIARY_ID_MAX {
        return Err(invalid(
            "LEGAL_REQUEST_INVALID",
            "Choose a current Legal resource.",
        ));
    }
    Ok(value.to_owned())
}

pub fn parse_search(value: &str) -> Result<String, BusinessError> {
    let value = value.trim();
    if value.len() < SEARCH_MIN || value.len() > SEARCH_MAX {
        return Err(invalid(
            "LEGAL_REQUEST_INVALID",
            "Use a search term between 2 and 200 characters.",
        ));
    }
    Ok(value.to_owned())
}

/// The route accepts an explicit Gregorian date or omits it for today. Any
/// other value is rejected instead of being passed to PostgreSQL as text.
pub fn parse_as_of(value: Option<&str>) -> Result<Option<String>, BusinessError> {
    let Some(value) = value else { return Ok(None) };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if !valid_date(value) {
        return Err(invalid(
            "LEGAL_REQUEST_INVALID",
            "Use a Gregorian date in YYYY-MM-DD form.",
        ));
    }
    Ok(Some(value.to_owned()))
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let digits = |range: std::ops::Range<usize>| -> Option<i64> {
        let mut value = 0_i64;
        for byte in &bytes[range] {
            if !byte.is_ascii_digit() {
                return None;
            }
            value = value * 10 + i64::from(*byte - b'0');
        }
        Some(value)
    };
    let (Some(year), Some(month), Some(day)) = (digits(0..4), digits(5..7), digits(8..10)) else {
        return false;
    };
    if !(1..=12).contains(&month) || day < 1 {
        return false;
    }
    day <= days_in_month(year, month)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// The retained route selects the current date in Bangkok (UTC+7), not the
/// server's local zone. Kept deterministic for replay and tests.
pub fn bangkok_date(now_millis: i64) -> String {
    let days = (now_millis + 7 * 3_600_000).div_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Howard Hinnant's civil-from-days, valid for the retained millisecond range.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Exact retained visibility: Company, selected branch/department, and either
/// an internal classification or explicit membership. A Legal requester only
/// sees their own request aggregates.
pub const VISIBILITY_PREDICATE: &str = "i.company_id=$1 \
 AND ($2::text IS NULL OR i.branch_id=$2) \
 AND ($3::text IS NULL OR i.department_id=$3) \
 AND (i.classification='internal' OR EXISTS(SELECT 1 FROM legal_members lm WHERE lm.item_id=i.id AND lm.user_id=$4))";
pub const REQUESTER_PREDICATE: &str = " AND i.kind='request' AND i.created_by=$4";

pub fn visibility_predicate(actor: &LegalAuxiliaryActor) -> String {
    visibility_for(actor, "i")
}

/// Same predicate for a compile-owned relation alias. Callers pass only string
/// literals written in this crate; no request text is ever interpolated.
pub fn visibility_for(actor: &LegalAuxiliaryActor, alias: &str) -> String {
    let predicate = VISIBILITY_PREDICATE.replace("i.", &format!("{alias}."));
    if actor.is_requester() {
        format!(
            "{predicate}{}",
            REQUESTER_PREDICATE.replace("i.", &format!("{alias}."))
        )
    } else {
        predicate
    }
}

pub fn authorize_read(
    actor: &LegalAuxiliaryActor,
    read: &LegalAuxiliaryRead,
) -> Result<(), BusinessError> {
    let permission = match read {
        LegalAuxiliaryRead::Trust => TRUST_VIEW_PERMISSION,
        _ => READ_PERMISSION,
    };
    if !actor.can(permission) {
        return Err(forbidden(
            "PERMISSION_REQUIRED",
            "Your selected role does not grant this permission.",
        ));
    }
    if actor.company_id.as_str().is_empty() || actor.holding_id.as_str().is_empty() {
        return Err(forbidden(
            "LEGAL_SCOPE_REQUIRED",
            "Select a current Company before reading Legal.",
        ));
    }
    if let LegalAuxiliaryRead::Knowledge { query, .. } = read {
        parse_search(query)?;
    }
    if let LegalAuxiliaryRead::LawCitation { id, .. } = read {
        parse_identifier(id)?;
    }
    Ok(())
}

/// Validated retention command that owns the whole policy/destruction lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegalRetentionAction {
    Policy,
    ApprovePolicy,
    RetirePolicy,
    Assign,
    Request,
    Approve,
    Cancel,
    Retry,
}

impl LegalRetentionAction {
    pub const fn operation(self) -> &'static str {
        match self {
            Self::Policy => "retention.policy",
            Self::ApprovePolicy => "retention.approve-policy",
            Self::RetirePolicy => "retention.retire-policy",
            Self::Assign => "retention.assign",
            Self::Request => "retention.request",
            Self::Approve => "retention.approve",
            Self::Cancel => "retention.cancel",
            Self::Retry => "retention.retry",
        }
    }
    pub fn parse(action: &str) -> Option<Self> {
        Some(match action {
            "retention.policy" => Self::Policy,
            "retention.approve-policy" => Self::ApprovePolicy,
            "retention.retire-policy" => Self::RetirePolicy,
            "retention.assign" => Self::Assign,
            "retention.request" => Self::Request,
            "retention.approve" => Self::Approve,
            "retention.cancel" => Self::Cancel,
            "retention.retry" => Self::Retry,
            _ => return None,
        })
    }
    pub const fn policy_lifecycle(self) -> bool {
        matches!(self, Self::Policy | Self::ApprovePolicy | Self::RetirePolicy)
    }
}

/// Actions whose business rules are native but whose provider credentials are
/// deferred to A1. Composition must keep these fail-closed; they never fall
/// back to a TypeScript business implementation.
pub const PROVIDER_GATED_ACTIONS: [&str; 6] = [
    "ai.generate",
    "ai.apply",
    "signing.send",
    "signing.action",
    "signing.cancel",
    "document.render",
];

pub fn provider_gated(action: &str) -> bool {
    PROVIDER_GATED_ACTIONS.contains(&action) || action.starts_with("signing.")
}

pub fn provider_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LEGAL_PROVIDER_UNAVAILABLE",
        "This Legal action needs a configured provider and stays unavailable.",
    )
}

/// True when this action belongs to the native auxiliary command slice. The
/// TypeScript transport uses the same set to stop selecting its retained
/// dispatcher once composition mounts the native port.
pub fn auxiliary_command_selected(action: &str) -> bool {
    LegalRetentionAction::parse(action).is_some()
}

pub fn validate_reason(value: &str) -> Result<String, BusinessError> {
    let value = value.trim();
    if value.len() < REASON_MIN || value.len() > REASON_MAX {
        return Err(invalid(
            "LEGAL_REASON_REQUIRED",
            "Explain the decision in 3 to 4,000 characters.",
        ));
    }
    Ok(value.to_owned())
}

pub fn positive_version(value: Option<i64>) -> Result<i64, BusinessError> {
    value.filter(|value| *value > 0).ok_or_else(|| {
        invalid(
            "LEGAL_VERSION_CONFLICT",
            "Refresh the Legal record before saving.",
        )
    })
}

pub fn close_states(state: &str) -> bool {
    matches!(
        state,
        "archived" | "closed" | "completed" | "expired" | "terminated"
    )
}

/// Retention assignment is only meaningful on a closed/archived record.
pub fn retention_assignable(state: &str) -> Result<(), BusinessError> {
    if close_states(state) {
        Ok(())
    } else {
        Err(invalid(
            "LEGAL_RETENTION_STATE",
            "Close or archive the record before assigning retention.",
        ))
    }
}

pub fn retention_requestable(state: &str, eligible: bool) -> Result<(), BusinessError> {
    if !eligible {
        return Err(invalid(
            "LEGAL_RETENTION_NOT_ELAPSED",
            "Retention has not elapsed under the currently approved policy.",
        ));
    }
    if close_states(state) {
        Ok(())
    } else {
        Err(invalid(
            "LEGAL_RETENTION_STATE",
            "This record is still active.",
        ))
    }
}

/// Independent-review rule shared by policy, destruction, form and authority.
pub fn require_independent_review(
    same_person: bool,
    state_ok: bool,
    message: &'static str,
) -> Result<(), BusinessError> {
    if !state_ok {
        return Err(invalid(
            "LEGAL_VERSION_CONFLICT",
            "This Legal decision changed. Refresh before saving.",
        ));
    }
    if same_person {
        return Err(forbidden("LEGAL_INDEPENDENT_REVIEW", message));
    }
    Ok(())
}

pub fn destruction_transition(action: LegalRetentionAction, state: &str) -> Result<&'static str, BusinessError> {
    match action {
        LegalRetentionAction::Approve => {
            if state != "requested" {
                return Err(invalid(
                    "LEGAL_VERSION_CONFLICT",
                    "This destruction request changed. Refresh before saving.",
                ));
            }
            Ok("approved")
        }
        LegalRetentionAction::Retry => match state {
            "failed" | "blocked" => Ok("requested"),
            "executing" => Ok("executing"),
            _ => Err(invalid(
                "LEGAL_RETENTION_STATE",
                "Only blocked or failed work can be retried.",
            )),
        },
        LegalRetentionAction::Cancel => match state {
            "completed" | "executing" => Err(invalid(
                "LEGAL_RETENTION_STATE",
                "This destruction cannot be cancelled.",
            )),
            _ => Ok("cancelled"),
        },
        _ => Err(invalid(
            "LEGAL_RETENTION_STATE",
            "Choose a current retention decision.",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(role: &str, permissions: &[&str]) -> LegalAuxiliaryActor {
        LegalAuxiliaryActor {
            user_id: UserId::new("user-one").unwrap(),
            assignment_id: AssignmentId::new("assignment-one").unwrap(),
            holding_id: HoldingId::new("holding-one").unwrap(),
            company_id: CompanyId::new("company-one").unwrap(),
            branch_id: None,
            department_id: None,
            role: role.into(),
            permissions: permissions.iter().map(|value| (*value).to_owned()).collect(),
        }
    }

    #[test]
    fn dates_are_gregorian_and_reject_overflow() {
        assert!(valid_date("2024-02-29"));
        assert!(!valid_date("2023-02-29"));
        assert!(!valid_date("2024-13-01"));
        assert!(!valid_date("2024-1-01"));
        assert!(parse_as_of(Some("2024-02-29")).unwrap().is_some());
        assert!(parse_as_of(Some("2024-02-30")).is_err());
        assert!(parse_as_of(Some("  ")).unwrap().is_none());
        assert!(parse_as_of(None).unwrap().is_none());
    }

    #[test]
    fn bangkok_date_uses_utc_plus_seven() {
        // 2026-11-11T00:00:00+07:00 is the next Bangkok day.
        assert_eq!(bangkok_date(1_794_330_000_000), "2026-11-11");
        // One second earlier remains the previous day.
        assert_eq!(bangkok_date(1_794_329_999_000), "2026-11-10");
    }

    #[test]
    fn reads_require_the_exact_permission_and_current_company() {
        assert!(authorize_read(&actor("legal_manager", &["legal.read"]), &LegalAuxiliaryRead::Workspace).is_ok());
        assert!(authorize_read(&actor("legal_manager", &["legal.view"]), &LegalAuxiliaryRead::Workspace).is_err());
        assert!(authorize_read(&actor("legal_manager", &["legal.read"]), &LegalAuxiliaryRead::Trust).is_err());
        assert!(authorize_read(&actor("legal_manager", &["legal.manage"]), &LegalAuxiliaryRead::Trust).is_ok());
        let mut scoped = actor("legal_manager", &["legal.read"]);
        scoped.branch_id = Some(RecordId::new("branch-one").unwrap());
        assert!(!scoped.company_scoped());
        assert!(authorize_read(&scoped, &LegalAuxiliaryRead::Workspace).is_ok());
    }

    #[test]
    fn searches_are_bounded_and_trimmed() {
        assert_eq!(parse_search("  contract ").unwrap(), "contract");
        assert!(parse_search("a").is_err());
        assert!(parse_search(&"x".repeat(SEARCH_MAX + 1)).is_err());
    }

    #[test]
    fn requester_visibility_is_narrowed_to_own_requests() {
        let requester = actor("legal_requester", &["legal.read"]);
        assert!(visibility_predicate(&requester).ends_with(REQUESTER_PREDICATE));
        assert_eq!(visibility_predicate(&actor("legal_manager", &["legal.read"])), VISIBILITY_PREDICATE);
    }

    #[test]
    fn retention_lifecycle_is_parseable_and_bounded() {
        assert_eq!(LegalRetentionAction::parse("retention.policy").unwrap().operation(), "retention.policy");
        assert!(LegalRetentionAction::parse("retention.assign").unwrap().policy_lifecycle() == false);
        assert!(LegalRetentionAction::parse("retention.policy").unwrap().policy_lifecycle());
        assert!(LegalRetentionAction::parse("retention.other").is_none());
        assert!(retention_assignable("open").is_err());
        assert!(retention_assignable("archived").is_ok());
        assert!(retention_requestable("closed", false).is_err());
        assert!(retention_requestable("open", true).is_err());
        assert!(retention_requestable("closed", true).is_ok());
        assert_eq!(destruction_transition(LegalRetentionAction::Approve, "requested").unwrap(), "approved");
        assert!(destruction_transition(LegalRetentionAction::Approve, "approved").is_err());
        assert_eq!(destruction_transition(LegalRetentionAction::Retry, "blocked").unwrap(), "requested");
        assert!(destruction_transition(LegalRetentionAction::Retry, "approved").is_err());
        assert_eq!(destruction_transition(LegalRetentionAction::Cancel, "requested").unwrap(), "cancelled");
        assert!(destruction_transition(LegalRetentionAction::Cancel, "executing").is_err());
    }

    #[test]
    fn independent_review_requires_a_different_person() {
        assert!(require_independent_review(false, true, "Independent review.").is_ok());
        assert_eq!(
            require_independent_review(true, true, "Independent review.").unwrap_err().kind,
            FailureKind::Forbidden
        );
        assert_eq!(
            require_independent_review(false, false, "Independent review.").unwrap_err().kind,
            FailureKind::InvalidInput
        );
    }

    #[test]
    fn provider_paths_stay_fail_closed() {
        assert!(provider_gated("ai.generate"));
        assert!(provider_gated("signing.action"));
        assert!(!provider_gated("retention.assign"));
        assert_eq!(provider_unavailable().kind, FailureKind::Unavailable);
    }

    #[test]
    fn positive_versions_and_reasons_are_enforced() {
        assert_eq!(positive_version(Some(2)).unwrap(), 2);
        assert!(positive_version(Some(0)).is_err());
        assert!(positive_version(None).is_err());
        assert!(validate_reason("  ok ").is_err());
        assert_eq!(validate_reason("  justified ").unwrap(), "justified");
    }
}
