//! Customer Authority Console vertical slice.

pub mod gateway;
pub mod presenter;
pub mod view;

pub use gateway::{CustomerAuthorityGateway, CustomerAuthorityProtoTransport};
pub use presenter::{CustomerAuthorityContent, CustomerAuthorityPresenter};
pub use view::{CustomerAuthorityRenderedState, render_customer_authority_state};
erpx_ui_kit::declare_current_workspace_feature!(
    CustomerAuthorityFeatureSurface,
    erpx_ui_kit::CurrentFeatureKind::CustomerAuthority,
    "customer-authority",
    [("customer-authority.workspace", "/customer", "customer")],
    "erpx-feature-customer-authority"
);
