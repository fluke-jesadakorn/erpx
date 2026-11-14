//! Bounded native generic-record write facts.
//!
//! This first slice intentionally supports only the `support` module and its
//! registered sections. The composition still resolves the current actor,
//! Company scope, module access, command receipt, audit and outbox. The module
//! list is checked against the installed catalogue registry by a test oracle.
use erpx_server_shared::public::BusinessError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_SECTION_ID: usize = 128;
pub const MAX_TITLE: usize = 160;
pub const MAX_DESCRIPTION: usize = 2_000;
pub const MAX_FIELDS: usize = 64;
pub const MAX_FIELD_KEY: usize = 80;
pub const MAX_FIELD_LABEL: usize = 120;
pub const MAX_FIELD_TYPE: usize = 30;
pub const MAX_FIELD_OPTIONS: usize = 50;
pub const MAX_FIELD_OPTION: usize = 120;
pub const MAX_FIELD_STRING: usize = 4_000;
pub const MAX_SAFE: i64 = 9_007_199_254_740_991;
pub const SUPPORT_SECTIONS: &[&str] = &[
    "tickets",
    "sla-watch",
    "incidents",
    "feature-support-sessions",
    "feature-capabilities",
    "feature-attachments",
    "overview",
    "records",
    "activity",
];

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordFieldInput {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: String,
    pub value: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateRecordInput {
    pub section_id: String,
    pub title: String,
    pub description: String,
    pub fields: Vec<RecordFieldInput>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateRecordInput {
    pub expected_version: i64,
    pub fields: std::collections::BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArchiveRecordInput {
    pub expected_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReferenceRecordInput {
    pub target_module: String,
    pub target_id: String,
    pub kind: String,
}

pub const REFERENCE_KINDS: [&str; 5] = [
    "dependsOn",
    "blocks",
    "billedBy",
    "fulfills",
    "authoredWith",
];
pub const MAX_REFERENCES: i64 = 50;
pub const MAX_NOTE: usize = 1_000;
pub const TRANSITION_ACTIONS: [&str; 5] = ["submit", "approve", "reject", "complete", "reopen"];

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransitionRecordInput {
    pub expected_version: i64,
    pub action: String,
    #[serde(default)]
    pub note: String,
}

pub fn next_status(current: &str, action: &str) -> Option<&'static str> {
    match (current, action) {
        ("draft", "submit") => Some("in_review"),
        ("in_review", "approve") => Some("approved"),
        ("in_review", "reject") => Some("rejected"),
        ("approved", "complete") => Some("completed"),
        ("approved" | "rejected" | "completed", "reopen") => Some("draft"),
        _ => None,
    }
}

pub fn review_forbidden() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Forbidden,
        "REVIEW_FORBIDDEN",
        "An owner or manager assignment is required to review this record.",
    )
}

pub fn self_review_forbidden() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Conflict,
        "SELF_REVIEW_FORBIDDEN",
        "A different person must review this record.",
    )
}

pub fn invalid_transition() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Conflict,
        "INVALID_TRANSITION",
        "The transition is unavailable while this record is in its current status.",
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedRecord {
    pub id: String,
    pub section_id: String,
    pub title: String,
    pub description: String,
    pub fields_json: String,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn text_units(value: &str) -> usize {
    value.encode_utf16().count()
}

fn valid_field_value(value: &Value) -> bool {
    match value {
        Value::String(value) => text_units(value) <= MAX_FIELD_STRING,
        Value::Number(value) => value.as_f64().is_some_and(f64::is_finite),
        Value::Bool(_) => true,
        _ => false,
    }
}

pub fn validate_create(
    module_id: &str,
    key: &str,
    input: &CreateRecordInput,
) -> Result<ValidatedRecord, BusinessError> {
    if module_id != "support" {
        return Err(invalid(
            "RECORD_MODULE_UNSUPPORTED",
            "This record module is not available in the native workbench yet.",
        ));
    }
    if !SUPPORT_SECTIONS.contains(&input.section_id.as_str()) {
        return Err(invalid("INVALID_SECTION", "Choose a valid module section."));
    }
    let title = input.title.trim();
    if title.is_empty() || text_units(title) > MAX_TITLE {
        return Err(invalid(
            "RECORD_TITLE_INVALID",
            "Use a title between 1 and 160 characters.",
        ));
    }
    let description = input.description.trim();
    if text_units(description) > MAX_DESCRIPTION {
        return Err(invalid(
            "RECORD_DESCRIPTION_INVALID",
            "Use a description of at most 2000 characters.",
        ));
    }
    if input.fields.is_empty() || input.fields.len() > MAX_FIELDS {
        return Err(invalid(
            "RECORD_FIELDS_INVALID",
            "Use between 1 and 64 fields.",
        ));
    }
    let mut seen = std::collections::HashSet::with_capacity(input.fields.len());
    for field in &input.fields {
        if field.key.is_empty()
            || text_units(&field.key) > MAX_FIELD_KEY
            || !seen.insert(field.key.as_str())
            || field.label.is_empty()
            || text_units(&field.label) > MAX_FIELD_LABEL
            || field.field_type.is_empty()
            || text_units(&field.field_type) > MAX_FIELD_TYPE
            || !valid_field_value(&field.value)
            || field.options.as_ref().is_some_and(|options| {
                options.len() > MAX_FIELD_OPTIONS
                    || options
                        .iter()
                        .any(|option| text_units(option) > MAX_FIELD_OPTION)
            })
        {
            return Err(invalid(
                "RECORD_FIELDS_INVALID",
                "Use unique bounded fields with supported values.",
            ));
        }
    }
    let fields_json = serde_json::to_string(&input.fields)
        .map_err(|_| invalid("RECORD_FIELDS_INVALID", "Use supported field values."))?;
    Ok(ValidatedRecord {
        id: format!("rec-{module_id}-{key}"),
        section_id: input.section_id.clone(),
        title: title.to_owned(),
        description: description.to_owned(),
        fields_json,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedRecordUpdate {
    pub version: i64,
    pub fields_json: String,
}

pub fn version_conflict() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Conflict,
        "VERSION_CONFLICT",
        "This record changed. Refresh and review it again.",
    )
}

pub fn validate_update(
    module_id: &str,
    input: &UpdateRecordInput,
    current_fields: &[RecordFieldInput],
    current_version: i64,
) -> Result<ValidatedRecordUpdate, BusinessError> {
    if module_id != "support" {
        return Err(invalid(
            "RECORD_MODULE_UNSUPPORTED",
            "This record module is not available in the native workbench yet.",
        ));
    }
    if !(1..=MAX_SAFE).contains(&input.expected_version) {
        return Err(invalid(
            "RECORD_VERSION_INVALID",
            "Reload the record and try again.",
        ));
    }
    if input.expected_version != current_version {
        return Err(version_conflict());
    }
    if input.fields.len() > MAX_FIELDS {
        return Err(invalid(
            "RECORD_FIELDS_INVALID",
            "Use between 1 and 64 fields.",
        ));
    }
    let mut seen = std::collections::HashSet::with_capacity(input.fields.len());
    for (key, value) in &input.fields {
        if key.is_empty() || text_units(key) > MAX_FIELD_KEY || !valid_field_value(value) {
            return Err(invalid(
                "RECORD_FIELDS_INVALID",
                "Use unique bounded fields with supported values.",
            ));
        }
        seen.insert(key.as_str());
    }
    let mut fields = current_fields.to_vec();
    for field in &mut fields {
        if seen.remove(field.key.as_str()) {
            field.value = input.fields[&field.key].clone();
        }
    }
    if !seen.is_empty() {
        return Err(invalid(
            "UNKNOWN_FIELD",
            "Use only fields that already exist on this record.",
        ));
    }
    let fields_json = serde_json::to_string(&fields)
        .map_err(|_| invalid("RECORD_FIELDS_INVALID", "Use supported field values."))?;
    Ok(ValidatedRecordUpdate {
        version: current_version + 1,
        fields_json,
    })
}

pub fn validate_archive(
    module_id: &str,
    input: &ArchiveRecordInput,
    current_version: i64,
) -> Result<(), BusinessError> {
    if module_id != "support" {
        return Err(invalid(
            "RECORD_MODULE_UNSUPPORTED",
            "This record module is not available in the native workbench yet.",
        ));
    }
    if !(1..=MAX_SAFE).contains(&input.expected_version) {
        return Err(invalid(
            "RECORD_VERSION_INVALID",
            "Reload the record and try again.",
        ));
    }
    if input.expected_version != current_version {
        return Err(version_conflict());
    }
    Ok(())
}

pub fn self_reference() -> BusinessError {
    invalid("SELF_REFERENCE", "A record cannot reference itself.")
}

pub fn reference_limit() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Conflict,
        "REFERENCE_LIMIT",
        "A record can contain up to 50 references.",
    )
}

pub fn validate_reference(
    module_id: &str,
    record_id: &str,
    input: &ReferenceRecordInput,
) -> Result<(), BusinessError> {
    if module_id != "support" || input.target_module != "support" {
        return Err(invalid(
            "RECORD_MODULE_UNSUPPORTED",
            "This record module is not available in the native workbench yet.",
        ));
    }
    if record_id.is_empty()
        || record_id.len() > 180
        || record_id.contains('\0')
        || input.target_id.is_empty()
        || input.target_id.len() > 180
        || input.target_id.contains('\0')
        || !REFERENCE_KINDS.contains(&input.kind.as_str())
    {
        return Err(invalid(
            "REFERENCE_INPUT_INVALID",
            "Choose a valid record and reference kind.",
        ));
    }
    if module_id == input.target_module && record_id == input.target_id {
        return Err(self_reference());
    }
    Ok(())
}

pub fn validate_transition(
    module_id: &str,
    input: &TransitionRecordInput,
    current_status: &str,
    current_version: i64,
    can_review: bool,
    created_by: &str,
    actor_id: &str,
) -> Result<&'static str, BusinessError> {
    if module_id != "support" {
        return Err(invalid(
            "RECORD_MODULE_UNSUPPORTED",
            "This record module is not available in the native workbench yet.",
        ));
    }
    if !(1..=MAX_SAFE).contains(&input.expected_version)
        || !TRANSITION_ACTIONS.contains(&input.action.as_str())
        || text_units(&input.note) > MAX_NOTE
    {
        return Err(invalid(
            "TRANSITION_INPUT_INVALID",
            "Choose a valid transition and note.",
        ));
    }
    if input.expected_version != current_version {
        return Err(version_conflict());
    }
    let status = next_status(current_status, &input.action).ok_or_else(invalid_transition)?;
    if matches!(input.action.as_str(), "approve" | "reject") {
        if !can_review {
            return Err(review_forbidden());
        }
        if created_by == actor_id {
            return Err(self_review_forbidden());
        }
    }
    Ok(status)
}
