pub mod features {
    pub mod assets;
    pub mod brand;
    pub mod commercial;
    pub mod crm;
    pub mod customer_360;
    pub mod customer_authority;
    pub mod finance;
    pub mod identity_access;
    pub mod inventory;
    pub mod legal;
    pub mod maintenance;
    pub mod marketing;
    pub mod operations;
    pub mod organization;
    pub mod people_payroll;
    pub mod platform_administration;
    pub mod pos;
    pub mod procurement;
    pub mod quality;
    pub mod sales;
    pub mod support;
}

mod app;
#[cfg(feature = "server")]
mod bff;
#[path = "transport/composition.rs"]
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
