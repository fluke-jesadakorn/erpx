//! Native Marketing campaign/attribution read use case.
//!
//! Repository SQL stays on the caller's retained serializable Holding
//! transaction. Package history arrives through the public Commercial boundary
//! supplied by composition; this owner never reads another owner's tables. The
//! application enforces the current actor policy, current package history
//! entitlement and every bounded campaign/attribution row before presentation.
use crate::domain::campaign_attribution::{
    ATTRIBUTION_LIMIT, CAMPAIGN_LIMIT, MarketingAttributionRow, MarketingCampaignActor,
    MarketingCampaignEntitlement, MarketingCampaignReport, MarketingCampaignRow, validate_report,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use std::future::Future;

pub type MarketingCampaignFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait MarketingCampaignRepository: Send {
    type Error: From<BusinessError> + Send;

    fn campaigns<'a>(
        &'a mut self,
        company: &'a CompanyId,
        limit: usize,
    ) -> MarketingCampaignFuture<'a, Vec<MarketingCampaignRow>, Self::Error>;

    fn attribution<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
        limit: usize,
    ) -> MarketingCampaignFuture<'a, Vec<MarketingAttributionRow>, Self::Error>;
}

/// Current Commercial package boundary. Disabled enforcement is the retained
/// policy; when enforced the current `sales.marketing` history entitlement is
/// required before any owner row is read.
pub trait MarketingCampaignEntitlementPort {
    type Error: Send;
    fn entitlement<'a>(
        &'a mut self,
        company_id: &'a str,
    ) -> MarketingCampaignFuture<'a, MarketingCampaignEntitlement, Self::Error>;
}

pub async fn marketing_campaign_report<R, E>(
    actor: &MarketingCampaignActor,
    repository: &mut R,
    entitlement: &mut E,
    now: TimestampMillis,
) -> Result<MarketingCampaignReport, R::Error>
where
    R: MarketingCampaignRepository,
    E: MarketingCampaignEntitlementPort,
    R::Error: From<E::Error>,
{
    actor.require_report(now)?;
    let entitlement = entitlement
        .entitlement(actor.company_id.as_str())
        .await
        .map_err(R::Error::from)?;
    entitlement.require_history()?;
    let campaigns = repository
        .campaigns(&actor.company_id, CAMPAIGN_LIMIT + 1)
        .await?;
    let attribution = repository
        .attribution(
            &actor.company_id,
            actor.branch_id.as_ref().map(|value| value.as_str()),
            actor.department_id.as_ref().map(|value| value.as_str()),
            ATTRIBUTION_LIMIT + 1,
        )
        .await?;
    let mut report = validate_report(actor, campaigns, attribution)?;
    report.purchase = entitlement.purchase();
    Ok(report)
}
