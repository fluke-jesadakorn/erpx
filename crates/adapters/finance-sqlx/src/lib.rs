#![doc = "PostgreSQL command, read, and projection adapters for Finance."]

mod command;
mod payables;
mod projection;
mod read;
mod records;

pub use command::{FinanceCommandAdapter, FinanceWriteRepository};
pub use payables::{
    FinancePaymentProjector, FinanceVendorInvoiceProjector, PAYABLES_EVENT_VERSION,
    PAYMENT_AGGREGATE_TYPE, PAYMENT_PROJECTION, PAYMENT_SNAPSHOT_EVENT, PayablesOutboxPayload,
    PayablesReadAdapter, SqlxPayablesAdapter, VENDOR_INVOICE_AGGREGATE_TYPE,
    VENDOR_INVOICE_PROJECTION, VENDOR_INVOICE_SNAPSHOT_EVENT,
};
pub use projection::{
    FinanceBankTransactionProjector, FinanceChartProjector, FinanceCustomerInvoiceProjector,
    FinanceJournalProjector, FinanceOutboxPayload, FinancePeriodProjector,
    FinanceRevenueScheduleProjector, FinanceSnapshot, FinanceStatutoryTaxProjector,
    FinanceSubledgerProjector, FinanceTrialBalanceProjector,
};
pub use read::{FinanceReadRepository, decode_cursor, encode_cursor};
pub use records::{
    FinanceRecordProjector, FinanceRecordReadAdapter, FinanceRecordSqlxAdapter,
    RECORD_PROJECTION_NAME, RECORD_SNAPSHOT_EVENT_TYPE,
};

pub const CONTEXT_NAME: &str = "finance";
pub const CHART_AGGREGATE_TYPE: &str = "chart";
pub const PERIOD_AGGREGATE_TYPE: &str = "period";
pub const JOURNAL_AGGREGATE_TYPE: &str = "journal";
pub const SUBLEDGER_AGGREGATE_TYPE: &str = "subledger";
pub const CUSTOMER_INVOICE_AGGREGATE_TYPE: &str = "customer_invoice";
pub const REVENUE_SCHEDULE_AGGREGATE_TYPE: &str = "revenue_schedule";
pub const CHART_PROJECTION: &str = "finance.charts";
pub const PERIOD_PROJECTION: &str = "finance.periods";
pub const JOURNAL_PROJECTION: &str = "finance.journals";
pub const SUBLEDGER_PROJECTION: &str = "finance.subledger";
pub const CUSTOMER_INVOICE_PROJECTION: &str = "finance.customer_invoices";
pub const REVENUE_SCHEDULE_PROJECTION: &str = "finance.revenue_schedules";
pub const TRIAL_BALANCE_PROJECTION: &str = "finance.trial_balance";
pub const BANK_TRANSACTION_PROJECTION: &str = "finance.bank_transactions";
pub const STATUTORY_TAX_PROJECTION: &str = "finance.statutory_tax";
pub const CHART_SNAPSHOT_EVENT: &str = "finance.chart_snapshot";
pub const PERIOD_SNAPSHOT_EVENT: &str = "finance.period_snapshot";
pub const JOURNAL_SNAPSHOT_EVENT: &str = "finance.journal_snapshot";
pub const SUBLEDGER_SNAPSHOT_EVENT: &str = "finance.subledger_snapshot";
pub const CUSTOMER_INVOICE_SNAPSHOT_EVENT: &str = "finance.customer_invoice_snapshot";
pub const REVENUE_SCHEDULE_SNAPSHOT_EVENT: &str = "finance.revenue_schedule_snapshot";
pub const EVENT_VERSION: u32 = 1;

#[cfg(test)]
pub const PAYABLES_WRITE_DDL: &str =
    include_str!("../../../migration/migrations_holding_write/0007_finance_ap_payment.sql");
#[cfg(test)]
pub const PAYABLES_READ_DDL: &str =
    include_str!("../../../migration/migrations_holding_read/0004_finance_ap_payment.sql");

#[cfg(test)]
pub const WRITE_DDL: &str =
    include_str!("../../../migration/migrations_holding_write/0001_baseline.sql");
#[cfg(test)]
pub const READ_DDL: &str =
    include_str!("../../../migration/migrations_holding_read/0001_baseline.sql");
#[cfg(test)]
pub const CUSTOMER_INVOICE_WRITE_DDL: &str =
    include_str!("../../../migration/migrations_holding_write/0001_baseline.sql");
#[cfg(test)]
pub const CUSTOMER_INVOICE_READ_DDL: &str =
    include_str!("../../../migration/migrations_holding_read/0001_baseline.sql");
#[cfg(test)]
pub const REVENUE_SCHEDULE_WRITE_DDL: &str =
    include_str!("../../../migration/migrations_holding_write/0001_baseline.sql");
#[cfg(test)]
pub const REVENUE_SCHEDULE_READ_DDL: &str =
    include_str!("../../../migration/migrations_holding_read/0001_baseline.sql");

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
    revenue_schedule_partition,
    "finance.revenue_schedule",
    erpx_finance::RevenueScheduleId
);
partition!(
    customer_invoice_partition,
    "finance.customer_invoice",
    erpx_finance::CustomerInvoiceId
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
        assert!(CUSTOMER_INVOICE_WRITE_DDL.contains("customer_invoice_guards"));
        assert!(CUSTOMER_INVOICE_READ_DDL.contains("finance.customer_invoices"));
        assert!(REVENUE_SCHEDULE_WRITE_DDL.contains("revenue_schedule_guards"));
        assert!(REVENUE_SCHEDULE_READ_DDL.contains("finance.revenue_schedules"));
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

    #[test]
    fn payables_schema_fences_exact_evidence_atomic_payable_and_rls() {
        for field in [
            "purchase_order_version BIGINT NOT NULL",
            "receipt_version BIGINT NOT NULL",
            "payable_version BIGINT NOT NULL",
            "DEFERRABLE INITIALLY DEFERRED",
            "finance_match_evidence_append_only",
            "ENABLE ROW LEVEL SECURITY",
            "FORCE ROW LEVEL SECURITY",
        ] {
            assert!(
                PAYABLES_WRITE_DDL.contains(field),
                "missing AP fence {field}"
            );
        }
        for projection in [
            "vendor_invoice_projection",
            "payable_projection",
            "payment_projection",
        ] {
            assert!(PAYABLES_READ_DDL.contains(projection));
        }
        assert!(PAYABLES_READ_DDL.contains("projection_generations"));
        assert!(PAYABLES_READ_DDL.contains("FORCE ROW LEVEL SECURITY"));
    }
}
