//! Pure domain for the native POS executive transaction-evidence search.
//!
//! The axum owner owns the SQL, the projection and the response shape. This
//! module contains no I/O: it validates the bounded request, defines the exact
//! response row and pins the wire field set. TypeScript is a transport only.

use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};

pub const METRIC_POS_SALES: &str = "pos-sales";
pub const METRIC_NET_POS_SALES: &str = "net-pos-sales";
pub const METRIC_POS_PAYMENT_MIX: &str = "pos-payment-mix";
pub const METRICS: [&str; 3] = [
    METRIC_POS_SALES,
    METRIC_NET_POS_SALES,
    METRIC_POS_PAYMENT_MIX,
];

/// Exact `EvidenceRow` field set of `pos-evidence-contract.ts`. A response row
/// has exactly these keys — no additions, no omissions.
pub const ROW_KEYS: [&str; 15] = [
    "id",
    "source_id",
    "source_type",
    "sale_id",
    "event_date",
    "receipt_number",
    "status",
    "sale_status",
    "currency",
    "method",
    "amount_minor",
    "contribution_minor",
    "refunded_minor",
    "revision",
    "sale_version",
];

pub const MIN_LIMIT: usize = 1;
pub const MAX_LIMIT: usize = 50;
pub const MAX_QUERY_CHARS: usize = 100;
pub const MAX_ID_LEN: usize = 240;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceCursor {
    pub event_date: String,
    pub id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceQuery {
    pub company: String,
    pub branch: Option<String>,
    pub metric: String,
    pub from: String,
    pub to: String,
    pub currency: String,
    pub query: String,
    pub limit: usize,
    pub cursor: Option<EvidenceCursor>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceRow {
    pub id: String,
    pub source_id: String,
    pub source_type: String,
    pub sale_id: String,
    pub event_date: String,
    pub receipt_number: String,
    pub status: String,
    pub sale_status: String,
    pub currency: String,
    pub method: Option<String>,
    pub amount_minor: String,
    pub contribution_minor: String,
    pub refunded_minor: String,
    pub revision: String,
    pub sale_version: String,
}

impl EvidenceRow {
    pub fn value(self) -> Value {
        json!({
            "id": self.id,
            "source_id": self.source_id,
            "source_type": self.source_type,
            "sale_id": self.sale_id,
            "event_date": self.event_date,
            "receipt_number": self.receipt_number,
            "status": self.status,
            "sale_status": self.sale_status,
            "currency": self.currency,
            "method": self.method,
            "amount_minor": self.amount_minor,
            "contribution_minor": self.contribution_minor,
            "refunded_minor": self.refunded_minor,
            "revision": self.revision,
            "sale_version": self.sale_version,
        })
    }
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid("POS_EVIDENCE_QUERY_INVALID", "Use the bounded POS evidence filters.")
}

fn bounded(value: &str, min: usize, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && value.chars().count() >= min
}

fn safe_id(value: &str) -> bool {
    bounded(value, 1, 200)
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

fn currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|b| b.is_ascii_uppercase())
}

fn calendar_date(value: &str) -> bool {
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
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    (1..=12).contains(&month) && day >= 1 && day <= days[month - 1]
}

/// All request invariants are owned here. The pool/HTTP layers never derive
/// company, metric or scope from an untrusted header without these checks.
pub fn validate(query: &EvidenceQuery) -> Result<(), BusinessError> {
    if !safe_id(&query.company)
        || query.branch.as_deref().is_some_and(|v| !safe_id(v))
        || !METRICS.contains(&query.metric.as_str())
        || !currency(&query.currency)
        || !calendar_date(&query.from)
        || !calendar_date(&query.to)
        || query.from > query.to
        || query.query.chars().count() > MAX_QUERY_CHARS
        || query.query.chars().any(char::is_control)
        || !(MIN_LIMIT..=MAX_LIMIT).contains(&query.limit)
    {
        return Err(invalid());
    }
    if let Some(cursor) = &query.cursor {
        if !calendar_date(&cursor.event_date)
            || !bounded(&cursor.id, 1, MAX_ID_LEN)
            || cursor.id.chars().any(char::is_control)
        {
            return Err(invalid());
        }
    }
    Ok(())
}

/// A projection is accepted only when every field is present and every
/// discriminator/amount is well-formed. Malformed native rows fail closed.
pub fn valid_row(row: &EvidenceRow) -> bool {
    let kind = row.source_type.as_str();
    matches!(kind, "sale" | "refund" | "payment")
        && !row.id.is_empty()
        && row.id.len() <= MAX_ID_LEN
        && row.id.starts_with(&format!("{kind}:"))
        && safe_id(&row.source_id)
        && safe_id(&row.sale_id)
        && calendar_date(&row.event_date)
        && currency(&row.currency)
        && row.status.len() <= 40
        && row.sale_status.len() <= 40
        && row.receipt_number.len() <= 200
        && row.method.as_deref().is_none_or(|value| value.len() <= 40)
        && [&row.amount_minor, &row.contribution_minor, &row.refunded_minor]
            .iter()
            .all(|value| !value.is_empty() && value.len() <= 40 && value.bytes().all(|b| b.is_ascii_digit() || b == b'-' || b == b'.'))
        && [&row.revision, &row.sale_version]
            .iter()
            .all(|value| !value.is_empty() && value.len() <= 40 && value.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query() -> EvidenceQuery {
        EvidenceQuery {
            company: "co".into(),
            branch: Some("br".into()),
            metric: METRIC_NET_POS_SALES.into(),
            from: "2026-01-01".into(),
            to: "2026-01-31".into(),
            currency: "USD".into(),
            query: "receipt".into(),
            limit: 25,
            cursor: None,
        }
    }

    fn row() -> EvidenceRow {
        EvidenceRow {
            id: "sale:s1".into(),
            source_id: "s1".into(),
            source_type: "sale".into(),
            sale_id: "s1".into(),
            event_date: "2026-01-02".into(),
            receipt_number: "R1".into(),
            status: "completed".into(),
            sale_status: "completed".into(),
            currency: "USD".into(),
            method: None,
            amount_minor: "100".into(),
            contribution_minor: "100".into(),
            refunded_minor: "0".into(),
            revision: "1".into(),
            sale_version: "1".into(),
        }
    }

    #[test]
    fn accepts_bounded_query() {
        assert!(validate(&query()).is_ok());
        let mut q = query();
        q.cursor = Some(EvidenceCursor {
            event_date: "2026-01-02".into(),
            id: "sale:s1".into(),
        });
        assert!(validate(&q).is_ok());
    }

    #[test]
    fn rejects_unknown_metric_and_out_of_bounds() {
        for mutate in [
            |q: &mut EvidenceQuery| q.metric = "other".into(),
            |q: &mut EvidenceQuery| q.company.clear(),
            |q: &mut EvidenceQuery| q.currency = "usd!".into(),
            |q: &mut EvidenceQuery| q.from = "2026-02-30".into(),
            |q: &mut EvidenceQuery| q.from = "2026-02-02".into(),
            |q: &mut EvidenceQuery| q.limit = 0,
            |q: &mut EvidenceQuery| q.limit = 51,
            |q: &mut EvidenceQuery| q.query = "x".repeat(101),
        ] {
            let mut q = query();
            mutate(&mut q);
            assert!(validate(&q).is_err());
        }
    }

    #[test]
    fn pins_row_field_set_and_validity() {
        assert_eq!(ROW_KEYS.len(), 15);
        let value = row().value();
        let object = value.as_object().unwrap();
        assert_eq!(object.len(), ROW_KEYS.len());
        for key in ROW_KEYS {
            assert!(object.contains_key(key));
        }
        assert!(valid_row(&row()));
        let mut bad = row();
        bad.source_type = "unknown".into();
        assert!(!valid_row(&bad));
        bad = row();
        bad.id = "refund:s1".into();
        assert!(!valid_row(&bad));
    }
}
