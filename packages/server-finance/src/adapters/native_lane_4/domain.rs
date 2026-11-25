//! Pure domain types and invariants for the native Finance reference-option
//! read family. This module owns no SQL and performs no I/O.
//!
//! The family is the three owner-local finance-table reads that feed the PMS
//! accounting/settings dropdowns (`readFinanceAccounts`, `readFinanceCharts`,
//! `readFinanceParties` in `adapters/pms-command-facts.ts`). Every response is
//! a bounded array of string-keyed rows; no caller-supplied predicate or SQL
//! reaches the owner.

use erpx_server_runtime::public::http_support::{HeaderMap, HeaderName, serde_json};
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use serde_json::{Map, Value};

/// The three registered reference-option reads. The wire accepts no other
/// discriminator; this mirrors the owner allow-list in `postgres.rs`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReferenceKind {
    Accounts,
    Charts,
    Parties,
}

impl ReferenceKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "accounts" => Some(Self::Accounts),
            "charts" => Some(Self::Charts),
            "parties" => Some(Self::Parties),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accounts => "accounts",
            Self::Charts => "charts",
            Self::Parties => "parties",
        }
    }
}

/// A projected row. All columns are strings because the retired TS reads
/// returned untyped `SqlDatabase` rows whose columns were consumed as text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceRow {
    pub id: String,
    pub name: String,
    pub code: Option<String>,
    pub chart_id: Option<String>,
    pub category: Option<String>,
}

pub(crate) fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_FINANCE_LANE4_UNAVAILABLE",
        "The native Finance reference-option read is unavailable.",
    )
}

pub(crate) fn invalid() -> BusinessError {
    BusinessError::invalid(
        "NATIVE_FINANCE_LANE4_REQUEST_INVALID",
        "Use one bounded Finance reference-option read.",
    )
}

pub(crate) fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

pub(crate) fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "NATIVE_FINANCE_LANE4_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

pub(crate) fn projection_failed() -> BusinessError {
    unavailable()
}

/// Project one owner row into the exact wire shape for `kind`. Missing columns
/// fail closed rather than emitting a partial or null-padded row.
pub fn project(kind: ReferenceKind, row: &ReferenceRow) -> Result<Value, BusinessError> {
    let mut object = Map::new();
    object.insert("id".to_owned(), Value::String(row.id.clone()));
    object.insert("name".to_owned(), Value::String(row.name.clone()));
    if kind == ReferenceKind::Accounts {
        let code = row.code.as_ref().ok_or_else(projection_failed)?;
        let chart_id = row.chart_id.as_ref().ok_or_else(projection_failed)?;
        let category = row.category.as_ref().ok_or_else(projection_failed)?;
        object.insert("code".to_owned(), Value::String(code.clone()));
        object.insert("chart_id".to_owned(), Value::String(chart_id.clone()));
        object.insert("category".to_owned(), Value::String(category.clone()));
    }
    Ok(Value::Object(object))
}

pub(crate) fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
}

/// Company identity is transported in `x-erpx-context-company` and used only as
/// a bind value in the owner query; the router never mints a Company scope.
pub(crate) fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' || byte == b'.' || byte == b':')
}

pub(crate) fn unique_header(
    headers: &HeaderMap,
    name: HeaderName,
) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_allow_list_is_exact() {
        assert_eq!(ReferenceKind::parse("accounts"), Some(ReferenceKind::Accounts));
        assert_eq!(ReferenceKind::parse("charts"), Some(ReferenceKind::Charts));
        assert_eq!(ReferenceKind::parse("parties"), Some(ReferenceKind::Parties));
        assert_eq!(ReferenceKind::parse("Accounts"), None);
        assert_eq!(ReferenceKind::parse("SELECT 1"), None);
        assert_eq!(ReferenceKind::parse(""), None);
        for kind in [
            ReferenceKind::Accounts,
            ReferenceKind::Charts,
            ReferenceKind::Parties,
        ] {
            assert_eq!(ReferenceKind::parse(kind.as_str()), Some(kind));
            assert!(!kind.as_str().contains(' '));
        }
    }

    #[test]
    fn project_pins_the_wire_shape_per_kind() {
        let row = ReferenceRow {
            id: "acct-1".to_owned(),
            name: "Cash".to_owned(),
            code: Some("100".to_owned()),
            chart_id: Some("chart-1".to_owned()),
            category: Some("asset".to_owned()),
        };
        assert_eq!(
            project(ReferenceKind::Accounts, &row).unwrap(),
            serde_json::json!({
                "id": "acct-1", "name": "Cash", "code": "100",
                "chart_id": "chart-1", "category": "asset"
            })
        );
        assert_eq!(
            project(ReferenceKind::Charts, &row).unwrap(),
            serde_json::json!({"id": "acct-1", "name": "Cash"})
        );
        assert_eq!(
            project(ReferenceKind::Parties, &row).unwrap(),
            serde_json::json!({"id": "acct-1", "name": "Cash"})
        );
    }

    #[test]
    fn missing_account_columns_fail_closed() {
        let row = ReferenceRow {
            id: "acct-1".to_owned(),
            name: "Cash".to_owned(),
            code: None,
            chart_id: Some("chart-1".to_owned()),
            category: Some("asset".to_owned()),
        };
        assert_eq!(
            project(ReferenceKind::Accounts, &row).unwrap_err(),
            unavailable()
        );
        assert!(project(ReferenceKind::Charts, &row).is_ok());
    }

    #[test]
    fn identifiers_and_slugs_are_bounded() {
        assert!(valid_slug("acme"));
        assert!(valid_slug("north-star_1"));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug("../acme"));
        assert!(valid_id("company-01"));
        assert!(valid_id("h001"));
        assert!(!valid_id("bad/id"));
        assert!(!valid_id(""));
        assert!(!valid_id(&"x".repeat(201)));
    }
}
