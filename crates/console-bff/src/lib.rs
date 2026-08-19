//! Server-only Browser-for-Frontend boundary shared by the Console router.

use std::io;

pub mod canonical;
pub mod reference;

pub const SESSION_PATH: &str = "/api/session";
pub const NAVIGATION_PATH: &str = "/api/navigation";
pub const DASHBOARD_PATH: &str = "/api/dashboard";
pub const WORK_ITEMS_PATH: &str = "/api/work-item";
pub const REFERENCE_ROUTE_TEMPLATE: &str = "/reference/item/:id";

pub fn validate_runtime_origins(
    auth_http_origin: &str,
    grpc_origin: &str,
    trusted_public_origins: &[impl AsRef<str>],
    production_profile: bool,
) -> Result<(), io::Error> {
    let service_origins_are_valid = [auth_http_origin, grpc_origin]
        .into_iter()
        .all(|origin| origin.starts_with("http://") || origin.starts_with("https://"));
    if !service_origins_are_valid {
        return Err(io::Error::other("BFF service origin is invalid"));
    }
    if production_profile
        && (!auth_http_origin.starts_with("https://")
            || !grpc_origin.starts_with("https://")
            || trusted_public_origins.is_empty()
            || trusted_public_origins
                .iter()
                .any(|origin| !origin.as_ref().starts_with("https://")))
    {
        return Err(io::Error::other(
            "Web production profile requires HTTPS service and public origins",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_origins_fail_closed() {
        assert!(
            validate_runtime_origins(
                "https://identity.example",
                "https://grpc.example",
                &["https://console.example"],
                true,
            )
            .is_ok()
        );
        assert!(
            validate_runtime_origins(
                "http://identity.example",
                "https://grpc.example",
                &["https://console.example"],
                true,
            )
            .is_err()
        );
    }
}
