//! Pure domain for the native Files commercial-usage read family.
//!
//! The owner-local successor of the four SELECTs in
//! `adapters/commercial-usage-facts.ts`. This module owns the request contract,
//! the response shape and the invariants. It contains no SQL and no I/O.

use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const KIND_STORED_ORIGINALS: &str = "stored_originals";
pub const KIND_UNRECONCILED_ORIGINALS: &str = "unreconciled_originals";
pub const KIND_RECONCILIATION: &str = "reconciliation";
pub const KIND_EARLIEST_ORIGINAL: &str = "earliest_original";

/// The closed set of reads this native family serves. No other discriminator
/// crosses the wire.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageReadKind {
    StoredOriginals,
    UnreconciledOriginals,
    Reconciliation,
    EarliestOriginal,
}

impl UsageReadKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StoredOriginals => KIND_STORED_ORIGINALS,
            Self::UnreconciledOriginals => KIND_UNRECONCILED_ORIGINALS,
            Self::Reconciliation => KIND_RECONCILIATION,
            Self::EarliestOriginal => KIND_EARLIEST_ORIGINAL,
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            KIND_STORED_ORIGINALS => Some(Self::StoredOriginals),
            KIND_UNRECONCILED_ORIGINALS => Some(Self::UnreconciledOriginals),
            KIND_RECONCILIATION => Some(Self::Reconciliation),
            KIND_EARLIEST_ORIGINAL => Some(Self::EarliestOriginal),
            _ => None,
        }
    }
}

/// Bounded request. The first `SqlDatabase` argument of the retired TS helpers
/// is intentionally absent: the native owner binds against its own holding
/// pool, so the transport supplies only the read discriminator and parameters.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct UsageReadRequest {
    pub kind: String,
    #[serde(default)]
    pub start: Option<i64>,
    #[serde(default)]
    pub end: Option<i64>,
    #[serde(default)]
    pub branch_id: Option<String>,
}

/// A storage-ledger row. Keys are intentionally snake_case: the retained
/// TypeScript consumer forwards these rows unchanged to Commercial usage.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct StorageRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub bytes: i64,
    pub created_at: i64,
    pub destroyed_at: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct UnreconciledFacts {
    pub count: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ReconciliationFacts {
    pub ready: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct EarliestFacts {
    pub earliest: Option<i64>,
}

/// Strict, discriminator-tagged projection. A client must validate the tag and
/// exactly one payload field before accepting the response.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UsageFacts {
    StoredOriginals { rows: Vec<StorageRow> },
    UnreconciledOriginals { count: i32 },
    Reconciliation { ready: bool },
    EarliestOriginal { earliest: Option<i64> },
}

impl UsageFacts {
    pub fn kind(&self) -> UsageReadKind {
        match self {
            Self::StoredOriginals { .. } => UsageReadKind::StoredOriginals,
            Self::UnreconciledOriginals { .. } => UsageReadKind::UnreconciledOriginals,
            Self::Reconciliation { .. } => UsageReadKind::Reconciliation,
            Self::EarliestOriginal { .. } => UsageReadKind::EarliestOriginal,
        }
    }
}

/// Response envelope. `witness` binds the company scope, the exact request and
/// the rendered facts into one auditable hash without exposing SQL to the
/// caller.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct UsageResponse {
    pub data: UsageFacts,
    pub witness: String,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "FILES_COMMERCIAL_USAGE_READ_INVALID",
        "Use one bounded Files commercial-usage read.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FILES_COMMERCIAL_USAGE_READ_UNAVAILABLE",
        "The native Files commercial-usage service is unavailable.",
    )
}

pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// Identifier validation mirrors the owner router contract: a bounded,
/// canonical record id restricted to a conservative wire charset.
pub fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

fn valid_timestamp(value: Option<i64>) -> bool {
    value.is_some_and(|v| (0..=9_007_199_254_740_991).contains(&v))
}

/// Validate one request and resolve its registered kind. No parameter is
/// accepted for a read that does not declare it, which pins the request shape.
pub fn validate_request(request: &UsageReadRequest) -> Result<UsageReadKind, BusinessError> {
    let kind = UsageReadKind::parse(&request.kind).ok_or_else(invalid)?;
    let no_params = request.start.is_none() && request.end.is_none() && request.branch_id.is_none();
    let ok = match kind {
        UsageReadKind::StoredOriginals => {
            valid_timestamp(request.start)
                && valid_timestamp(request.end)
                && request.branch_id.as_deref().is_none_or(|v| valid_id(v))
        }
        UsageReadKind::UnreconciledOriginals => {
            valid_timestamp(request.end) && request.branch_id.as_deref().is_none_or(|v| valid_id(v))
        }
        UsageReadKind::Reconciliation | UsageReadKind::EarliestOriginal => no_params,
    };
    if ok { Ok(kind) } else { Err(invalid()) }
}

/// SHA-256 of the canonical `[company, kind, start, end, branch_id, facts]`
/// tuple. This is the result oracle a contract test can pin without a database.
pub fn response_witness(
    company: Option<&str>,
    request: &UsageReadRequest,
    facts: &UsageFacts,
) -> String {
    let canonical = serde_json::json!([
        company,
        request.kind,
        request.start,
        request.end,
        request.branch_id,
        facts
    ]);
    let raw = serde_json::to_string(&canonical).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hex::encode(hasher.finalize())
}

// --- Finance tax-signature attachment reads -------------------------------
//
// Owner-local successor of the two SELECTs in `adapters/finance-tax-signatures.ts`.
// The guard statement in that file stays TypeScript: it is a write assertion
// executed inside the caller's command transaction, not a read.

pub const TAX_METADATA: &str = "metadata";
pub const TAX_WORKSPACE: &str = "workspace";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaxSignatureReadKind {
    Metadata,
    Workspace,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct TaxSignatureRequest {
    pub kind: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub link_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct TaxSignatureMetadata {
    pub checksum_sha256: String,
    pub media_type: String,
    pub bytes: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct TaxSignatureRow {
    pub id: String,
    pub target_id: String,
    pub original_name: String,
    pub checksum_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaxSignatureFacts {
    Metadata {
        metadata: Option<TaxSignatureMetadata>,
    },
    Workspace {
        rows: Vec<TaxSignatureRow>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct TaxSignatureResponse {
    pub data: TaxSignatureFacts,
    pub witness: String,
}

pub fn tax_invalid() -> BusinessError {
    BusinessError::invalid(
        "FILES_TAX_SIGNATURE_READ_INVALID",
        "Use one bounded Files tax-signature read.",
    )
}

pub fn tax_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FILES_TAX_SIGNATURE_READ_UNAVAILABLE",
        "The native Files tax-signature service is unavailable.",
    )
}

/// These reads are company-scoped: the company id is the caller authority and
/// is also a predicate bound by the owner's SQL.
pub fn validate_tax_request(
    request: &TaxSignatureRequest,
) -> Result<TaxSignatureReadKind, BusinessError> {
    let kind = match request.kind.as_str() {
        TAX_METADATA => TaxSignatureReadKind::Metadata,
        TAX_WORKSPACE => TaxSignatureReadKind::Workspace,
        _ => return Err(tax_invalid()),
    };
    let ok = match kind {
        TaxSignatureReadKind::Metadata => {
            request.id.as_deref().is_some_and(valid_id)
                && request.link_id.as_deref().is_some_and(valid_id)
        }
        TaxSignatureReadKind::Workspace => request.id.is_none() && request.link_id.is_none(),
    };
    if ok { Ok(kind) } else { Err(tax_invalid()) }
}

pub fn tax_response_witness(
    company: &str,
    request: &TaxSignatureRequest,
    facts: &TaxSignatureFacts,
) -> String {
    let canonical = serde_json::json!([company, request.id, request.link_id, facts]);
    let raw = serde_json::to_string(&canonical).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hex::encode(hasher.finalize())
}

// --- Finance review-file attachment list -----------------------------------
//
// Owner-local successor of the single SELECT in
// `adapters/finance-review-file-sources.ts`. The two guard assertions in that
// file stay TypeScript: they execute in the caller's command transaction.

pub const REVIEW_FILES: &str = "review_files";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ReviewFilesRequest {
    pub kind: String,
    pub target_type: String,
    pub target_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ReviewFileRow {
    pub link_id: String,
    pub source_id: Option<String>,
    pub link_state: String,
    pub set_state: String,
    pub purpose_key: String,
    pub checksum_sha256: Option<String>,
    pub media_type: Option<String>,
    pub bytes: Option<i64>,
    pub source_state: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReviewFilesFacts {
    ReviewFiles { rows: Vec<ReviewFileRow> },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ReviewFilesResponse {
    pub data: ReviewFilesFacts,
    pub witness: String,
}

pub fn review_invalid() -> BusinessError {
    BusinessError::invalid(
        "FILES_REVIEW_FILE_READ_INVALID",
        "Use one bounded Files review-file read.",
    )
}

pub fn review_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FILES_REVIEW_FILE_READ_UNAVAILABLE",
        "The native Files review-file service is unavailable.",
    )
}

pub fn validate_review_request(request: &ReviewFilesRequest) -> Result<(), BusinessError> {
    if request.kind == REVIEW_FILES
        && valid_id(&request.target_type)
        && valid_id(&request.target_id)
    {
        Ok(())
    } else {
        Err(review_invalid())
    }
}

pub fn review_response_witness(
    company: &str,
    request: &ReviewFilesRequest,
    facts: &ReviewFilesFacts,
) -> String {
    let canonical = serde_json::json!([company, request.target_type, request.target_id, facts]);
    let raw = serde_json::to_string(&canonical).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(kind: &str) -> UsageReadRequest {
        UsageReadRequest {
            kind: kind.to_owned(),
            start: None,
            end: None,
            branch_id: None,
        }
    }

    #[test]
    fn validates_registered_kinds_and_parameters() {
        let mut stored = req(KIND_STORED_ORIGINALS);
        assert_eq!(validate_request(&stored), Err(invalid()));
        stored.start = Some(1);
        stored.end = Some(9);
        stored.branch_id = Some("branch-1".to_owned());
        assert_eq!(
            validate_request(&stored),
            Ok(UsageReadKind::StoredOriginals)
        );

        let mut unreconciled = req(KIND_UNRECONCILED_ORIGINALS);
        unreconciled.end = Some(9);
        assert_eq!(
            validate_request(&unreconciled),
            Ok(UsageReadKind::UnreconciledOriginals)
        );

        assert_eq!(
            validate_request(&req(KIND_RECONCILIATION)),
            Ok(UsageReadKind::Reconciliation)
        );
        assert_eq!(
            validate_request(&req(KIND_EARLIEST_ORIGINAL)),
            Ok(UsageReadKind::EarliestOriginal)
        );
        assert_eq!(validate_request(&req("other")), Err(invalid()));
    }

    #[test]
    fn rejects_overloaded_and_malformed_parameters() {
        let mut reconciliation = req(KIND_RECONCILIATION);
        reconciliation.end = Some(1);
        assert_eq!(validate_request(&reconciliation), Err(invalid()));

        let mut stored = req(KIND_STORED_ORIGINALS);
        stored.start = Some(1);
        stored.end = Some(2);
        stored.branch_id = Some("bad branch".to_owned());
        assert_eq!(validate_request(&stored), Err(invalid()));
        assert!(!valid_id("bad branch"));
        assert!(valid_slug("acme-1"));
        assert!(!valid_slug("Acme"));
    }

    #[test]
    fn pins_result_oracle() {
        let mut request = req(KIND_RECONCILIATION);
        request.kind = KIND_EARLIEST_ORIGINAL.to_owned();
        let facts = UsageFacts::EarliestOriginal {
            earliest: Some(1_700_000_000_000),
        };
        let witness = response_witness(Some("company-1"), &request, &facts);
        assert_eq!(
            witness,
            "710a81c8a3c7464127b1d18ac9d7e90022c09edfc9407d9510b1c84955c9bf04"
        );
        assert_eq!(witness.len(), 64);
        assert!(witness.bytes().all(|b| b.is_ascii_hexdigit()));
        // Stable across processes; changing scope or facts changes the oracle.
        assert_eq!(
            witness,
            response_witness(Some("company-1"), &request, &facts)
        );
        assert_ne!(
            witness,
            response_witness(Some("company-2"), &request, &facts)
        );
        // A holding-wide read has no company scope and a distinct oracle.
        let holding_wide = response_witness(None, &request, &facts);
        assert_eq!(holding_wide.len(), 64);
        assert_ne!(witness, holding_wide);
    }

    #[test]
    fn validates_tax_requests_and_pins_witness() {
        let mut metadata = TaxSignatureRequest {
            kind: TAX_METADATA.to_owned(),
            id: Some("identity-1".to_owned()),
            link_id: Some("link-1".to_owned()),
        };
        assert_eq!(
            validate_tax_request(&metadata),
            Ok(TaxSignatureReadKind::Metadata)
        );
        metadata.link_id = None;
        assert_eq!(validate_tax_request(&metadata), Err(tax_invalid()));
        let workspace = TaxSignatureRequest {
            kind: TAX_WORKSPACE.to_owned(),
            id: None,
            link_id: None,
        };
        assert_eq!(
            validate_tax_request(&workspace),
            Ok(TaxSignatureReadKind::Workspace)
        );

        let facts = TaxSignatureFacts::Workspace { rows: Vec::new() };
        let witness = tax_response_witness("company-1", &workspace, &facts);
        assert_eq!(
            witness,
            "a87824938e173f7c2af1fc74b342ef281d112558fddf045a4851975f4dc9f068"
        );
        assert_eq!(witness.len(), 64);
        assert_eq!(
            witness,
            tax_response_witness("company-1", &workspace, &facts)
        );
        assert_ne!(
            witness,
            tax_response_witness("company-2", &workspace, &facts)
        );

        metadata.link_id = Some("link-1".to_owned());
        let facts = TaxSignatureFacts::Metadata { metadata: None };
        assert_eq!(
            tax_response_witness("company-1", &metadata, &facts),
            "e281495c77041b2e1e48d5d525d7397152e36cfc5ef38f7c60ca803f7fe650ec"
        );
    }
}
