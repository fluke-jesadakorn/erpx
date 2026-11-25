//! Owner query port and handler for lane 27 Projects actuals reads. No SQL and
//! no HTTP live here; the port is implemented by the PostgreSQL adapter.

use super::domain::{ActualsScope, CostAllocationRow, WorkLogRow};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type ActualsFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// The Projects owner owns the query, the projection and the response shape.
/// Composition injects the pg reader; the router only transports the answers.
pub trait ProjectActualsRead: Send + Sync {
    fn work_logs<'a>(&'a self, scope: &'a ActualsScope) -> ActualsFuture<'a, Vec<WorkLogRow>>;
    fn cost_allocations<'a>(
        &'a self,
        scope: &'a ActualsScope,
    ) -> ActualsFuture<'a, Vec<CostAllocationRow>>;
}

pub async fn read_work_logs<R: ProjectActualsRead + ?Sized>(
    reader: &R,
    scope: &ActualsScope,
) -> Result<Vec<WorkLogRow>, BusinessError> {
    let rows = reader.work_logs(scope).await?;
    for row in &rows {
        super::domain::check_work_log(row)?;
    }
    Ok(rows)
}

pub async fn read_cost_allocations<R: ProjectActualsRead + ?Sized>(
    reader: &R,
    scope: &ActualsScope,
) -> Result<Vec<CostAllocationRow>, BusinessError> {
    let rows = reader.cost_allocations(scope).await?;
    for row in &rows {
        super::domain::check_cost_allocation(row)?;
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_27::domain;

    struct Fixed {
        logs: Vec<WorkLogRow>,
        costs: Vec<CostAllocationRow>,
    }

    impl ProjectActualsRead for Fixed {
        fn work_logs<'a>(&'a self, _: &'a ActualsScope) -> ActualsFuture<'a, Vec<WorkLogRow>> {
            Box::pin(async { Ok(self.logs.clone()) })
        }
        fn cost_allocations<'a>(
            &'a self,
            _: &'a ActualsScope,
        ) -> ActualsFuture<'a, Vec<CostAllocationRow>> {
            Box::pin(async { Ok(self.costs.clone()) })
        }
    }

    #[tokio::test]
    async fn port_owns_projected_rows() {
        let scope = domain::scope("company-1", "user-1", "project-1").unwrap();
        let reader = Fixed {
            logs: vec![WorkLogRow {
                id: "log-1".into(),
                project_id: "project-1".into(),
                user_id: "user-1".into(),
                work_date: "2026-11-25".into(),
                minutes: 60,
                state: "submitted".into(),
                version: 1,
                reviewed_by: None,
                rejection_reason: String::new(),
                created_at: 1,
                updated_at: 1,
                is_actor: true,
                can_review: false,
            }],
            costs: vec![CostAllocationRow {
                id: "cost-1".into(),
                project_id: "project-1".into(),
                journal_line_id: "line-1".into(),
                amount_minor: "1250".into(),
                amount: "12.50".into(),
                state: "draft".into(),
                version: 1,
                created_by: "user-1".into(),
                approved_by: None,
                rejection_reason: String::new(),
                created_at: 1,
                updated_at: 1,
                journal_id: "journal-1".into(),
                journal_number: "JV-1".into(),
                entry_date: "2026-11-25".into(),
                currency: "THB".into(),
                journal_status: "posted".into(),
                account_code: "5000".into(),
                account_name: "Expense".into(),
                can_review: true,
                is_creator: true,
            }],
        };
        assert_eq!(read_work_logs(&reader, &scope).await.unwrap().len(), 1);
        let costs = read_cost_allocations(&reader, &scope).await.unwrap();
        assert_eq!(costs[0].amount, "12.50");
    }
}
