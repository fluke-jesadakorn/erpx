//! The exact read-only Journal record workspace data; never an action grant.
use super::journal_detail::FinanceLedgerRow;
use serde::Serialize;
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceJournalRecordWorkspace {
    pub journal: FinanceLedgerRow,
    pub lines: Vec<FinanceLedgerRow>,
    pub history: Vec<FinanceLedgerRow>,
    pub chart: FinanceLedgerRow,
    pub accounts: Vec<FinanceLedgerRow>,
    pub periods: Vec<FinanceLedgerRow>,
    pub dimensions: Vec<FinanceLedgerRow>,
    #[serde(rename = "dimensionValues")]
    pub dimension_values: Vec<FinanceLedgerRow>,
}
pub use crate::adapters::postgres_journal_workspace::{
    CurrentFieldServiceJournalWorkspace, CurrentJournalRecordWorkspace,
    lock_journal_workspace_sources, read_field_service_journal_workspace,
    read_journal_record_workspace,
};
