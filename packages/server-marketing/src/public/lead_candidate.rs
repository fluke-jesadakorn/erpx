//! Native Marketing campaign/lead/engagement reads and commands.
pub use crate::adapters::lead_candidate::PgMarketingLead;
pub use crate::adapters::lead_candidate_use_cases::{
    MarketingAuditEntry, MarketingCrmHandoffPort, MarketingLeadRepository, MarketingOutboxEntry,
    MarketingReceiptEntry, MarketingStoredReceipt, asset_profiles, create_asset, create_campaign,
    create_candidate, handoff_candidate, lead_candidates, lead_detail, record_engagement,
};
pub use crate::domain::lead_candidate::{
    ASSET_LIMIT, CANDIDATE_LIMIT, HISTORY_LIMIT, MarketingAssetCreateInput, MarketingAssetProfile,
    MarketingAssetProfiles, MarketingCampaignCreateInput, MarketingCommandReceipt,
    MarketingCrmLeadRequest, MarketingCrmLeadResult, MarketingEngagementRow, MarketingLeadAction,
    MarketingLeadActionInput, MarketingLeadActor, MarketingLeadCandidateInput,
    MarketingLeadCandidateRow, MarketingLeadDetail, crm_request,
};
