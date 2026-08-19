//! Marketing Console vertical slice.
pub mod gateway;
pub mod models;
pub mod presenter;
pub mod view;
pub use gateway::{MarketingGateway, MarketingProtoTransport};
pub use models::{MarketingLeadItem, MarketingLeadItems};
pub use presenter::{MarketingContent, MarketingPresenter};
pub use view::{MarketingRenderedState, render_marketing_state};
erpx_ui_kit::declare_current_workspace_feature!(
    AuthenticatedMarketingFeatureSurface,
    erpx_ui_kit::CurrentFeatureKind::Marketing,
    "marketing",
    [("marketing.workspace", "/marketing", "customer")],
    "erpx-feature-marketing"
);
