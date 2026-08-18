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
erpx_ui_kit::declare_current_workspace_feature!(
    SupportFeatureSurface,
    erpx_ui_kit::CurrentFeatureKind::Support,
    "support",
    [("support.workspace", "/support", "support")],
    "erpx-feature-support-v2"
);

#[cfg(test)]
mod registration_contract_tests {
    use super::*;

    #[test]
    fn web_and_native_share_the_authenticated_canonical_route() {
        assert_eq!(REGISTRATION.id, "support");
        assert_eq!(REGISTRATION.artifact_key, "erpx-feature-support-v2");
        assert!(REGISTRATION.routes.iter().all(|route| {
            route.authorization == "authenticated"
                && route.canonical_path.starts_with('/')
                && REGISTRATION.matches_path(route.canonical_path)
        }));
        assert!(REGISTRATION.matches_path("/support/ticket/record-001"));
    }
}
