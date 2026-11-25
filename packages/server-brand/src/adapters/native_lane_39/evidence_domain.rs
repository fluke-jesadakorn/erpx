//! Lane 39 Brand evidence domain: pure request/response types and invariants
//! for the owner-executed executive Brand profile evidence search.

use erpx_server_shared::public::BusinessError;

pub const MAX_ID_LEN: usize = 200;
pub const MAX_QUERY_LEN: usize = 100;
pub const MAX_CURSOR_LEN: usize = 2048;
/// The public search input caps a page at 50 rows; the retained caller adds a
/// one-row lookahead to derive `hasMore`, so the owner accepts up to 51.
pub const MAX_LIMIT: i64 = 51;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceCursor {
    pub branch_name: String,
    pub id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceSearchRequest {
    pub company_id: String,
    pub branch_id: Option<String>,
    pub from: Option<i64>,
    pub to_exclusive: Option<i64>,
    pub query: Option<String>,
    pub cursor: Option<EvidenceCursor>,
    pub limit: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrandProfileRow {
    pub id: String,
    pub branch_name: String,
    pub document_mode: String,
    pub updated_at: i64,
    pub primary_attachment_id: Option<String>,
    pub inverse_attachment_id: Option<String>,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_LEN
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

/// Validate the declared predicate families before any bind is rendered.
pub fn validate(request: &EvidenceSearchRequest) -> Result<(), BusinessError> {
    if !valid_id(&request.company_id)
        || !request.branch_id.as_deref().map(valid_id).unwrap_or(true)
        || request.limit < 1
        || request.limit > MAX_LIMIT
        || (request.from.is_some() != request.to_exclusive.is_some())
        || matches!((request.from, request.to_exclusive), (Some(from), Some(to)) if from >= to)
        || request.query.as_deref().map(|q| q.len() > MAX_QUERY_LEN).unwrap_or(false)
        || request.cursor.as_ref().map(|cursor| {
            cursor.branch_name.is_empty()
                || cursor.branch_name.len() > MAX_CURSOR_LEN
                || !valid_id(&cursor.id)
        }).unwrap_or(false)
    {
        return Err(invalid(
            "BRAND_EVIDENCE_SEARCH_REQUEST_INVALID",
            "Use one bounded Brand evidence search.",
        ));
    }
    Ok(())
}

/// Refuse drifted rows instead of returning a partial evidence page.
pub fn project(rows: Vec<BrandProfileRow>) -> Result<Vec<BrandProfileRow>, BusinessError> {
    if rows.len() > MAX_LIMIT.saturating_mul(2) as usize {
        return Err(invalid(
            "BRAND_EVIDENCE_SEARCH_RESPONSE_INVALID",
            "The native Brand evidence search returned too many rows.",
        ));
    }
    for row in &rows {
        if !valid_id(&row.id)
            || row.branch_name.is_empty()
            || row.branch_name.len() > MAX_CURSOR_LEN
            || !matches!(row.document_mode.as_str(), "holding" | "branch" | "both")
            || !row.primary_attachment_id.as_deref().map(valid_id).unwrap_or(true)
            || !row.inverse_attachment_id.as_deref().map(valid_id).unwrap_or(true)
        {
            return Err(invalid(
                "BRAND_EVIDENCE_SEARCH_RESPONSE_INVALID",
                "The native Brand evidence search returned an invalid row.",
            ));
        }
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> EvidenceSearchRequest {
        EvidenceSearchRequest {
            company_id: "co-1".into(),
            branch_id: None,
            from: None,
            to_exclusive: None,
            query: None,
            cursor: None,
            limit: 25,
        }
    }

    #[test]
    fn validation_accepts_a_bounded_search_and_refuses_drift() {
        assert!(validate(&request()).is_ok());
        let mut bad = request();
        bad.limit = 52;
        assert_eq!(validate(&bad).unwrap_err().code, "BRAND_EVIDENCE_SEARCH_REQUEST_INVALID");
        let mut range = request();
        range.from = Some(2);
        assert!(validate(&range).is_err());
        let mut query = request();
        query.query = Some("x".repeat(MAX_QUERY_LEN + 1));
        assert!(validate(&query).is_err());
    }

    #[test]
    fn projection_refuses_unknown_document_modes() {
        let mut row = BrandProfileRow {
            id: "profile-1".into(),
            branch_name: "North".into(),
            document_mode: "branch".into(),
            updated_at: 1,
            primary_attachment_id: None,
            inverse_attachment_id: None,
        };
        assert_eq!(project(vec![row.clone()]).unwrap().len(), 1);
        row.document_mode = "unknown".into();
        assert_eq!(project(vec![row]).unwrap_err().code, "BRAND_EVIDENCE_SEARCH_RESPONSE_INVALID");
    }
}
