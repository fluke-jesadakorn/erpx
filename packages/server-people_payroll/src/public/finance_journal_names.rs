//! Company-scoped display facts, never an access or engagement grant. Call the
//! source utility before the first Holding snapshot; users must come only from
//! the already-authorized journal. Absence alone yields 'Unlinked account'.
pub use crate::adapters::finance_journal_names::{
    CurrentFinanceJournalNames, lock_finance_journal_name_sources, read_finance_journal_names,
};
