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

// ---- Wave 2: POS executive evidence detail rows -------------------------

pub const DETAIL_KIND_REFUND: &str = "refund";
pub const DETAIL_KIND_PAYMENT: &str = "payment";
pub const DETAIL_KINDS: [&str; 2] = [DETAIL_KIND_REFUND, DETAIL_KIND_PAYMENT];

/// Exact column order of the retired `readPosRefunds2` projection.
pub const REFUND_DETAIL_KEYS: [&str; 14] = [
    "source_id", "sale_id", "source_type", "receipt_number", "business_date",
    "accounting_date", "currency", "state", "amount_minor", "contribution_minor",
    "refunded_minor", "revision", "sale_version", "branch_id",
];
/// Exact column order of the retired `readPosRefundPayments3` projection.
pub const PAYMENT_DETAIL_KEYS: [&str; 17] = [
    "source_id", "source_type", "sale_id", "receipt_number", "business_date",
    "accounting_date", "currency", "sale_state", "state", "method", "amount_minor",
    "refunded_minor", "contribution_minor", "revision", "sale_version", "branch_id",
    "created_at",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceDetailQuery {
    pub company: String,
    pub branch: Option<String>,
    pub kind: String,
    pub id: String,
    pub from: String,
    pub to: String,
    pub currency: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceDetailRow {
    pub values: Vec<(String, Option<String>)>,
}

impl EvidenceDetailRow {
    pub fn value(self) -> Value {
        let mut map = serde_json::Map::new();
        for (key, value) in self.values {
            map.insert(key, value.map(Value::String).unwrap_or(Value::Null));
        }
        Value::Object(map)
    }
}

pub fn validate_detail(query: &EvidenceDetailQuery) -> Result<(), BusinessError> {
    if !DETAIL_KINDS.contains(&query.kind.as_str())
        || !safe_id(&query.company)
        || query.branch.as_deref().is_some_and(|value| !safe_id(value))
        || !bounded(&query.id, 1, MAX_ID_LEN)
        || query.id.chars().any(char::is_control)
        || !currency(&query.currency)
        || !calendar_date(&query.from)
        || !calendar_date(&query.to)
        || query.from > query.to
    {
        return Err(invalid());
    }
    Ok(())
}

/// Fail closed on a malformed native detail row: exact key set per kind and a
/// discriminator matching the registered read kind.
pub fn valid_detail_row(kind: &str, row: &EvidenceDetailRow) -> bool {
    let expected: &[&str] = match kind {
        DETAIL_KIND_REFUND => &REFUND_DETAIL_KEYS,
        DETAIL_KIND_PAYMENT => &PAYMENT_DETAIL_KEYS,
        _ => return false,
    };
    if row.values.len() != expected.len() {
        return false;
    }
    let get = |key: &str| -> Option<&Option<String>> {
        row.values.iter().find(|(name, _)| name == key).map(|(_, value)| value)
    };
    if !expected.iter().all(|key| get(key).is_some()) {
        return false;
    }
    let type_matches = get("source_type").and_then(|value| value.as_deref()) == Some(kind);
    let source = get("source_id").and_then(|value| value.as_deref());
    let sale = get("sale_id").and_then(|value| value.as_deref());
    type_matches
        && source.is_some_and(|value| bounded(value, 1, MAX_ID_LEN))
        && sale.is_some_and(|value| bounded(value, 1, MAX_ID_LEN))
        && get("amount_minor").and_then(|value| value.as_deref()).is_some_and(|v| !v.is_empty())
}

// ---- Wave 3: POS workspace day-close report list ------------------------

/// Exact `readPosDayCloses1` projection column order retired from `pos.ts`.
pub const DAY_CLOSES_KEYS: [&str; 10] = [
    "id",
    "company_id",
    "branch_id",
    "business_date",
    "currency",
    "state",
    "prepared_by",
    "confirmed_by",
    "version",
    "snapshot_json",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DayClosesQuery {
    pub company: String,
    pub branch: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DayClosesRow {
    pub values: Vec<(String, Option<String>)>,
}

impl DayClosesRow {
    pub fn value(self) -> Value {
        let mut map = serde_json::Map::new();
        for (key, value) in self.values {
            map.insert(key, value.map(Value::String).unwrap_or(Value::Null));
        }
        Value::Object(map)
    }
}

/// Company/branch scope is the only untrusted input; both must be bounded
/// identities. The date range, metric and pagination are fixed by the owner.
pub fn validate_day_closes(query: &DayClosesQuery) -> Result<(), BusinessError> {
    if !safe_id(&query.company) || query.branch.as_deref().is_some_and(|v| !safe_id(v)) {
        return Err(invalid());
    }
    Ok(())
}

/// Fail closed on a malformed native day-close row: exact key set and a
/// well-formed identity/date/currency discriminator.
pub fn valid_day_closes_row(row: &DayClosesRow) -> bool {
    if row.values.len() != DAY_CLOSES_KEYS.len() {
        return false;
    }
    let get = |key: &str| -> Option<&Option<String>> {
        row.values.iter().find(|(name, _)| name == key).map(|(_, value)| value)
    };
    if !DAY_CLOSES_KEYS.iter().all(|key| get(key).is_some()) {
        return false;
    }
    let id = get("id").and_then(|value| value.as_deref());
    let date = get("business_date").and_then(|value| value.as_deref());
    let ccy = get("currency").and_then(|value| value.as_deref());
    id.is_some_and(|value| bounded(value, 1, MAX_ID_LEN))
        && date.is_some_and(calendar_date)
        && ccy.is_some_and(|value| currency(value))
        && get("snapshot_json").and_then(|value| value.as_deref()).is_some()
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

#[cfg(test)]
mod detail_tests {
    use super::*;

    fn query(kind: &str) -> EvidenceDetailQuery {
        EvidenceDetailQuery {
            company: "co".into(),
            branch: None,
            kind: kind.into(),
            id: "r1".into(),
            from: "2026-01-01".into(),
            to: "2026-01-31".into(),
            currency: "USD".into(),
        }
    }

    fn row(kind: &str) -> EvidenceDetailRow {
        let keys: &[&str] = if kind == DETAIL_KIND_REFUND {
            &REFUND_DETAIL_KEYS
        } else {
            &PAYMENT_DETAIL_KEYS
        };
        EvidenceDetailRow {
            values: keys
                .iter()
                .map(|key| {
                    let value = match *key {
                        "source_type" => kind.to_owned(),
                        "accounting_date" if kind == DETAIL_KIND_PAYMENT => return (key.to_string(), None),
                        "method" => return (key.to_string(), None),
                        "currency" => "USD".to_owned(),
                        "amount_minor" => "100".to_owned(),
                        _ => "v".to_owned(),
                    };
                    (key.to_string(), Some(value))
                })
                .collect(),
        }
    }

    #[test]
    fn detail_query_validates_kind_scope_and_dates() {
        assert!(validate_detail(&query(DETAIL_KIND_REFUND)).is_ok());
        let mut bad = query(DETAIL_KIND_REFUND);
        bad.kind = "other".into();
        assert!(validate_detail(&bad).is_err());
        bad = query(DETAIL_KIND_PAYMENT);
        bad.company.clear();
        assert!(validate_detail(&bad).is_err());
        bad = query(DETAIL_KIND_PAYMENT);
        bad.to = "2025-12-31".into();
        assert!(validate_detail(&bad).is_err());
    }

    #[test]
    fn detail_row_pins_exact_key_set_per_kind() {
        assert_eq!(REFUND_DETAIL_KEYS.len(), 14);
        assert_eq!(PAYMENT_DETAIL_KEYS.len(), 17);
        assert!(valid_detail_row(DETAIL_KIND_REFUND, &row(DETAIL_KIND_REFUND)));
        assert!(valid_detail_row(DETAIL_KIND_PAYMENT, &row(DETAIL_KIND_PAYMENT)));
        assert!(!valid_detail_row(DETAIL_KIND_PAYMENT, &row(DETAIL_KIND_REFUND)));
        let mut bad = row(DETAIL_KIND_REFUND);
        bad.values.pop();
        assert!(!valid_detail_row(DETAIL_KIND_REFUND, &bad));
        let value = row(DETAIL_KIND_REFUND).value();
        assert_eq!(value.as_object().unwrap().len(), REFUND_DETAIL_KEYS.len());
    }
}

#[cfg(test)]
mod day_closes_tests {
    use super::*;

    fn query() -> DayClosesQuery {
        DayClosesQuery {
            company: "co".into(),
            branch: Some("br".into()),
        }
    }

    fn row() -> DayClosesRow {
        DayClosesRow {
            values: DAY_CLOSES_KEYS
                .iter()
                .map(|key| {
                    let value = match *key {
                        "id" => "dc1".to_owned(),
                        "business_date" => "2026-01-31".to_owned(),
                        "currency" => "USD".to_owned(),
                        _ => "v".to_owned(),
                    };
                    ((*key).to_owned(), Some(value))
                })
                .collect(),
        }
    }

    #[test]
    fn day_closes_query_validates_bounded_scope() {
        assert!(validate_day_closes(&query()).is_ok());
        let mut q = query();
        q.branch = None;
        assert!(validate_day_closes(&q).is_ok());
        q.company.clear();
        assert!(validate_day_closes(&q).is_err());
        q = query();
        q.branch = Some("bad branch".into());
        assert!(validate_day_closes(&q).is_err());
    }

    #[test]
    fn day_closes_row_pins_exact_key_set() {
        assert_eq!(DAY_CLOSES_KEYS.len(), 10);
        assert!(valid_day_closes_row(&row()));
        let value = row().value();
        assert_eq!(value.as_object().unwrap().len(), DAY_CLOSES_KEYS.len());
        let mut bad = row();
        bad.values.pop();
        assert!(!valid_day_closes_row(&bad));
        bad = row();
        bad.values[3].1 = Some("not-a-date".into());
        assert!(!valid_day_closes_row(&bad));
    }
}
