//! Identity & Access Console vertical slice.

pub mod gateway;
pub mod model;
pub mod presenter;
pub mod view;

pub use gateway::{IdentityAccessGateway, IdentityAccessProtoTransport};
pub use model::{IdentityAccessContent, PrincipalItem, RoleItem};
pub use presenter::IdentityAccessPresenter;
pub use view::{IdentityAccessRenderedState, render_identity_access_state};
erpx_ui_core::declare_feature_registration!(
    IdentityAccessFeatureSurface,
    "identity-access",
    "identity-access.workspace",
    "/platform/identity-access",
    "platform",
    "erpx-feature-identity-access-v1",
    dioxus
);
