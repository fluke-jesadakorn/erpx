pub mod gateway;
pub mod models;
pub mod presenter;
pub mod view;

pub use gateway::{Customer360Gateway, Customer360QueryTransport, present_customer360};
pub use models::*;
pub use presenter::Customer360Presenter;
pub use view::{Customer360RenderedState, render_customer360_state};
erpx_ui_core::declare_feature_registration!(
    Customer360FeatureSurface,
    "customer-360",
    "customer-360.workspace",
    "/platform/customers",
    "customer",
    "erpx-feature-customer-360-v1",
    dioxus
);
