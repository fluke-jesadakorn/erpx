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
erpx_ui_kit::declare_current_workspace_feature!(
    PlatformAdministrationFeatureSurface,
    erpx_ui_kit::CurrentFeatureKind::PlatformAdministration,
    "platform-administration",
    [("platform-administration.workspace", "/platform", "platform")],
    "erpx-feature-platform-administration-v2"
);
