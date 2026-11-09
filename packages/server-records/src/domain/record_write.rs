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
