#![doc = "Marketing bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    ApplyCampaignBudgetDecision, CancelCampaignBudget, CaptureEngagement, CompleteCampaignBudget,
    CreateCampaignBudget, CreateLeadCandidate, GetCampaignBudget, GetLeadCandidate,
    ListLeadCandidates, MarketingApplicationError, MarketingCommand, MarketingCommandHandler,
    MarketingMutationResult, MarketingQuery, MarketingQueryHandler, MarketingQueryResponse,
    QualifyLeadCandidate, RejectLeadCandidate, ReleaseCampaignSpend, ReserveCampaignSpend,
    StartCampaignBudget, SubmitCampaignBudget,
};
pub use domain::{
    AudienceMemberId, BudgetAllocation, BudgetAllocationId, CampaignBudget, CampaignBudgetId,
    CampaignBudgetStatus, CampaignId, LeadCandidate, LeadCandidateId, LeadCandidateStatus,
    MarketingAggregate, MarketingDomainError, SpendRequestId, SpendReservation,
    SpendReservationStatus,
};
pub use ports::{
    AuthorizationPort, CampaignBudgetView, CommandUnitOfWork, EntitlementPort, LeadCandidateList,
    LeadCandidateListFilter, LeadCandidateView, MarketingAction, MarketingCommit, PortError,
    ReadRepository, WriteRepository,
};
