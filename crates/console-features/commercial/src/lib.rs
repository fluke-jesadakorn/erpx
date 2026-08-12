//! Commercial Console vertical slice. Provider-originated commands are not
//! exported to the UI boundary.

pub mod gateway;
pub mod presenter;
pub mod view;

pub use gateway::{CommercialGateway, CommercialProtoTransport};
pub use presenter::{CommercialContent, CommercialPresenter};
pub use view::{CommercialRenderedState, render_commercial_state};
erpx_ui_core::declare_feature_registration!(
    CommercialFeatureSurface,
    "commercial",
    "commercial.workspace",
    "/platform/commercial",
    "platform",
    "erpx-feature-commercial-v1",
    dioxus
);
