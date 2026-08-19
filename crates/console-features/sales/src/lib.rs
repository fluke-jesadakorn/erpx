//! Sales Console vertical slice.

use dioxus::prelude::*;
use erpx_ui_core::{FeatureContext, FeatureRegistration, FeatureRouteDescriptor};

pub mod gateway;
pub mod models;
pub mod presenter;
pub mod view;
pub mod workspace;

pub use gateway::{SalesGateway, SalesProtoTransport};
pub use models::{SalesOrderItem, SalesOrderItems, SalesQuoteItem};
pub use presenter::{SalesContent, SalesPresenter};
pub use view::{SalesRenderedState, render_sales_state};
pub use workspace::{
    CatalogChoice, CustomerChoice, SalesFinanceWorkspace, SalesProjectionState,
    SalesSuggestionState, SalesWorkspaceModel, SalesWorkspaceRecord,
};
const FEATURE_ROUTES: &[FeatureRouteDescriptor] = &[FeatureRouteDescriptor {
    id: "sales.workspace",
    canonical_path: "/sales",
    authorization: "authenticated",
    navigation_group: "customer",
}];

/// Sales owns every `/sales/*` route, including the governed fulfillment
/// handoff route. Keeping one surface across these routes makes the primary
/// New quote action available without exposing technical identifiers.
#[component]
pub fn SalesFeatureSurface(context: FeatureContext) -> Element {
    let initial_tab = if context.canonical_path == "/sales/quote"
        || context.canonical_path.starts_with("/sales/quote/")
    {
        "quotes"
    } else if context.canonical_path == "/sales/order"
        || context.canonical_path.starts_with("/sales/order/")
    {
        "orders"
    } else if context.canonical_path == "/sales/fulfillment-handoff"
        || context
            .canonical_path
            .starts_with("/sales/fulfillment-handoff/")
    {
        "revenue"
    } else {
        "dashboard"
    };
    let navigate = context.navigate.clone();
    let on_new_quote = EventHandler::new(move |_| navigate("/sales/quote".to_owned()));
    let scope_label = context
        .active_scope
        .as_ref()
        .map(|scope| scope.reference.clone())
        .unwrap_or_else(|| "Authorized session scope".to_owned());
    let projection = if context.session.authenticated && context.active_scope.is_some() {
        SalesProjectionState::Failed
    } else {
        SalesProjectionState::Lagging
    };
    rsx! {
        SalesFinanceWorkspace {
            model: SalesWorkspaceModel {
                scope_label,
                records: Vec::new(),
                customers: Vec::new(),
                catalog: Vec::new(),
                projection,
                suggestion: SalesSuggestionState::None,
                selected_customer_key: None,
                selected_catalog_key: None,
                custom_line_reason: String::new(),
                language: match context.locale { erpx_ui_core::Locale::Th => "th", erpx_ui_core::Locale::En => "en" }.to_owned(),
                quote_subtotal: "—".to_owned(),
                quote_tax: "—".to_owned(),
                quote_total: "—".to_owned(),
                approval_status: "Draft · not submitted".to_owned(),
            },
            locale: context.locale,
            initial_tab: Some(initial_tab.to_owned()),
            on_new_quote: Some(on_new_quote),
        }
    }
}

fn render_feature_surface(context: &FeatureContext) -> Element {
    rsx! { SalesFeatureSurface { context: context.clone() } }
}

pub static REGISTRATION: FeatureRegistration = FeatureRegistration::with_renderer_and_artifact(
    "sales",
    FEATURE_ROUTES,
    "erpx-feature-sales",
    render_feature_surface,
);

#[cfg(test)]
mod registration_contract_tests {
    use super::*;

    #[test]
    fn web_and_native_share_the_authenticated_canonical_route() {
        assert_eq!(REGISTRATION.id, "sales");
        assert_eq!(REGISTRATION.artifact_key, "erpx-feature-sales");
        assert!(REGISTRATION.routes.iter().all(|route| {
            route.authorization == "authenticated"
                && route.canonical_path.starts_with('/')
                && REGISTRATION.matches_path(route.canonical_path)
        }));
        assert!(REGISTRATION.matches_path("/sales/sales-order/record-001"));
    }
}
