//! Identity & Access Console vertical slice.

pub mod gateway;
pub mod model;
pub mod presenter;
pub mod view;

pub use gateway::{IdentityAccessGateway, IdentityAccessProtoTransport};
pub use model::{IdentityAccessContent, PrincipalItem, RoleItem};
pub use presenter::IdentityAccessPresenter;
pub use view::{IdentityAccessRenderedState, render_identity_access_state};
erpx_ui_kit::declare_current_workspace_feature!(
    IdentityAccessFeatureSurface,
    erpx_ui_kit::CurrentFeatureKind::IdentityAccess,
    "identity-access",
    [("identity-access.workspace", "/identity", "platform")],
    "erpx-feature-identity-access-v2"
);
