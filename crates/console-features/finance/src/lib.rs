//! Finance Console vertical slice.
pub mod gateway;
pub mod models;
pub mod owner_detail;
pub mod presenter;
pub mod sales_workspace;
pub mod view;
pub use gateway::{
    FinanceGateway, FinancePayablesGateway, FinancePayablesProtoTransport, FinanceProtoTransport,
    present_payables_mutation, present_payables_query,
};
pub use models::{FinanceItem, FinanceItems};
pub use owner_detail::*;
pub use presenter::{FinanceContent, FinancePresenter};
pub use sales_workspace::{
    FinanceApprovalWorkItem, FinanceSalesApprovalWorkspace, FinanceSalesProjectionState,
    FinanceSalesWorkspaceModel, SalesFinanceReconciliationRow,
};
pub use view::{FinanceRenderedState, render_finance_state};
erpx_ui_kit::declare_current_workspace_feature!(
    FinanceFeatureSurface,
    erpx_ui_kit::CurrentFeatureKind::Finance,
    "finance",
    [("finance.workspace", "/finance", "finance")],
    "erpx-feature-finance"
);
