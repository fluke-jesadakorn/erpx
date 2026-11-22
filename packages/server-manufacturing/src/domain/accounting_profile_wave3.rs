//! Pure Manufacturing accounting-profile command facts (wave 3 retirement).
//!
//! This module owns the bounds and the create/review transition decision for
//! the native `manufacturing_accounting_profiles` writer. It performs no IO,
//! opens no transaction and emits no SQL: the application layer turns its typed
//! plan into repository descriptors and the owner Postgres adapter supplies the
//! only SQL in this capability.
use erpx_server_runtime::public::http_support::serde_json::Value;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

pub const PROFILE_CREATE_OPERATION: &str = "manufacturing.accounting.create";
pub const PROFILE_APPROVE_OPERATION: &str = "manufacturing.accounting.approve";
pub const PROFILE_REJECT_OPERATION: &str = "manufacturing.accounting.reject";
pub const PROFILE_RETIRE_OPERATION: &str = "manufacturing.accounting.retire";
pub const PROFILE_RECORD_TYPE: &str = "manufacturing_accounting_profiles";
pub const MAX_IDENTIFIER: usize = 200;
pub const MAX_REASON: usize = 3_000;

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "MANUFACTURING_ACCOUNTING_REQUEST_INVALID",
        "Review the accounting profile and try again.",
    )
}

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "MANUFACTURING_ACCOUNTING_NOT_FOUND",
        "The accounting profile is not available.",
    )
}

pub fn review_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CONFLICT",
        "An independent current review is required.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MANUFACTURING_ACCOUNTING_UNAVAILABLE",
        "The native Manufacturing accounting service is unavailable.",
    )
}

/// Validated create input. Every account is a bounded identifier; the Finance
/// chart/account existence check stays in the caller's Finance port.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileDraft {
    pub chart_id: String,
    pub inventory_account_id: String,
    pub wip_account_id: String,
    pub labor_account_id: String,
    pub overhead_account_id: String,
    pub scrap_account_id: String,
}

impl ProfileDraft {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        Ok(Self {
            chart_id: identifier(raw, "chartId")?,
            inventory_account_id: identifier(raw, "inventoryAccountId")?,
            wip_account_id: identifier(raw, "wipAccountId")?,
            labor_account_id: identifier(raw, "laborAccountId")?,
            overhead_account_id: identifier(raw, "overheadAccountId")?,
            scrap_account_id: identifier(raw, "scrapAccountId")?,
        })
    }
}

fn identifier(raw: &Value, field: &str) -> Result<String, BusinessError> {
    match raw.get(field) {
        Some(Value::String(value))
            if !value.is_empty()
                && value.len() <= MAX_IDENTIFIER
                && !value.contains('\u{0000}') =>
        {
            Ok(value.clone())
        }
        _ => Err(invalid()),
    }
}

/// Current persisted profile projection used for the optimistic review CAS.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileRow {
    pub status: String,
    pub version: i64,
    pub created_by: String,
}

/// The three independent review transitions retained by the TypeScript caller.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewAction {
    Approve,
    Reject,
    Retire,
}

impl ReviewAction {
    pub fn status(self) -> &'static str {
        match self {
            Self::Approve => "approved",
            Self::Reject => "rejected",
            Self::Retire => "retired",
        }
    }

    pub fn operation(self) -> &'static str {
        match self {
            Self::Approve => PROFILE_APPROVE_OPERATION,
            Self::Reject => PROFILE_REJECT_OPERATION,
            Self::Retire => PROFILE_RETIRE_OPERATION,
        }
    }

    fn expected_status(self) -> &'static str {
        match self {
            Self::Approve | Self::Reject => "draft",
            Self::Retire => "approved",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewInput {
    pub action: ReviewAction,
    pub expected_version: i64,
    pub reason: String,
}

impl ReviewInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let action = match raw.get("action").and_then(Value::as_str) {
            Some("approve") => ReviewAction::Approve,
            Some("reject") => ReviewAction::Reject,
            Some("retire") => ReviewAction::Retire,
            _ => return Err(invalid()),
        };
        let expected_version = match raw.get("expectedVersion").and_then(Value::as_i64) {
            Some(value) if value >= 1 => value,
            _ => return Err(invalid()),
        };
        let reason = match raw.get("reason") {
            Some(Value::String(value)) if !value.trim().is_empty() && value.len() <= MAX_REASON => {
                value.clone()
            }
            _ => return Err(invalid()),
        };
        Ok(Self {
            action,
            expected_version,
            reason,
        })
    }
}

/// A create plan: draft at version 1 with the caller's identifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatePlan {
    pub id: String,
    pub draft: ProfileDraft,
    pub version: i64,
}

/// A review plan: the exact status/version CAS and version increment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewPlan {
    pub action: ReviewAction,
    pub expected_version: i64,
    pub next_version: i64,
    pub reason: String,
}

/// Pure create plan. `id` is the composition-owned aggregate key.
pub fn plan_create(id: String, draft: ProfileDraft) -> Result<CreatePlan, BusinessError> {
    if id.is_empty() || id.len() > MAX_IDENTIFIER {
        return Err(invalid());
    }
    Ok(CreatePlan {
        id,
        draft,
        version: 1,
    })
}

/// Source rules the retained Finance boundary enforced before the wave-3
/// cutover: WIP must be distinct from the inventory and conversion-cost
/// accounts, and the profile must use an existing THB functional-currency
/// chart. Both are checked after command replay so a retained receipt still
/// replays even if the sources changed afterwards.
pub fn require_profile_sources(
    draft: &ProfileDraft,
    chart_currency: Option<&str>,
) -> Result<(), BusinessError> {
    if draft.wip_account_id == draft.inventory_account_id
        || draft.wip_account_id == draft.labor_account_id
        || draft.wip_account_id == draft.overhead_account_id
        || draft.wip_account_id == draft.scrap_account_id
    {
        return Err(BusinessError::invalid(
            "MANUFACTURING_WIP_ACCOUNT",
            "WIP must be separate from inventory and conversion-cost clearing accounts.",
        ));
    }
    if chart_currency != Some("THB") {
        return Err(BusinessError::invalid(
            "MANUFACTURING_CURRENCY_ADAPTER",
            "The production accounting adapter requires a THB functional-currency chart.",
        ));
    }
    Ok(())
}

/// Independent current review: the reviewer must not be the preparer, the row
/// must exist at the expected version and the source status must permit it.
pub fn plan_review(
    input: &ReviewInput,
    current: Option<&ProfileRow>,
    actor: &str,
) -> Result<ReviewPlan, BusinessError> {
    let row = current.ok_or_else(not_found)?;
    if actor == row.created_by {
        return Err(review_conflict());
    }
    if row.version != input.expected_version || row.status != input.action.expected_status() {
        return Err(review_conflict());
    }
    Ok(ReviewPlan {
        action: input.action,
        expected_version: input.expected_version,
        next_version: input.expected_version + 1,
        reason: input.reason.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_runtime::public::http_support::serde_json::json;

    fn draft() -> Value {
        json!({
            "chartId":"chart-1","inventoryAccountId":"inv","wipAccountId":"wip",
            "laborAccountId":"labor","overheadAccountId":"overhead","scrapAccountId":"scrap"
        })
    }

    #[test]
    fn parses_and_rejects_bounded_draft() {
        assert!(ProfileDraft::parse(&draft()).is_ok());
        let mut bad = draft();
        bad["chartId"] = json!("");
        assert!(ProfileDraft::parse(&bad).is_err());
    }

    #[test]
    fn create_is_draft_at_version_one() {
        let plan = plan_create("profile-1".into(), ProfileDraft::parse(&draft()).unwrap()).unwrap();
        assert_eq!(plan.id, "profile-1");
        assert_eq!(plan.version, 1);
    }

    #[test]
    fn rejects_shared_wip_and_non_thb_chart() {
        let draft = ProfileDraft::parse(&draft()).unwrap();
        assert!(require_profile_sources(&draft, Some("THB")).is_ok());
        assert_eq!(
            require_profile_sources(&draft, Some("USD")).unwrap_err().code,
            "MANUFACTURING_CURRENCY_ADAPTER"
        );
        assert_eq!(
            require_profile_sources(&draft, None).unwrap_err().code,
            "MANUFACTURING_CURRENCY_ADAPTER"
        );
        let shared = ProfileDraft {
            wip_account_id: draft.inventory_account_id.clone(),
            ..draft
        };
        assert_eq!(
            require_profile_sources(&shared, Some("THB")).unwrap_err().code,
            "MANUFACTURING_WIP_ACCOUNT"
        );
    }

    #[test]
    fn review_requires_independent_current_transition() {
        let input = ReviewInput::parse(
            &json!({"action":"approve","expectedVersion":1,"reason":"Independent review"}),
        )
        .unwrap();
        let row = ProfileRow {
            status: "draft".into(),
            version: 1,
            created_by: "maker".into(),
        };
        assert!(plan_review(&input, Some(&row), "maker").is_err());
        assert_eq!(
            plan_review(&input, Some(&row), "reviewer")
                .unwrap()
                .next_version,
            2
        );
        let stale = ProfileRow { version: 2, ..row };
        assert!(plan_review(&input, Some(&stale), "reviewer").is_err());
    }
}
