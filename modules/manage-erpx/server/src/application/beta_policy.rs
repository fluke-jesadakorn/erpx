use crate::domain::beta_policy::CompanyBetaPolicy;
use erpx_platform_shared_server::public::{BusinessError, CompanyId};
use std::future::Future;

pub trait BetaPolicyReadPort {
    type Error: From<BusinessError> + Send;
    fn company_beta_policy(
        &mut self,
        company: &CompanyId,
    ) -> impl Future<Output = Result<CompanyBetaPolicy, Self::Error>> + Send;
}
