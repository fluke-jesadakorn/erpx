//! Pure request, validation and response types for the native Marketing
//! workspace candidate/attribution read family (lane 40). No SQL and no I/O
//! live here: the application layer owns validation and result invariants, the
//! postgres layer owns the statements, and the HTTP layer owns transport.
//!
//! Every admitted statement reads only Marketing-owned tables
//! (`marketing_lead_candidates`, `marketing_assets`, `marketing_asset_events`,
//! `marketing_engagement_events`). No owner's package, assignment or CRM table
//! is read here. Company scope arrives on a validated request header and is
//! rechecked against every returned row before a response is produced.
use erpx_platform_shared_server::public::BusinessError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const RD_CANDIDATE_LIST: &str = "candidate-list";
pub const RD_CANDIDATE_DETAIL: &str = "candidate-detail";
pub const RD_CANDIDATE_HISTORY: &str = "candidate-history";
pub const RD_CAMPAIGN_SUMMARY: &str = "campaign-summary";
// Wave 2: active-form and form-content reads on Marketing-owned assets.
pub const RD_ACTIVE_FORM_SOURCE: &str = "active-form-source-v2";
pub const RD_FORM_CONTENT_ROW: &str = "form-content-row-v2";
pub const RD_PUBLIC_RECEIPT: &str = "public-receipt-v2";
pub const KINDS: &[&str] = &[
    RD_CANDIDATE_LIST,
    RD_CANDIDATE_DETAIL,
    RD_CANDIDATE_HISTORY,
    RD_CAMPAIGN_SUMMARY,
    RD_ACTIVE_FORM_SOURCE,
    RD_FORM_CONTENT_ROW,
    RD_PUBLIC_RECEIPT,
];

/// Maximum length of any identifier accepted from the wire.
pub const MAX_ID: usize = 200;
pub const MAX_LIMIT: i64 = 5_000;
pub const UNSPECIFIED: &str = "Unspecified";

/// Wire envelope. Unknown fields are rejected so a caller cannot widen the
/// owner-local read contract; company scope is *not* accepted here.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Lane40ReadInput {
    pub kind: String,
    #[serde(default)]
    pub branch_id: Option<String>,
    #[serde(default)]
    pub department_id: Option<String>,
    #[serde(default)]
    pub record_id: Option<String>,
    /// Form / content asset id for the wave-2 active-form family.
    #[serde(default)]
    pub asset_id: Option<String>,
    #[serde(default)]
    pub version: Option<i64>,
    /// Trusted current enforcement mode resolved by the caller. It selects
    /// the retained form-provenance predicate; it never authorizes a row.
    #[serde(default)]
    pub enforced: bool,
    #[serde(default)]
    pub limit: Option<i64>,
}

/// Owned, validated request passed from HTTP to application to postgres.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Lane40Request {
    pub kind: &'static str,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub record_id: Option<String>,
    pub asset_id: Option<String>,
    pub version: Option<i64>,
    pub enforced: bool,
    pub limit: Option<i64>,
}

/// One attribution aggregate row. Field names match the retained TypeScript
/// projection (`captured`, `pending`, `handed_off`, `rejected`) byte for byte.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AttributionRow {
    pub company_id: String,
    pub campaign: String,
    pub source: String,
    pub captured: i64,
    pub pending: i64,
    pub handed_off: i64,
    pub rejected: i64,
}

/// Owner response. Every row carries the authorized `company_id`, including
/// aggregate rows, so one invariant checks company isolation on all kinds.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Lane40ReadResult {
    pub kind: &'static str,
    pub company_id: String,
    pub rows: Vec<Value>,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "MARKETING_LANE40_READ_REQUEST_INVALID",
        "Use one bounded Marketing lane-40 read.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        erpx_platform_shared_server::public::FailureKind::Unavailable,
        "MARKETING_LANE40_READ_UNAVAILABLE",
        "The native Marketing read service is unavailable.",
    )
}

pub fn missing() -> BusinessError {
    BusinessError::new(
        erpx_platform_shared_server::public::FailureKind::NotFound,
        "NOT_FOUND",
        "Not found.",
    )
}

pub fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "MARKETING_LANE40_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

/// Identifiers are public Marketing ids (not typed `RecordId`s). Restrict the
/// byte set so no bind can carry query syntax or a separator.
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

/// Build an owned request from transport facts. Company is supplied by the
/// header verifier, never by the body.
pub fn request(input: &Lane40ReadInput, company_id: &str) -> Result<Lane40Request, BusinessError> {
    let kind = KINDS
        .iter()
        .find(|candidate| **candidate == input.kind)
        .copied()
        .ok_or_else(invalid)?;
    if !valid_id(company_id) {
        return Err(invalid());
    }
    for value in [
        input.branch_id.as_deref(),
        input.department_id.as_deref(),
        input.record_id.as_deref(),
        input.asset_id.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if !valid_id(value) {
            return Err(invalid());
        }
    }
    let needs_record = matches!(
        kind,
        RD_CANDIDATE_DETAIL | RD_CANDIDATE_HISTORY | RD_PUBLIC_RECEIPT
    );
    if needs_record && input.record_id.is_none() {
        return Err(invalid());
    }
    let needs_asset = matches!(kind, RD_ACTIVE_FORM_SOURCE | RD_FORM_CONTENT_ROW);
    if needs_asset && (input.asset_id.is_none() || !input.version.is_some_and(|v| (1..=i64::MAX).contains(&v))) {
        return Err(invalid());
    }
    if input
        .limit
        .is_some_and(|limit| !(1..=MAX_LIMIT).contains(&limit))
    {
        return Err(invalid());
    }
    Ok(Lane40Request {
        kind,
        company_id: company_id.to_owned(),
        branch_id: input.branch_id.clone(),
        department_id: input.department_id.clone(),
        record_id: input.record_id.clone(),
        asset_id: input.asset_id.clone(),
        version: input.version,
        enforced: input.enforced,
        limit: input.limit,
    })
}

/// Rust owns the attribution calculation invariant after the SQL aggregation:
/// statuses always partition the captured candidate count.
pub fn check_attribution(row: &AttributionRow) -> Result<(), BusinessError> {
    if row.company_id.is_empty()
        || row.campaign.is_empty()
        || row.campaign.len() > 300
        || row.source.is_empty()
        || row.source.len() > 300
        || row.captured < 1
        || row.pending < 0
        || row.handed_off < 0
        || row.rejected < 0
        || row
            .pending
            .checked_add(row.handed_off)
            .and_then(|value| value.checked_add(row.rejected))
            != Some(row.captured)
    {
        return Err(unavailable());
    }
    Ok(())
}

/// The wave-2 active-form projection invariant: a form source row carries its
/// configuration text, and a content row carries its id, version and preview.
pub fn check_form_row(kind: &'static str, row: &Value) -> Result<(), BusinessError> {
    if !row
        .get("company_id")
        .and_then(Value::as_str)
        .is_some_and(valid_id)
    {
        return Err(unavailable());
    }
    if kind == RD_ACTIVE_FORM_SOURCE
        && !row
            .get("configuration_json")
            .is_some_and(Value::is_string)
    {
        return Err(unavailable());
    }
    if kind == RD_FORM_CONTENT_ROW
        && !(row.get("id").is_some_and(Value::is_string)
            && row.get("version").is_some_and(Value::is_i64)
            && row.get("preview_json").is_some_and(Value::is_string))
    {
        return Err(unavailable());
    }
    Ok(())
}

/// The public-receipt projection invariant: a receipt carries a SHA-256 request
/// hash and a candidate id from the selected Company.
pub fn check_receipt_row(row: &Value) -> Result<(), BusinessError> {
    if !row
        .get("company_id")
        .and_then(Value::as_str)
        .is_some_and(valid_id)
        || !row
            .get("request_hash")
            .and_then(Value::as_str)
            .is_some_and(|value| value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()))
        || !row
            .get("candidate_id")
            .and_then(Value::as_str)
            .is_some_and(valid_id)
    {
        return Err(unavailable());
    }
    Ok(())
}

/// The final authorization-safe company scope check. Aggregate rows carry a
/// `company_id` too, so no row can escape the selected Company.
pub fn check_result(company_id: &str, result: &Lane40ReadResult) -> Result<(), BusinessError> {
    if result.company_id != company_id {
        return Err(unavailable());
    }
    for row in &result.rows {
        match row.get("company_id").and_then(Value::as_str) {
            Some(value) if value == company_id => {}
            _ => return Err(unavailable()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(kind: &str) -> Lane40ReadInput {
        Lane40ReadInput {
            kind: kind.to_owned(),
            branch_id: None,
            department_id: None,
            record_id: None,
            asset_id: Some("asset-1".into()),
            version: Some(1),
            enforced: false,
            limit: None,
        }
    }

    #[test]
    fn closed_kinds_and_bounded_identifiers() {
        assert_eq!(KINDS.len(), 7);
        assert!(valid_id("marketing-candidate-1"));
        assert!(!valid_id(""));
        assert!(!valid_id(&"x".repeat(MAX_ID + 1)));
        assert!(!valid_id("id; DROP TABLE marketing_assets"));
        assert!(request(&input(RD_CANDIDATE_LIST), "company-1").is_ok());
        assert_eq!(
            request(&input("bogus"), "company-1").unwrap_err().code,
            "MARKETING_LANE40_READ_REQUEST_INVALID"
        );
        assert!(request(&input(RD_CANDIDATE_LIST), "").is_err());
    }

    #[test]
    fn detail_and_history_require_a_record_id() {
        assert!(request(&input(RD_CANDIDATE_DETAIL), "company-1").is_err());
        assert!(request(&input(RD_CANDIDATE_HISTORY), "company-1").is_err());
        let mut detail = input(RD_CANDIDATE_DETAIL);
        detail.record_id = Some("marketing-candidate-1".into());
        assert_eq!(
            request(&detail, "company-1").unwrap().record_id.as_deref(),
            Some("marketing-candidate-1")
        );
        let mut list = input(RD_CANDIDATE_LIST);
        list.limit = Some(MAX_LIMIT + 1);
        assert!(request(&list, "company-1").is_err());
    }

    #[test]
    fn result_oracle_pins_attribution_shape_and_company_scope() {
        let row = AttributionRow {
            company_id: "company-1".into(),
            campaign: UNSPECIFIED.into(),
            source: UNSPECIFIED.into(),
            captured: 4,
            pending: 1,
            handed_off: 2,
            rejected: 1,
        };
        check_attribution(&row).unwrap();
        let value = serde_json::to_value(&row).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "company_id": "company-1",
                "campaign": "Unspecified",
                "source": "Unspecified",
                "captured": 4,
                "pending": 1,
                "handed_off": 2,
                "rejected": 1,
            })
        );
        let result = Lane40ReadResult {
            kind: RD_CAMPAIGN_SUMMARY,
            company_id: "company-1".into(),
            rows: vec![value],
        };
        check_result("company-1", &result).unwrap();
        assert_eq!(
            check_result("company-2", &result).unwrap_err().code,
            "MARKETING_LANE40_READ_UNAVAILABLE"
        );
        let mut wrong = row.clone();
        wrong.pending = 2;
        assert_eq!(
            check_attribution(&wrong).unwrap_err().code,
            "MARKETING_LANE40_READ_UNAVAILABLE"
        );
    }
}
