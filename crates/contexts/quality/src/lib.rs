#![doc = "Quality bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    CancelInspection, CreateInspection, CreateInspectionResult, GetInspection, GetInspectionResult,
    InspectionList, InspectionListFilter, InspectionMutationResult, InspectionView,
    ListInspections, QualityApplicationError, QualityCommandHandler, QualityQueryHandler,
    RecordInspectionResult, ReleaseInspection, StartInspection,
};
pub use domain::{
    InspectionId, InspectionKind, InspectionStatus, QualityDomainError, QualityInspection,
    deterministic_reference,
};
pub use ports::{
    AuthorizationPort, CommandFingerprint, CommandIdentity, CommandUnitOfWork, EntitlementPort,
    PortError, QualityAction, QualityCommit, ReadProjection, ReadRepository, StoredCommit,
    WriteRepository,
};
