//! Target-neutral route contracts for the canonical ERPX business-unit shell.
//!
//! Navigation metadata never authorizes reads or mutations. The authenticated
//! server boundary still evaluates identity, role, scope, entitlement, billing,
//! MFA, and separation-of-duties evidence for every request.

use erpx_ui_core::{Locale, NavItem, RouteAuthRequirement, redact_deep_link};

pub use erpx_domain::data_plane::{
    BUSINESS_UNIT_ROUTE_CATALOG, BusinessUnitId as ProductContextId, BusinessUnitRouteDescriptor,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductContextRoute {
    pub id: ProductContextId,
    pub route_id: &'static str,
    pub path: &'static str,
    pub label: &'static str,
    pub label_key: &'static str,
    pub permission: &'static str,
    pub entitlement_key: &'static str,
    pub auth: RouteAuthRequirement,
    pub read_only_shell: bool,
}

pub const PRODUCT_CONTEXT_ROUTES: &[ProductContextRoute] = &[
    route(&BUSINESS_UNIT_ROUTE_CATALOG[0]),
    route(&BUSINESS_UNIT_ROUTE_CATALOG[1]),
    route(&BUSINESS_UNIT_ROUTE_CATALOG[2]),
    route(&BUSINESS_UNIT_ROUTE_CATALOG[3]),
    route(&BUSINESS_UNIT_ROUTE_CATALOG[4]),
    route(&BUSINESS_UNIT_ROUTE_CATALOG[5]),
    route(&BUSINESS_UNIT_ROUTE_CATALOG[6]),
    route(&BUSINESS_UNIT_ROUTE_CATALOG[7]),
    route(&BUSINESS_UNIT_ROUTE_CATALOG[8]),
    route(&BUSINESS_UNIT_ROUTE_CATALOG[9]),
    route(&BUSINESS_UNIT_ROUTE_CATALOG[10]),
    route(&BUSINESS_UNIT_ROUTE_CATALOG[11]),
];

const fn route(descriptor: &'static BusinessUnitRouteDescriptor) -> ProductContextRoute {
    ProductContextRoute {
        id: descriptor.unit,
        route_id: descriptor.route_id,
        path: descriptor.canonical_path,
        label: descriptor.label_en,
        label_key: descriptor.label_key,
        permission: descriptor.read_permission,
        entitlement_key: descriptor.entitlement_key,
        auth: RouteAuthRequirement::Authenticated,
        read_only_shell: true,
    }
}

const fn product_context_icon(id: ProductContextId) -> &'static str {
    match id {
        ProductContextId::Operations => "▸",
        ProductContextId::Finance => "¤",
        ProductContextId::Sales => "↗",
        ProductContextId::Procurement => "⇄",
        ProductContextId::Inventory => "▤",
        ProductContextId::Assets => "◆",
        ProductContextId::Maintenance => "⚙",
        ProductContextId::Quality => "✓",
        ProductContextId::MarketingCrm => "◇",
        ProductContextId::HrPayroll => "♙",
        ProductContextId::PointOfSale => "▣",
        ProductContextId::Legal => "§",
    }
}

impl ProductContextRoute {
    pub fn label_for(self, locale: Locale) -> &'static str {
        match locale {
            Locale::En => self.id.route().label_en,
            Locale::Th => self.id.route().label_th,
        }
    }

    pub fn nav_item(self, locale: Locale) -> NavItem {
        NavItem::new(
            self.label_for(locale),
            self.path,
            product_context_icon(self.id),
        )
        .with_manifest_metadata(self.route_id, self.label_key)
    }

    pub fn resolve(path: &str) -> Option<Self> {
        let normalized = redact_deep_link(path);
        PRODUCT_CONTEXT_ROUTES
            .iter()
            .copied()
            .find(|route| route.path == normalized)
    }
}

pub fn product_context_nav_items(locale: Locale) -> Vec<NavItem> {
    PRODUCT_CONTEXT_ROUTES
        .iter()
        .copied()
        .map(|route| route.nav_item(locale))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        PRODUCT_CONTEXT_ROUTES, ProductContextId, ProductContextRoute, product_context_nav_items,
    };

    #[test]
    fn registry_covers_the_canonical_twelve_business_units() {
        assert_eq!(PRODUCT_CONTEXT_ROUTES.len(), 12);
        assert!(PRODUCT_CONTEXT_ROUTES.iter().all(|route| {
            route.read_only_shell
                && matches!(
                    route.auth,
                    erpx_ui_core::RouteAuthRequirement::Authenticated
                )
        }));
        assert_eq!(PRODUCT_CONTEXT_ROUTES[0].id, ProductContextId::Operations);
        assert_eq!(PRODUCT_CONTEXT_ROUTES[5].id, ProductContextId::Assets);
        assert_eq!(ProductContextId::MarketingCrm.as_str(), "marketing-crm");
        assert!(
            !PRODUCT_CONTEXT_ROUTES
                .iter()
                .any(|route| route.path.contains("support"))
        );
    }

    #[test]
    fn route_resolution_redacts_credentials_and_rejects_old_aliases() {
        let route = ProductContextRoute::resolve("/workspaces/sales?view=summary#details")
            .expect("safe sales route");
        assert_eq!(route.id, ProductContextId::Sales);
        assert!(
            ProductContextRoute::resolve("/workspaces/sales?access_token=secret#details").is_none()
        );
        assert!(ProductContextRoute::resolve("/workspaces/sales/../legal").is_none());
        assert!(ProductContextRoute::resolve("/sales").is_none());
        assert_eq!(
            product_context_nav_items(erpx_ui_core::Locale::En).len(),
            PRODUCT_CONTEXT_ROUTES.len()
        );
    }
}
