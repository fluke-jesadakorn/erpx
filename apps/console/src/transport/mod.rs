mod asset_maintenance_workspace;
pub(crate) use erpx_console_transport as typed_cqrs;
pub(crate) mod quality;
pub(crate) mod wave4_5;
pub(crate) mod wave6_7;
pub(crate) mod wave8;
pub(crate) mod wave9;

pub(crate) use asset_maintenance_workspace::ActiveAssetMaintenanceSurface;

// Endpoints are compile-time deployment inputs for packaged/native clients. The loopback
// defaults keep local development usable; production and device builds must provide the
// explicit HTTPS origins through `ERPX_CONSOLE_GRPC_ENDPOINT` and
// `ERPX_CONSOLE_AUTH_HTTP_ORIGIN` rather than silently targeting a developer workstation.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const GRPC_ENDPOINT: &str = match option_env!("ERPX_CONSOLE_GRPC_ENDPOINT") {
    Some(value) => value,
    None => "http://127.0.0.1:50051",
};

#[cfg(not(target_arch = "wasm32"))]
pub(crate) const AUTH_HTTP_ORIGIN: &str = match option_env!("ERPX_CONSOLE_AUTH_HTTP_ORIGIN") {
    Some(value) => value,
    // Keep the loopback browser and auth service same-site so the HttpOnly
    // refresh cookie can retain SameSite=Strict in development.
    None => "http://localhost:50052",
};

/// Compile-time authority fence for the packaged Console. Fresh-state builds
/// default to the direct-v1 typed family; a retained rollback bundle must set
/// this to `legacy` explicitly when it is rebuilt.
pub(crate) const V1_SERVICE_AUTHORITY: &str = match option_env!("ERPX_CONSOLE_V1_SERVICE_AUTHORITY")
{
    Some(value) => value,
    None => "typed",
};

#[cfg(target_arch = "wasm32")]
pub(crate) fn grpc_endpoint() -> String {
    web_sys::window()
        .and_then(|window| window.location().origin().ok())
        .filter(|origin| !origin.is_empty() && origin != "null")
        .map(|origin| format!("{origin}/api/bff/v1/grpc"))
        .unwrap_or_else(|| "/api/bff/v1/grpc".to_owned())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn auth_http_origin() -> String {
    AUTH_HTTP_ORIGIN.to_owned()
}
