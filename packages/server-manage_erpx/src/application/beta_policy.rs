use crate::domain::beta_policy::CompanyBetaPolicy;
use erpx_server_shared::public::{BusinessError, CompanyId};
use std::future::Future;

pub trait BetaPolicyReadPort {
    type Error: From<BusinessError> + Send;
    fn company_beta_policy(
        &mut self,
        company: &CompanyId,
    ) -> impl Future<Output = Result<CompanyBetaPolicy, Self::Error>> + Send;
}
