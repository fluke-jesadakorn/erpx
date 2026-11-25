//! Domain types and invariants for the native project-worker person-image read.
//!
//! The image is a deterministic JSON snapshot of the People owner tables for one
//! person in one Company. Rust owns validation and the response shape; the TS
//! adapter only transports the bounded string.

use erpx_server_shared::public::{BusinessError, RecordId};

/// The exact keys rendered by the byte-identical `jsonb_build_object` projection.
pub const IMAGE_KEYS: [&str; 7] = [
    "person",
    "engagements",
    "employees",
    "cases",
    "tasks",
    "compensation",
    "leave",
];

const MAX_IMAGE_BYTES: usize = 1_048_576;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersonImageRequest {
    pub company_id: String,
    pub person_id: String,
}

impl PersonImageRequest {
    pub fn new(company_id: String, person_id: String) -> Self {
        Self {
            company_id,
            person_id,
        }
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if !valid_id(&self.company_id) || !valid_id(&self.person_id) {
            return Err(invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersonImageFacts {
    pub image: String,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_LANE9_REQUEST_INVALID",
        "Use one bounded Company and person identity.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "PEOPLE_PAYROLL_LANE9_READ_UNAVAILABLE",
        "The native People Payroll person-image service is unavailable.",
    )
}

/// Identity validation matching the owner read router: a bounded RecordId with
/// no transport metacharacters. Company and person never travel as SQL text.
pub fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

/// Fail-closed projection guard: the native reply must be a bounded JSON object
/// with exactly the owner projection keys. Anything else is an unavailable
/// native service, never a partial image.
pub fn validate_image(image: &str) -> Result<(), BusinessError> {
    if image.is_empty() || image.len() > MAX_IMAGE_BYTES {
        return Err(unavailable());
    }
    let value: serde_json::Value =
        serde_json::from_str(image).map_err(|_| unavailable())?;
    let object = value.as_object().ok_or_else(unavailable)?;
    if object.len() != IMAGE_KEYS.len() || IMAGE_KEYS.iter().any(|key| !object.contains_key(*key)) {
        return Err(unavailable());
    }
    Ok(())
}
