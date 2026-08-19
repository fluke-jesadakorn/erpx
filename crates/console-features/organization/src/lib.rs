//! Organization Console vertical slice.

pub mod gateway;
pub mod presenter;
pub mod view;

pub use gateway::{OrganizationGateway, OrganizationProtoTransport};
pub use presenter::{OrganizationContent, OrganizationPresenter};
pub use view::{OrganizationRenderedState, render_organization_state};
erpx_ui_kit::declare_current_workspace_feature!(
    OrganizationFeatureSurface,
    erpx_ui_kit::CurrentFeatureKind::Organization,
    "organization",
    [("organization.workspace", "/organization", "platform")],
    "erpx-feature-organization"
);
