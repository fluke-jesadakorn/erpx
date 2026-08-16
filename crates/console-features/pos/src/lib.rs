//! Target-neutral POS Console vertical slice.

pub mod gateway;
pub mod models;
pub mod presenter;
pub mod view;

pub use gateway::{PosGateway, PosProtoTransport, present_pos_query};
pub use models::{PosRefundItem, PosRefundItems, PosSaleItem, PosSaleItems};
pub use presenter::{PosContent, PosPresenter};
pub use view::{PosRenderedState, render_pos_state};
erpx_ui_kit::declare_current_workspace_feature!(
    PosFeatureSurface,
    erpx_ui_kit::CurrentFeatureKind::PointOfSale,
    "pos",
    [("pos.workspace", "/pos", "operations")],
    "erpx-feature-pos-v2"
);
