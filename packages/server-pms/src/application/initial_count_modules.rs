//! Portable read contract; no persistence or authority grant.
use erpx_server_shared::public::CompanyId;
use std::future::Future;
pub trait InitialCountModulesPort {
    type Error;
    type Witness;
    fn inspect(
        &mut self,
        company: &CompanyId,
    ) -> impl Future<Output = Result<Self::Witness, Self::Error>> + Send;
}
