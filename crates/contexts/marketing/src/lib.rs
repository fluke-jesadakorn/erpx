#![doc = "Marketing bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    CaptureEngagement, CreateLeadCandidate, GetLeadCandidate, ListLeadCandidates,
    MarketingApplicationError, MarketingCommand, MarketingCommandHandler, MarketingMutationResult,
    MarketingQuery, MarketingQueryHandler, MarketingQueryResponse, QualifyLeadCandidate,
    RejectLeadCandidate,
};
pub use domain::{
    AudienceMemberId, CampaignId, LeadCandidate, LeadCandidateId, LeadCandidateStatus,
    MarketingDomainError,
};
pub use ports::{
    AuthorizationPort, CommandUnitOfWork, EntitlementPort, LeadCandidateList,
    LeadCandidateListFilter, LeadCandidateView, MarketingAction, MarketingCommit, PortError,
    ReadRepository, WriteRepository,
};
