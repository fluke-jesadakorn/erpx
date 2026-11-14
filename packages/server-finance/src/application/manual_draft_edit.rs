//! Pure checks for an edit against an admitted, captured manual journal.
use crate::application::journal::{
    FinanceActor, JournalDraft, JournalPreparation, ManualJournalEditBasis,
    validate_manual_journal_edit,
};
use crate::domain::journal::JournalError;
use erpx_server_shared::public::FailureKind;

pub(crate) fn check_prepared_edit(
    actor: &FinanceActor,
    draft: &JournalDraft,
    basis: &ManualJournalEditBasis<'_>,
    expected_version: i64,
    preparation: &JournalPreparation,
) -> Result<i64, JournalError> {
    let version = validate_manual_journal_edit(basis, draft, expected_version)?;
    let actual = preparation.draft();
    let lines_equal = draft.lines.len() == actual.lines.len()
        && draft.lines.iter().zip(&actual.lines).all(|(a, b)| {
            a.account_id == b.account_id
                && a.description == b.description
                && a.debit == b.debit
                && a.credit == b.credit
                && a.dimensions == b.dimensions
        });
    if preparation.actor() != actor
        || actual.chart_id != draft.chart_id
        || actual.period_id != draft.period_id
        || actual.entry_date != draft.entry_date
        || actual.currency != draft.currency
        || actual.fx_rate != draft.fx_rate
        || actual.memo != draft.memo
        || draft.source_type != "manual"
        || draft.source_id.is_some()
        || draft.is_opening
        || actual.source_type != "manual"
        || actual.source_id.is_some()
        || actual.is_opening
        || !lines_equal
    {
        return Err(JournalError {
            kind: FailureKind::Conflict,
            code: "MANUAL_DRAFT_PREPARATION_CHANGED",
            message: "Prepare the original edit in its owner transaction.".into(),
        });
    }
    Ok(version)
}
