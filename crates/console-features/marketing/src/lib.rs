//! Marketing Console vertical slice.
pub mod gateway;
pub mod models;
pub mod presenter;
pub mod view;
pub use gateway::{MarketingGateway, MarketingProtoTransport};
pub use models::{MarketingLeadItem, MarketingLeadItems};
pub use presenter::{MarketingContent, MarketingPresenter};
pub use view::{MarketingRenderedState, render_marketing_state};
erpx_ui_core::declare_feature_registration!(
    AuthenticatedMarketingFeatureSurface,
    "marketing",
    "marketing.workspace",
    "/workspaces/marketing",
    "customer",
    "erpx-feature-marketing-v1",
    dioxus
);
