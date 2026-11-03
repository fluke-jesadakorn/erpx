//! Pure Field evidence request, cursor and wire identity policy.
use base64::{
    Engine, alphabet,
    engine::{GeneralPurpose, GeneralPurposeConfig},
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
pub const FIELDS: &[&str] = &["scheduled_at", "visit_completed_at", "status", "version"];
pub const PROVIDER: &str = "field-service-orders";
pub const METRIC: &str = "field-visit-completion";
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceFilters {
    #[serde(default = "from_default")]
    pub from: String,
    #[serde(default = "to_default")]
    pub to: String,
    #[serde(default = "currency_default")]
    pub currency: String,
}
pub fn from_default() -> String {
    "2026-01-01".into()
}
fn to_default() -> String {
    "2026-12-31".into()
}
fn currency_default() -> String {
    "THB".into()
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldEvidenceRequest {
    pub metric: String,
    pub filters: EvidenceFilters,
    #[serde(default)]
    pub query: String,
    #[serde(default, deserialize_with = "optional_string")]
    pub cursor: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: Value,
    pub record_id: Option<String>,
}
#[derive(Clone, Copy)]
pub enum EvidenceAction {
    Search,
    Open,
}
pub fn invalid() -> BusinessError {
    BusinessError::invalid("VALIDATION_FAILED", "Review the Field evidence request.")
}
pub fn missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "EVIDENCE_NOT_FOUND",
        "This source record is unavailable in the selected role, scope or filters.",
    )
}
pub fn trim(v: &str) -> &str {
    v.trim_matches(|c:char|matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}'))
}
pub fn date(v: &str) -> Result<i64, BusinessError> {
    if v.len() != 10 {
        return Err(invalid());
    }
    let format = time::format_description::parse_borrowed::<2>("[year]-[month]-[day]").map_err(|_| invalid())?;
    let date = time::Date::parse(v, &format).map_err(|_| invalid())?;
    Ok(date.midnight().assume_utc().unix_timestamp() * 1000)
}
fn optional_string<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    String::deserialize(d).map(Some)
}
fn default_limit() -> Value {
    Value::from(25)
}
fn numeric_string(v: &str) -> Result<f64, BusinessError> {
    let s = trim(v);
    if s.is_empty() {
        return Ok(0.0);
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0b", 2),
        ("0B", 2),
        ("0o", 8),
        ("0O", 8),
    ] {
        if let Some(s) = s.strip_prefix(prefix) {
            return u64::from_str_radix(s, radix)
                .map(|n| n as f64)
                .map_err(|_| invalid());
        }
    }
    s.parse::<f64>().map_err(|_| invalid())
}
fn array_string(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Array(items) => items.iter().map(array_string).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".into(),
        other => other.to_string(),
    }
}
pub fn limit(value: &Value) -> Result<usize, BusinessError> {
    let n = match value {
        Value::Number(v) => v.as_f64().ok_or_else(invalid)?,
        Value::String(v) => numeric_string(v)?,
        Value::Array(_) => numeric_string(&array_string(value))?,
        Value::Bool(v) => {
            if *v {
                1.0
            } else {
                0.0
            }
        }
        Value::Null => 0.0,
        _ => return Err(invalid()),
    };
    if !n.is_finite() || n.fract() != 0.0 || !(1.0..=50.0).contains(&n) {
        return Err(invalid());
    }
    Ok(n as usize)
}
pub fn cursor(v: Option<&str>) -> Result<Option<(i64, String)>, BusinessError> {
    let Some(v) = v.filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let fail = || {
        BusinessError::invalid(
            "EVIDENCE_CURSOR_INVALID",
            "The source record cursor is invalid. Refresh the evidence list.",
        )
    };
    if v.encode_utf16().count() > 2000 {
        return Err(invalid());
    }
    let mut encoded = v
        .split('=')
        .next()
        .unwrap_or("")
        .bytes()
        .filter(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'+' | b'/'))
        .map(|b| match b {
            b'+' => b'-',
            b'/' => b'_',
            other => other,
        })
        .collect::<Vec<_>>();
    if encoded.len() % 4 == 1 {
        encoded.pop();
    }
    let codec = GeneralPurpose::new(
        &alphabet::URL_SAFE,
        GeneralPurposeConfig::new()
            .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent)
            .with_decode_allow_trailing_bits(true),
    );
    let bytes = codec.decode(encoded).map_err(|_| fail())?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| fail())?;
    let at = value["scheduledAt"]
        .as_f64()
        .filter(|v| v.is_finite() && v.fract() == 0.0 && v.abs() <= 9_007_199_254_740_991.0)
        .ok_or_else(fail)? as i64;
    let id = value["id"]
        .as_str()
        .filter(|v| !v.is_empty() && v.encode_utf16().count() <= 200)
        .ok_or_else(fail)?;
    Ok(Some((at, id.into())))
}
pub fn encode(id: &str) -> String {
    id.bytes()
        .map(|b| match b {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'~'
            | b'!'
            | b'\''
            | b'('
            | b')'
            | b'*' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}
pub fn hash(value: String) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHash<'a> {
    pub provider_id: &'a str,
    pub metric: &'a str,
    pub filters: &'a EvidenceFilters,
    pub query: &'a str,
    pub cursor: Option<&'a str>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenHash<'a> {
    pub provider_id: &'a str,
    pub metric: &'a str,
    pub filters: &'a EvidenceFilters,
    pub record_id: &'a str,
}
