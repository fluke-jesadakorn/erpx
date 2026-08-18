//! Target-neutral POS Console vertical slice.

pub mod gateway;
pub mod models;
pub mod presenter;
pub mod view;

pub use gateway::{PosGateway, PosProtoTransport, present_pos_query};
pub use models::{PosRefundItem, PosRefundItems, PosSaleItem, PosSaleItems};
pub use presenter::{PosContent, PosPresenter};
pub use view::{PosRenderedState, render_pos_state};
erpx_ui_kit::declare_current_workspace_feature!(
    PosFeatureSurface,
    erpx_ui_kit::CurrentFeatureKind::PointOfSale,
    "pos",
    [("pos.workspace", "/pos", "operations")],
    "erpx-feature-pos-v2"
);

#[cfg(test)]
mod registration_contract_tests {
    use super::*;

    #[test]
    fn web_and_native_share_the_authenticated_canonical_route() {
        assert_eq!(REGISTRATION.id, "pos");
        assert_eq!(REGISTRATION.artifact_key, "erpx-feature-pos-v2");
        assert!(REGISTRATION.routes.iter().all(|route| {
            route.authorization == "authenticated"
                && route.canonical_path.starts_with('/')
                && REGISTRATION.matches_path(route.canonical_path)
        }));
        assert!(REGISTRATION.matches_path("/pos/sale/record-001"));
    }
}
