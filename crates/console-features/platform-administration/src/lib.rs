//! Platform Administration Console vertical slice.
pub mod gateway;
pub mod model;
pub mod presenter;
pub mod view;

pub use gateway::{
    PlatformAdministrationGateway, PlatformAdministrationProtoTransport, admin_context_item,
    connector_item, tenant_item,
};
pub use model::{AdminContextItem, ConnectorItem, PlatformAdministrationContent, TenantItem};
pub use presenter::PlatformAdministrationPresenter;
pub use view::{PlatformAdministrationRenderedState, render_platform_administration_state};
erpx_ui_core::declare_feature_registration!(
    PlatformAdministrationFeatureSurface,
    "platform-administration",
    "platform-administration.workspace",
    "/platform/administration",
    "platform",
    "erpx-feature-platform-administration-v1",
    dioxus
);
