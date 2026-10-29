use axum::http::HeaderMap;
use erpx_server_shared::public::BusinessError;
/// Scope selectors only, never principal/authority facts. The root gateway
/// adapter must supply actual branch/department requested path selectors.
pub trait BusinessScopeSlugs: Send + Sync + 'static {
    fn slugs(&self, headers: &HeaderMap)
    -> Result<(Option<String>, Option<String>), BusinessError>;
}
