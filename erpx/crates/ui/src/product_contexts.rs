//! Target-neutral route contracts for the ERPX product-context shell.
//!
//! These definitions are navigation metadata only.  The authenticated
//! application boundary still owns authorization, data loading, mutation
//! commands, and projection freshness.  Product pages therefore remain safe
//! to render as read-only shells until an authorized read model is supplied.

use crate::{NavItem, RouteAuthRequirement, redact_deep_link};

/// Product contexts exposed by the ERPX shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductContextId {
    /// Sales and Lead-to-Cash.
    Sales,
    /// Supplier and Procure-to-Pay workflows.
    Procurement,
    /// Stock and inventory flow.
    Inventory,
    /// Asset lifecycle.
    Asset,
    /// Maintenance and service work.
    Maintenance,
    /// Quality inspections and release disposition.
    Quality,
    /// HR and payroll.
    HrPayroll,
    /// Legal contracts and documents.
    Legal,
    /// Point of sale.
    Pos,
    /// Marketing and CRM.
    MarketingCrm,
}

impl ProductContextId {
    /// Return the stable context token used by authenticated application
    /// adapters and telemetry labels.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sales => "sales",
            Self::Procurement => "procurement",
            Self::Inventory => "inventory",
            Self::Asset => "asset",
            Self::Maintenance => "maintenance",
            Self::Quality => "quality",
            Self::HrPayroll => "hr_payroll",
            Self::Legal => "legal",
            Self::Pos => "pos",
            Self::MarketingCrm => "marketing_crm",
        }
    }
}

/// One authenticated product shell entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductContextRoute {
    /// Stable product context identifier.
    pub id: ProductContextId,
    /// Canonical application path.
    pub path: &'static str,
    /// Fallback label before the consuming app applies its catalog.
    pub label: &'static str,
    /// Server-side session requirement mirrored for navigation UX.
    pub auth: RouteAuthRequirement,
    /// Product shell must not infer mutation authority from navigation.
    pub read_only_shell: bool,
}

/// Canonical G9–G11 product-context shell registry.
pub const PRODUCT_CONTEXT_ROUTES: &[ProductContextRoute] = &[
    ProductContextRoute {
        id: ProductContextId::Sales,
        path: "/sales",
        label: "Sales",
        auth: RouteAuthRequirement::Authenticated,
        read_only_shell: true,
    },
    ProductContextRoute {
        id: ProductContextId::Procurement,
        path: "/procurement",
        label: "Procurement",
        auth: RouteAuthRequirement::Authenticated,
        read_only_shell: true,
    },
    ProductContextRoute {
        id: ProductContextId::Inventory,
        path: "/inventory",
        label: "Inventory",
        auth: RouteAuthRequirement::Authenticated,
        read_only_shell: true,
    },
    ProductContextRoute {
        id: ProductContextId::Asset,
        path: "/assets",
        label: "Assets",
        auth: RouteAuthRequirement::Authenticated,
        read_only_shell: true,
    },
    ProductContextRoute {
        id: ProductContextId::Maintenance,
        path: "/maintenance",
        label: "Maintenance",
        auth: RouteAuthRequirement::Authenticated,
        read_only_shell: true,
    },
    ProductContextRoute {
        id: ProductContextId::Quality,
        path: "/quality",
        label: "Quality",
        auth: RouteAuthRequirement::Authenticated,
        read_only_shell: true,
    },
    ProductContextRoute {
        id: ProductContextId::HrPayroll,
        path: "/hr",
        label: "HR & Payroll",
        auth: RouteAuthRequirement::Authenticated,
        read_only_shell: true,
    },
    ProductContextRoute {
        id: ProductContextId::Legal,
        path: "/legal",
        label: "Legal",
        auth: RouteAuthRequirement::Authenticated,
        read_only_shell: true,
    },
    ProductContextRoute {
        id: ProductContextId::Pos,
        path: "/pos",
        label: "Point of Sale",
        auth: RouteAuthRequirement::Authenticated,
        read_only_shell: true,
    },
    ProductContextRoute {
        id: ProductContextId::MarketingCrm,
        path: "/marketing",
        label: "Marketing & CRM",
        auth: RouteAuthRequirement::Authenticated,
        read_only_shell: true,
    },
];

impl ProductContextRoute {
    /// Convert this definition into a safe navigation item.
    pub fn nav_item(self) -> NavItem {
        NavItem::new(self.label, self.path, self.id.icon())
    }

    /// Resolve an exact shell path after removing query and fragment data.
    pub fn resolve(path: &str) -> Option<Self> {
        let normalized = redact_deep_link(path);
        PRODUCT_CONTEXT_ROUTES
            .iter()
            .copied()
            .find(|route| route.path == normalized)
    }
}

impl ProductContextId {
    const fn icon(self) -> &'static str {
        match self {
            Self::Sales => "↗",
            Self::Procurement => "⇄",
            Self::Inventory => "▤",
            Self::Asset => "◆",
            Self::Maintenance => "⚙",
            Self::Quality => "✓",
            Self::HrPayroll => "♙",
            Self::Legal => "§",
            Self::Pos => "▣",
            Self::MarketingCrm => "◇",
        }
    }
}

/// Return every product shell navigation item in canonical order.
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
    fn registry_covers_all_product_shell_contexts_as_read_only_authenticated_routes() {
        assert_eq!(PRODUCT_CONTEXT_ROUTES.len(), 10);
        assert!(PRODUCT_CONTEXT_ROUTES.iter().all(|route| {
            route.read_only_shell
                && matches!(route.auth, crate::RouteAuthRequirement::Authenticated)
        }));
        assert_eq!(ProductContextId::Sales.as_str(), "sales");
        assert_eq!(ProductContextId::MarketingCrm.as_str(), "marketing_crm");
    }

    #[test]
    fn route_resolution_redacts_credentials_and_queries() {
        let route =
            ProductContextRoute::resolve("/sales?view=summary#details").expect("safe sales route");
        assert_eq!(route.id, ProductContextId::Sales);
        assert!(ProductContextRoute::resolve("/sales?access_token=secret#details").is_none());
        assert!(ProductContextRoute::resolve("/sales/../legal").is_none());
        assert_eq!(
            product_context_nav_items().len(),
            PRODUCT_CONTEXT_ROUTES.len()
        );
    }
}
