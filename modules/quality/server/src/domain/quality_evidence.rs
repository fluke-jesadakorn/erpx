//! Pure domain for the native Executive Quality inspection-evidence search.
//!
//! No I/O, no SQL and no HTTP: bounded request invariants, the exact retained
//! `QualityInspectionRow` field set and the base64url cursor reader. The axum
//! owner and the sqlx adapter consume these facts; TypeScript is a transport
//! only and keeps no query authority.

use erpx_platform_shared_server::public::BusinessError;
use serde_json::{Value, json};

/// The only Quality metric backed by inspection source records.
pub const METRIC_INSPECTION_PASS_RATE: &str = "inspection-pass-rate";

/// Exact `qualitySearchFields` row shape retired from the TypeScript search.
pub const ROW_KEYS: [&str; 7] = [
    "id",
    "hold_id",
    "result",
    "created_at",
    "reviewed_version",
    "product_name",
    "warehouse_name",
];

pub const MIN_LIMIT: usize = 1;
pub const MAX_LIMIT: usize = 50;
pub const MAX_QUERY_CHARS: usize = 100;
pub const MAX_CURSOR_CHARS: usize = 2000;
pub const MAX_ID_LEN: usize = 200;
pub const MAX_DATE_SPAN_DAYS: i64 = 366;
const MAX_SAFE: i64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceCursor {
    pub created_at: i64,
    pub id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectionEvidenceQuery {
    pub company: String,
    pub branch: Option<String>,
    pub from: String,
    pub to: String,
    pub currency: String,
    pub query: String,
    pub limit: usize,
    pub cursor: Option<EvidenceCursor>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectionEvidenceRow {
    pub id: String,
    pub hold_id: String,
    pub result: String,
    pub created_at: i64,
    pub reviewed_version: i64,
    pub product_name: String,
    pub warehouse_name: String,
}

impl InspectionEvidenceRow {
    /// The retained JSON row: seven keys and numeric time/version values.
    pub fn value(self) -> Value {
        json!({
            "id": self.id,
            "hold_id": self.hold_id,
            "result": self.result,
            "created_at": self.created_at,
            "reviewed_version": self.reviewed_version,
            "product_name": self.product_name,
            "warehouse_name": self.warehouse_name,
        })
    }
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "QUALITY_EVIDENCE_QUERY_INVALID",
        "Use the bounded Quality evidence filters.",
    )
}

fn safe_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_LEN
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
        })
}

/// Days since 1970-01-01 (Howard Hinnant's civil-date algorithm). Used both to
/// validate the retained one-year window and to produce the exact `Date.parse`
/// millisecond binds the TypeScript search sent.
fn days_from_civil(value: &str) -> Option<i64> {
    if value.len() != 10
        || value.as_bytes()[4] != b'-'
        || value.as_bytes()[7] != b'-'
        || !value
            .bytes()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
    {
        return None;
    }
    let year = value[0..4].parse::<i64>().ok()?;
    let month = value[5..7].parse::<i64>().ok()?;
    let day = value[8..10].parse::<i64>().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let lengths = [
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
    if day > lengths[(month - 1) as usize] {
        return None;
    }
    let y = year - if month <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

/// The exact millisecond value of the retired `Date.parse(date)` bind.
pub fn date_millis(value: &str) -> Option<i64> {
    days_from_civil(value).and_then(|days| days.checked_mul(86_400_000))
}

/// All request invariants are owned here. The pool and HTTP layers never
/// derive Company, scope, date range or cursor authority on their own.
pub fn validate(query: &InspectionEvidenceQuery) -> Result<(), BusinessError> {
    let from = date_millis(&query.from);
    let to = date_millis(&query.to);
    let span = match (from, to) {
        (Some(from), Some(to)) => (to - from) / 86_400_000,
        _ => return Err(invalid()),
    };
    if !safe_id(&query.company)
        || query.branch.as_deref().is_some_and(|value| !safe_id(value))
        || query.currency.len() != 3
        || !query.currency.bytes().all(|byte| byte.is_ascii_uppercase())
        || query.query.chars().count() > MAX_QUERY_CHARS
        || query.query.chars().any(char::is_control)
        || !(MIN_LIMIT..=MAX_LIMIT).contains(&query.limit)
        || span < 0
        || span > MAX_DATE_SPAN_DAYS
    {
        return Err(invalid());
    }
    if let Some(cursor) = &query.cursor {
        if !(-MAX_SAFE..=MAX_SAFE).contains(&cursor.created_at)
            || !safe_id(&cursor.id)
        {
            return Err(invalid());
        }
    }
    Ok(())
}

/// Reject a malformed native row instead of returning a partial payload.
pub fn valid_row(row: &InspectionEvidenceRow) -> bool {
    !row.id.is_empty()
        && row.id.len() <= MAX_ID_LEN
        && safe_id(&row.hold_id)
        && matches!(row.result.as_str(), "pass" | "fail")
        && (-MAX_SAFE..=MAX_SAFE).contains(&row.created_at)
        && (0..=MAX_SAFE).contains(&row.reviewed_version)
        && !row.product_name.is_empty()
        && row.product_name.len() <= 500
        && row.warehouse_name.len() <= 500
}

fn base64url(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}

/// Decode the same base64url cursor the retained `qualityCursorValue` accepts.
/// No crate is added: the cursor is a small JSON envelope of two bounded facts.
pub fn decode_cursor(raw: &str) -> Result<EvidenceCursor, BusinessError> {
    if raw.is_empty() || raw.len() > MAX_CURSOR_CHARS || raw.len() % 4 == 1 {
        return Err(invalid());
    }
    let mut bytes = Vec::with_capacity(raw.len() * 3 / 4 + 3);
    let mut chunk = 0_u32;
    let mut bits = 0_u32;
    for byte in raw.bytes() {
        let value = base64url(byte).ok_or_else(invalid)?;
        chunk = (chunk << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push((chunk >> bits) as u8);
            chunk &= (1 << bits) - 1;
        }
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    let created_at = value
        .get("createdAt")
        .and_then(Value::as_i64)
        .ok_or_else(invalid)?;
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?
        .to_owned();
    let cursor = EvidenceCursor { created_at, id };
    if !(-MAX_SAFE..=MAX_SAFE).contains(&cursor.created_at) || !safe_id(&cursor.id) {
        return Err(invalid());
    }
    Ok(cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query() -> InspectionEvidenceQuery {
        InspectionEvidenceQuery {
            company: "co".into(),
            branch: Some("br".into()),
            from: "2026-01-01".into(),
            to: "2026-01-31".into(),
            currency: "THB".into(),
            query: String::new(),
            limit: 25,
            cursor: None,
        }
    }

    fn row() -> InspectionEvidenceRow {
        InspectionEvidenceRow {
            id: "i1".into(),
            hold_id: "h1".into(),
            result: "pass".into(),
            created_at: 1_767_225_600_000,
            reviewed_version: 3,
            product_name: "Rice".into(),
            warehouse_name: "Main".into(),
        }
    }

    #[test]
    fn accepts_bounded_query_and_exact_row_shape() {
        assert!(validate(&query()).is_ok());
        let value = row().value();
        let object = value.as_object().unwrap();
        assert_eq!(object.len(), ROW_KEYS.len());
        for key in ROW_KEYS {
            assert!(object.contains_key(key));
        }
        assert_eq!(object["created_at"].as_i64(), Some(1_767_225_600_000));
        assert!(valid_row(&row()));
    }

    #[test]
    fn rejects_unknown_metric_bad_scope_dates_and_bounds() {
        for mutate in [
            |q: &mut InspectionEvidenceQuery| q.company.clear(),
            |q: &mut InspectionEvidenceQuery| q.branch = Some("bad branch".into()),
            |q: &mut InspectionEvidenceQuery| q.currency = "thb".into(),
            |q: &mut InspectionEvidenceQuery| q.from = "2026-02-30".into(),
            |q: &mut InspectionEvidenceQuery| q.from = "2026-02-02".into(),
            |q: &mut InspectionEvidenceQuery| q.to = "2027-02-01".into(),
            |q: &mut InspectionEvidenceQuery| q.query = "x".repeat(101),
            |q: &mut InspectionEvidenceQuery| q.limit = 0,
            |q: &mut InspectionEvidenceQuery| q.limit = 51,
        ] {
            let mut q = query();
            mutate(&mut q);
            assert!(validate(&q).is_err());
        }
    }

    #[test]
    fn cursor_round_trips_the_retained_json_envelope() {
        let cursor = EvidenceCursor {
            created_at: 1_767_225_600_000,
            id: "i1".into(),
        };
        // Hand-built base64url of {"createdAt":1767225600000,"id":"i1"}.
        let encoded = "eyJjcmVhdGVkQXQiOjE3NjcyMjU2MDAwMDAsImlkIjoiaTEifQ";
        assert_eq!(decode_cursor(encoded).unwrap(), cursor);
        assert!(decode_cursor("").is_err());
        assert!(decode_cursor("%%%%").is_err());
        assert!(decode_cursor("eyJjcmVhdGVkQXQiOiJ4IiwiaWQiOiJpMSJ9").is_err());
    }

    #[test]
    fn date_bind_math_matches_date_parse_millis() {
        assert_eq!(date_millis("1970-01-01"), Some(0));
        assert_eq!(date_millis("2026-01-01"), Some(1_767_225_600_000));
        assert!(date_millis("2026-13-01").is_none());
    }

    #[test]
    fn malformed_rows_fail_closed() {
        for mutate in [
            |r: &mut InspectionEvidenceRow| r.id.clear(),
            |r: &mut InspectionEvidenceRow| r.result = "pending".into(),
            |r: &mut InspectionEvidenceRow| r.reviewed_version = -1,
            |r: &mut InspectionEvidenceRow| r.product_name.clear(),
        ] {
            let mut r = row();
            mutate(&mut r);
            assert!(!valid_row(&r));
        }
    }
}
