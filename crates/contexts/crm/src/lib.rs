#![doc = "CRM bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    AcceptMarketingLead, CreateOpportunity, CrmApplicationError, CrmCommand, CrmCommandHandler,
    CrmMutationResult, CrmQuery, CrmQueryHandler, CrmQueryResponse, CrmRecordApplicationError,
    CrmRecordCommandHandler, GetCrmLead, GetOpportunity, ListOpportunities, TransitionOpportunity,
    UnavailableQualifiedLeadResolver, UpdateOpportunityForecast,
};
pub use domain::{
    CrmAggregate, CrmDomainError, CrmLead, CrmLeadId, CrmLeadState, CrmOpportunity,
    CrmOpportunityState, CrmRecord, CrmRecordKind, CrmRecordStatus, OpportunityId,
    OpportunityStage,
};
pub use ports::{
    AuthorizationPort, CommandUnitOfWork, CrmAction, CrmCommit, CrmLeadView, CrmRecordAction,
    CrmRecordAuthorizationPort, CrmRecordAuthorizationRequest, CrmRecordCommand,
    CrmRecordCommandIdentity, CrmRecordCommandRequest, CrmRecordCommit, CrmRecordReadRepository,
    CrmRecordUnitOfWork, CrmRecordWriteRepository, EntitlementPort, OpportunityList,
    OpportunityListFilter, OpportunityView, PortError, QualifiedLeadEvidence,
    QualifiedLeadResolveRequest, QualifiedLeadResolverPort, ReadProjection, ReadRepository,
    StoredCrmRecordCommit, WriteRepository,
};
