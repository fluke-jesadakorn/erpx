pub mod gateway;
pub mod models;
pub mod presenter;
pub mod view;

pub use gateway::{
    ProcurementGateway, ProcurementProtoTransport, present_procurement_order,
    present_procurement_orders,
};
pub use models::{ProcurementOrder, ProcurementOrderLine, ProcurementOrders};
pub use presenter::{ProcurementContent, ProcurementPresenter};
pub use view::{ProcurementRenderedState, render_procurement_state};
erpx_ui_kit::declare_current_workspace_feature!(
    ProcurementFeatureSurface,
    erpx_ui_kit::CurrentFeatureKind::Procurement,
    "procurement",
    [("procurement.workspace", "/procurement", "operations")],
    "erpx-feature-procurement-v2"
);
