//! Native engineering verification read use case. The caller owns the
//! transaction; this layer bounds inputs and delegates the owner port. No
//! attestation, identity, authority or rollout decision is made here.
use crate::domain::native_engineering_verification_read::{
    ActiveVerificationQuery, EngineeringVerificationPrior,
};
use erpx_platform_shared_server::public::BusinessError;
use std::future::Future;

pub trait EngineeringVerificationReadRepository: Send {
    type Error: From<BusinessError> + Send;

    fn prior<'a>(
        &'a mut self,
        company_id: &'a str,
        run: &'a str,
    ) -> impl Future<Output = Result<Option<EngineeringVerificationPrior>, Self::Error>> + Send + 'a;

    fn active<'a>(
        &'a mut self,
        query: &'a ActiveVerificationQuery<'a>,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send + 'a;

    fn reply_rollout_guard<'a>(
        &'a mut self,
        company_id: &'a str,
    ) -> impl Future<Output = Result<Vec<String>, Self::Error>> + Send + 'a;
}

pub async fn read_prior<R: EngineeringVerificationReadRepository>(
    repository: &mut R,
    company_id: &str,
    run: &str,
) -> Result<Option<EngineeringVerificationPrior>, R::Error> {
    repository.prior(company_id, run).await
}

pub async fn read_active<R: EngineeringVerificationReadRepository>(
    repository: &mut R,
    query: &ActiveVerificationQuery<'_>,
) -> Result<bool, R::Error> {
    query.validate().map_err(R::Error::from)?;
    repository.active(query).await
}

pub async fn read_reply_rollout_guard<R: EngineeringVerificationReadRepository>(
    repository: &mut R,
    company_id: &str,
) -> Result<Vec<String>, R::Error> {
    repository.reply_rollout_guard(company_id).await
}
