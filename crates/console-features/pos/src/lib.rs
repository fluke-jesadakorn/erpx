//! Target-neutral POS Console vertical slice.

pub mod gateway;
pub mod models;
pub mod presenter;
pub mod view;

pub use gateway::{PosGateway, PosProtoTransport, present_pos_query};
pub use models::{PosRefundItem, PosRefundItems, PosSaleItem, PosSaleItems};
pub use presenter::{PosContent, PosPresenter};
pub use view::{PosRenderedState, render_pos_state};
erpx_ui_core::declare_feature_registration!(
    PosFeatureSurface,
    "pos",
    "pos.workspace",
    "/workspaces/pos",
    "operations",
    "erpx-feature-pos-v1",
    dioxus
);
