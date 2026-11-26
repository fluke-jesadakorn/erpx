//! Pure domain for the native public Marketing form source read.
//!
//! The axum owner owns the SQL, the projection and the response shape. This
//! module contains no I/O: it validates the bounded form id, pins the exact
//! retired `PublicMarketingForm` field set and fails closed on a malformed row.
//! TypeScript is a transport only; no SQL or business rule remains there for
//! this operation.

use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};

pub const MAX_ID_LEN: usize = 200;

/// Exact field set returned by the retired `active-form-public` source read.
pub const SOURCE_KEYS: [&str; 6] = [
    "id",
    "company_id",
    "name",
    "version",
    "preview_json",
    "configuration_json",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormQuery {
    pub form_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormRow {
    pub id: String,
    pub company_id: String,
    pub name: String,
    pub version: i64,
    pub preview_json: String,
    pub configuration_json: String,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "MARKETING_PUBLIC_FORM_INVALID",
        "Use a bounded public form id.",
    )
}

pub fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "FORM_NOT_FOUND", "Form not found.")
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MARKETING_PUBLIC_FORM_UNAVAILABLE",
        "The native Marketing form service is unavailable.",
    )
}

fn bounded(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_ID_LEN
}

/// Public asset ids are plain owner identifiers, not typed `RecordId`s.
pub fn valid_id(value: &str) -> bool {
    bounded(value)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

pub fn validate(query: &FormQuery) -> Result<(), BusinessError> {
    if valid_id(&query.form_id) {
        Ok(())
    } else {
        Err(invalid())
    }
}

/// Fail closed on a malformed native row. The active/status predicates stay in
/// SQL; here the owner only checks the shape it is willing to expose publicly.
pub fn valid_row(row: &FormRow) -> bool {
    valid_id(&row.id)
        && valid_id(&row.company_id)
        && !row.name.is_empty()
        && row.version >= 0
        && !row.configuration_json.is_empty()
}

impl FormRow {
    /// Build the exact retired source payload. A row has exactly `SOURCE_KEYS`:
    /// no additions and no omissions. Presentation/consent rules remain with
    /// the caller that consumes this source.
    pub fn value(self) -> Value {
        json!({
            "id": self.id,
            "company_id": self.company_id,
            "name": self.name,
            "version": self.version,
            "preview_json": self.preview_json,
            "configuration_json": self.configuration_json,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> FormRow {
        FormRow {
            id: "form-1".into(),
            company_id: "co-1".into(),
            name: "Contact".into(),
            version: 3,
            preview_json: "{\"headline\":\"Hi\"}".into(),
            configuration_json: "{\"consentRequired\":true}".into(),
        }
    }

    #[test]
    fn pins_exact_source_field_set_and_types() {
        let value = row().value();
        let object = value.as_object().unwrap();
        assert_eq!(object.len(), SOURCE_KEYS.len());
        for key in SOURCE_KEYS {
            assert!(object.contains_key(key), "missing field: {key}");
        }
        assert_eq!(value["version"], 3);
        assert_eq!(value["name"], "Contact");
        assert_eq!(value["preview_json"], "{\"headline\":\"Hi\"}");
    }

    #[test]
    fn validates_bound_form_ids_and_rows() {
        let query = FormQuery {
            form_id: "form-1".into(),
        };
        assert!(validate(&query).is_ok());
        assert!(validate(&FormQuery {
            form_id: "bad id".into()
        })
        .is_err());
        assert!(validate(&FormQuery {
            form_id: String::new()
        })
        .is_err());
        assert!(valid_row(&row()));
        assert!(!valid_row(&FormRow {
            version: -1,
            ..row()
        }));
    }
}
