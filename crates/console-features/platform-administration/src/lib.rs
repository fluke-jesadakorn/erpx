//! Platform Administration Console vertical slice.
pub mod gateway;
pub mod model;
pub mod presenter;
pub mod surface;
pub mod view;

pub use gateway::{
    PlatformAdministrationGateway, PlatformAdministrationProtoTransport, admin_context_item,
    connector_item, holding_item,
};
pub use model::{AdminContextItem, ConnectorItem, HoldingItem, PlatformAdministrationContent};
pub use presenter::PlatformAdministrationPresenter;
pub use surface::{
    FieldRedaction, HoldingDetail, HoldingPage, HoldingProjectionEvidence, HoldingsSurface,
    PlatformHoldingDetailSurface,
};
pub use view::{PlatformAdministrationRenderedState, render_platform_administration_state};
erpx_ui_kit::declare_current_workspace_feature!(
    PlatformAdministrationFeatureSurface,
    erpx_ui_kit::CurrentFeatureKind::PlatformAdministration,
    "platform-administration",
    [("platform-administration.workspace", "/platform", "platform")],
    "erpx-feature-platform-administration"
);

#[cfg(test)]
mod registration_contract_tests {
    use super::*;

    #[test]
    fn web_and_native_share_the_authenticated_canonical_route() {
        assert_eq!(REGISTRATION.id, "platform-administration");
        assert_eq!(
            REGISTRATION.artifact_key,
            "erpx-feature-platform-administration"
        );
        assert!(REGISTRATION.routes.iter().all(|route| {
            route.authorization == "authenticated"
                && route.canonical_path.starts_with('/')
                && REGISTRATION.matches_path(route.canonical_path)
        }));
        assert!(REGISTRATION.matches_path("/platform/holding/record-001"));
    }
}
