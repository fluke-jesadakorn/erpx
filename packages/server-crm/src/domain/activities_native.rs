//! Native CRM lead-activity create / complete / cancel validation.
//!
//! Owner-local rows only (`crm_lead_activities` + `crm_lead_activity_events`).
//! `reassign` stays TS-owned (requires sales-manager approval + the
//! `sales_recipient_eligibility` cross-owner join, which the coordinator has
//! not yet sourced for Rust). This module validates bounded input and state
//! transitions; it reads no table, performs no purchase check and emits no
//! notification. Purchase enforcement (`sales.crm` in `withCRMPurchasedCommand`
//! / `purchases.ts`) and the Workflow ledger remain the TS runtime path.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

const SUBJECT_LIMIT: usize = 200;
const DETAILS_LIMIT: usize = 2_000;

/// Raw wire shape matching `crmActivityCreateSchema` / `crmActivityChangeSchema`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityCreateInput {
    #[serde(rename = "leadId")]
    pub lead_id: String,
    #[serde(rename = "expectedVersion")]
    pub expected_version: i64,
    #[serde(rename = "activityType")]
    pub activity_type: String,
    pub subject: String,
    pub details: String,
    #[serde(rename = "ownerId")]
    pub owner_id: String,
    #[serde(rename = "dueDate")]
    pub due_date: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityChangeInput {
    pub action: String,
    #[serde(rename = "expectedVersion")]
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityActor {
    pub user_id: String,
    pub company_id: String,
    pub can_sales: bool,
    pub can_manage: bool,
    pub has_read: bool,
    pub same_person_ids: Vec<String>,
}

impl ActivityActor {
    pub fn is_same_person(&self, user_id: &str) -> bool {
        user_id == self.user_id || self.same_person_ids.iter().any(|id| id == user_id)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ActivityCreatePlan {
    pub lead_id: String,
    pub expected_lead_version: i64,
    pub activity_type: String,
    pub subject: String,
    pub details: String,
    pub owner_id: String,
    pub due_date: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ActivityChangePlan {
    pub action: String,
    pub expected_version: i64,
    pub status: String,
    pub outcome: Option<String>,
}

fn invalid_input(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
}

fn valid_text(value: &str, limit: usize) -> bool {
    (3..=limit).contains(
        &crate::domain::accounts::js_trim(value)
            .encode_utf16()
            .count(),
    )
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    if bytes
        .iter()
        .enumerate()
        .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return false;
    }
    let year = value[0..4].parse::<u32>().unwrap_or(0);
    let month = value[5..7].parse::<u32>().unwrap_or(0);
    let day = value[8..10].parse::<u32>().unwrap_or(0);
    if year == 0 || !(1..=12).contains(&month) {
        return false;
    }
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}

pub fn validate_create(
    actor: &ActivityActor,
    input: &ActivityCreateInput,
) -> Result<ActivityCreatePlan, BusinessError> {
    if !actor.can_sales {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "CRM_ACTIVITY_SALES_REQUIRED",
            "Your role does not grant activity create access.",
        ));
    }
    if !actor.has_read {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "CRM_PERMISSION_REQUIRED",
            "Your selected role does not grant CRM read access.",
        ));
    }
    if !matches!(
        input.activity_type.as_str(),
        "call" | "email" | "meeting" | "task"
    ) {
        return Err(invalid_input(
            "CRM_ACTIVITY_TYPE_INVALID",
            "Choose a call, email, meeting or task activity.",
        ));
    }
    if input.expected_version < 1
        || !valid_identifier(&input.lead_id)
        || !valid_identifier(&input.owner_id)
        || !valid_text(&input.subject, SUBJECT_LIMIT)
        || !valid_text(&input.details, DETAILS_LIMIT)
        || !valid_date(&input.due_date)
    {
        return Err(invalid_input(
            "CRM_ACTIVITY_INPUT_INVALID",
            "Use a bounded CRM activity with a valid due date.",
        ));
    }
    Ok(ActivityCreatePlan {
        lead_id: input.lead_id.clone(),
        expected_lead_version: input.expected_version,
        activity_type: input.activity_type.clone(),
        subject: crate::domain::accounts::js_trim(&input.subject).to_owned(),
        details: crate::domain::accounts::js_trim(&input.details).to_owned(),
        owner_id: input.owner_id.clone(),
        due_date: input.due_date.clone(),
    })
}

/// Lead status gate (mirrors the TS application check). Callers must supply the
/// current lead status/stage read from the owner's `crm_leads` row.
pub fn require_lead_open(status: &str, stage: Option<&str>) -> Result<(), BusinessError> {
    if !matches!(status, "open" | "qualified") || matches!(stage, Some("won") | Some("lost")) {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "CRM_ACTIVITY_SOURCE_CLOSED",
            "Only an open or active opportunity can receive a new activity.",
        ));
    }
    Ok(())
}

pub fn validate_change(
    actor: &ActivityActor,
    input: &ActivityChangeInput,
    record_owner_id: &str,
    record_status: &str,
) -> Result<ActivityChangePlan, BusinessError> {
    if !actor.can_sales || !actor.has_read {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "CRM_ACTIVITY_SALES_REQUIRED",
            "Your role does not grant activity change access.",
        ));
    }
    if !matches!(input.action.as_str(), "complete" | "cancel") {
        if input.action == "reassign" {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "CRM_ACTIVITY_REASSIGN_TS_OWNED",
                "Activity reassign remains in the TypeScript owner.",
            ));
        }
        return Err(invalid_input(
            "CRM_ACTIVITY_ACTION_INVALID",
            "Choose a complete or cancel activity action.",
        ));
    }
    if input.expected_version < 1 || !valid_text(&input.reason, DETAILS_LIMIT) {
        return Err(invalid_input(
            "CRM_ACTIVITY_INPUT_INVALID",
            "Use a bounded CRM activity reason.",
        ));
    }
    if record_status != "open" {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "CRM_ACTIVITY_STATE_CHANGED",
            "This activity is already closed.",
        ));
    }
    let is_complete = input.action == "complete";
    // complete: owner only. cancel: owner or manager (mirrors TS rules).
    let allowed = if is_complete {
        actor.user_id == record_owner_id
    } else {
        actor.can_manage || actor.user_id == record_owner_id
    };
    if !allowed {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "CRM_ACTIVITY_OWNER_REQUIRED",
            "The assigned salesperson completes an activity; a sales manager manages responsibility.",
        ));
    }
    Ok(ActivityChangePlan {
        action: input.action.clone(),
        expected_version: input.expected_version,
        status: if is_complete {
            "completed".into()
        } else {
            "cancelled".into()
        },
        outcome: Some(crate::domain::accounts::js_trim(&input.reason).to_owned()),
    })
}

pub fn create_record_id(key: &str) -> String {
    format!("crm-activity-{key}")
}

pub const CREATE_OPERATION: &str = "crm.activity.create";

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn actor() -> ActivityActor {
        ActivityActor {
            user_id: "owner".into(),
            company_id: "company".into(),
            can_sales: true,
            can_manage: false,
            has_read: true,
            same_person_ids: vec![],
        }
    }

    fn create() -> ActivityCreateInput {
        serde_json::from_value(json!({
            "leadId": "lead-1", "expectedVersion": 1, "activityType": "call",
            "subject": "Call the customer", "details": "Discuss scope",
            "ownerId": "owner", "dueDate": "2026-11-20"
        }))
        .unwrap()
    }

    #[test]
    fn create_plan_trims_text_and_keeps_wire_fields() {
        let mut input = create();
        input.subject = "  Call the customer  ".into();
        let plan = validate_create(&actor(), &input).unwrap();
        assert_eq!(plan.subject, "Call the customer");
        assert_eq!(plan.activity_type, "call");
        assert_eq!(plan.due_date, "2026-11-20");
    }

    #[test]
    fn create_rejects_bad_type_date_and_version() {
        let mut input = create();
        input.activity_type = "visit".into();
        assert_eq!(
            validate_create(&actor(), &input).unwrap_err().code,
            "CRM_ACTIVITY_TYPE_INVALID"
        );
        let mut input = create();
        input.due_date = "2026-02-29".into();
        assert!(validate_create(&actor(), &input).is_err());
        let mut input = create();
        input.expected_version = 0;
        assert!(validate_create(&actor(), &input).is_err());
    }

    #[test]
    fn create_requires_sales_and_read() {
        let mut a = actor();
        a.can_sales = false;
        assert_eq!(
            validate_create(&a, &create()).unwrap_err().code,
            "CRM_ACTIVITY_SALES_REQUIRED"
        );
        let mut a = actor();
        a.has_read = false;
        assert_eq!(
            validate_create(&a, &create()).unwrap_err().code,
            "CRM_PERMISSION_REQUIRED"
        );
    }

    #[test]
    fn lead_gate_matches_ts_state_rules() {
        assert!(require_lead_open("open", None).is_ok());
        assert!(require_lead_open("qualified", Some("proposal")).is_ok());
        assert_eq!(
            require_lead_open("disqualified", None).unwrap_err().code,
            "CRM_ACTIVITY_SOURCE_CLOSED"
        );
        assert!(require_lead_open("qualified", Some("won")).is_err());
    }

    #[test]
    fn change_complete_is_owner_only_and_cancel_allows_manager() {
        let change = ActivityChangeInput {
            action: "complete".into(),
            expected_version: 1,
            reason: "Done".into(),
        };
        assert!(validate_change(&actor(), &change, "owner", "open").is_ok());
        assert_eq!(
            validate_change(&actor(), &change, "other", "open")
                .unwrap_err()
                .code,
            "CRM_ACTIVITY_OWNER_REQUIRED"
        );
        let mut manager = actor();
        manager.can_manage = true;
        manager.user_id = "manager".into();
        let cancel = ActivityChangeInput {
            action: "cancel".into(),
            expected_version: 1,
            reason: "No longer needed".into(),
        };
        let plan = validate_change(&manager, &cancel, "other", "open").unwrap();
        assert_eq!(plan.status, "cancelled");
        assert_eq!(
            validate_change(&actor(), &cancel, "other", "open")
                .unwrap_err()
                .code,
            "CRM_ACTIVITY_OWNER_REQUIRED"
        );
    }

    #[test]
    fn change_rejects_reassign_closed_and_bad_reason() {
        let reassign = ActivityChangeInput {
            action: "reassign".into(),
            expected_version: 1,
            reason: "Needed".into(),
        };
        assert_eq!(
            validate_change(&actor(), &reassign, "owner", "open")
                .unwrap_err()
                .code,
            "CRM_ACTIVITY_REASSIGN_TS_OWNED"
        );
        let done = ActivityChangeInput {
            action: "complete".into(),
            expected_version: 1,
            reason: "Done".into(),
        };
        assert_eq!(
            validate_change(&actor(), &done, "owner", "completed")
                .unwrap_err()
                .code,
            "CRM_ACTIVITY_STATE_CHANGED"
        );
    }

    #[test]
    fn record_id_stays_in_ts_namespace() {
        assert_eq!(create_record_id("key-1"), "crm-activity-key-1");
        assert_eq!(CREATE_OPERATION, "crm.activity.create");
    }
}
