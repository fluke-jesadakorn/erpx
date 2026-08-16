//! Finance Console vertical slice.
pub mod gateway;
pub mod models;
pub mod presenter;
pub mod view;
pub use gateway::{FinanceGateway, FinanceProtoTransport};
pub use models::{FinanceItem, FinanceItems};
pub use presenter::{FinanceContent, FinancePresenter};
pub use view::{FinanceRenderedState, render_finance_state};
erpx_ui_kit::declare_current_workspace_feature!(
    FinanceFeatureSurface,
    erpx_ui_kit::CurrentFeatureKind::Finance,
    "finance",
    [("finance.workspace", "/finance", "finance")],
    "erpx-feature-finance-v2"
);
