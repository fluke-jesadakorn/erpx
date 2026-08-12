//! Quality Console vertical slice.
//!
//! Transport conversion is isolated in [`gateway`]. Presenter and view code
//! consume only the target-neutral Quality read model and UI foundation.

pub mod gateway;
pub mod presenter;
pub mod view;

pub use gateway::{QualityGateway, QualityProtoTransport};
pub use presenter::{QualityContent, QualityPresenter};
pub use view::{QualityRenderedState, render_quality_state};
erpx_ui_core::declare_feature_registration!(
    QualityFeatureSurface,
    "quality",
    "quality.workspace",
    "/workspaces/quality",
    "operations",
    "erpx-feature-quality-v1",
    dioxus
);
