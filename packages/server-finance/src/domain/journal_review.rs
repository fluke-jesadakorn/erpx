//! Retained journal decisions. These values are source facts, never work grants.
use erpx_server_shared::public::FailureKind;
use num_bigint::BigInt;
use serde::{Deserialize, Serialize};

use super::journal::JournalError;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewJournal {
    pub id: String,
    pub company_id: String,
    pub chart_id: String,
    pub period_id: String,
    pub journal_number: String,
    pub entry_date: String,
    pub currency: String,
    pub status: String,
    pub version: i64,
    pub created_by: String,
    pub source_type: String,
    pub source_id: Option<String>,
}

/// Must be projected from current sealed Identity authority by the owner port.
/// In particular, `same_person` includes every current same-Person user alias.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JournalReviewerFacts {
    pub can_read: bool,
    pub can_approve: bool,
    pub same_person: bool,
}

pub fn require_post_reviewer(facts: JournalReviewerFacts) -> Result<(), JournalError> {
    if facts.same_person || !facts.can_approve {
        return Err(review_error(
            FailureKind::Forbidden,
            "INDEPENDENT_APPROVER_REQUIRED",
            "A different finance approver must post a manual journal.",
        ));
    }
    Ok(())
}

pub fn require_reversal_reviewer(facts: JournalReviewerFacts) -> Result<(), JournalError> {
    if !facts.can_read {
        return Err(review_error(
            FailureKind::Forbidden,
            "FINANCE_FORBIDDEN",
            "A finance assignment is required.",
        ));
    }
    if !facts.can_approve {
        return Err(review_error(
            FailureKind::Forbidden,
            "FINANCE_APPROVAL_REQUIRED",
            "An independent finance approver is required.",
        ));
    }
    Ok(())
}

/// The repository supplies these facts from the protected same-Company source.
/// Preserve the B0 decision order; Money ancestry is validated separately.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalReversalFacts {
    pub opening_allocated: bool,
    pub source_opening: bool,
    pub root_source_type: Option<String>,
}

pub fn review_journal_reversal(
    journal: &ReviewJournal,
    entry_date: &str,
    facts: &JournalReversalFacts,
) -> Result<bool, JournalError> {
    if !matches!(
        journal.source_type.as_str(),
        "manual" | "opening_import" | "reversal"
    ) {
        return Err(review_error(
            FailureKind::Conflict,
            "REVERSE_AT_SOURCE",
            "Reverse the originating business document so its stock and settlement records remain consistent.",
        ));
    }
    let opening = journal.source_type == "opening_import";
    if opening && facts.opening_allocated {
        return Err(review_error(
            FailureKind::Conflict,
            "OPENING_SUBLEDGER_SETTLED",
            "This opening journal has payment or receipt allocations. Reverse those settlements and correct the affected documents instead.",
        ));
    }
    // Dates are the already validated ISO business dates, as in B0.
    if entry_date < journal.entry_date.as_str() {
        return Err(review_error(
            FailureKind::InvalidInput,
            "REVERSAL_BEFORE_SOURCE",
            "The reversal cannot precede the original accounting date.",
        ));
    }
    if journal.source_type == "reversal" && facts.source_opening {
        return Err(review_error(
            FailureKind::Conflict,
            "REIMPORT_OPENING_BALANCE",
            "Prepare a corrected opening import instead of reversing an opening reversal.",
        ));
    }
    if journal.source_type == "reversal" && facts.root_source_type.as_deref() != Some("manual") {
        return Err(review_error(
            FailureKind::Conflict,
            "REVERSE_AT_SOURCE",
            "Correct the originating business workflow so its retained subledger and reversal evidence stay consistent.",
        ));
    }
    Ok(opening)
}

/// Actual retained minor-unit rows. No FX recomputation belongs to post/reverse.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalLedgerLine {
    pub id: String,
    pub journal_id: String,
    pub account_id: String,
    pub description: String,
    pub debit_minor: i64,
    pub credit_minor: i64,
    pub base_debit_minor: i64,
    pub base_credit_minor: i64,
    pub dimension_json: String,
    pub ordinal: i64,
}

/// Mirrors the B0 COUNT/SUM posting assertion with arbitrary-precision totals.
/// This does not replace the database row, account, period or dimension checks.
pub fn journal_ledger_balanced(lines: &[JournalLedgerLine]) -> bool {
    lines.len() >= 2
        && lines
            .iter()
            .map(|l| BigInt::from(l.debit_minor))
            .sum::<BigInt>()
            == lines
                .iter()
                .map(|l| BigInt::from(l.credit_minor))
                .sum::<BigInt>()
        && lines
            .iter()
            .map(|l| BigInt::from(l.base_debit_minor))
            .sum::<BigInt>()
            == lines
                .iter()
                .map(|l| BigInt::from(l.base_credit_minor))
                .sum::<BigInt>()
}

/// Reverse the recorded amounts/dimensions verbatim; never retranslate FX.
pub fn inverse_journal_lines(
    lines: &[JournalLedgerLine],
    reversal_id: &str,
) -> Vec<JournalLedgerLine> {
    lines
        .iter()
        .map(|l| JournalLedgerLine {
            id: format!("{reversal_id}-{}", l.ordinal),
            journal_id: reversal_id.into(),
            debit_minor: l.credit_minor,
            credit_minor: l.debit_minor,
            base_debit_minor: l.base_credit_minor,
            base_credit_minor: l.base_debit_minor,
            ..l.clone()
        })
        .collect()
}

pub(crate) fn review_error(
    kind: FailureKind,
    code: &'static str,
    message: &'static str,
) -> JournalError {
    JournalError {
        kind,
        code,
        message: message.into(),
    }
}
