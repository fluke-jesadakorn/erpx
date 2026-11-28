//! Actual Finance target identity/version, never an attachment read grant.
pub use crate::adapters::attachment_target_source::{
    CurrentCanonicalFinanceAttachmentObservation, CurrentCanonicalFinanceAttachmentTarget,
    CurrentFinanceAttachmentTarget, FinanceAttachmentTargetKind,
    inspect_canonical_finance_attachment_target, inspect_finance_attachment_target,
    lock_canonical_finance_attachment_targets, lock_finance_attachment_target_sources,
    observe_canonical_finance_attachment_target,
};
pub use crate::adapters::attachment_target_source::{
    CurrentCanonicalFinanceHistoryJournalRoots, inspect_canonical_finance_history_journal_roots,
    lock_canonical_finance_history_book_roots,
};
