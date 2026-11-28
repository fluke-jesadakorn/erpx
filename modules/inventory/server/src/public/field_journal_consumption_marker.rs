//! Global Field journal routing marker; does not establish authorization.
pub use crate::adapters::field_journal_consumption_marker::{
    FieldJournalTargets, CurrentFieldJournalConsumptionBatch, inspect_field_journal_consumption_batch,
    lock_field_journal_consumption_batch_sources,
    CurrentFieldJournalConsumptionMarker, inspect_field_journal_consumption_marker,
    lock_field_journal_consumption_marker,
};
pub use crate::adapters::field_journal_consumption_marker::{FieldJournalRoutingSource,FieldJournalRoutingVector,CurrentFieldJournalRouting,inspect_field_journal_routing,lock_field_journal_routing_sources};
