//! Same-unit Finance routing facts; not an authorization or target witness.
pub use crate::adapters::field_attachment_routing::{
    CurrentFinanceFieldRouting, CurrentGlobalFieldJournalFinanceSource,
    CurrentGlobalFieldPaymentFinanceSources, FinanceFieldRoutingVector, PathFlags,
    inspect_finance_field_routing, inspect_global_field_journal_finance_source,
    inspect_global_field_payment_finance_sources, lock_field_attachment_finance_routing,
};
