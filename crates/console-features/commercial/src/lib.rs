//! Commercial Console vertical slice. Provider-originated commands are not
//! exported to the UI boundary.

pub mod gateway;
pub mod presenter;
pub mod view;

pub use gateway::{CommercialGateway, CommercialProtoTransport};
pub use presenter::{CommercialContent, CommercialPresenter};
pub use view::{CommercialRenderedState, render_commercial_state};
erpx_ui_kit::declare_current_workspace_feature!(
    CommercialFeatureSurface,
    erpx_ui_kit::CurrentFeatureKind::Commercial,
    "commercial",
    [("commercial.workspace", "/commercial", "platform")],
    "erpx-feature-commercial-v2"
);
