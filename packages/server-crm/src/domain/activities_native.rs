//! Native CRM lead-activity create / complete / cancel validation.
//!
//! Owner-local rows only (`crm_lead_activities` + `crm_lead_activity_events`).
//! `reassign` has a separate schema/state intent path for the unmounted owner
//! adapter; composition must still supply sales-manager authority, the
//! `sales_recipient_eligibility` cross-owner join, Workflow replay and the
//! Collaboration notification. This module validates bounded input and state
//! transitions; it reads no table, performs no purchase check and emits no
//! notification. Purchase enforcement (`sales.crm` in `withCRMPurchasedCommand`
//! / `purchases.ts`) and the Workflow ledger remain the TS runtime path.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

const SUBJECT_LIMIT: usize = 200;
const DETAILS_LIMIT: usize = 2_000;

/// Raw wire shape matching `crmActivityCreateSchema` / `crmActivityChangeSchema`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActivityCreateInput {
    #[serde(rename = "leadId")]
    pub lead_id: String,
    #[serde(rename = "expectedVersion", deserialize_with = "safe_integer")]
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
pub struct ActivityChangeInput {
    pub action: String,
    #[serde(rename = "expectedVersion", deserialize_with = "safe_integer")]
    pub expected_version: i64,
    pub reason: String,
    #[serde(rename = "ownerId", default)]
    pub owner_id: Option<serde_json::Value>,
    #[serde(rename = "dueDate", default)]
    pub due_date: Option<serde_json::Value>,
}

fn safe_integer<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
    use serde::de::Error;
    let value = serde_json::Value::deserialize(deserializer)?;
    let number = value.as_f64().filter(|n| n.is_finite() && n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_991.0)
        .ok_or_else(|| D::Error::custom("expected a safe integer"))?;
    Ok(number as i64)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityActor {
    pub user_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
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
        // Zod 4 string length counts Unicode code points for valid UTF-8.
        &crate::domain::accounts::js_trim(value).chars().count(),
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
    if !(1..=12).contains(&month) {
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
    if !matches!(
        input.activity_type.as_str(),
        "call" | "email" | "meeting" | "task"
    ) {
        return Err(invalid_input(
            "VALIDATION_FAILED",
            "Choose a call, email, meeting or task activity.",
        ));
    }
    if !(1..=9_007_199_254_740_991).contains(&input.expected_version)
        || !valid_identifier(&input.lead_id)
        || !valid_identifier(&input.owner_id)
        || !valid_text(&input.subject, SUBJECT_LIMIT)
        || !valid_text(&input.details, DETAILS_LIMIT)
        || !valid_date(&input.due_date)
    {
        return Err(invalid_input(
            "VALIDATION_FAILED",
            "Use a bounded CRM activity with a valid due date.",
        ));
    }
    if !actor.can_sales {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "PERMISSION_REQUIRED",
            "Your selected role does not grant this permission.",
        ));
    }
    if !actor.has_read {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "PERMISSION_REQUIRED",
            "Your selected role does not grant this permission.",
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

fn validate_change_schema(input: &ActivityChangeInput) -> Result<(), BusinessError> {
    if !matches!(input.action.as_str(), "complete" | "cancel" | "reassign") {
        return Err(invalid_input(
            "VALIDATION_FAILED",
            "Choose a complete or cancel activity action.",
        ));
    }
    if !(1..=9_007_199_254_740_991).contains(&input.expected_version) || !valid_text(&input.reason, DETAILS_LIMIT)
        || (input.action == "reassign" && (!input.owner_id.as_ref().and_then(serde_json::Value::as_str).is_some_and(valid_identifier) || !input.due_date.as_ref().and_then(serde_json::Value::as_str).is_some_and(valid_date))) {
        return Err(invalid_input(
            "VALIDATION_FAILED",
            "Use a bounded CRM activity reason.",
        ));
    }
    Ok(())
}

pub(crate) fn validate_change_schema_and_permissions(actor: &ActivityActor, input: &ActivityChangeInput) -> Result<(), BusinessError> {
    validate_change_schema(input)?;
    if !actor.can_sales || !actor.has_read {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "PERMISSION_REQUIRED",
            "Your selected role does not grant this permission.",
        ));
    }
    Ok(())
}

pub(crate) fn validate_change_access(actor: &ActivityActor, input: &ActivityChangeInput) -> Result<(), BusinessError> {
    validate_change_schema_and_permissions(actor, input)?;
    if input.action == "reassign" {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "CRM_ACTIVITY_REASSIGN_TS_OWNED",
            "Activity reassign remains in the TypeScript owner.",
        ));
    }
    Ok(())
}

/// Source-compatible reassign string units before UTF-8 persistence. This
/// represents schema normalization only; it is not a source or actor proof.
/// Transport parsing and ordered Workflow JSON remain the coordinator's job.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityReassignUtf16Input {
    expected_version: i64,
    owner_id: Vec<u16>,
    due_date: Vec<u16>,
    reason: Vec<u16>,
}
impl ActivityReassignUtf16Input {
    pub fn expected_version(&self)->i64 { self.expected_version }
    pub fn owner_id(&self)->&[u16] { &self.owner_id }
    pub fn due_date(&self)->&[u16] { &self.due_date }
    pub fn reason(&self)->&[u16] { &self.reason }
}
/// Validate the reassign schema on JavaScript UTF-16 units without replacing
/// isolated surrogates. The caller has already checked the JSON value types and
/// JS safe-integer conversion; unknown object properties are not passed here.
pub fn normalize_activity_reassign_utf16(action:&[u16],expected_version:i64,owner_id:&[u16],due_date:&[u16],reason:&[u16])->Result<ActivityReassignUtf16Input,BusinessError>{
    let whitespace=|v:&u16|matches!(*v,0x0009..=0x000d|0x0020|0x00a0|0x1680|0x2000..=0x200a|0x2028|0x2029|0x202f|0x205f|0x3000|0xfeff);
    let start=reason.iter().position(|v|!whitespace(v)).unwrap_or(reason.len());
    let end=reason.iter().rposition(|v|!whitespace(v)).map_or(start,|i|i+1);
    let trimmed=&reason[start..end];
    // Zod string length counts Unicode code points. A valid surrogate pair
    // counts as one; an isolated surrogate also counts as one.
    let length=char::decode_utf16(trimmed.iter().copied()).count();
    let date=String::from_utf16(due_date).ok();
    if !action.iter().copied().eq("reassign".encode_utf16()) || !(1..=9_007_199_254_740_991).contains(&expected_version)
        || owner_id.is_empty() || !date.as_deref().is_some_and(valid_date) || !(3..=DETAILS_LIMIT).contains(&length) {
        return Err(invalid_input("VALIDATION_FAILED","Use a bounded CRM activity reason."));
    }
    Ok(ActivityReassignUtf16Input{expected_version,owner_id:owner_id.to_vec(),due_date:due_date.to_vec(),reason:trimmed.to_vec()})
}

/// Parsed reassign command in the exact TypeScript schema property order.
/// It is request identity only: no selected actor, permission or source proof.
/// Input has already passed the caller's JSON transport; this UTF-8 DTO alone
/// does not attest preservation of lone UTF-16 surrogates at that boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityReassignWireInput {
    action: &'static str,
    expected_version: i64,
    owner_id: String,
    due_date: String,
    reason: String,
}
/// Validate schema and construct the value before permission/replay checks.
/// Workflow can serialize this DTO directly without an unordered JSON map.
pub fn normalized_activity_reassign_input(input:&ActivityChangeInput)->Result<ActivityReassignWireInput,BusinessError>{
    validate_change_schema(input)?;
    if input.action!="reassign" {return Err(invalid_input("VALIDATION_FAILED","Choose a reassign activity action."));}
    Ok(ActivityReassignWireInput {action:"reassign", expected_version:input.expected_version,
        owner_id:input.owner_id.as_ref().and_then(serde_json::Value::as_str).expect("validated owner").to_owned(),
        due_date:input.due_date.as_ref().and_then(serde_json::Value::as_str).expect("validated date").to_owned(),
        reason:crate::domain::accounts::js_trim(&input.reason).to_owned()})
}

/// A schema-checked reassign intent, not an eligibility proof or an applied command.
/// Composition must retain the actual lead-owner recipient witness, purchase and
/// IAM authority, Workflow replay/commit, and Collaboration notification in the
/// same transaction before exposing this action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityReassignIntent {
    pub expected_version: i64,
    pub owner_id: String,
    pub due_date: String,
    pub reason: String,
}

/// Validate the owner-level state gate only. The caller supplies facts from its
/// selected-scope retained activity read; this function performs no write.
pub fn prepare_activity_reassign(
    actor: &ActivityActor,
    input: &ActivityChangeInput,
    record_status: &str,
) -> Result<ActivityReassignIntent, BusinessError> {
    validate_change_schema_and_permissions(actor, input)?;
    if input.action != "reassign" {
        return Err(invalid_input("VALIDATION_FAILED", "Choose a reassign activity action."));
    }
    if !actor.can_manage {
        return Err(BusinessError::new(FailureKind::Forbidden, "CRM_ACTIVITY_OWNER_REQUIRED", "The assigned salesperson completes an activity; a sales manager manages responsibility."));
    }
    if record_status != "open" {
        return Err(BusinessError::new(FailureKind::Conflict, "CRM_ACTIVITY_STATE_CHANGED", "This activity is already closed."));
    }
    // The schema check above proves both are present strings with valid values.
    let owner_id = input.owner_id.as_ref().and_then(serde_json::Value::as_str).expect("validated owner").to_owned();
    let due_date = input.due_date.as_ref().and_then(serde_json::Value::as_str).expect("validated due date").to_owned();
    Ok(ActivityReassignIntent { expected_version: input.expected_version, owner_id, due_date, reason: crate::domain::accounts::js_trim(&input.reason).to_owned() })
}

pub fn validate_change(
    actor: &ActivityActor,
    input: &ActivityChangeInput,
    record_owner_id: &str,
    record_status: &str,
) -> Result<ActivityChangePlan, BusinessError> {
    validate_change_access(actor, input)?;
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
    if record_status != "open" {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "CRM_ACTIVITY_STATE_CHANGED",
            "This activity is already closed.",
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
            company_id: "company".into(), branch_id: None, department_id: None,
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
            "VALIDATION_FAILED"
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
            "PERMISSION_REQUIRED"
        );
        let mut a = actor();
        a.has_read = false;
        assert_eq!(
            validate_create(&a, &create()).unwrap_err().code,
            "PERMISSION_REQUIRED"
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
         owner_id: None, due_date: None, };
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
         owner_id: None, due_date: None, };
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
         owner_id: Some("owner".into()), due_date: Some("2026-11-20".into()), };
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
         owner_id: None, due_date: None, };
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
