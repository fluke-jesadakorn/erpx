//! Pure policy for native Field visit attachment sets.
use erpx_server_shared::public::{BusinessError, FailureKind};

#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FieldSetRequest {
    pub target_type: String,
    pub target_id: String,
    pub component_kind: String,
    pub component_ref: Option<String>,
    pub purpose_key: String,
    pub expected_target_version: i64,
}
#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FieldSetListRequest {
    pub target_type: String,
    pub target_id: String,
    pub purpose_key: Option<String>,
    pub component_kind: Option<String>,
    pub expected_target_version: Option<i64>,
}

pub fn classification(
    target: &str,
    component: &str,
    purpose: &str,
) -> Result<&'static str, BusinessError> {
    if !super::field_visit_link::valid_field_purpose(target, component, purpose) {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "ATTACHMENT_POLICY_UNKNOWN",
            "This file purpose is not registered for the selected record and context.",
        ));
    }
    Ok(if purpose == "site_photo" {
        "internal"
    } else {
        "confidential"
    })
}

pub fn component_ref(value: Option<&str>) -> Option<&str> {
    value.map(|s| s.trim_matches(|c: char| matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'))).filter(|s| !s.is_empty())
}

pub fn validate_component_ref(value: Option<&str>) -> Result<(), BusinessError> {
    if value.is_some_and(|s| s.encode_utf16().count() > 180) {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "ATTACHMENT_COMPONENT_REF",
            "The file context is too long.",
        ));
    }
    Ok(())
}

/// Existing Files identity, scoped to the authenticated Company and User.
pub fn field_set_id(company: &str, user: &str, wire_key: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = hex::encode(Sha256::digest(
        format!("{company}\0{user}\0{wire_key}").as_bytes(),
    ));
    format!("attachment-set-{}", &hash[..32])
}
/// Workflow's global key carries Files' already-scoped owner identity.
pub fn field_set_command_id(company: &str, user: &str, wire_key: &str) -> String {
    format!(
        "attachment.set.create:{}",
        field_set_id(company, user, wire_key)
    )
}
