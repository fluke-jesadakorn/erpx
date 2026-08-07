//! Tenant People & Payroll projection runtimes.

use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionPool, ProjectionRun, ProjectionRunner, ProjectionRunnerConfig,
    ProjectionSource, ProjectionStore, Projector, RebuildHandle, WritePool,
};
use erpx_people_payroll_sqlx::{
    EmployeeProjector, ExpenseClaimProjector, LeaveRequestProjector, PayrollRunProjector,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeoplePayrollProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeoplePayrollProjectionKind {
    Employees,
    LeaveRequests,
    ExpenseClaims,
    PayrollRuns,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PeoplePayrollProjectionFactory;

impl PeoplePayrollProjectionFactory {
    pub fn registrations(self) -> Vec<PeoplePayrollProjectionRegistration> {
        [
            registration(&EmployeeProjector::new()),
            registration(&LeaveRequestProjector::new()),
            registration(&ExpenseClaimProjector::new()),
            registration(&PayrollRunProjector::new()),
        ]
        .into()
    }

    pub fn build(
        self,
        write: WritePool,
        projection: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<PeoplePayrollProjectionRuntime, CqrsSqlxError> {
        let store = ProjectionStore::new(projection);
        let runner = ProjectionRunner::new(ProjectionSource::new(write), store.clone(), config)?;
        Ok(PeoplePayrollProjectionRuntime {
            employees: EmployeeProjector::new(),
            leave_requests: LeaveRequestProjector::new(),
            expense_claims: ExpenseClaimProjector::new(),
            payroll_runs: PayrollRunProjector::new(),
            runner,
            store,
        })
    }
}

#[derive(Debug, Clone)]
pub struct PeoplePayrollProjectionRuntime {
    employees: EmployeeProjector,
    leave_requests: LeaveRequestProjector,
    expense_claims: ExpenseClaimProjector,
    payroll_runs: PayrollRunProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}

impl PeoplePayrollProjectionRuntime {
    pub fn registrations(&self) -> Vec<PeoplePayrollProjectionRegistration> {
        PeoplePayrollProjectionFactory.registrations()
    }

    pub async fn run_once(
        &self,
        kind: PeoplePayrollProjectionKind,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            PeoplePayrollProjectionKind::Employees => self.runner.run_active(&self.employees).await,
            PeoplePayrollProjectionKind::LeaveRequests => {
                self.runner.run_active(&self.leave_requests).await
            }
            PeoplePayrollProjectionKind::ExpenseClaims => {
                self.runner.run_active(&self.expense_claims).await
            }
            PeoplePayrollProjectionKind::PayrollRuns => {
                self.runner.run_active(&self.payroll_runs).await
            }
        }
    }

    pub async fn begin_rebuild(
        &self,
        kind: PeoplePayrollProjectionKind,
    ) -> Result<RebuildHandle, CqrsSqlxError> {
        match kind {
            PeoplePayrollProjectionKind::Employees => {
                self.store.begin_rebuild(&self.employees).await
            }
            PeoplePayrollProjectionKind::LeaveRequests => {
                self.store.begin_rebuild(&self.leave_requests).await
            }
            PeoplePayrollProjectionKind::ExpenseClaims => {
                self.store.begin_rebuild(&self.expense_claims).await
            }
            PeoplePayrollProjectionKind::PayrollRuns => {
                self.store.begin_rebuild(&self.payroll_runs).await
            }
        }
    }

    pub async fn run_rebuild_page(
        &self,
        kind: PeoplePayrollProjectionKind,
        handle: &RebuildHandle,
        after_sequence: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            PeoplePayrollProjectionKind::Employees => {
                self.runner
                    .run_rebuild(&self.employees, handle, after_sequence)
                    .await
            }
            PeoplePayrollProjectionKind::LeaveRequests => {
                self.runner
                    .run_rebuild(&self.leave_requests, handle, after_sequence)
                    .await
            }
            PeoplePayrollProjectionKind::ExpenseClaims => {
                self.runner
                    .run_rebuild(&self.expense_claims, handle, after_sequence)
                    .await
            }
            PeoplePayrollProjectionKind::PayrollRuns => {
                self.runner
                    .run_rebuild(&self.payroll_runs, handle, after_sequence)
                    .await
            }
        }
    }

    pub async fn activate_rebuild(
        &self,
        kind: PeoplePayrollProjectionKind,
        handle: &RebuildHandle,
    ) -> Result<(), CqrsSqlxError> {
        let expected = match kind {
            PeoplePayrollProjectionKind::Employees => self.employees.name(),
            PeoplePayrollProjectionKind::LeaveRequests => self.leave_requests.name(),
            PeoplePayrollProjectionKind::ExpenseClaims => self.expense_claims.name(),
            PeoplePayrollProjectionKind::PayrollRuns => self.payroll_runs.name(),
        };
        if handle.projection != expected {
            return Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another People & Payroll projector",
            ));
        }
        self.store.activate_rebuild(handle).await
    }
}

fn registration(projector: &impl Projector) -> PeoplePayrollProjectionRegistration {
    PeoplePayrollProjectionRegistration {
        name: projector.name().into(),
        event_types: projector.event_types().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_people_payroll_sqlx::{
        EMPLOYEE_PROJECTION, EMPLOYEE_SNAPSHOT_EVENT, EXPENSE_PROJECTION, EXPENSE_SNAPSHOT_EVENT,
        LEAVE_PROJECTION, LEAVE_SNAPSHOT_EVENT, PAYROLL_PROJECTION, PAYROLL_SNAPSHOT_EVENT,
    };

    #[test]
    fn registrations_cover_four_rebuildable_snapshot_projections() {
        assert_eq!(
            PeoplePayrollProjectionFactory.registrations(),
            vec![
                PeoplePayrollProjectionRegistration {
                    name: EMPLOYEE_PROJECTION.into(),
                    event_types: vec![EMPLOYEE_SNAPSHOT_EVENT.into()]
                },
                PeoplePayrollProjectionRegistration {
                    name: LEAVE_PROJECTION.into(),
                    event_types: vec![LEAVE_SNAPSHOT_EVENT.into()]
                },
                PeoplePayrollProjectionRegistration {
                    name: EXPENSE_PROJECTION.into(),
                    event_types: vec![EXPENSE_SNAPSHOT_EVENT.into()]
                },
                PeoplePayrollProjectionRegistration {
                    name: PAYROLL_PROJECTION.into(),
                    event_types: vec![PAYROLL_SNAPSHOT_EVENT.into()]
                },
            ]
        );
    }
}
