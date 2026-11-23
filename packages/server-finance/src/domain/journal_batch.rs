//! Owner-side batch-post invariants. The retained issued rules are source
//! admission facts only; every effect stays inside the caller's serializable
//! Finance unit and is finalized by its authoritative ports.
use std::collections::BTreeSet;

use erpx_server_shared::public::FailureKind;
use serde::{Deserialize, Serialize};

use super::journal::JournalError;
use super::journal_review::{JournalLedgerLine, review_error};

pub const MAX_JOURNALS: usize = 100;
pub const MAX_JOURNAL_ID_UNITS: usize = 128;
pub const MAX_LINES: usize = 1_000;
pub const MIN_REASON_UNITS: usize = 10;
pub const MAX_REASON_UNITS: usize = 2_000;
pub const SAFE_INTEGER_LIMIT: i64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct BatchJournalRef {
    pub id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub expected_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct JournalBatchInput {
    pub journals: Vec<BatchJournalRef>,
    pub reason: String,
}

/// The protected current source snapshot and its actual retained ledger rows.
/// Ports must project this from the same SERIALIZABLE unit that applies effects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalBatchSnapshot {
    pub id: String,
    pub company_id: String,
    pub chart_id: String,
    pub period_id: String,
    pub currency: String,
    pub status: String,
    pub version: i64,
    pub source_type: String,
    pub lines: Vec<JournalLedgerLine>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct PostedJournal {
    pub id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct JournalBatchResult {
    pub id: String,
    pub status: String,
    pub journals: Vec<PostedJournal>,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 0))]
    pub line_count: i64,
}

fn units(value: &str) -> usize {
    value.encode_utf16().count()
}

/// Reject a malformed batch before any source lock or effect. The wire applies
/// the same bounds; the owner never trusts a transport to have done so.
pub fn validate_input(input: &JournalBatchInput) -> Result<(), JournalError> {
    if input.journals.is_empty() || input.journals.len() > MAX_JOURNALS {
        return Err(batch_error(
            FailureKind::InvalidInput,
            "BATCH_SIZE_INVALID",
            "Select between one and one hundred journals.",
        ));
    }
    let mut seen = BTreeSet::new();
    for item in &input.journals {
        if units(&item.id) == 0 || units(&item.id) > MAX_JOURNAL_ID_UNITS {
            return Err(batch_error(
                FailureKind::InvalidInput,
                "BATCH_JOURNAL_ID_INVALID",
                "A selected journal id is malformed.",
            ));
        }
        if item.expected_version < 1 || item.expected_version >= SAFE_INTEGER_LIMIT {
            return Err(batch_error(
                FailureKind::InvalidInput,
                "BATCH_EXPECTED_VERSION_INVALID",
                "A selected expected version is malformed.",
            ));
        }
        if !seen.insert(item.id.as_str()) {
            return Err(batch_error(
                FailureKind::InvalidInput,
                "DUPLICATE_JOURNAL",
                "Select each draft once.",
            ));
        }
    }
    let reason = input.reason.trim();
    if units(reason) < MIN_REASON_UNITS || units(reason) > MAX_REASON_UNITS {
        return Err(batch_error(
            FailureKind::InvalidInput,
            "BATCH_REASON_INVALID",
            "A batch reason of 10 to 2000 characters is required.",
        ));
    }
    Ok(())
}

/// One selection must still be an owned, unchanged, single-scope draft. Scope
/// is anchored on the first admitted journal; labels and request data are not
/// authority for the remaining items.
pub fn require_journal(
    snapshot: &JournalBatchSnapshot,
    item: &BatchJournalRef,
    first: Option<&JournalBatchSnapshot>,
) -> Result<(), JournalError> {
    if snapshot.id != item.id {
        return Err(changed());
    }
    if !matches!(snapshot.source_type.as_str(), "manual" | "opening_import") {
        return Err(batch_error(
            FailureKind::Conflict,
            "BATCH_SOURCE_ADAPTER_REQUIRED",
            "Review source-owned drafts through their module until its batch adapter validates source versions and approved rules.",
        ));
    }
    if snapshot.version != item.expected_version
        || !matches!(snapshot.status.as_str(), "draft" | "in_review")
    {
        return Err(batch_error(
            FailureKind::Conflict,
            "VERSION_CONFLICT",
            "A selected draft changed. Review the entire batch again.",
        ));
    }
    if let Some(first) = first
        && (snapshot.chart_id != first.chart_id
            || snapshot.period_id != first.period_id
            || snapshot.currency != first.currency)
    {
        return Err(batch_error(
            FailureKind::InvalidInput,
            "BATCH_SCOPE_MISMATCH",
            "A batch requires one Company, ledger, period and transaction currency.",
        ));
    }
    Ok(())
}

pub fn enforce_line_limit(total: usize) -> Result<(), JournalError> {
    if total > MAX_LINES {
        return Err(batch_error(
            FailureKind::InvalidInput,
            "BATCH_LINE_LIMIT",
            "A batch may contain at most 1,000 lines.",
        ));
    }
    Ok(())
}

/// Canonical ordered projection of the retained ledger rows. The PG adapter
/// stages the same projection as a transaction-local command assertion, so any
/// concurrent or intra-unit mutation fails closed before the receipt is kept.
pub fn line_fingerprint(lines: &[JournalLedgerLine]) -> String {
    let rows: Vec<serde_json::Value> = lines
        .iter()
        .map(|l| {
            serde_json::json!([
                l.id,
                l.account_id,
                l.debit_minor,
                l.credit_minor,
                l.base_debit_minor,
                l.base_credit_minor,
                l.dimension_json,
                l.ordinal,
            ])
        })
        .collect();
    serde_json::Value::Array(rows).to_string()
}

pub fn changed() -> JournalError {
    batch_error(
        FailureKind::Conflict,
        "JOURNAL_BATCH_SOURCE_CHANGED",
        "The protected journals, ledger lines or retained Finance authority changed.",
    )
}

fn batch_error(kind: FailureKind, code: &'static str, message: &'static str) -> JournalError {
    review_error(kind, code, message)
}
