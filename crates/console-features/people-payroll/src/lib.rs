//! Target-neutral People & Payroll Console vertical slice.

pub mod gateway;
pub mod models;
pub mod presenter;
pub mod view;

pub use gateway::{
    PeoplePayrollGateway, PeoplePayrollProtoTransport, present_mutation, present_query,
};
pub use models::{
    EmployeeItem, EmployeeItems, ExpenseClaimItem, ExpenseClaimItems, LeaveRequestItem,
    LeaveRequestItems, PayrollLineItem, PayrollRunItem, PayrollRunItems,
};
pub use presenter::{PeoplePayrollContent, PeoplePayrollPresenter};
pub use view::{PeoplePayrollRenderedState, render_people_payroll_state};
erpx_ui_kit::declare_current_workspace_feature!(
    PeoplePayrollFeatureSurface,
    erpx_ui_kit::CurrentFeatureKind::PeoplePayroll,
    "people-payroll",
    [
        ("people-payroll.workspace", "/people", "people"),
        ("payroll.workspace", "/payroll", "people")
    ],
    "erpx-feature-people-payroll-v2"
);

#[cfg(test)]
mod registration_contract_tests {
    use super::*;

    #[test]
    fn web_and_native_share_authenticated_canonical_routes() {
        assert_eq!(REGISTRATION.id, "people-payroll");
        assert_eq!(REGISTRATION.artifact_key, "erpx-feature-people-payroll-v2");
        assert!(REGISTRATION.routes.iter().all(|route| {
            route.authorization == "authenticated"
                && route.canonical_path.starts_with('/')
                && REGISTRATION.matches_path(route.canonical_path)
        }));
        assert!(REGISTRATION.matches_path("/people/employee/record-001"));
        assert!(REGISTRATION.matches_path("/payroll/payroll-run/record-001"));
    }
}
