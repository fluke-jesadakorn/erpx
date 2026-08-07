use erpx_cqrs::QueryResult;
use erpx_ui_foundation::{Presenter, PresenterEvent, PresenterState, UiError};

use super::models::{
    EmployeeItem, EmployeeItems, ExpenseClaimItem, ExpenseClaimItems, LeaveRequestItem,
    LeaveRequestItems, PayrollRunItem, PayrollRunItems,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeoplePayrollContent {
    Employee(EmployeeItem),
    Employees(EmployeeItems),
    LeaveRequest(LeaveRequestItem),
    LeaveRequests(LeaveRequestItems),
    ExpenseClaim(ExpenseClaimItem),
    ExpenseClaims(ExpenseClaimItems),
    PayrollRun(PayrollRunItem),
    PayrollRuns(PayrollRunItems),
}

#[derive(Debug, Default)]
pub struct PeoplePayrollPresenter {
    inner: Presenter<PeoplePayrollContent>,
}
impl PeoplePayrollPresenter {
    pub fn state(&self) -> &PresenterState<PeoplePayrollContent> {
        self.inner.state()
    }
    pub fn loading(&mut self) {
        self.inner.transition(PresenterEvent::LoadStarted)
    }
    pub fn submitting(&mut self) {
        self.inner.transition(PresenterEvent::SubmitStarted)
    }
    pub fn resolved(&mut self, result: QueryResult<Option<PeoplePayrollContent>>) {
        self.inner.transition(PresenterEvent::QueryResolved(result))
    }
    pub fn apply(&mut self, event: PresenterEvent<PeoplePayrollContent>) {
        self.inner.transition(event)
    }
    pub fn failed(&mut self, error: UiError) {
        self.inner.transition(PresenterEvent::Failed(error))
    }
    pub fn reset(&mut self) {
        self.inner.transition(PresenterEvent::Reset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_ui_foundation::{PresenterState, UiErrorKind};
    #[test]
    fn progress_and_bounded_recovery_states_are_explicit() {
        let mut presenter = PeoplePayrollPresenter::default();
        presenter.loading();
        assert!(matches!(presenter.state(), PresenterState::Loading));
        presenter.submitting();
        assert!(matches!(presenter.state(), PresenterState::Submitting));
        for kind in [
            UiErrorKind::Denied,
            UiErrorKind::Conflict,
            UiErrorKind::Offline,
            UiErrorKind::NotReady,
        ] {
            presenter.failed(UiError::bounded(
                kind,
                "people_payroll",
                "people payroll",
                true,
                None,
            ));
            assert!(matches!(
                presenter.state(),
                PresenterState::Denied { .. }
                    | PresenterState::Conflict { .. }
                    | PresenterState::Offline { .. }
                    | PresenterState::NotReady { .. }
            ));
        }
    }
}
