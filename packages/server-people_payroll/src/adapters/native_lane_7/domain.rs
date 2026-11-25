//! Pure response shape and validation for the lane-7 legacy people clock.
//!
//! The owner owns the projection: one Company-scoped deadline derived from its
//! own access-cutoff, lifecycle-case and applied-package sources. No SQL and no
//! transport live in this module.

use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use serde::{Deserialize, Serialize};

/// Wire discriminator pinned by the native client and the contract test.
pub const READ_KIND: &str = "people_payroll_lane_7_legacy_people_transition";
/// Owner-local route mounted by the lead alongside the other holding routers.
pub const ROUTE: &str = "/api/v1/holdings/{holding_slug}/people-payroll/legacy-people-clock";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LegacyPeopleTransition {
    pub deadline: Option<i64>,
}

/// Wire discriminator and owner route for the second lane-7 read family: the
/// Company-scoped People payment projection consumed by Finance's workspace.
pub const PAYMENTS_READ_KIND: &str = "people_payroll_lane_7_workspace_people_payments";
pub const PAYMENTS_ROUTE: &str =
    "/api/v1/holdings/{holding_slug}/people-payroll/finance-workspace-payments";

/// Wire discriminator and route for the third lane-7 family: the Company-scoped
/// Organization authority person lookups (actor and appointee).
pub const AUTHORITY_READ_KIND: &str = "people_payroll_lane_7_organization_authority_person";
pub const AUTHORITY_ROUTE: &str =
    "/api/v1/holdings/{holding_slug}/people-payroll/organization-authority-person";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityKind {
    Actor,
    Appointee,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct AuthorityPersonQuery {
    pub kind: AuthorityKind,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub person_id: String,
    #[serde(default)]
    pub position_id: String,
    #[serde(default)]
    pub appointment_kind: String,
    #[serde(default)]
    pub scope_id: String,
    #[serde(default)]
    pub self_ceo: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct AuthorityPerson {
    pub id: Option<String>,
}

pub fn validate_authority_query(query: &AuthorityPersonQuery) -> Result<(), BusinessError> {
    validate_authority_fields(
        query.kind,
        &query.user_id,
        &query.person_id,
        &query.position_id,
        &query.appointment_kind,
        &query.scope_id,
    )
}

/// The owned-query form is validated again at the application boundary, so no
/// reader can be reached with an unbounded discriminator or identifier.
pub fn validate_authority_fields(
    kind: AuthorityKind,
    user_id: &str,
    person_id: &str,
    position_id: &str,
    appointment_kind: &str,
    scope_id: &str,
) -> Result<(), BusinessError> {
    let invalid = || {
        BusinessError::invalid(
            "PEOPLE_PAYROLL_LANE7_AUTHORITY_QUERY_INVALID",
            "Use one bounded authority person lookup.",
        )
    };
    match kind {
        AuthorityKind::Actor => {
            if valid_id(user_id) && user_id.len() <= 200 {
                Ok(())
            } else {
                Err(invalid())
            }
        }
        AuthorityKind::Appointee => {
            if !valid_id(person_id)
                || person_id.len() > 200
                || position_id.len() > 200
                || !matches!(appointment_kind, "ceo" | "hr" | "head")
                || !valid_id(scope_id)
                || scope_id.len() > 200
            {
                Err(invalid())
            } else {
                Ok(())
            }
        }
    }
}

/// Keys are intentionally snake_case: the Finance workspace maps these rows
/// directly by their original column names.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct WorkspacePaymentRow {
    pub id: String,
    pub source_type: String,
    pub source_id: String,
    pub currency: String,
    pub status: String,
    pub due_date: Option<String>,
    pub amount_minor: i64,
}

pub fn invalid_company() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_LANE7_COMPANY_INVALID",
        "Use one bounded Company scope.",
    )
}

pub fn invalid_observed_at() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_LANE7_OBSERVED_AT_INVALID",
        "Use a non-negative epoch-millisecond clock.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_PAYROLL_LANE7_UNAVAILABLE",
        "The native People Payroll clock service is unavailable.",
    )
}

pub fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_LANE7_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && RecordId::new(value).is_ok()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

pub fn validate_company(company: &str) -> Result<(), BusinessError> {
    if valid_id(company) {
        Ok(())
    } else {
        Err(invalid_company())
    }
}

pub fn parse_observed_at(raw: &str) -> Result<i64, BusinessError> {
    if raw.is_empty() || raw.len() > 19 || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid_observed_at());
    }
    match raw.parse::<i64>() {
        Ok(value) if value >= 0 => Ok(value),
        _ => Err(invalid_observed_at()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_company_scope() {
        assert!(validate_company("company-1").is_ok());
        assert!(validate_company("acme:company:01").is_ok());
        assert!(validate_company("").is_err());
        assert!(validate_company("../other").is_err());
        assert!(validate_company(&"x".repeat(201)).is_err());
    }

    #[test]
    fn parses_observed_clock() {
        assert_eq!(parse_observed_at("0"), Ok(0));
        assert_eq!(parse_observed_at("1764010200000"), Ok(1_764_010_200_000));
        assert!(parse_observed_at("-1").is_err());
        assert!(parse_observed_at("12ms").is_err());
        assert!(parse_observed_at("99999999999999999999").is_err());
    }

    #[test]
    fn validates_authority_queries() {
        let mut q = AuthorityPersonQuery {
            kind: AuthorityKind::Actor,
            user_id: "usr_1".to_owned(),
            person_id: String::new(),
            position_id: String::new(),
            appointment_kind: String::new(),
            scope_id: String::new(),
            self_ceo: false,
        };
        assert!(validate_authority_query(&q).is_ok());
        q.user_id.clear();
        assert!(validate_authority_query(&q).is_err());
        q.kind = AuthorityKind::Appointee;
        q.person_id = "per_1".to_owned();
        q.appointment_kind = "head".to_owned();
        q.scope_id = "dept_1".to_owned();
        assert!(validate_authority_query(&q).is_ok());
        q.appointment_kind = "other".to_owned();
        assert!(validate_authority_query(&q).is_err());
    }

    #[test]
    fn pins_response_shape() {
        let value = LegacyPeopleTransition { deadline: Some(42) };
        assert_eq!(
            serde_json::to_string(&value).expect("serialize"),
            r#"{"deadline":42}"#
        );
        let none = LegacyPeopleTransition { deadline: None };
        assert_eq!(
            serde_json::to_string(&none).expect("serialize"),
            r#"{"deadline":null}"#
        );
    }
}
