//! Brand Console vertical slice.

pub mod gateway;
pub mod presenter;
pub mod view;

pub use gateway::{BrandGateway, BrandProtoTransport};
pub use presenter::{BrandContent, BrandPresenter};
pub use view::{BrandRenderedState, render_brand_state};
erpx_ui_kit::declare_current_workspace_feature!(
    BrandFeatureSurface,
    erpx_ui_kit::CurrentFeatureKind::Brand,
    "brand",
    [("brand.workspace", "/brand", "platform")],
    "erpx-feature-brand-v2"
);
