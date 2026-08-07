#![doc = "PostgreSQL command, read, and projection adapters for Finance."]

mod command;
mod projection;
mod read;

pub use command::{FinanceCommandAdapter, FinanceWriteRepository};
pub use projection::{
    FinanceBankTransactionProjector, FinanceChartProjector, FinanceJournalProjector,
    FinanceOutboxPayload, FinancePeriodProjector, FinanceSnapshot, FinanceStatutoryTaxProjector,
    FinanceSubledgerProjector, FinanceTrialBalanceProjector,
};
pub use read::{FinanceReadRepository, decode_cursor, encode_cursor};

pub const CONTEXT_NAME: &str = "finance";
pub const CHART_AGGREGATE_TYPE: &str = "chart";
pub const PERIOD_AGGREGATE_TYPE: &str = "period";
pub const JOURNAL_AGGREGATE_TYPE: &str = "journal";
pub const SUBLEDGER_AGGREGATE_TYPE: &str = "subledger";
pub const CHART_PROJECTION: &str = "finance.charts";
pub const PERIOD_PROJECTION: &str = "finance.periods";
pub const JOURNAL_PROJECTION: &str = "finance.journals";
pub const SUBLEDGER_PROJECTION: &str = "finance.subledger";
pub const TRIAL_BALANCE_PROJECTION: &str = "finance.trial_balance";
pub const BANK_TRANSACTION_PROJECTION: &str = "finance.bank_transactions";
pub const STATUTORY_TAX_PROJECTION: &str = "finance.statutory_tax";
pub const CHART_SNAPSHOT_EVENT: &str = "finance.chart_snapshot";
pub const PERIOD_SNAPSHOT_EVENT: &str = "finance.period_snapshot";
pub const JOURNAL_SNAPSHOT_EVENT: &str = "finance.journal_snapshot";
pub const SUBLEDGER_SNAPSHOT_EVENT: &str = "finance.subledger_snapshot";
pub const EVENT_VERSION: u32 = 1;

#[cfg(test)]
pub const WRITE_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_write/0062_wave8_dependent_context_guards.sql"
);
#[cfg(test)]
pub const READ_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0054_wave8_dependent_context_projections.sql"
);

macro_rules! partition {
    ($name:ident, $prefix:literal, $id:ty) => {
        pub fn $name(id: $id) -> String {
            format!(concat!($prefix, ":{}"), id.as_uuid().simple())
        }
    };
}

partition!(chart_partition, "finance.chart", erpx_finance::ChartId);
partition!(period_partition, "finance.period", erpx_finance::PeriodId);
partition!(
    journal_partition,
    "finance.journal",
    erpx_finance::JournalId
);
partition!(
    subledger_partition,
    "finance.subledger",
    erpx_finance::SubledgerId
);

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn frozen_schema_and_partitions_are_stable() {
        let id = erpx_finance::JournalId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        assert_eq!(journal_partition(id).len(), "finance.journal:".len() + 32);
        assert!(WRITE_DDL.contains("journal_line_guards"));
        assert!(WRITE_DDL.contains("DEFERRABLE INITIALLY DEFERRED"));
        assert!(READ_DDL.contains("finance.statutory_tax"));
    }

    #[test]
    fn read_adapter_has_no_write_store_fallback() {
        let source = include_str!("read.rs");
        assert!(!source.contains(concat!("Write", "Pool")));
        assert!(!source.contains(concat!("aggregate_", "snapshots")));
    }

    #[test]
    fn immutable_source_facts_are_insert_once_and_compared() {
        let source = include_str!("command.rs");
        assert!(source.contains("compare_bank_fact"));
        assert!(source.contains("compare_tax_fact"));
        assert!(!source.contains("DELETE FROM erpx_finance_write.bank_transaction_facts"));
        assert!(!source.contains("DELETE FROM erpx_finance_write.statutory_tax_facts"));
    }
}
