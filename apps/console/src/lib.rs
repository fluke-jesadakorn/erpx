pub mod features {
    pub use erpx_console_feature_ai_collaboration as ai_collaboration;
    pub use erpx_console_feature_assets as assets;
    pub use erpx_console_feature_benefits as benefits;
    pub use erpx_console_feature_brand as brand;
    pub use erpx_console_feature_commercial as commercial;
    pub use erpx_console_feature_crm as crm;
    pub use erpx_console_feature_customer_360 as customer_360;
    pub use erpx_console_feature_customer_authority as customer_authority;
    pub use erpx_console_feature_ecommerce as ecommerce;
    pub use erpx_console_feature_field_service as field_service;
    pub use erpx_console_feature_finance as finance;
    pub use erpx_console_feature_identity_access as identity_access;
    pub use erpx_console_feature_inventory as inventory;
    pub use erpx_console_feature_learning as learning;
    pub use erpx_console_feature_legal as legal;
    pub use erpx_console_feature_maintenance as maintenance;
    pub use erpx_console_feature_manufacturing as manufacturing;
    pub use erpx_console_feature_marketing as marketing;
    pub use erpx_console_feature_operations as operations;
    pub use erpx_console_feature_organization as organization;
    pub use erpx_console_feature_people_payroll as people_payroll;
    pub use erpx_console_feature_planning as planning;
    pub use erpx_console_feature_platform_administration as platform_administration;
    pub use erpx_console_feature_pos as pos;
    pub use erpx_console_feature_procurement as procurement;
    pub use erpx_console_feature_projects as projects;
    pub use erpx_console_feature_quality as quality;
    pub use erpx_console_feature_sales as sales;
    pub use erpx_console_feature_support as support;
    pub use erpx_console_feature_talent as talent;
}

use erpx_ui_core::{FeatureRegistration, FeatureRegistry, FeatureRegistryError};

pub static FEATURE_REGISTRATIONS: [&FeatureRegistration; 30] = [
    &features::ai_collaboration::REGISTRATION,
    &features::assets::REGISTRATION,
    &features::benefits::REGISTRATION,
    &features::brand::REGISTRATION,
    &features::commercial::REGISTRATION,
    &features::crm::REGISTRATION,
    &features::customer_360::REGISTRATION,
    &features::customer_authority::REGISTRATION,
    &features::ecommerce::REGISTRATION,
    &features::field_service::REGISTRATION,
    &features::finance::REGISTRATION,
    &features::identity_access::REGISTRATION,
    &features::inventory::REGISTRATION,
    &features::legal::REGISTRATION,
    &features::learning::REGISTRATION,
    &features::maintenance::REGISTRATION,
    &features::marketing::REGISTRATION,
    &features::manufacturing::REGISTRATION,
    &features::operations::REGISTRATION,
    &features::organization::REGISTRATION,
    &features::people_payroll::REGISTRATION,
    &features::planning::REGISTRATION,
    &features::platform_administration::REGISTRATION,
    &features::pos::REGISTRATION,
    &features::procurement::REGISTRATION,
    &features::projects::REGISTRATION,
    &features::quality::REGISTRATION,
    &features::sales::REGISTRATION,
    &features::support::REGISTRATION,
    &features::talent::REGISTRATION,
];

pub fn feature_registry() -> Result<FeatureRegistry, FeatureRegistryError> {
    erpx_console_shell::ConsoleShell::try_new(FEATURE_REGISTRATIONS)
        .map(|shell| shell.registry().clone())
}

mod app;
#[cfg(feature = "server")]
mod bff;
mod composition;
mod future_enterprise_gateway;
mod localization;
#[cfg(feature = "native")]
mod native_spike;
pub mod platform;
#[allow(dead_code)]
mod product_completion;
#[cfg(test)]
mod qa_completion;
#[cfg(feature = "server")]
mod server;
mod transport;

pub(crate) use composition::App;
pub use composition::run;
#[cfg(feature = "server")]
pub(crate) use composition::{
    LOGIN_PATH, authenticated_landing_path, console_route_http_status, fetch_navigation_manifest,
    is_legacy_login_path, login_redirect_location, login_return_path_from_url,
    navigation_authorizes_console_path, navigation_http_groups, validated_login_return_path,
};

#[cfg(test)]
pub(crate) use transport::AUTH_HTTP_ORIGIN;
#[cfg(all(test, not(target_arch = "wasm32")))]
pub(crate) use transport::GRPC_ENDPOINT;

#[cfg(test)]
mod feature_registry_tests {
    use super::*;

    #[test]
    fn static_registry_preserves_feature_route_contract() {
        let registry = feature_registry().expect("static feature registry must be valid");
        let actual = registry
            .registrations()
            .iter()
            .flat_map(|registration| {
                registration.routes.iter().map(move |route| {
                    (
                        registration.id,
                        route.id,
                        route.canonical_path,
                        route.authorization,
                        route.navigation_group,
                    )
                })
            })
            .collect::<Vec<_>>();
        let expected = vec![
            (
                "ai-collaboration",
                "ai.workspace",
                "/ai",
                "authenticated",
                "ai-collaboration",
            ),
            (
                "assets",
                "assets.workspace",
                "/asset",
                "authenticated",
                "operations",
            ),
            (
                "benefits",
                "benefits.workspace",
                "/benefits",
                "authenticated",
                "people",
            ),
            (
                "brand",
                "brand.workspace",
                "/brand",
                "authenticated",
                "platform",
            ),
            (
                "commercial",
                "commercial.workspace",
                "/commercial",
                "authenticated",
                "platform",
            ),
            ("crm", "crm.workspace", "/crm", "authenticated", "customer"),
            (
                "customer-360",
                "customer-360.workspace",
                "/customer-360",
                "authenticated",
                "customer",
            ),
            (
                "customer-authority",
                "customer-authority.workspace",
                "/customer",
                "authenticated",
                "customer",
            ),
            (
                "ecommerce",
                "ecommerce.workspace",
                "/commerce",
                "authenticated",
                "customer",
            ),
            (
                "field-service",
                "field-service.workspace",
                "/field-service",
                "authenticated",
                "operations",
            ),
            (
                "finance",
                "finance.workspace",
                "/finance",
                "authenticated",
                "finance",
            ),
            (
                "identity-access",
                "identity-access.workspace",
                "/identity",
                "authenticated",
                "platform",
            ),
            (
                "inventory",
                "inventory.workspace",
                "/inventory",
                "authenticated",
                "operations",
            ),
            (
                "legal",
                "legal.workspace",
                "/legal",
                "authenticated",
                "governance",
            ),
            (
                "learning",
                "learning.workspace",
                "/learning",
                "authenticated",
                "people",
            ),
            (
                "maintenance",
                "maintenance.workspace",
                "/maintenance",
                "authenticated",
                "operations",
            ),
            (
                "marketing",
                "marketing.workspace",
                "/marketing",
                "authenticated",
                "customer",
            ),
            (
                "manufacturing",
                "manufacturing.workspace",
                "/manufacturing",
                "authenticated",
                "operations",
            ),
            (
                "operations",
                "operations.workspace",
                "/operations",
                "authenticated",
                "operations",
            ),
            (
                "organization",
                "organization.workspace",
                "/organization",
                "authenticated",
                "platform",
            ),
            (
                "people-payroll",
                "people-payroll.workspace",
                "/people",
                "authenticated",
                "people",
            ),
            (
                "people-payroll",
                "payroll.workspace",
                "/payroll",
                "authenticated",
                "people",
            ),
            (
                "planning",
                "planning.workspace",
                "/planning",
                "authenticated",
                "operations",
            ),
            (
                "platform-administration",
                "platform-administration.workspace",
                "/platform",
                "authenticated",
                "platform",
            ),
            (
                "pos",
                "pos.workspace",
                "/pos",
                "authenticated",
                "operations",
            ),
            (
                "procurement",
                "procurement.workspace",
                "/procurement",
                "authenticated",
                "operations",
            ),
            (
                "projects",
                "projects.workspace",
                "/project",
                "authenticated",
                "operations",
            ),
            (
                "quality",
                "quality.workspace",
                "/quality",
                "authenticated",
                "operations",
            ),
            (
                "sales",
                "sales.workspace",
                "/sales",
                "authenticated",
                "customer",
            ),
            (
                "support",
                "support.workspace",
                "/support",
                "authenticated",
                "support",
            ),
            (
                "talent",
                "talent.workspace",
                "/talent",
                "authenticated",
                "people",
            ),
        ];
        assert_eq!(actual, expected);

        for registration in registry.registrations() {
            for route in registration.routes {
                assert_eq!(
                    registry.resolve(route.canonical_path).map(|item| item.id),
                    Some(registration.id)
                );
            }
        }
    }

    #[test]
    fn registry_contains_every_typed_workspace_root() {
        let registry = feature_registry().expect("static feature registry must be valid");
        let registered = registry
            .registrations()
            .iter()
            .flat_map(|registration| registration.routes)
            .map(|route| route.canonical_path)
            .collect::<std::collections::BTreeSet<_>>();
        let expected = erpx_integration_contracts::catalog::WORKSPACES
            .iter()
            .map(|workspace| workspace.route)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(expected.len(), 21);
        assert!(expected.is_subset(&registered));
    }
}
