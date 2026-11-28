//! Pure domain facts for the native directory "connection by id" read.
//!
//! This is the byte-payload successor of `connectionById` in
//! `adapters/directory-lookup.ts`, which prepared
//! `SELECT * FROM iam_connections WHERE company_id=? AND id=?`. Every field
//! below is projected from a persisted row; no SQL, no I/O and no caller SQL
//! live in this module.

use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Wire discriminator for this bounded read kind.
pub const KIND_CONNECTION_BY_ID: &str = "connection_by_id";

/// One persisted Directory connection exactly as the retired TypeScript read
/// returned it. The retired query was `SELECT *`, so the runtime row carried
/// all thirteen columns even though its `ConnectionRow` type annotated ten.
/// The `serde` names are the original Control column names, so the JSON object
/// is byte-identical to the retired `first<ConnectionRow>()` result.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ConnectionRow {
    pub id: String,
    pub company_id: String,
    pub name: String,
    pub provider: String,
    pub config: Value,
    pub status: String,
    pub version: i64,
    pub tested_version: Option<i64>,
    pub last_sync_at: Option<i64>,
    pub health: String,
    pub last_error: Option<String>,
    pub created_by: String,
    pub created_at: i64,
}

/// Untyped sqlx projection. The owner validates every value before it becomes
/// a `ConnectionRow`; a malformed row fails closed instead of widening the
/// reply.
#[derive(Clone, Debug)]
pub struct ConnectionFields {
    pub id: String,
    pub company_id: String,
    pub name: String,
    pub provider: String,
    pub config: Value,
    pub status: String,
    pub version: i64,
    pub tested_version: Option<i64>,
    pub last_sync_at: Option<i64>,
    pub health: String,
    pub last_error: Option<String>,
    pub created_by: String,
    pub created_at: i64,
}

impl ConnectionFields {
    pub fn into_row(self) -> Result<ConnectionRow, BusinessError> {
        if !valid_id(&self.id)
            || !valid_token(&self.company_id)
            || !valid_text(&self.name, 200)
            || !valid_provider(&self.provider)
            || !self.config.is_object()
            || !valid_status(&self.status)
            || self.version < 0
            || self
                .tested_version
                .is_some_and(|value| !(0..=i32::MAX as i64).contains(&value))
            || self.last_sync_at.is_some_and(|value| value < 0)
            || !valid_text(&self.health, 80)
            || self
                .last_error
                .as_deref()
                .is_some_and(|value| value.len() > 2048 || value.chars().any(char::is_control))
            || !valid_id(&self.created_by)
            || self.created_at < 0
        {
            return Err(invalid_result());
        }
        Ok(ConnectionRow {
            id: self.id,
            company_id: self.company_id,
            name: self.name,
            provider: self.provider,
            config: self.config,
            status: self.status,
            version: self.version,
            tested_version: self.tested_version,
            last_sync_at: self.last_sync_at,
            health: self.health,
            last_error: self.last_error,
            created_by: self.created_by,
            created_at: self.created_at,
        })
    }
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "IDENTITY_CONNECTION_READ_REQUEST_INVALID",
        "Use one bounded directory connection read.",
    )
}

pub fn invalid_result() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "IDENTITY_CONNECTION_READ_RESPONSE_INVALID",
        "The owner connection projection was invalid.",
    )
}

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CONNECTION_NOT_FOUND",
        "Directory connection not found.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "IDENTITY_CONNECTION_READ_UNAVAILABLE",
        "The native directory connection read is unavailable.",
    )
}

/// Connection ids are bounded, printable RecordIds with no controls.
pub fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

/// Company scope tokens are bounded and printable; `platform` is a valid
/// Company for Platform connections, so letters and `_` are accepted too.
pub fn valid_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'*')
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && !value.chars().any(char::is_control)
}

fn valid_provider(value: &str) -> bool {
    matches!(value, "active_directory" | "entra_id" | "univention")
}

fn valid_status(value: &str) -> bool {
    matches!(value, "draft" | "tested" | "active" | "disabled")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields() -> ConnectionFields {
        ConnectionFields {
            id: "connection-1".into(),
            company_id: "acme-company".into(),
            name: "Acme AD".into(),
            provider: "active_directory".into(),
            config: Value::Object(serde_json::Map::new()),
            status: "active".into(),
            version: 2,
            tested_version: Some(1),
            last_sync_at: Some(1_700_000_000_000),
            health: "healthy".into(),
            last_error: None,
            created_by: "actor-1".into(),
            created_at: 1_699_000_000_000,
        }
    }

    #[test]
    fn row_invariants_match_the_retired_full_row() {
        let row = fields().into_row().expect("valid row");
        assert_eq!(row.id, "connection-1");
        assert_eq!(row.company_id, "acme-company");
        assert_eq!(row.version, 2);
        assert_eq!(row.tested_version, Some(1));
        assert!(row.config.is_object());
    }

    #[test]
    fn malformed_rows_fail_closed() {
        let mut bad = fields();
        bad.provider = "arbitrary".into();
        assert_eq!(bad.clone().into_row(), Err(invalid_result()));
        bad = fields();
        bad.config = Value::from("not-an-object");
        assert_eq!(bad.clone().into_row(), Err(invalid_result()));
        bad = fields();
        bad.id = "bad id".into();
        assert_eq!(bad.into_row(), Err(invalid_result()));
    }

    #[test]
    fn serialized_row_pins_the_retired_column_names() {
        let value = serde_json::to_value(fields().into_row().expect("row")).expect("json");
        assert_eq!(value["company_id"], "acme-company");
        assert_eq!(value["tested_version"], 1);
        assert_eq!(value["last_error"], Value::Null);
        assert_eq!(
            value.as_object().expect("object").len(),
            13,
            "the retired SELECT * row carried all thirteen columns"
        );
    }

    #[test]
    fn inputs_are_bounded() {
        assert!(valid_id("connection_1"));
        assert!(!valid_id(""));
        assert!(!valid_id("a b"));
        assert!(!valid_id(&"x".repeat(201)));
        assert!(valid_token("platform"));
        assert!(valid_token("acme-company"));
        assert!(!valid_token("a b"));
    }
}
