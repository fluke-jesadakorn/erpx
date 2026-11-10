//! Pure Work Projection shortcut destination projection.
//!
//! Mirrors the ordered shortcut-relevant branches of `workplaceNavigation`.
//! The caller supplies already-authorized permission/module facts; this module
//! only decides which registered shortcut destinations are visible. It grants
//! no page authority and performs no I/O.

use std::collections::BTreeSet;

/// The exact registered shortcut destination IDs in catalogue order.
pub const SHORTCUT_DESTINATIONS: &[&str] = &[
    "customer-followups",
    "crm-pipeline",
    "customer-timeline",
    "operations-quotations",
    "operations-sales",
    "operations-stock",
    "operations-purchasing",
    "procurement-rfqs",
    "operations-transit",
    "finance-payments",
    "finance-ledger",
    "finance-bank",
    "finance-payables",
    "finance-receivables",
    "projects-list",
    "projects-routines",
    "support-cases",
    "quality-holds",
    "assets",
    "maintenance",
    "field-service",
    "learning",
    "benefits",
    "talent",
    "marketing",
    "planning",
    "manufacturing",
    "executive-canvas",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NavigationShortcutFacts<'a> {
    pub beta: bool,
    pub scope_is_department: bool,
    pub executive_visible: bool,
    pub commerce_read: bool,
    pub inventory_read: bool,
    pub finance_read: bool,
    pub support_read: bool,
    pub quality_read: bool,
    pub records_read: bool,
    /// Enabled product/application module IDs; not page permissions.
    pub active_modules: &'a [String],
}

fn module<'a>(facts: &NavigationShortcutFacts<'a>, id: &str) -> bool {
    facts.active_modules.iter().any(|value| value == id)
}

/// Returns the ordered subset of registered shortcut IDs the navigation search
/// would expose for these facts.
pub fn shortcut_destination_ids(facts: &NavigationShortcutFacts<'_>) -> Vec<String> {
    let mut visible = BTreeSet::new();
    if !facts.beta {
        if facts.commerce_read {
            for id in [
                "customer-followups",
                "crm-pipeline",
                "customer-timeline",
                "operations-sales",
                "operations-quotations",
                "operations-purchasing",
                "operations-stock",
                "procurement-rfqs",
            ] {
                visible.insert(id);
            }
            if facts.inventory_read && !facts.scope_is_department {
                visible.insert("operations-transit");
            }
        }
        if facts.finance_read {
            for id in [
                "finance-payments",
                "finance-ledger",
                "finance-bank",
                "finance-payables",
                "finance-receivables",
            ] {
                visible.insert(id);
            }
        }
        if facts.support_read {
            visible.insert("support-cases");
        }
        if facts.quality_read && !facts.scope_is_department {
            visible.insert("quality-holds");
        }
        visible.insert("projects-list");
        visible.insert("projects-routines");
        if facts.records_read {
            for (module_id, destination) in [
                ("assets", "assets"),
                ("maintenance", "maintenance"),
                ("field_service", "field-service"),
                ("learning", "learning"),
                ("benefits", "benefits"),
                ("talent", "talent"),
                ("marketing", "marketing"),
                ("planning", "planning"),
                ("manufacturing", "manufacturing"),
            ] {
                if module(facts, module_id) {
                    visible.insert(destination);
                }
            }
        }
    }
    if facts.executive_visible {
        visible.insert("executive-canvas");
    }
    SHORTCUT_DESTINATIONS
        .iter()
        .filter(|id| visible.contains(**id))
        .map(|id| (*id).to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modules(value: &[&str]) -> Vec<String> {
        value.iter().map(|id| (*id).to_owned()).collect()
    }

    fn facts(modules: &[String]) -> NavigationShortcutFacts<'_> {
        NavigationShortcutFacts {
            beta: false,
            scope_is_department: false,
            executive_visible: false,
            commerce_read: false,
            inventory_read: false,
            finance_read: false,
            support_read: false,
            quality_read: false,
            records_read: false,
            active_modules: modules,
        }
    }

    #[test]
    fn commerce_covers_sales_procurement_inventory_and_crm_shortcuts() {
        let modules = modules(&[]);
        let mut value = facts(&modules);
        value.commerce_read = true;
        assert_eq!(
            shortcut_destination_ids(&value),
            [
                "customer-followups",
                "crm-pipeline",
                "customer-timeline",
                "operations-quotations",
                "operations-sales",
                "operations-stock",
                "operations-purchasing",
                "procurement-rfqs",
                "projects-list",
                "projects-routines",
            ]
        );
        value.inventory_read = true;
        assert!(
            shortcut_destination_ids(&value)
                .iter()
                .any(|id| id == "operations-transit")
        );
        value.scope_is_department = true;
        assert!(
            !shortcut_destination_ids(&value)
                .iter()
                .any(|id| id == "operations-transit")
        );
    }

    #[test]
    fn finance_support_quality_projects_and_executive_keep_order() {
        let modules = modules(&[]);
        let mut value = facts(&modules);
        value.finance_read = true;
        value.support_read = true;
        value.quality_read = true;
        value.executive_visible = true;
        assert_eq!(
            shortcut_destination_ids(&value),
            [
                "finance-payments",
                "finance-ledger",
                "finance-bank",
                "finance-payables",
                "finance-receivables",
                "projects-list",
                "projects-routines",
                "support-cases",
                "quality-holds",
                "executive-canvas",
            ]
        );
        value.scope_is_department = true;
        assert!(
            !shortcut_destination_ids(&value)
                .iter()
                .any(|id| id == "quality-holds")
        );
    }

    #[test]
    fn owner_modules_require_records_read_and_are_ordered_by_catalogue() {
        let modules = modules(&[
            "field_service",
            "assets",
            "learning",
            "manufacturing",
            "planning",
            "benefits",
            "talent",
            "marketing",
            "maintenance",
        ]);
        let mut value = facts(&modules);
        assert_eq!(
            shortcut_destination_ids(&value),
            ["projects-list", "projects-routines"]
        );
        value.records_read = true;
        assert_eq!(
            shortcut_destination_ids(&value),
            [
                "projects-list",
                "projects-routines",
                "assets",
                "maintenance",
                "field-service",
                "learning",
                "benefits",
                "talent",
                "marketing",
                "planning",
                "manufacturing",
            ]
        );
    }

    #[test]
    fn beta_removes_business_shortcuts_but_keeps_authorized_executive() {
        let modules = modules(&["assets"]);
        let mut value = facts(&modules);
        value.beta = true;
        value.records_read = true;
        value.commerce_read = true;
        value.finance_read = true;
        value.support_read = true;
        value.quality_read = true;
        assert!(shortcut_destination_ids(&value).is_empty());
        value.executive_visible = true;
        assert_eq!(shortcut_destination_ids(&value), ["executive-canvas"]);
    }

    #[test]
    fn every_returned_id_is_a_registered_shortcut() {
        let modules = modules(&[
            "assets",
            "maintenance",
            "field_service",
            "learning",
            "benefits",
            "talent",
            "marketing",
            "planning",
            "manufacturing",
        ]);
        let mut value = facts(&modules);
        value.commerce_read = true;
        value.inventory_read = true;
        value.finance_read = true;
        value.support_read = true;
        value.quality_read = true;
        value.records_read = true;
        value.executive_visible = true;
        for id in shortcut_destination_ids(&value) {
            assert!(SHORTCUT_DESTINATIONS.contains(&id.as_str()), "{id}");
        }
    }
}
