//! Target-neutral Assets Console vertical slice.

pub mod gateway;
pub mod models;
pub mod presenter;
pub mod view;

pub use gateway::{AssetsGateway, AssetsProtoTransport, present_asset, present_assets};
pub use models::{AssetItem, AssetItems};
pub use presenter::{AssetsContent, AssetsPresenter};
pub use view::{AssetsRenderedState, render_assets_state};
erpx_ui_kit::declare_current_workspace_feature!(
    AssetsFeatureSurface,
    erpx_ui_kit::CurrentFeatureKind::Assets,
    "assets",
    [("assets.workspace", "/asset", "operations")],
    "erpx-feature-assets"
);
