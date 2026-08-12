pub mod features {
    pub use erpx_console_feature_assets as assets;
    pub use erpx_console_feature_brand as brand;
    pub use erpx_console_feature_commercial as commercial;
    pub use erpx_console_feature_crm as crm;
    pub use erpx_console_feature_customer_360 as customer_360;
    pub use erpx_console_feature_customer_authority as customer_authority;
    pub use erpx_console_feature_finance as finance;
    pub use erpx_console_feature_identity_access as identity_access;
    pub use erpx_console_feature_inventory as inventory;
    pub use erpx_console_feature_legal as legal;
    pub use erpx_console_feature_maintenance as maintenance;
    pub use erpx_console_feature_marketing as marketing;
    pub use erpx_console_feature_operations as operations;
    pub use erpx_console_feature_organization as organization;
    pub use erpx_console_feature_people_payroll as people_payroll;
    pub use erpx_console_feature_platform_administration as platform_administration;
    pub use erpx_console_feature_pos as pos;
    pub use erpx_console_feature_procurement as procurement;
    pub use erpx_console_feature_quality as quality;
    pub use erpx_console_feature_sales as sales;
    pub use erpx_console_feature_support as support;
}

use erpx_ui_core::{FeatureRegistration, FeatureRegistry, FeatureRegistryError};

pub static FEATURE_REGISTRATIONS: [&FeatureRegistration; 21] = [
    &features::assets::REGISTRATION,
    &features::brand::REGISTRATION,
    &features::commercial::REGISTRATION,
    &features::crm::REGISTRATION,
    &features::customer_360::REGISTRATION,
    &features::customer_authority::REGISTRATION,
    &features::finance::REGISTRATION,
    &features::identity_access::REGISTRATION,
    &features::inventory::REGISTRATION,
    &features::legal::REGISTRATION,
    &features::maintenance::REGISTRATION,
    &features::marketing::REGISTRATION,
    &features::operations::REGISTRATION,
    &features::organization::REGISTRATION,
    &features::people_payroll::REGISTRATION,
    &features::platform_administration::REGISTRATION,
    &features::pos::REGISTRATION,
    &features::procurement::REGISTRATION,
    &features::quality::REGISTRATION,
    &features::sales::REGISTRATION,
    &features::support::REGISTRATION,
];

pub fn feature_registry() -> Result<FeatureRegistry, FeatureRegistryError> {
    erpx_console_shell::ConsoleShell::try_new(FEATURE_REGISTRATIONS)
        .map(|shell| shell.registry().clone())
}

mod app;
#[cfg(feature = "server")]
mod bff;
mod composition;
mod localization;
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
                "assets",
                "assets.workspace",
                "/workspaces/assets",
                "authenticated",
                "operations",
            ),
            (
                "brand",
                "brand.workspace",
                "/platform/brand",
                "authenticated",
                "platform",
            ),
            (
                "commercial",
                "commercial.workspace",
                "/platform/commercial",
                "authenticated",
                "platform",
            ),
            (
                "crm",
                "crm.workspace",
                "/workspaces/crm",
                "authenticated",
                "customer",
            ),
            (
                "customer-360",
                "customer-360.workspace",
                "/platform/customers",
                "authenticated",
                "customer",
            ),
            (
                "customer-authority",
                "customer-authority.workspace",
                "/platform/customer-authority",
                "authenticated",
                "customer",
            ),
            (
                "finance",
                "finance.workspace",
                "/workspaces/finance",
                "authenticated",
                "finance",
            ),
            (
                "identity-access",
                "identity-access.workspace",
                "/platform/identity-access",
                "authenticated",
                "platform",
            ),
            (
                "inventory",
                "inventory.workspace",
                "/workspaces/inventory",
                "authenticated",
                "operations",
            ),
            (
                "legal",
                "legal.workspace",
                "/workspaces/legal",
                "authenticated",
                "governance",
            ),
            (
                "maintenance",
                "maintenance.workspace",
                "/workspaces/maintenance",
                "authenticated",
                "operations",
            ),
            (
                "marketing",
                "marketing.workspace",
                "/workspaces/marketing",
                "authenticated",
                "customer",
            ),
            (
                "operations",
                "operations.workspace",
                "/workspaces/operations",
                "authenticated",
                "operations",
            ),
            (
                "organization",
                "organization.workspace",
                "/platform/organization",
                "authenticated",
                "platform",
            ),
            (
                "people-payroll",
                "people-payroll.workspace",
                "/workspaces/people-payroll",
                "authenticated",
                "people",
            ),
            (
                "platform-administration",
                "platform-administration.workspace",
                "/platform/administration",
                "authenticated",
                "platform",
            ),
            (
                "pos",
                "pos.workspace",
                "/workspaces/pos",
                "authenticated",
                "operations",
            ),
            (
                "procurement",
                "procurement.workspace",
                "/workspaces/procurement",
                "authenticated",
                "operations",
            ),
            (
                "quality",
                "quality.workspace",
                "/workspaces/quality",
                "authenticated",
                "operations",
            ),
            (
                "sales",
                "sales.workspace",
                "/workspaces/sales",
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
}
