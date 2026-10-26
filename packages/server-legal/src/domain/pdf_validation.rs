use serde::{Deserialize, Serialize};

/// Evidence can be proven, disproven, or insufficient. Unknown evidence never passes.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ValidationState {
    Valid,
    Invalid,
    Indeterminate,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureValidation {
    pub field: String,
    pub status: ValidationState,
    pub integrity: ValidationState,
    pub certificate_chain: ValidationState,
    pub certificate_validity: ValidationState,
    pub revocation: ValidationState,
    pub timestamp: ValidationState,
    pub subject: String,
    pub issuer: String,
    pub serial: String,
    pub certificate_sha256: String,
    pub not_before: String,
    pub not_after: String,
    pub timestamp_time: Option<String>,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfValidationReport {
    pub status: ValidationState,
    pub signatures: Vec<SignatureValidation>,
    pub reasons: Vec<String>,
    pub validated_at: String,
    pub trust_store_digest: String,
    pub mode: String,
    pub engine: String,
}

impl SignatureValidation {
    pub(crate) fn unresolved(field: String, reason: &str) -> Self {
        Self {
            field,
            status: ValidationState::Indeterminate,
            integrity: ValidationState::Indeterminate,
            certificate_chain: ValidationState::Indeterminate,
            certificate_validity: ValidationState::Indeterminate,
            revocation: ValidationState::Indeterminate,
            timestamp: ValidationState::Indeterminate,
            subject: String::new(),
            issuer: String::new(),
            serial: String::new(),
            certificate_sha256: String::new(),
            not_before: String::new(),
            not_after: String::new(),
            timestamp_time: None,
            reasons: vec![reason.to_owned()],
        }
    }
}

pub(crate) fn aggregate(states: impl IntoIterator<Item = ValidationState>) -> ValidationState {
    let mut found = false;
    let mut unknown = false;
    for state in states {
        found = true;
        match state {
            ValidationState::Invalid => return ValidationState::Invalid,
            ValidationState::Indeterminate => unknown = true,
            ValidationState::Valid => {}
        }
    }
    if found && !unknown {
        ValidationState::Valid
    } else {
        ValidationState::Indeterminate
    }
}
