//! Customer Authority Console vertical slice.

pub mod gateway;
pub mod presenter;
pub mod view;

pub use gateway::{CustomerAuthorityGateway, CustomerAuthorityProtoTransport};
pub use presenter::{CustomerAuthorityContent, CustomerAuthorityPresenter};
pub use view::{CustomerAuthorityRenderedState, render_customer_authority_state};
erpx_ui_core::declare_feature_registration!(
    CustomerAuthorityFeatureSurface,
    "customer-authority",
    "customer-authority.workspace",
    "/platform/customer-authority",
    "customer",
    "erpx-feature-customer-authority-v1",
    dioxus
);
