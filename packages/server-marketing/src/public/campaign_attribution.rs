//! Native Marketing campaign/attribution ports and the PostgreSQL owner
//! adapter.
pub use crate::adapters::campaign_attribution::PgMarketingCampaignRead;
pub use crate::application::campaign_attribution::{
    MarketingCampaignEntitlementPort, MarketingCampaignFuture, MarketingCampaignRepository,
    marketing_campaign_report,
};
pub use crate::domain::campaign_attribution::{
    ATTRIBUTION_LIMIT, CAMPAIGN_LIMIT, MarketingAttributionRow, MarketingCampaignActor,
    MarketingCampaignEntitlement, MarketingCampaignPurchaseControls, MarketingCampaignReport,
    MarketingCampaignRow, UNSPECIFIED,
};
