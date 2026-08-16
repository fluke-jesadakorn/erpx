#![doc = "Operations orchestration bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    AcknowledgeException, CloseException, DecideApproval, EscalateException, GetException,
    GetOperationsSummary, ListExceptionQueue, ListWorkQueuePolicies, OperationsApplicationError,
    OperationsCommand, OperationsCommandHandler, OperationsLifecycleRecordApplicationError,
    OperationsLifecycleRecordCommandHandler, OperationsMutationResult, OperationsQuery,
    OperationsQueryHandler, OperationsQueryResponse, OperationsSourceHandler,
    PublishWorkQueuePolicy, ReassignException, ResolveException, SourceHandlingContext,
    SourceHandlingOutcome, SourceHandlingResult,
};
pub use domain::{
    DetectionPolicy, ExceptionId, ExceptionStatus, OperationsDomainError, OperationsException,
    OperationsExceptionState, OperationsLifecycleRecord, OperationsLifecycleRecordError,
    OperationsLifecycleRecordKind, OperationsLifecycleRecordStatus, PrincipalAssignment,
    SourceFactMessage, SourceReference, WorkQueuePolicy, WorkQueuePolicyId, WorkQueuePolicyState,
    WorkQueuePolicyStatus,
};
pub use integration::PublishedSourceEvent;
pub use ports::{
    AssignmentPort, AuthorizationPort, CommandUnitOfWork, DetectionDecision, DetectionPolicyPort,
    EntitlementPort, ExceptionQueue, ExceptionView, OperationsAction, OperationsCommit,
    OperationsLifecycleRecordAction, OperationsLifecycleRecordAuthorizationPort,
    OperationsLifecycleRecordAuthorizationRequest, OperationsLifecycleRecordCommand,
    OperationsLifecycleRecordCommandIdentity, OperationsLifecycleRecordCommandRequest,
    OperationsLifecycleRecordCommit, OperationsLifecycleRecordReadRepository,
    OperationsLifecycleRecordUnitOfWork, OperationsLifecycleRecordWriteRepository,
    OperationsSourceCount, OperationsSummary, PortError, QueueFilter, ReadProjection,
    ReadRepository, SourceCommit, SourceDatabase, SourceHighWatermarkPort, SourceIdentity,
    SourcePosition, SourceUnitOfWork, StoredCommit, StoredOperationsLifecycleRecordCommit,
    StoredSourceCommit, StoredWorkQueuePolicyCommit, WorkQueuePolicyCommandIdentity,
    WorkQueuePolicyCommit, WorkQueuePolicyList, WorkQueuePolicyListFilter,
    WorkQueuePolicyUnitOfWork, WorkQueuePolicyView, WriteRepository,
};
