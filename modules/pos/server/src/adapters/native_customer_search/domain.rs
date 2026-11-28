//! Pure domain for the native POS customer-search read.
//!
//! No I/O here. The axum owner owns the SQL, the projection and the response
//! shape; this module pins the bounded request, the composition-supplied
//! authority/package decisions and the exact retired wire field set.

use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::{Value, json};

pub const MAX_ID_LEN: usize = 200;
pub const MAX_QUERY_CHARS: usize = 100;

/// Exact field set of the retired `posCustomers` row. A row has exactly these
/// keys — no additions and no omissions. `address_json` is the raw JSONB text
/// because the retained runtime registers an identity parser for JSONB OIDs.
pub const ROW_KEYS: [&str; 6] = [
    "id",
    "name",
    "phone",
    "notification_email",
    "address_json",
    "version",
];

/// Authority facts composition supplies for the caller. `read` is the IAM
/// `pos.read` decision; `department` is the actor's department-mode flag.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomerActorDecision {
    pub read: bool,
    pub department: bool,
}

/// Package decision composition supplies for the caller. `enforced = false`
/// is the retained "disabled" mode where no purchase history gate applies.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomerPackageDecision {
    pub enforced: bool,
    pub history: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomerSearchQuery {
    pub company: String,
    pub q: String,
    pub actor: CustomerActorDecision,
    pub package: CustomerPackageDecision,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomerRow {
    pub id: String,
    pub name: String,
    pub phone: Option<String>,
    pub notification_email: Option<String>,
    pub address_json: String,
    pub version: i64,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid("POS_REQUEST_INVALID", "Use the bounded POS customer filters.")
}

pub fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}

fn bounded(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_ID_LEN
}

fn safe_id(value: &str) -> bool {
    bounded(value)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

/// All request invariants are owned here. No scope is derived from an
/// untrusted value before these checks pass.
pub fn validate(query: &CustomerSearchQuery) -> Result<(), BusinessError> {
    if !safe_id(&query.company) || query.q.chars().count() > MAX_QUERY_CHARS {
        return Err(invalid());
    }
    Ok(())
}

/// Fail closed on a malformed owner row: identity must be present and the
/// record version must be a positive number, as the schema guarantees.
pub fn valid_row(row: &CustomerRow) -> bool {
    bounded(&row.id)
        && !row.name.is_empty()
        && row.name.len() <= 200
        && row.version >= 1
}

impl CustomerRow {
    /// Build the exact retired payload. The six fields mirror the retained
    /// `SELECT id,name,phone,notification_email,address_json,version` row.
    pub fn value(self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "phone": self.phone,
            "notification_email": self.notification_email,
            "address_json": self.address_json,
            "version": self.version,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> CustomerRow {
        CustomerRow {
            id: "party1".into(),
            name: "Acme".into(),
            phone: Some("0812345678".into()),
            notification_email: None,
            address_json: "{}".into(),
            version: 3,
        }
    }

    fn query() -> CustomerSearchQuery {
        CustomerSearchQuery {
            company: "co".into(),
            q: "acme".into(),
            actor: CustomerActorDecision {
                read: true,
                department: false,
            },
            package: CustomerPackageDecision {
                enforced: false,
                history: false,
            },
        }
    }

    #[test]
    fn pins_exact_payload_field_set_and_types() {
        let value = row().value();
        let object = value.as_object().unwrap();
        assert_eq!(object.len(), ROW_KEYS.len());
        for key in ROW_KEYS {
            assert!(object.contains_key(key), "missing payload field: {key}");
        }
        assert_eq!(value["id"], "party1");
        assert_eq!(value["version"], 3);
        assert_eq!(value["notification_email"], Value::Null);
        assert_eq!(value["address_json"], "{}");
    }

    #[test]
    fn validates_bounded_scope() {
        assert!(validate(&query()).is_ok());
        let mut bad = query();
        bad.company.clear();
        assert!(validate(&bad).is_err());
        let mut bad = query();
        bad.q = "x".repeat(MAX_QUERY_CHARS + 1);
        assert!(validate(&bad).is_err());
        assert!(valid_row(&row()));
        let mut bad = row();
        bad.version = 0;
        assert!(!valid_row(&bad));
    }
}
