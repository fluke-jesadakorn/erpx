pub const CONTRACT_VERSION: &str = "collaboration.field-journal-flow-notification.v1";
pub use crate::adapters::field_journal_flow_notification::{
    FieldJournalFlowNotice, RetainedFieldJournalFlowNotice, lock_field_journal_flow_notices,
    stage_field_journal_flow_notice,
};
