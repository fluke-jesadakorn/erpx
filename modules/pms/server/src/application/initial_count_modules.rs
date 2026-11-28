//! Portable read contract; no persistence or authority grant.
use erpx_platform_shared_server::public::CompanyId;
use std::future::Future;
pub trait InitialCountModulesPort {
    type Error;
    type Witness;
    fn inspect(
        &mut self,
        company: &CompanyId,
    ) -> impl Future<Output = Result<Self::Witness, Self::Error>> + Send;
}
