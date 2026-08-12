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
erpx_ui_core::declare_feature_registration!(
    PeoplePayrollFeatureSurface,
    "people-payroll",
    "people-payroll.workspace",
    "/workspaces/people-payroll",
    "people",
    "erpx-feature-people-payroll-v1",
    dioxus
);
