//! Native Marketing campaign/attribution HTTP ingress and owner-local router.
pub use crate::adapters::campaign_attribution_http::{
    MarketingCampaignHttpBackend, MarketingCampaignHttpFuture, MarketingCampaignHttpReadRequest,
    MarketingCampaignHttpResponse, MarketingCampaignHttpTarget, marketing_campaign_http_router,
};
