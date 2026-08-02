//! Target-neutral route contracts for the canonical ERPX business-unit shell.
//!
//! Navigation metadata never authorizes reads or mutations. The authenticated
//! server boundary still evaluates identity, role, scope, entitlement, billing,
//! MFA, and separation-of-duties evidence for every request.

use crate::{NavItem, RouteAuthRequirement, redact_deep_link};

pub use erpx_domain::data_plane::BusinessUnitId as ProductContextId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductContextRoute {
    pub id: ProductContextId,
    pub path: &'static str,
    pub label: &'static str,
    pub auth: RouteAuthRequirement,
    pub read_only_shell: bool,
}

pub const PRODUCT_CONTEXT_ROUTES: &[ProductContextRoute] = &[
    route(ProductContextId::Operations, "Operations"),
    route(ProductContextId::Finance, "Finance"),
    route(ProductContextId::Sales, "Sales"),
    route(ProductContextId::Procurement, "Procurement"),
    route(ProductContextId::Inventory, "Inventory"),
    route(ProductContextId::Assets, "Assets"),
    route(ProductContextId::Maintenance, "Maintenance"),
    route(ProductContextId::Quality, "Quality"),
    route(ProductContextId::MarketingCrm, "Marketing & CRM"),
    route(ProductContextId::HrPayroll, "HR & Payroll"),
    route(ProductContextId::PointOfSale, "Point of Sale"),
    route(ProductContextId::Legal, "Legal"),
];

const fn route(id: ProductContextId, label: &'static str) -> ProductContextRoute {
    ProductContextRoute {
        id,
        path: workspace_path(id),
        label,
        auth: RouteAuthRequirement::Authenticated,
        read_only_shell: true,
    }
}

const fn workspace_path(id: ProductContextId) -> &'static str {
    match id {
        ProductContextId::Operations => "/workspaces/operations",
        ProductContextId::Finance => "/workspaces/finance",
        ProductContextId::Sales => "/workspaces/sales",
        ProductContextId::Procurement => "/workspaces/procurement",
        ProductContextId::Inventory => "/workspaces/inventory",
        ProductContextId::Assets => "/workspaces/assets",
        ProductContextId::Maintenance => "/workspaces/maintenance",
        ProductContextId::Quality => "/workspaces/quality",
        ProductContextId::MarketingCrm => "/workspaces/marketing-crm",
        ProductContextId::HrPayroll => "/workspaces/hr-payroll",
        ProductContextId::PointOfSale => "/workspaces/point-of-sale",
        ProductContextId::Legal => "/workspaces/legal",
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
    pub fn nav_item(self) -> NavItem {
        NavItem::new(self.label, self.path, product_context_icon(self.id))
    }

    pub fn resolve(path: &str) -> Option<Self> {
        let normalized = redact_deep_link(path);
        PRODUCT_CONTEXT_ROUTES
            .iter()
            .copied()
            .find(|route| route.path == normalized)
    }
}

pub fn product_context_nav_items() -> Vec<NavItem> {
    PRODUCT_CONTEXT_ROUTES
        .iter()
        .copied()
        .map(ProductContextRoute::nav_item)
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
                && matches!(route.auth, crate::RouteAuthRequirement::Authenticated)
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
            product_context_nav_items().len(),
            PRODUCT_CONTEXT_ROUTES.len()
        );
    }
}
