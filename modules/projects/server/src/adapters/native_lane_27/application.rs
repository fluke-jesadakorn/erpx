//! Owner query port and handler for lane 27 Projects actuals reads. No SQL and
//! no HTTP live here; the port is implemented by the PostgreSQL adapter.

use super::domain::{
    ActualsScope, CostAllocationRow, HandoffProject, HandoffRow, HandoffScope, HandoffWorkRow,
    ProjectRow, WorkLogRow,
};
use erpx_platform_shared_server::public::BusinessError;
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

    /// The three owner-local handoff reads. The same per-Holding reader serves
    /// both families, so each Holding still uses only its own database pool.
    fn handoffs<'a>(
        &'a self,
        scope: &'a HandoffScope,
    ) -> ActualsFuture<'a, Vec<HandoffRow>>;
    fn handoff_project<'a>(
        &'a self,
        scope: &'a HandoffScope,
    ) -> ActualsFuture<'a, Option<HandoffProject>>;
    fn handoff_work<'a>(
        &'a self,
        scope: &'a HandoffScope,
    ) -> ActualsFuture<'a, Vec<HandoffWorkRow>>;

    /// Owner-or-member visible Project rows, point and list.
    fn visible_project<'a>(
        &'a self,
        scope: &'a ActualsScope,
    ) -> ActualsFuture<'a, Option<ProjectRow>>;
    fn visible_projects<'a>(
        &'a self,
        scope: &'a ActualsScope,
    ) -> ActualsFuture<'a, Vec<ProjectRow>>;
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

pub async fn read_handoffs<R: ProjectActualsRead + ?Sized>(
    reader: &R,
    scope: &HandoffScope,
) -> Result<Vec<HandoffRow>, BusinessError> {
    if scope.project.is_empty() {
        return Err(super::domain::invalid());
    }
    let rows = reader.handoffs(scope).await?;
    for row in &rows {
        super::domain::check_handoff(row)?;
    }
    Ok(rows)
}

pub async fn read_handoff_project<R: ProjectActualsRead + ?Sized>(
    reader: &R,
    scope: &HandoffScope,
) -> Result<Option<HandoffProject>, BusinessError> {
    if scope.handoff.is_empty() {
        return Err(super::domain::invalid());
    }
    let row = reader.handoff_project(scope).await?;
    if let Some(project) = &row {
        if !super::domain::valid_id(&project.project_id) {
            return Err(super::domain::unavailable());
        }
    }
    Ok(row)
}

pub async fn read_handoff_work<R: ProjectActualsRead + ?Sized>(
    reader: &R,
    scope: &HandoffScope,
) -> Result<Vec<HandoffWorkRow>, BusinessError> {
    let rows = reader.handoff_work(scope).await?;
    for row in &rows {
        super::domain::check_handoff_work(row)?;
    }
    Ok(rows)
}

pub async fn read_visible_project<R: ProjectActualsRead + ?Sized>(
    reader: &R,
    scope: &ActualsScope,
) -> Result<Option<ProjectRow>, BusinessError> {
    if scope.project.is_empty() {
        return Err(super::domain::invalid());
    }
    let row = reader.visible_project(scope).await?;
    if let Some(project) = &row {
        super::domain::check_project(project)?;
    }
    Ok(row)
}

pub async fn read_visible_projects<R: ProjectActualsRead + ?Sized>(
    reader: &R,
    scope: &ActualsScope,
) -> Result<Vec<ProjectRow>, BusinessError> {
    let rows = reader.visible_projects(scope).await?;
    for row in &rows {
        super::domain::check_project(row)?;
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
        fn handoffs<'a>(
            &'a self,
            _: &'a HandoffScope,
        ) -> ActualsFuture<'a, Vec<HandoffRow>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn handoff_project<'a>(
            &'a self,
            _: &'a HandoffScope,
        ) -> ActualsFuture<'a, Option<HandoffProject>> {
            Box::pin(async { Ok(None) })
        }
        fn handoff_work<'a>(
            &'a self,
            _: &'a HandoffScope,
        ) -> ActualsFuture<'a, Vec<HandoffWorkRow>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn visible_project<'a>(
            &'a self,
            _: &'a ActualsScope,
        ) -> ActualsFuture<'a, Option<ProjectRow>> {
            Box::pin(async { Ok(None) })
        }
        fn visible_projects<'a>(
            &'a self,
            _: &'a ActualsScope,
        ) -> ActualsFuture<'a, Vec<ProjectRow>> {
            Box::pin(async { Ok(vec![]) })
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

        let handoff = domain::handoff_scope("company-1", "user-1", "project-1", "").unwrap();
        assert!(read_handoffs(&reader, &handoff).await.unwrap().is_empty());
        assert!(read_handoff_work(&reader, &handoff).await.unwrap().is_empty());
        let pointed = domain::handoff_scope("company-1", "user-1", "", "handoff-1").unwrap();
        assert!(read_handoff_project(&reader, &pointed).await.unwrap().is_none());
        assert!(read_handoffs(&reader, &pointed).await.is_err());
        assert!(read_handoff_project(&reader, &handoff).await.is_err());

        assert!(read_visible_project(&reader, &scope).await.unwrap().is_none());
        assert!(read_visible_projects(&reader, &scope).await.unwrap().is_empty());
        let list = domain::company_scope("company-1", "user-1").unwrap();
        assert!(read_visible_projects(&reader, &list).await.unwrap().is_empty());
        assert!(read_visible_project(&reader, &list).await.is_err());
    }
}
