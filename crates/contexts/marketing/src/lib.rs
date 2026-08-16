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
    MarketingRecordApplicationError, MarketingRecordCommandHandler, QualifyLeadCandidate,
    RejectLeadCandidate, ReleaseCampaignSpend, ReserveCampaignSpend, StartCampaignBudget,
    SubmitCampaignBudget,
};
pub use domain::{
    AudienceMemberId, BudgetAllocation, BudgetAllocationId, CampaignBudget, CampaignBudgetId,
    CampaignBudgetState, CampaignBudgetStatus, CampaignId, LeadCandidate, LeadCandidateId,
    LeadCandidateState, LeadCandidateStatus, MarketingAggregate, MarketingDomainError,
    MarketingRecord, MarketingRecordKind, MarketingRecordStatus, SpendRequestId, SpendReservation,
    SpendReservationStatus,
};
pub use ports::{
    AuthorizationPort, CampaignBudgetView, CommandUnitOfWork, EntitlementPort, LeadCandidateList,
    LeadCandidateListFilter, LeadCandidateView, MarketingAction, MarketingCommit,
    MarketingRecordAction, MarketingRecordAuthorizationPort, MarketingRecordAuthorizationRequest,
    MarketingRecordCommand, MarketingRecordCommandIdentity, MarketingRecordCommandRequest,
    MarketingRecordCommit, MarketingRecordReadRepository, MarketingRecordUnitOfWork,
    MarketingRecordWriteRepository, PortError, ReadProjection, ReadRepository,
    StoredMarketingRecordCommit, WriteRepository,
};
