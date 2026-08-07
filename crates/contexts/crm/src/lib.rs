#![doc = "CRM bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    AcceptMarketingLead, CreateOpportunity, CrmApplicationError, CrmCommand, CrmCommandHandler,
    CrmMutationResult, CrmQuery, CrmQueryHandler, CrmQueryResponse, GetCrmLead, GetOpportunity,
    ListOpportunities, TransitionOpportunity,
};
pub use domain::{
    CrmAggregate, CrmDomainError, CrmLead, CrmLeadId, CrmOpportunity, OpportunityId,
    OpportunityStage,
};
pub use ports::{
    AuthorizationPort, CommandUnitOfWork, CrmAction, CrmCommit, CrmLeadView, EntitlementPort,
    OpportunityList, OpportunityListFilter, OpportunityView, PortError, ReadRepository,
    WriteRepository,
};
