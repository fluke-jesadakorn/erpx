//! Wire data for the explicit non-parts branch of the shared POST.
use crate::field_service_http_wire::OtherPostBranch;
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_runtime::public::postgres::PgWorkFuture;
use erpx_server_shared::public::BusinessError;
use erpx_server_workflow::public::commands::CommandJson;
use sqlx::PgConnection;
use std::sync::Arc;

#[derive(Clone)]
pub struct OtherPostInput {
    pub branch: OtherPostBranch,
    pub id_utf16: Option<Vec<u16>>,
    pub body: Arc<[u8]>,
    pub key: String,
}
/// The caller must provide a real dispatcher; this module supplies no fallback.
pub trait OtherPost: Send + Sync + 'static {
    fn validate(&self, input: &OtherPostInput) -> Result<(), String>;
    fn admission_error(&self, branch: OtherPostBranch, error: BusinessError) -> BusinessError;
    fn execute<'a>(
        &'a self,
        c: &'a mut PgConnection,
        current: &'a AuthorizedWorkContext,
        input: OtherPostInput,
    ) -> PgWorkFuture<'a, CommandJson, BusinessError>;
}
