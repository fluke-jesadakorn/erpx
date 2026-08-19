//! Holding worker composition for the Wave 7 transactional bounded contexts.

use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionPool, ProjectionRun, ProjectionRunnerConfig, WritePool};
use erpx_infrastructure::{
    HoldingDatabaseRouter, ProjectionClaimRecord, ProjectionJobKind, RoutedDatabaseSide,
};
use erpx_worker::{HoldingProjectionExecutor, HoldingProjectionExecutorError};

use crate::contexts::{
    people_payroll::{
        PeoplePayrollProjectionFactory, PeoplePayrollProjectionKind, PeoplePayrollProjectionRuntime,
    },
    pos::{PosProjectionFactory, PosProjectionKind, PosProjectionRuntime},
    sales::{SalesProjectionFactory, SalesProjectionKind, SalesProjectionRuntime},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wave7ProjectionKind {
    SalesQuotes,
    SalesOrders,
    SalesRevenueArrangements,
    SalesLifecycleRecords,
    PeoplePayrollEmployees,
    PeoplePayrollLeaveRequests,
    PeoplePayrollExpenseClaims,
    PeoplePayrollPayrollRuns,
    PosSales,
    PosRefunds,
    PosLifecycleRecords,
}

#[derive(Debug, Clone)]
pub struct Wave7ProjectionExecutor {
    router: HoldingDatabaseRouter,
    kind: Wave7ProjectionKind,
}

impl Wave7ProjectionExecutor {
    pub const fn new(router: HoldingDatabaseRouter, kind: Wave7ProjectionKind) -> Self {
        Self { router, kind }
    }

    pub const fn projection_name(&self) -> &'static str {
        match self.kind {
            Wave7ProjectionKind::SalesQuotes => erpx_sales_sqlx::QUOTE_PROJECTION,
            Wave7ProjectionKind::SalesOrders => erpx_sales_sqlx::ORDER_PROJECTION,
            Wave7ProjectionKind::SalesRevenueArrangements => {
                erpx_sales_sqlx::ARRANGEMENT_PROJECTION
            }
            Wave7ProjectionKind::SalesLifecycleRecords => {
                erpx_sales_sqlx::LIFECYCLE_RECORD_PROJECTION_NAME
            }
            Wave7ProjectionKind::PeoplePayrollEmployees => {
                erpx_people_payroll_sqlx::EMPLOYEE_PROJECTION
            }
            Wave7ProjectionKind::PeoplePayrollLeaveRequests => {
                erpx_people_payroll_sqlx::LEAVE_PROJECTION
            }
            Wave7ProjectionKind::PeoplePayrollExpenseClaims => {
                erpx_people_payroll_sqlx::EXPENSE_PROJECTION
            }
            Wave7ProjectionKind::PeoplePayrollPayrollRuns => {
                erpx_people_payroll_sqlx::PAYROLL_PROJECTION
            }
            Wave7ProjectionKind::PosSales => erpx_pos_sqlx::SALE_PROJECTION_NAME,
            Wave7ProjectionKind::PosRefunds => erpx_pos_sqlx::REFUND_PROJECTION_NAME,
            Wave7ProjectionKind::PosLifecycleRecords => {
                erpx_pos_sqlx::LIFECYCLE_RECORD_PROJECTION_NAME
            }
        }
    }

    async fn runtime(
        &self,
        holding_id: &str,
        data_profile_key: &str,
        page_size: u32,
    ) -> Result<SnapshotRuntime, HoldingProjectionExecutorError> {
        let write = self
            .router
            .route(holding_id, data_profile_key, RoutedDatabaseSide::Write)
            .await
            .map_err(|_| HoldingProjectionExecutorError::Retryable)?;
        let read = self
            .router
            .route(holding_id, data_profile_key, RoutedDatabaseSide::Read)
            .await
            .map_err(|_| HoldingProjectionExecutorError::Retryable)?;
        let holding_id = crate::cqrs_holding_uuid(holding_id)?;
        let write = WritePool::for_holding(write.database_pool().clone(), holding_id)
            .map_err(crate::map_cqrs_projection_error)?;
        let read = ProjectionPool::for_holding(read.database_pool().clone(), holding_id)
            .map_err(crate::map_cqrs_projection_error)?;
        let config = ProjectionRunnerConfig {
            page_size,
            ..ProjectionRunnerConfig::default()
        };
        match self.kind {
            Wave7ProjectionKind::SalesQuotes => SalesProjectionFactory
                .build(write, read, config)
                .map(|runtime| {
                    debug_assert_eq!(
                        runtime.registrations(),
                        SalesProjectionFactory.registrations()
                    );
                    SnapshotRuntime::Sales(runtime, SalesProjectionKind::Quotes)
                }),
            Wave7ProjectionKind::SalesOrders => SalesProjectionFactory
                .build(write, read, config)
                .map(|runtime| {
                    debug_assert_eq!(
                        runtime.registrations(),
                        SalesProjectionFactory.registrations()
                    );
                    SnapshotRuntime::Sales(runtime, SalesProjectionKind::Orders)
                }),
            Wave7ProjectionKind::SalesRevenueArrangements => SalesProjectionFactory
                .build(write, read, config)
                .map(|runtime| {
                    debug_assert_eq!(
                        runtime.registrations(),
                        SalesProjectionFactory.registrations()
                    );
                    SnapshotRuntime::Sales(runtime, SalesProjectionKind::RevenueArrangements)
                }),
            Wave7ProjectionKind::SalesLifecycleRecords => SalesProjectionFactory
                .build(write, read, config)
                .map(|runtime| {
                    debug_assert_eq!(
                        runtime.registrations(),
                        SalesProjectionFactory.registrations()
                    );
                    SnapshotRuntime::Sales(runtime, SalesProjectionKind::LifecycleRecords)
                }),
            Wave7ProjectionKind::PeoplePayrollEmployees => PeoplePayrollProjectionFactory
                .build(write, read, config)
                .map(|runtime| {
                    debug_assert_eq!(
                        runtime.registrations(),
                        PeoplePayrollProjectionFactory.registrations()
                    );
                    SnapshotRuntime::PeoplePayroll(runtime, PeoplePayrollProjectionKind::Employees)
                }),
            Wave7ProjectionKind::PeoplePayrollLeaveRequests => PeoplePayrollProjectionFactory
                .build(write, read, config)
                .map(|runtime| {
                    debug_assert_eq!(
                        runtime.registrations(),
                        PeoplePayrollProjectionFactory.registrations()
                    );
                    SnapshotRuntime::PeoplePayroll(
                        runtime,
                        PeoplePayrollProjectionKind::LeaveRequests,
                    )
                }),
            Wave7ProjectionKind::PeoplePayrollExpenseClaims => PeoplePayrollProjectionFactory
                .build(write, read, config)
                .map(|runtime| {
                    debug_assert_eq!(
                        runtime.registrations(),
                        PeoplePayrollProjectionFactory.registrations()
                    );
                    SnapshotRuntime::PeoplePayroll(
                        runtime,
                        PeoplePayrollProjectionKind::ExpenseClaims,
                    )
                }),
            Wave7ProjectionKind::PeoplePayrollPayrollRuns => PeoplePayrollProjectionFactory
                .build(write, read, config)
                .map(|runtime| {
                    debug_assert_eq!(
                        runtime.registrations(),
                        PeoplePayrollProjectionFactory.registrations()
                    );
                    SnapshotRuntime::PeoplePayroll(
                        runtime,
                        PeoplePayrollProjectionKind::PayrollRuns,
                    )
                }),
            Wave7ProjectionKind::PosSales => {
                PosProjectionFactory
                    .build(write, read, config)
                    .map(|runtime| {
                        debug_assert_eq!(
                            runtime.registrations(),
                            PosProjectionFactory.registrations()
                        );
                        SnapshotRuntime::Pos(runtime, PosProjectionKind::Sales)
                    })
            }
            Wave7ProjectionKind::PosRefunds => {
                PosProjectionFactory
                    .build(write, read, config)
                    .map(|runtime| {
                        debug_assert_eq!(
                            runtime.registrations(),
                            PosProjectionFactory.registrations()
                        );
                        SnapshotRuntime::Pos(runtime, PosProjectionKind::Refunds)
                    })
            }
            Wave7ProjectionKind::PosLifecycleRecords => PosProjectionFactory
                .build(write, read, config)
                .map(|runtime| {
                    debug_assert_eq!(
                        runtime.registrations(),
                        PosProjectionFactory.registrations()
                    );
                    SnapshotRuntime::Pos(runtime, PosProjectionKind::LifecycleRecords)
                }),
        }
        .map_err(crate::map_cqrs_projection_error)
    }
}

#[async_trait]
impl HoldingProjectionExecutor for Wave7ProjectionExecutor {
    fn projection_name_filter(&self) -> Option<&str> {
        Some(self.projection_name())
    }

    async fn execute(
        &self,
        claim: &ProjectionClaimRecord,
    ) -> Result<(), HoldingProjectionExecutorError> {
        if claim.job.payload.get("after").is_some()
            || claim.job.payload.get("cursor").is_some()
            || claim.job.payload.get("candidate_database_ref").is_some()
            || claim.job.payload.get("source_version").is_some()
            || claim.job.payload.get("generation").is_some()
        {
            return Err(HoldingProjectionExecutorError::Permanent);
        }
        let holding_id = &claim.job.binding.holding_id;
        let data_profile_key = &claim.job.binding.data_profile_key;
        let page_size = crate::projection_page_size(&claim.job.payload, 500)?;
        let runtime = self
            .runtime(holding_id, data_profile_key, page_size)
            .await?;
        match claim.job.kind {
            ProjectionJobKind::Incremental => validate_run(
                runtime
                    .run_once()
                    .await
                    .map_err(crate::map_cqrs_projection_error)?,
            ),
            ProjectionJobKind::Rebuild => runtime.rebuild(page_size).await,
        }
    }
}

#[derive(Debug, Clone)]
enum SnapshotRuntime {
    Sales(SalesProjectionRuntime, SalesProjectionKind),
    PeoplePayroll(PeoplePayrollProjectionRuntime, PeoplePayrollProjectionKind),
    Pos(PosProjectionRuntime, PosProjectionKind),
}

impl SnapshotRuntime {
    async fn run_once(&self) -> Result<ProjectionRun, erpx_cqrs_sqlx::CqrsSqlxError> {
        match self {
            Self::Sales(runtime, kind) => runtime.run_once(*kind).await,
            Self::PeoplePayroll(runtime, kind) => runtime.run_once(*kind).await,
            Self::Pos(runtime, kind) => runtime.run_once(*kind).await,
        }
    }

    async fn rebuild(&self, page_size: u32) -> Result<(), HoldingProjectionExecutorError> {
        macro_rules! rebuild {
            ($runtime:expr, $kind:expr) => {{
                let runtime = $runtime;
                let kind = *$kind;
                let handle = runtime
                    .begin_rebuild(kind)
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
            Self::Sales(runtime, kind) => rebuild!(runtime, kind),
            Self::PeoplePayroll(runtime, kind) => rebuild!(runtime, kind),
            Self::Pos(runtime, kind) => rebuild!(runtime, kind),
        }
    }
}

fn validate_run(run: ProjectionRun) -> Result<(), HoldingProjectionExecutorError> {
    if run.retry_scheduled || run.dead_lettered != 0 {
        return Err(HoldingProjectionExecutorError::Retryable);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduler_inventory_covers_all_wave7_read_models() {
        let kinds = [
            Wave7ProjectionKind::SalesQuotes,
            Wave7ProjectionKind::SalesOrders,
            Wave7ProjectionKind::SalesRevenueArrangements,
            Wave7ProjectionKind::PeoplePayrollEmployees,
            Wave7ProjectionKind::PeoplePayrollLeaveRequests,
            Wave7ProjectionKind::PeoplePayrollExpenseClaims,
            Wave7ProjectionKind::PeoplePayrollPayrollRuns,
            Wave7ProjectionKind::PosSales,
            Wave7ProjectionKind::PosRefunds,
        ];
        assert_eq!(kinds.len(), 9);
    }
}
