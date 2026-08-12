#![doc = "Operations orchestration bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    AcknowledgeException, CloseException, DecideApproval, EscalateException, GetException,
    GetOperationsSummary, ListExceptionQueue, ListWorkQueuePolicies, OperationsApplicationError,
    OperationsCommand, OperationsCommandHandler, OperationsMutationResult, OperationsQuery,
    OperationsQueryHandler, OperationsQueryResponse, OperationsSourceHandler,
    PublishWorkQueuePolicy, ReassignException, ResolveException, SourceHandlingContext,
    SourceHandlingOutcome, SourceHandlingResult,
};
pub use domain::{
    DetectionPolicy, ExceptionId, ExceptionStatus, OperationsDomainError, OperationsException,
    PrincipalAssignment, SourceFactMessage, SourceReference, WorkQueuePolicy, WorkQueuePolicyId,
    WorkQueuePolicyStatus,
};
pub use integration::PublishedSourceEvent;
pub use ports::{
    AssignmentPort, AuthorizationPort, CommandUnitOfWork, DetectionDecision, DetectionPolicyPort,
    EntitlementPort, ExceptionQueue, ExceptionView, OperationsAction, OperationsCommit,
    OperationsSourceCount, OperationsSummary, PortError, QueueFilter, ReadRepository, SourceCommit,
    SourceDatabase, SourceIdentity, SourcePosition, SourceUnitOfWork, StoredCommit,
    StoredSourceCommit, StoredWorkQueuePolicyCommit, WorkQueuePolicyCommandIdentity,
    WorkQueuePolicyCommit, WorkQueuePolicyList, WorkQueuePolicyListFilter,
    WorkQueuePolicyUnitOfWork, WorkQueuePolicyView, WriteRepository,
};
