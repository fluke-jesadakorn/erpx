//! Organization Console vertical slice.

pub mod gateway;
pub mod presenter;
pub mod view;

pub use gateway::{OrganizationGateway, OrganizationProtoTransport};
pub use presenter::{OrganizationContent, OrganizationPresenter};
pub use view::{OrganizationRenderedState, render_organization_state};
erpx_ui_core::declare_feature_registration!(
    OrganizationFeatureSurface,
    "organization",
    "organization.workspace",
    "/platform/organization",
    "platform",
    "erpx-feature-organization-v1",
    dioxus
);
