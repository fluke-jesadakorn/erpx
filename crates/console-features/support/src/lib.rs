//! Target-neutral Support Console vertical slice.

pub mod gateway;
pub mod models;
pub mod presenter;
pub mod view;

pub use gateway::{SupportGateway, SupportProtoTransport, present_mutation, present_query};
pub use models::{
    CapabilityMetadataItem, CustomerMessageItem, CustomerTicketItem, CustomerTicketItems,
    IncidentBroadcastItem, IncidentItem, IncidentItems, IncidentTimelineItem, InternalMessageItem,
    InternalTicketItem, InternalTicketItems, SessionItem,
};
pub use presenter::{SupportContent, SupportPresenter};
pub use view::{SupportRenderedState, render_support_state};
erpx_ui_core::declare_feature_registration!(
    SupportFeatureSurface,
    "support",
    "support.workspace",
    "/support",
    "support",
    "erpx-feature-support-v1",
    dioxus
);
