//! Pure response types, validation and invariants for the Finance-owned
//! customer master read family. The SQL heads are kept byte-identical to the
//! retired TypeScript `sales-customer-sources` statements; the Postgres adapter
//! converts the `?` binds to sqlx placeholders and owns every projection.
//!
//! No business fallback lives in TypeScript: a missing or malformed native
//! runtime fails closed with an explicit unavailable error.

use base64::Engine;
use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const KIND_PRICING_CUSTOMERS: &str = "pricing_customers";
pub const KIND_PRICING_CUSTOMER_VALID: &str = "pricing_customer_valid";
pub const KIND_CUSTOMER_PROFILE: &str = "customer_profile";
pub const KIND_CUSTOMER_DIRECTORY: &str = "customer_directory";
pub const KIND_NOTE_CUSTOMER: &str = "note_customer";

pub const RF_KIND_PARTY: &str = "record_facts_party";
pub const RF_KIND_ACCOUNT: &str = "record_facts_account";
pub const RF_KIND_PERIOD: &str = "record_facts_period";
pub const RF_KIND_ROWS: &str = "record_facts_rows";

/// Byte-identical heads of the four retired record-supplement statements.
pub const RF_PARTY_SQL: &str = "SELECT * FROM finance_parties WHERE id=? AND company_id=? AND kind IN ('customer','supplier') AND person_id IS NULL";
pub const RF_ACCOUNT_SQL: &str = "SELECT a.*,c.base_currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.id=? AND a.company_id=?";
pub const RF_PERIOD_SQL: &str = "SELECT start_date,end_date FROM finance_periods WHERE id=? AND company_id=?";
pub const RF_ROWS_SQL: &str = "SELECT l.*,j.id journal_id,j.journal_number,j.entry_date,j.currency,j.status FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE l.account_id=? AND j.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND (?='0' OR NOT (j.source_type='fiscal_close' OR (j.source_type='book_projection' AND EXISTS (SELECT 1 FROM finance_journals fiscal_origin WHERE fiscal_origin.id=j.source_id AND fiscal_origin.company_id=j.company_id AND fiscal_origin.source_type='fiscal_close')))) AND (?::text IS NULL OR j.entry_date>=?) AND (?::text IS NULL OR j.entry_date<=?) ORDER BY j.entry_date DESC,j.id,l.ordinal LIMIT 31 OFFSET ?";

/// Default `listPageSize(undefined)` from the work-projection cursor contract.
pub const PAGE_SIZE: usize = 30;
pub const MAX_BODY_BYTES: usize = 16 * 1024;

/// Byte-identical heads of the five retired TypeScript statements.
pub const PRICING_CUSTOMERS_SQL: &str = "SELECT id,code,name,currency FROM finance_parties WHERE company_id=? AND kind='customer' AND status='active' AND person_id IS NULL ORDER BY name";
pub const PRICING_CUSTOMER_VALID_SQL: &str = "SELECT id FROM finance_parties WHERE id=? AND company_id=? AND kind='customer' AND status='active' AND currency=? AND person_id IS NULL";
pub const CUSTOMER_PROFILE_SQL: &str = "SELECT * FROM finance_parties WHERE id=? AND company_id=? AND kind='customer' AND person_id IS NULL";
pub const CUSTOMER_DIRECTORY_SQL: &str = "SELECT p.*,p.name page_key FROM finance_parties p WHERE p.company_id=? AND p.kind='customer' AND p.person_id IS NULL AND (?='' OR strpos(lower(p.code||' '||p.name),lower(?))>0) AND (?='' OR p.status=?)";
pub const NOTE_CUSTOMER_SQL: &str = "SELECT p.id,p.name,p.code,p.version FROM finance_parties p WHERE p.id=? AND p.company_id=? AND p.kind='customer' AND (?::int IS NULL OR p.version=?)";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomerReadRequest {
    pub kind: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub version: Option<i64>,
}

/// Wire shape posted by the strict TS record-facts transport. `chart_id`
/// stays snake_case so the Rust reader binds exactly what the client sends.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RecordFactsRequest {
    pub kind: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub chart_id: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub value2: Option<String>,
    #[serde(default)]
    pub value3: Option<String>,
    #[serde(default)]
    pub value4: Option<String>,
    #[serde(default)]
    pub value5: Option<String>,
    #[serde(default)]
    pub offset: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomerPage {
    pub items: Vec<Value>,
    pub next_cursor: Option<String>,
    pub previous_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CustomerReadResponse {
    pub kind: String,
    pub value: Value,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "FINANCE_CUSTOMER_READ_REQUEST_INVALID",
        "Use one bounded Finance customer read.",
    )
}

pub fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "FINANCE_CUSTOMER_READ_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

pub fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_CUSTOMER_READ_UNAVAILABLE",
        "The native Finance customer read service is unavailable.",
    )
}

pub fn invalid_cursor() -> BusinessError {
    BusinessError::invalid("FINANCE_CUSTOMER_CURSOR_INVALID", "Restart this search.")
}

pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

pub fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

pub fn valid_company(value: &str) -> bool {
    valid_id(value)
}

/// The branch is scope-only: it never widens the company-bound row set.
pub fn valid_branch(value: &str) -> bool {
    value.is_empty() || valid_id(value)
}

fn bounded_text(value: &Option<String>, max: usize) -> bool {
    value.as_deref().is_none_or(|v| v.len() <= max)
}

pub fn validate_request(request: &CustomerReadRequest) -> Result<(), BusinessError> {
    let require_id = || {
        request
            .id
            .as_deref()
            .filter(|id| valid_id(id))
            .ok_or_else(invalid)
    };
    match request.kind.as_str() {
        KIND_PRICING_CUSTOMERS => {}
        KIND_PRICING_CUSTOMER_VALID => {
            require_id()?;
            if !request
                .currency
                .as_deref()
                .is_some_and(|currency| !currency.is_empty() && currency.len() <= 16)
            {
                return Err(invalid());
            }
        }
        KIND_CUSTOMER_PROFILE => {
            require_id()?;
        }
        KIND_CUSTOMER_DIRECTORY => {
            if !bounded_text(&request.search, 200)
                || !bounded_text(&request.status, 40)
                || !bounded_text(&request.cursor, 4096)
            {
                return Err(invalid());
            }
        }
        KIND_NOTE_CUSTOMER => {
            require_id()?;
        }
        _ => return Err(invalid()),
    }
    Ok(())
}

pub fn validate_record_facts(request: &RecordFactsRequest) -> Result<(), BusinessError> {
    let require_id = || {
        request
            .id
            .as_deref()
            .filter(|id| valid_id(id))
            .ok_or_else(invalid)
    };
    match request.kind.as_str() {
        RF_KIND_PARTY | RF_KIND_ACCOUNT | RF_KIND_PERIOD => {
            require_id()?;
        }
        RF_KIND_ROWS => {
            require_id()?;
            if !matches!(request.value.as_deref(), Some("0") | Some("1")) {
                return Err(invalid());
            }
            if request.chart_id.as_deref().is_some_and(|id| id.is_empty() || id.len() > 200) {
                return Err(invalid());
            }
            for bound in [
                &request.value2,
                &request.value3,
                &request.value4,
                &request.value5,
            ] {
                if bound.as_deref().is_some_and(|value| value.len() > 40) {
                    return Err(invalid());
                }
            }
            if request.offset.is_none_or(|offset| offset < 0 || offset > 1_000_000) {
                return Err(invalid());
            }
        }
        _ => return Err(invalid()),
    }
    Ok(())
}

fn hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Cursor {
    pub key: String,
    pub id: String,
    pub reverse: bool,
    pub scope: String,
}

/// Mirrors `readListPage`: `scope` from the caller plus the fixed descending
/// flag and page size, hashed as canonical JSON.
pub fn scope_hash(company: &str, branch: &str, search: &str, status: &str) -> String {
    let mut scope = vec![
        Value::String(company.to_owned()),
        if branch.is_empty() {
            Value::Null
        } else {
            Value::String(branch.to_owned())
        },
        Value::String("parties".to_owned()),
        Value::String("parties".to_owned()),
        Value::String("customer".to_owned()),
        Value::String(search.to_owned()),
    ];
    if !status.is_empty() {
        scope.push(Value::String(status.to_owned()));
    }
    let raw = serde_json::json!([scope, false, PAGE_SIZE]).to_string();
    hash(&raw)
}

pub fn encode_cursor(key: &str, id: &str, reverse: bool, scope: &str) -> Result<String, BusinessError> {
    if key.is_empty() || id.is_empty() || key.len() > 4096 || id.len() > 4096 {
        return Err(invalid_cursor());
    }
    serde_json::to_vec(&Cursor {
        key: key.to_owned(),
        id: id.to_owned(),
        reverse,
        scope: scope.to_owned(),
    })
    .map(|raw| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw))
    .map_err(|_| invalid_cursor())
}

pub fn decode_cursor(raw: Option<&str>, scope: &str) -> Result<Option<Cursor>, BusinessError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    if raw.is_empty() {
        return Ok(None);
    }
    if raw.len() > 4096 {
        return Err(invalid_cursor());
    }
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|_| invalid_cursor())?;
    let cursor: Cursor = serde_json::from_slice(&bytes).map_err(|_| invalid_cursor())?;
    if cursor.scope != scope || cursor.key.is_empty() || cursor.id.is_empty() {
        return Err(invalid_cursor());
    }
    Ok(Some(cursor))
}

/// Public accessors so the SQL/result oracle tests can exercise the cursor
/// contract without reaching into private fields.
pub mod cursor {
    pub use super::{decode_cursor, encode_cursor, scope_hash, Cursor};
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(kind: &'static str) -> CustomerReadRequest {
        CustomerReadRequest {
            kind: kind.to_owned(),
            id: None,
            currency: None,
            search: None,
            status: None,
            cursor: None,
            version: None,
        }
    }

    #[test]
    fn native_lane_5_validates_each_registered_kind() {
        let mut r = request(KIND_PRICING_CUSTOMERS);
        assert!(validate_request(&r).is_ok());
        r = request(KIND_PRICING_CUSTOMER_VALID);
        assert!(validate_request(&r).is_err());
        r.id = Some("party-1".into());
        assert!(validate_request(&r).is_err());
        r.currency = Some("THB".into());
        assert!(validate_request(&r).is_ok());
        r = request(KIND_CUSTOMER_PROFILE);
        r.id = Some("party-1".into());
        assert!(validate_request(&r).is_ok());
        r = request(KIND_CUSTOMER_DIRECTORY);
        assert!(validate_request(&r).is_ok());
        r.cursor = Some("x".repeat(4097));
        assert!(validate_request(&r).is_err());
        r = request(KIND_NOTE_CUSTOMER);
        r.id = Some("party-1".into());
        assert!(validate_request(&r).is_ok());
        r = request("other");
        assert!(validate_request(&r).is_err());
    }

    #[test]
    fn native_lane_5_cursor_round_trip_and_scope_isolation() {
        let scope = scope_hash("company-1", "", "smit", "active");
        let encoded = encode_cursor("Smit", "party-1", false, &scope).unwrap();
        let decoded = decode_cursor(Some(&encoded), &scope).unwrap().unwrap();
        assert_eq!(decoded.key, "Smit");
        assert_eq!(decoded.id, "party-1");
        assert!(!decoded.reverse);
        assert_eq!(decoded.scope, scope);
        assert!(decode_cursor(None, &scope).unwrap().is_none());
        assert!(decode_cursor(Some(""), &scope).unwrap().is_none());
        assert!(decode_cursor(Some(&encoded), "other-scope").is_err());
        assert!(decode_cursor(Some(&"x".repeat(5000)), &scope).is_err());
    }

    #[test]
    fn native_lane_5_sql_oracles_are_byte_identical() {
        let oracle = [
            ("pricingCustomers", PRICING_CUSTOMERS_SQL, "e1476d8f12e47f331c5ce8dfe50ce29cc4552657d4975c4d3f23c6fbd0481df6"),
            ("pricingCustomerValid", PRICING_CUSTOMER_VALID_SQL, "04bbf6a2022aaf69a2d88424e831430d6308263d9b56890fda9942a0f206d391"),
            ("customerProfile", CUSTOMER_PROFILE_SQL, "cdde093d36fa76d24922ecac74dc6e2a6993aba8d42c72b7b30b9c71dbffff78"),
            ("customerDirectory", CUSTOMER_DIRECTORY_SQL, "047b48e0be125e3704d0990121ade0773b4aff840d83e1fb830a86733e9732a0"),
            ("noteCustomer", NOTE_CUSTOMER_SQL, "b7d66bcfdf8b5941db80ff2509a369f2d9d644ca0b3a98d0387cb77a527451c0"),
        ];
        for (kind, sql, expected) in oracle {
            assert!(sql.starts_with("SELECT "), "{kind}");
            assert!(sql.contains("finance_parties"), "{kind}");
            assert_eq!(hash(sql), expected, "{kind}");
        }
    }

    fn facts(kind: &'static str) -> RecordFactsRequest {
        RecordFactsRequest {
            kind: kind.to_owned(),
            id: Some("record-1".into()),
            chart_id: None,
            value: None,
            value2: None,
            value3: None,
            value4: None,
            value5: None,
            offset: None,
        }
    }

    #[test]
    fn native_lane_5_record_facts_wire_shape_is_snake_case() {
        let request: RecordFactsRequest = serde_json::from_value(serde_json::json!({
            "kind": RF_KIND_ROWS,
            "id": "record-1",
            "chart_id": "7",
            "value": "1",
            "value2": "2026-01-01",
            "value3": "2026-01-01",
            "value4": "2026-01-31",
            "value5": "2026-01-31",
            "offset": 30,
        }))
        .unwrap();
        assert_eq!(request.chart_id.as_deref(), Some("7"));
        assert_eq!(request.offset, Some(30));
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(value["chart_id"], "7");
        assert!(value.get("chartId").is_none());
    }

    #[test]
    fn native_lane_5_record_facts_validate_each_kind() {
        for kind in [RF_KIND_PARTY, RF_KIND_ACCOUNT, RF_KIND_PERIOD] {
            assert!(validate_record_facts(&facts(kind)).is_ok(), "{kind}");
        }
        let mut r = facts(RF_KIND_ROWS);
        assert!(validate_record_facts(&r).is_err());
        r.value = Some("0".into());
        assert!(validate_record_facts(&r).is_err());
        r.offset = Some(0);
        assert!(validate_record_facts(&r).is_ok());
        r.value = Some("1".into());
        r.value2 = Some("2026-01-01".into());
        assert!(validate_record_facts(&r).is_ok());
        r.offset = Some(-1);
        assert!(validate_record_facts(&r).is_err());
        r = facts("other");
        assert!(validate_record_facts(&r).is_err());
    }

    #[test]
    fn native_lane_5_record_facts_sql_oracles_are_byte_identical() {
        let oracle = [
            ("recordFactsParty", RF_PARTY_SQL, "79689b31d9cfbff8f1bda82066aac43ad5bb25e894c5409433976e4e8c62a797"),
            ("recordFactsAccount", RF_ACCOUNT_SQL, "96f24f09d9e1b31101e112a94f6c4e4fa53df19c6e2d7463aecdc7c9e64ee6ee"),
            ("recordFactsPeriod", RF_PERIOD_SQL, "d447a713437d5c3dc293c50cdb1835d31131b3b3c1f2f181d36e3d1b0bbece3b"),
            ("recordFactsRows", RF_ROWS_SQL, "cb40a97c5a78297b84e46acb663e5a0687dd09c99b63fed75e47439f05c1cefc"),
        ];
        for (kind, sql, expected) in oracle {
            assert!(sql.starts_with("SELECT "), "{kind}");
            assert_eq!(hash(sql), expected, "{kind}");
        }
        assert!(!RF_ROWS_SQL.contains("${"), "no TS interpolation survived");
        assert!(RF_ROWS_SQL.contains("fiscal_close"));
    }
}
