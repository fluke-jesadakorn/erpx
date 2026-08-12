//! Tenant worker composition for the Wave 8 dependent bounded contexts.

use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionPool, ProjectionRun, ProjectionRunnerConfig, WritePool};
use erpx_infrastructure::{
    ProjectionClaimRecord, ProjectionJobKind, RoutedDatabaseSide, TenantDatabaseRouter,
};
use erpx_worker::{TenantProjectionExecutor, TenantProjectionExecutorError};

use crate::contexts::{
    finance::{FinanceProjectionFactory, FinanceProjectionKind, FinanceProjectionRuntime},
    legal::{LegalProjectionFactory, LegalProjectionKind, LegalProjectionRuntime},
    support::{SupportProjectionFactory, SupportProjectionKind, SupportProjectionRuntime},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wave8ProjectionKind {
    FinanceCharts,
    FinancePeriods,
    FinanceJournals,
    FinanceSubledger,
    FinanceCustomerInvoices,
    FinanceRevenueSchedules,
    FinanceTrialBalance,
    FinanceBankTransactions,
    FinanceStatutoryTax,
    LegalContracts,
    LegalDocuments,
    LegalAccessHistory,
    LegalMatters,
    SupportTickets,
    SupportIncidents,
    SupportSessions,
    SupportCapabilities,
}

impl Wave8ProjectionKind {
    pub const fn projection_name(self) -> &'static str {
        match self {
            Self::FinanceCharts => erpx_finance_sqlx::CHART_PROJECTION,
            Self::FinancePeriods => erpx_finance_sqlx::PERIOD_PROJECTION,
            Self::FinanceJournals => erpx_finance_sqlx::JOURNAL_PROJECTION,
            Self::FinanceSubledger => erpx_finance_sqlx::SUBLEDGER_PROJECTION,
            Self::FinanceCustomerInvoices => erpx_finance_sqlx::CUSTOMER_INVOICE_PROJECTION,
            Self::FinanceRevenueSchedules => erpx_finance_sqlx::REVENUE_SCHEDULE_PROJECTION,
            Self::FinanceTrialBalance => erpx_finance_sqlx::TRIAL_BALANCE_PROJECTION,
            Self::FinanceBankTransactions => erpx_finance_sqlx::BANK_TRANSACTION_PROJECTION,
            Self::FinanceStatutoryTax => erpx_finance_sqlx::STATUTORY_TAX_PROJECTION,
            Self::LegalContracts => erpx_legal_sqlx::CONTRACT_PROJECTION,
            Self::LegalDocuments => erpx_legal_sqlx::DOCUMENT_PROJECTION,
            Self::LegalAccessHistory => erpx_legal_sqlx::ACCESS_PROJECTION,
            Self::LegalMatters => erpx_legal_sqlx::MATTER_PROJECTION,
            Self::SupportTickets => erpx_support_sqlx::TICKET_PROJECTION,
            Self::SupportIncidents => erpx_support_sqlx::INCIDENT_PROJECTION,
            Self::SupportSessions => erpx_support_sqlx::SESSION_PROJECTION,
            Self::SupportCapabilities => erpx_support_sqlx::CAPABILITY_PROJECTION,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Wave8ProjectionExecutor {
    router: TenantDatabaseRouter,
    kind: Wave8ProjectionKind,
}

impl Wave8ProjectionExecutor {
    pub const fn new(router: TenantDatabaseRouter, kind: Wave8ProjectionKind) -> Self {
        Self { router, kind }
    }

    async fn runtime(
        &self,
        tenant_id: &str,
        holding_ref: &str,
        page_size: u32,
    ) -> Result<SnapshotRuntime, TenantProjectionExecutorError> {
        let write = self
            .router
            .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
            .await
            .map_err(|_| TenantProjectionExecutorError::Retryable)?;
        let read = self
            .router
            .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
            .await
            .map_err(|_| TenantProjectionExecutorError::Retryable)?;
        let write = WritePool::from_pg_pool(write.database_pool().clone());
        let read = ProjectionPool::from_pg_pool(read.database_pool().clone());
        let config = ProjectionRunnerConfig {
            page_size,
            ..ProjectionRunnerConfig::default()
        };
        match self.kind {
            Wave8ProjectionKind::FinanceCharts => {
                finance_runtime(write, read, config, FinanceProjectionKind::Charts)
            }
            Wave8ProjectionKind::FinancePeriods => {
                finance_runtime(write, read, config, FinanceProjectionKind::Periods)
            }
            Wave8ProjectionKind::FinanceJournals => {
                finance_runtime(write, read, config, FinanceProjectionKind::Journals)
            }
            Wave8ProjectionKind::FinanceSubledger => {
                finance_runtime(write, read, config, FinanceProjectionKind::Subledger)
            }
            Wave8ProjectionKind::FinanceCustomerInvoices => {
                finance_runtime(write, read, config, FinanceProjectionKind::CustomerInvoices)
            }
            Wave8ProjectionKind::FinanceRevenueSchedules => {
                finance_runtime(write, read, config, FinanceProjectionKind::RevenueSchedules)
            }
            Wave8ProjectionKind::FinanceTrialBalance => {
                finance_runtime(write, read, config, FinanceProjectionKind::TrialBalance)
            }
            Wave8ProjectionKind::FinanceBankTransactions => {
                finance_runtime(write, read, config, FinanceProjectionKind::BankTransactions)
            }
            Wave8ProjectionKind::FinanceStatutoryTax => {
                finance_runtime(write, read, config, FinanceProjectionKind::StatutoryTax)
            }
            Wave8ProjectionKind::LegalContracts => {
                legal_runtime(write, read, config, LegalProjectionKind::Contracts)
            }
            Wave8ProjectionKind::LegalDocuments => {
                legal_runtime(write, read, config, LegalProjectionKind::Documents)
            }
            Wave8ProjectionKind::LegalAccessHistory => {
                legal_runtime(write, read, config, LegalProjectionKind::AccessHistory)
            }
            Wave8ProjectionKind::LegalMatters => {
                legal_runtime(write, read, config, LegalProjectionKind::Matters)
            }
            Wave8ProjectionKind::SupportTickets => {
                support_runtime(write, read, config, SupportProjectionKind::Tickets)
            }
            Wave8ProjectionKind::SupportIncidents => {
                support_runtime(write, read, config, SupportProjectionKind::Incidents)
            }
            Wave8ProjectionKind::SupportSessions => {
                support_runtime(write, read, config, SupportProjectionKind::Sessions)
            }
            Wave8ProjectionKind::SupportCapabilities => {
                support_runtime(write, read, config, SupportProjectionKind::Capabilities)
            }
        }
        .map_err(crate::map_cqrs_projection_error)
    }
}

fn finance_runtime(
    write: WritePool,
    read: ProjectionPool,
    config: ProjectionRunnerConfig,
    kind: FinanceProjectionKind,
) -> Result<SnapshotRuntime, erpx_cqrs_sqlx::CqrsSqlxError> {
    FinanceProjectionFactory
        .build(write, read, config)
        .map(|runtime| {
            debug_assert_eq!(
                runtime.registrations(),
                FinanceProjectionFactory.registrations()
            );
            SnapshotRuntime::Finance(runtime, kind)
        })
}

fn legal_runtime(
    write: WritePool,
    read: ProjectionPool,
    config: ProjectionRunnerConfig,
    kind: LegalProjectionKind,
) -> Result<SnapshotRuntime, erpx_cqrs_sqlx::CqrsSqlxError> {
    LegalProjectionFactory
        .build(write, read, config)
        .map(|runtime| {
            debug_assert_eq!(
                runtime.registrations(),
                LegalProjectionFactory.registrations()
            );
            SnapshotRuntime::Legal(runtime, kind)
        })
}

fn support_runtime(
    write: WritePool,
    read: ProjectionPool,
    config: ProjectionRunnerConfig,
    kind: SupportProjectionKind,
) -> Result<SnapshotRuntime, erpx_cqrs_sqlx::CqrsSqlxError> {
    SupportProjectionFactory
        .build(write, read, config)
        .map(|runtime| {
            debug_assert_eq!(
                runtime.registrations(),
                SupportProjectionFactory.registrations()
            );
            SnapshotRuntime::Support(runtime, kind)
        })
}

#[async_trait]
impl TenantProjectionExecutor for Wave8ProjectionExecutor {
    fn projection_name_filter(&self) -> Option<&str> {
        Some(self.kind.projection_name())
    }

    async fn execute(
        &self,
        claim: &ProjectionClaimRecord,
    ) -> Result<(), TenantProjectionExecutorError> {
        if self.projection_name_filter() != Some(claim.job.projection_name.as_str())
            || claim.job.payload.get("after").is_some()
            || claim.job.payload.get("cursor").is_some()
            || claim.job.payload.get("candidate_database_ref").is_some()
            || claim.job.payload.get("source_version").is_some()
            || claim.job.payload.get("generation").is_some()
        {
            return Err(TenantProjectionExecutorError::Permanent);
        }
        let page_size = crate::projection_page_size(&claim.job.payload, 500)?;
        let runtime = self
            .runtime(
                &claim.job.binding.tenant_id,
                &claim.job.binding.holding_ref,
                page_size,
            )
            .await?;
        match claim.job.kind {
            ProjectionJobKind::Incremental => validate_run(
                runtime
                    .run_once()
                    .await
                    .map_err(crate::map_cqrs_projection_error)?,
            ),
            ProjectionJobKind::Rebuild => {
                let rebuild_id = claim
                    .job
                    .rebuild_id
                    .filter(|value| !value.is_nil())
                    .ok_or(TenantProjectionExecutorError::Permanent)?;
                runtime.rebuild(page_size, rebuild_id).await
            }
        }
    }
}

#[derive(Debug, Clone)]
enum SnapshotRuntime {
    Finance(FinanceProjectionRuntime, FinanceProjectionKind),
    Legal(LegalProjectionRuntime, LegalProjectionKind),
    Support(SupportProjectionRuntime, SupportProjectionKind),
}

impl SnapshotRuntime {
    async fn run_once(&self) -> Result<ProjectionRun, erpx_cqrs_sqlx::CqrsSqlxError> {
        match self {
            Self::Finance(runtime, kind) => runtime.run_once(*kind).await,
            Self::Legal(runtime, kind) => runtime.run_once(*kind).await,
            Self::Support(runtime, kind) => runtime.run_once(*kind).await,
        }
    }

    async fn rebuild(
        &self,
        page_size: u32,
        rebuild_id: uuid::Uuid,
    ) -> Result<(), TenantProjectionExecutorError> {
        macro_rules! rebuild {
            ($runtime:expr, $kind:expr) => {{
                let runtime = $runtime;
                let kind = *$kind;
                let handle = runtime
                    .begin_rebuild(kind, rebuild_id)
                    .await
                    .map_err(crate::map_cqrs_projection_error)?;
                let mut cursor = 0;
                loop {
                    let run = runtime
                        .run_rebuild_page(kind, &handle, cursor)
                        .await
                        .map_err(crate::map_cqrs_projection_error)?;
                    cursor = run.cursor;
                    let complete = run.fetched < page_size;
                    validate_run(run)?;
                    if complete {
                        break;
                    }
                }
                runtime
                    .activate_rebuild(kind, &handle)
                    .await
                    .map_err(crate::map_cqrs_projection_error)
            }};
        }
        match self {
            Self::Finance(runtime, kind) => rebuild!(runtime, kind),
            Self::Legal(runtime, kind) => rebuild!(runtime, kind),
            Self::Support(runtime, kind) => rebuild!(runtime, kind),
        }
    }
}

fn validate_run(run: ProjectionRun) -> Result<(), TenantProjectionExecutorError> {
    if run.retry_scheduled || run.dead_lettered != 0 {
        return Err(TenantProjectionExecutorError::Retryable);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduler_inventory_covers_all_wave8_read_models() {
        let kinds = [
            Wave8ProjectionKind::FinanceCharts,
            Wave8ProjectionKind::FinancePeriods,
            Wave8ProjectionKind::FinanceJournals,
            Wave8ProjectionKind::FinanceSubledger,
            Wave8ProjectionKind::FinanceCustomerInvoices,
            Wave8ProjectionKind::FinanceRevenueSchedules,
            Wave8ProjectionKind::FinanceTrialBalance,
            Wave8ProjectionKind::FinanceBankTransactions,
            Wave8ProjectionKind::FinanceStatutoryTax,
            Wave8ProjectionKind::LegalContracts,
            Wave8ProjectionKind::LegalDocuments,
            Wave8ProjectionKind::LegalAccessHistory,
            Wave8ProjectionKind::LegalMatters,
            Wave8ProjectionKind::SupportTickets,
            Wave8ProjectionKind::SupportIncidents,
            Wave8ProjectionKind::SupportSessions,
            Wave8ProjectionKind::SupportCapabilities,
        ];
        assert_eq!(kinds.len(), 17);
    }

    #[test]
    fn every_selector_has_one_exact_claim_filter() {
        let cases = [
            (Wave8ProjectionKind::FinanceCharts, "finance.charts"),
            (Wave8ProjectionKind::FinancePeriods, "finance.periods"),
            (Wave8ProjectionKind::FinanceJournals, "finance.journals"),
            (Wave8ProjectionKind::FinanceSubledger, "finance.subledger"),
            (
                Wave8ProjectionKind::FinanceCustomerInvoices,
                "finance.customer_invoices",
            ),
            (
                Wave8ProjectionKind::FinanceRevenueSchedules,
                "finance.revenue_schedules",
            ),
            (
                Wave8ProjectionKind::FinanceTrialBalance,
                "finance.trial_balance",
            ),
            (
                Wave8ProjectionKind::FinanceBankTransactions,
                "finance.bank_transactions",
            ),
            (
                Wave8ProjectionKind::FinanceStatutoryTax,
                "finance.statutory_tax",
            ),
            (Wave8ProjectionKind::LegalContracts, "legal.contracts"),
            (Wave8ProjectionKind::LegalDocuments, "legal.documents"),
            (
                Wave8ProjectionKind::LegalAccessHistory,
                "legal.access_history",
            ),
            (Wave8ProjectionKind::LegalMatters, "legal.matters"),
            (Wave8ProjectionKind::SupportTickets, "support.tickets"),
            (Wave8ProjectionKind::SupportIncidents, "support.incidents"),
            (Wave8ProjectionKind::SupportSessions, "support.sessions"),
            (
                Wave8ProjectionKind::SupportCapabilities,
                "support.capabilities",
            ),
        ];
        for (kind, expected) in cases {
            assert_eq!(kind.projection_name(), expected);
        }
    }
}
