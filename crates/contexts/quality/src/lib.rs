#![doc = "Quality bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    CancelInspection, CreateInspection, CreateInspectionResult, GetInspection, GetInspectionResult,
    InspectionList, InspectionListFilter, InspectionMutationResult, InspectionView,
    ListInspections, QualityApplicationError, QualityCommandHandler,
    QualityLifecycleRecordApplicationError, QualityLifecycleRecordCommandHandler,
    QualityQueryHandler, RecordInspectionResult, ReleaseInspection, StartInspection,
};
pub use domain::{
    InspectionId, InspectionKind, InspectionStatus, QualityDomainError, QualityInspection,
    QualityLifecycleRecord, QualityLifecycleRecordError, QualityLifecycleRecordKind,
    QualityLifecycleRecordStatus, deterministic_reference,
};
pub use ports::{
    AuthorizationPort, CommandFingerprint, CommandIdentity, CommandUnitOfWork, EntitlementPort,
    PortError, QualityAction, QualityCommit, QualityLifecycleRecordAction,
    QualityLifecycleRecordAuthorizationPort, QualityLifecycleRecordAuthorizationRequest,
    QualityLifecycleRecordCommand, QualityLifecycleRecordCommandIdentity,
    QualityLifecycleRecordCommandRequest, QualityLifecycleRecordCommit,
    QualityLifecycleRecordReadRepository, QualityLifecycleRecordUnitOfWork,
    QualityLifecycleRecordWriteRepository, ReadProjection, ReadRepository, StoredCommit,
    StoredQualityLifecycleRecordCommit, WriteRepository,
};
