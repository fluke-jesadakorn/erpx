//! Pure domain types for the native Finance overview-counts and invoice-read
//! families.
//!
//! The axum owner owns the SQL and the projection. This module carries no SQL:
//! it validates bounded Company identity and query inputs, and pins the wire
//! shapes of both families.

use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The seven company-scoped counts that back the Finance workbench and the
/// workspace assistant. Field order is part of the wire contract.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OverviewCounts {
    pub journals: i64,
    pub posting: i64,
    pub invoices: i64,
    pub payments: i64,
    pub receivable: i64,
    pub payable: i64,
    pub reconcile: i64,
}

impl OverviewCounts {
    /// Fail closed on any impossible (negative) count rather than emitting a
    /// malformed projection. `count(*)` can never be negative.
    pub fn validated(self) -> Result<Self, BusinessError> {
        let counts = [
            self.journals,
            self.posting,
            self.invoices,
            self.payments,
            self.receivable,
            self.payable,
            self.reconcile,
        ];
        if counts.iter().any(|count| *count < 0) {
            return Err(unavailable());
        }
        Ok(self)
    }
}

pub fn invalid_request() -> BusinessError {
    BusinessError::invalid(
        "FINANCE_NATIVE_LANE_2_REQUEST_INVALID",
        "Use a bounded Company and registered Holding target.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_NATIVE_LANE_2_UNAVAILABLE",
        "The native Finance overview read is unavailable.",
    )
}

pub fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

pub fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "FINANCE_NATIVE_LANE_2_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

/// Company ids are opaque but bounded identifiers used as a SQL bind only.
pub fn valid_company_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// A Company's tool eligibility as projected by the native owner. The TS
/// client keeps `enabled` checks and all commit guards inside their write unit;
/// only this list read crosses the transport.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompanyTool {
    pub tool: String,
    pub enabled: i64,
    pub version: i64,
}

/// Bounded filters for the native invoice page read. Absent filters are empty.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InvoicePageRequest {
    pub q: Option<String>,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub queue: Option<String>,
    pub chart_id: Option<String>,
    pub cursor: Option<String>,
}

/// Bounded filters for the native historical aging read.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgingRequest {
    pub kind: String,
    pub as_of: String,
    pub chart_id: Option<String>,
}

/// The cursor page returned by the owner. Field order is the wire contract.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoicePageWire {
    pub items: Vec<Value>,
    pub next_cursor: Option<String>,
    pub previous_cursor: Option<String>,
    pub has_more: bool,
}

fn bounded_optional(value: &Option<String>, max: usize) -> bool {
    value
        .as_deref()
        .is_none_or(|value| !value.is_empty() && value.len() <= max)
}

/// Reject unbounded or malformed invoice filters before any SQL is executed.
pub fn validate_page_request(request: &InvoicePageRequest) -> Result<(), BusinessError> {
    let invalid = || {
        BusinessError::invalid(
            "FINANCE_NATIVE_LANE_2_INVOICE_QUERY_INVALID",
            "Use the invoice list filters.",
        )
    };
    let kind = request.kind.as_deref().unwrap_or("");
    let queue = request.queue.as_deref().unwrap_or("");
    if !matches!(kind, "" | "receivable" | "payable")
        || !matches!(queue, "" | "open")
        || request.q.as_deref().is_some_and(|value| value.len() > 200)
        || request.status.as_deref().is_some_and(|value| value.len() > 40)
        || !bounded_optional(&request.chart_id, 200)
        || request.cursor.as_deref().is_some_and(|value| value.is_empty() || value.len() > 4096)
    {
        return Err(invalid());
    }
    Ok(())
}

pub fn valid_calendar_date(value: &str) -> bool {
    if value.len() != 10
        || value.as_bytes()[4] != b'-'
        || value.as_bytes()[7] != b'-'
        || !value
            .bytes()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return false;
    }
    let year = value[0..4].parse::<i32>().ok();
    let month = value[5..7].parse::<usize>().ok();
    let day = value[8..10].parse::<u32>().ok();
    let (Some(year), Some(month), Some(day)) = (year, month, day) else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = [
        31,
        if leap { 29 } else { 28 },
        31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
    ];
    (1..=12).contains(&month) && day >= 1 && day <= days[month - 1]
}

pub fn validate_aging_request(request: &AgingRequest) -> Result<(), BusinessError> {
    if !matches!(request.kind.as_str(), "payable" | "receivable")
        || !valid_calendar_date(&request.as_of)
        || !bounded_optional(&request.chart_id, 200)
    {
        return Err(BusinessError::invalid(
            "FINANCE_NATIVE_LANE_2_AGING_QUERY_INVALID",
            "Use a valid aging kind, cutoff date and ledger.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_bounded_company_and_slug() {
        assert!(valid_company_id("acme:company-01"));
        assert!(valid_company_id("c.1_2"));
        assert!(!valid_company_id(""));
        assert!(!valid_company_id("bad/id?"));
        assert!(!valid_company_id(&"x".repeat(201)));
        assert!(valid_slug("acme-holding_1"));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug(""));
        assert!(!valid_slug(&"a".repeat(129)));
    }

    #[test]
    fn rejects_negative_counts() {
        let good = OverviewCounts {
            journals: 1,
            posting: 0,
            invoices: 2,
            payments: 3,
            receivable: 4,
            payable: 5,
            reconcile: 6,
        };
        assert_eq!(good.validated(), Ok(good));
        let bad = OverviewCounts {
            reconcile: -1,
            ..good
        };
        assert_eq!(bad.validated(), Err(unavailable()));
    }

    #[test]
    fn validates_invoice_and_aging_filters() {
        let page = InvoicePageRequest::default();
        assert_eq!(validate_page_request(&page), Ok(()));
        let bad_page = InvoicePageRequest {
            kind: Some("all".into()),
            ..InvoicePageRequest::default()
        };
        assert_eq!(
            validate_page_request(&bad_page).unwrap_err().code,
            "FINANCE_NATIVE_LANE_2_INVOICE_QUERY_INVALID"
        );
        let aging = AgingRequest { kind: "receivable".into(), as_of: "2026-02-01".into(), chart_id: None };
        assert_eq!(validate_aging_request(&aging), Ok(()));
        assert!(valid_calendar_date("2024-02-29"));
        assert!(!valid_calendar_date("2026-02-29"));
        assert!(!valid_calendar_date("2026-13-01"));
        let bad_aging = AgingRequest { kind: "receivable".into(), as_of: "2026-02-30".into(), chart_id: None };
        assert_eq!(
            validate_aging_request(&bad_aging).unwrap_err().code,
            "FINANCE_NATIVE_LANE_2_AGING_QUERY_INVALID"
        );
    }

    #[test]
    fn pins_the_invoice_page_wire_shape() {
        let page = InvoicePageWire {
            items: vec![serde_json::json!({"id": "i1", "page_key": "2026-01-01:INV-1"})],
            next_cursor: Some("next".into()),
            previous_cursor: None,
            has_more: true,
        };
        assert_eq!(
            serde_json::to_string(&page).expect("serialize"),
            r#"{"items":[{"id":"i1","page_key":"2026-01-01:INV-1"}],"nextCursor":"next","previousCursor":null,"hasMore":true}"#
        );
    }
}
