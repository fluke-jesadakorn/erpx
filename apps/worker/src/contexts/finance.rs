//! Holding Finance projection runtimes.

use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionPool, ProjectionRun, ProjectionRunner, ProjectionRunnerConfig,
    ProjectionSource, ProjectionStore, Projector, RebuildHandle, WritePool,
};
use erpx_finance_sqlx::{
    FinanceBankTransactionProjector, FinanceChartProjector, FinanceCustomerInvoiceProjector,
    FinanceJournalProjector, FinancePeriodProjector, FinanceRecordProjector,
    FinanceRevenueScheduleProjector, FinanceStatutoryTaxProjector, FinanceSubledgerProjector,
    FinanceTrialBalanceProjector,
};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinanceProjectionKind {
    Charts,
    Periods,
    Journals,
    Subledger,
    CustomerInvoices,
    RevenueSchedules,
    TrialBalance,
    BankTransactions,
    StatutoryTax,
    BusinessRecords,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FinanceProjectionFactory;

impl FinanceProjectionFactory {
    pub fn registrations(self) -> Vec<FinanceProjectionRegistration> {
        vec![
            registration(&FinanceChartProjector::new()),
            registration(&FinancePeriodProjector::new()),
            registration(&FinanceJournalProjector::new()),
            registration(&FinanceSubledgerProjector::new()),
            registration(&FinanceCustomerInvoiceProjector::new()),
            registration(&FinanceRevenueScheduleProjector::new()),
            registration(&FinanceTrialBalanceProjector::new()),
            registration(&FinanceBankTransactionProjector::new()),
            registration(&FinanceStatutoryTaxProjector::new()),
            registration(&FinanceRecordProjector::new()),
        ]
    }
    pub fn build(
        self,
        write: WritePool,
        projection: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<FinanceProjectionRuntime, CqrsSqlxError> {
        let store = ProjectionStore::new(projection);
        let runner = ProjectionRunner::new(ProjectionSource::new(write), store.clone(), config)?;
        Ok(FinanceProjectionRuntime {
            charts: FinanceChartProjector::new(),
            periods: FinancePeriodProjector::new(),
            journals: FinanceJournalProjector::new(),
            subledger: FinanceSubledgerProjector::new(),
            customer_invoices: FinanceCustomerInvoiceProjector::new(),
            revenue_schedules: FinanceRevenueScheduleProjector::new(),
            trial_balance: FinanceTrialBalanceProjector::new(),
            bank_transactions: FinanceBankTransactionProjector::new(),
            statutory_tax: FinanceStatutoryTaxProjector::new(),
            records: FinanceRecordProjector::new(),
            runner,
            store,
        })
    }
}

#[derive(Debug, Clone)]
pub struct FinanceProjectionRuntime {
    charts: FinanceChartProjector,
    periods: FinancePeriodProjector,
    journals: FinanceJournalProjector,
    subledger: FinanceSubledgerProjector,
    customer_invoices: FinanceCustomerInvoiceProjector,
    revenue_schedules: FinanceRevenueScheduleProjector,
    trial_balance: FinanceTrialBalanceProjector,
    bank_transactions: FinanceBankTransactionProjector,
    statutory_tax: FinanceStatutoryTaxProjector,
    records: FinanceRecordProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}

impl FinanceProjectionRuntime {
    pub fn registrations(&self) -> Vec<FinanceProjectionRegistration> {
        FinanceProjectionFactory.registrations()
    }
    pub async fn run_once(
        &self,
        kind: FinanceProjectionKind,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            FinanceProjectionKind::Charts => self.runner.run_active(&self.charts).await,
            FinanceProjectionKind::Periods => self.runner.run_active(&self.periods).await,
            FinanceProjectionKind::Journals => self.runner.run_active(&self.journals).await,
            FinanceProjectionKind::Subledger => self.runner.run_active(&self.subledger).await,
            FinanceProjectionKind::CustomerInvoices => {
                self.runner.run_active(&self.customer_invoices).await
            }
            FinanceProjectionKind::RevenueSchedules => {
                self.runner.run_active(&self.revenue_schedules).await
            }
            FinanceProjectionKind::TrialBalance => {
                self.runner.run_active(&self.trial_balance).await
            }
            FinanceProjectionKind::BankTransactions => {
                self.runner.run_active(&self.bank_transactions).await
            }
            FinanceProjectionKind::StatutoryTax => {
                self.runner.run_active(&self.statutory_tax).await
            }
            FinanceProjectionKind::BusinessRecords => self.runner.run_active(&self.records).await,
        }
    }
    pub async fn begin_rebuild(
        &self,
        kind: FinanceProjectionKind,
        rebuild_id: Uuid,
    ) -> Result<RebuildHandle, CqrsSqlxError> {
        match kind {
            FinanceProjectionKind::Charts => {
                self.store
                    .begin_or_resume_rebuild(&self.charts, rebuild_id)
                    .await
            }
            FinanceProjectionKind::Periods => {
                self.store
                    .begin_or_resume_rebuild(&self.periods, rebuild_id)
                    .await
            }
            FinanceProjectionKind::Journals => {
                self.store
                    .begin_or_resume_rebuild(&self.journals, rebuild_id)
                    .await
            }
            FinanceProjectionKind::Subledger => {
                self.store
                    .begin_or_resume_rebuild(&self.subledger, rebuild_id)
                    .await
            }
            FinanceProjectionKind::CustomerInvoices => {
                self.store
                    .begin_or_resume_rebuild(&self.customer_invoices, rebuild_id)
                    .await
            }
            FinanceProjectionKind::RevenueSchedules => {
                self.store
                    .begin_or_resume_rebuild(&self.revenue_schedules, rebuild_id)
                    .await
            }
            FinanceProjectionKind::TrialBalance => {
                self.store
                    .begin_or_resume_rebuild(&self.trial_balance, rebuild_id)
                    .await
            }
            FinanceProjectionKind::BankTransactions => {
                self.store
                    .begin_or_resume_rebuild(&self.bank_transactions, rebuild_id)
                    .await
            }
            FinanceProjectionKind::StatutoryTax => {
                self.store
                    .begin_or_resume_rebuild(&self.statutory_tax, rebuild_id)
                    .await
            }
            FinanceProjectionKind::BusinessRecords => {
                self.store
                    .begin_or_resume_rebuild(&self.records, rebuild_id)
                    .await
            }
        }
    }
    pub async fn run_rebuild_page(
        &self,
        kind: FinanceProjectionKind,
        handle: &RebuildHandle,
        after: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            FinanceProjectionKind::Charts => {
                self.runner.run_rebuild(&self.charts, handle, after).await
            }
            FinanceProjectionKind::Periods => {
                self.runner.run_rebuild(&self.periods, handle, after).await
            }
            FinanceProjectionKind::Journals => {
                self.runner.run_rebuild(&self.journals, handle, after).await
            }
            FinanceProjectionKind::Subledger => {
                self.runner
                    .run_rebuild(&self.subledger, handle, after)
                    .await
            }
            FinanceProjectionKind::CustomerInvoices => {
                self.runner
                    .run_rebuild(&self.customer_invoices, handle, after)
                    .await
            }
            FinanceProjectionKind::RevenueSchedules => {
                self.runner
                    .run_rebuild(&self.revenue_schedules, handle, after)
                    .await
            }
            FinanceProjectionKind::TrialBalance => {
                self.runner
                    .run_rebuild(&self.trial_balance, handle, after)
                    .await
            }
            FinanceProjectionKind::BankTransactions => {
                self.runner
                    .run_rebuild(&self.bank_transactions, handle, after)
                    .await
            }
            FinanceProjectionKind::StatutoryTax => {
                self.runner
                    .run_rebuild(&self.statutory_tax, handle, after)
                    .await
            }
            FinanceProjectionKind::BusinessRecords => {
                self.runner.run_rebuild(&self.records, handle, after).await
            }
        }
    }
    pub async fn activate_rebuild(
        &self,
        kind: FinanceProjectionKind,
        handle: &RebuildHandle,
    ) -> Result<(), CqrsSqlxError> {
        let expected = match kind {
            FinanceProjectionKind::Charts => self.charts.name(),
            FinanceProjectionKind::Periods => self.periods.name(),
            FinanceProjectionKind::Journals => self.journals.name(),
            FinanceProjectionKind::Subledger => self.subledger.name(),
            FinanceProjectionKind::CustomerInvoices => self.customer_invoices.name(),
            FinanceProjectionKind::RevenueSchedules => self.revenue_schedules.name(),
            FinanceProjectionKind::TrialBalance => self.trial_balance.name(),
            FinanceProjectionKind::BankTransactions => self.bank_transactions.name(),
            FinanceProjectionKind::StatutoryTax => self.statutory_tax.name(),
            FinanceProjectionKind::BusinessRecords => self.records.name(),
        };
        if handle.projection != expected {
            return Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another Finance projector",
            ));
        }
        self.store.activate_rebuild(handle).await
    }
}

fn registration(p: &impl Projector) -> FinanceProjectionRegistration {
    FinanceProjectionRegistration {
        name: p.name().into(),
        event_types: p.event_types().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_finance_sqlx::{
        BANK_TRANSACTION_PROJECTION, CHART_PROJECTION, CUSTOMER_INVOICE_PROJECTION,
        JOURNAL_PROJECTION, PERIOD_PROJECTION, RECORD_PROJECTION_NAME, REVENUE_SCHEDULE_PROJECTION,
        STATUTORY_TAX_PROJECTION, SUBLEDGER_PROJECTION, TRIAL_BALANCE_PROJECTION,
    };
    #[test]
    fn registrations_are_collision_free() {
        let values = FinanceProjectionFactory.registrations();
        assert_eq!(values.len(), 10);
        assert_eq!(
            values
                .iter()
                .map(|v| v.name.as_str())
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            10
        );
        for expected in [
            CHART_PROJECTION,
            PERIOD_PROJECTION,
            JOURNAL_PROJECTION,
            SUBLEDGER_PROJECTION,
            CUSTOMER_INVOICE_PROJECTION,
            REVENUE_SCHEDULE_PROJECTION,
            TRIAL_BALANCE_PROJECTION,
            BANK_TRANSACTION_PROJECTION,
            STATUTORY_TAX_PROJECTION,
            RECORD_PROJECTION_NAME,
        ] {
            assert!(values.iter().any(|v| v.name == expected));
        }
    }
    #[test]
    fn runtime_source_has_rebuild_and_failure_guarantees() {
        let source = include_str!("finance.rs");
        for seam in [
            "run_active",
            "begin_or_resume_rebuild",
            "run_rebuild",
            "activate_rebuild",
        ] {
            assert!(source.contains(seam));
        }
        let shared = include_str!("../../../../crates/adapters/cqrs-sqlx/src/projection.rs");
        for guarantee in [
            "projection_applied_events",
            "projection_checkpoints",
            "projection_failures",
            "dead_lettered_at",
            "projection_receipts",
        ] {
            assert!(shared.contains(guarantee));
        }
    }
}
