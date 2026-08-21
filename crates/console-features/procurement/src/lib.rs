pub mod gateway;
pub mod models;
pub mod owner_detail;
pub mod presenter;
pub mod view;

pub use gateway::{
    ProcurementGateway, ProcurementProtoTransport, present_procurement_order,
    present_procurement_orders,
};
pub use models::{ProcurementOrder, ProcurementOrderLine, ProcurementOrders};
pub use owner_detail::*;
pub use presenter::{ProcurementContent, ProcurementPresenter};
pub use view::{ProcurementRenderedState, render_procurement_state};
erpx_ui_kit::declare_current_workspace_feature!(
    ProcurementFeatureSurface,
    erpx_ui_kit::CurrentFeatureKind::Procurement,
    "procurement",
    [("procurement.workspace", "/procurement", "operations")],
    "erpx-feature-procurement"
);

#[cfg(test)]
mod registration_contract_tests {
    use super::*;

    #[test]
    fn web_and_native_share_the_authenticated_canonical_route() {
        assert_eq!(REGISTRATION.id, "procurement");
        assert_eq!(REGISTRATION.artifact_key, "erpx-feature-procurement");
        assert!(REGISTRATION.routes.iter().all(|route| {
            route.authorization == "authenticated"
                && route.canonical_path.starts_with('/')
                && REGISTRATION.matches_path(route.canonical_path)
        }));
        assert!(REGISTRATION.matches_path("/procurement/procurement-order/record-001"));
    }
}
