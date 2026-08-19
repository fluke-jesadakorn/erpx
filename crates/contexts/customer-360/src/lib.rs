#![doc = "Customer 360 composed read-model core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    Customer360ProjectionHandler, Customer360QueryHandler, Customer360RecordCommandHandler,
    GetCustomer360, GetCustomer360Result,
};
pub use domain::{
    CONTROL_CUSTOMERS_PROJECTION_NAME, Customer360DomainError, Customer360OwnedRecord,
    Customer360ProjectionInput, Customer360ProjectionStream, Customer360RecordKind,
    Customer360RecordStatus, Customer360SectionAvailability, Customer360View,
    HOLDING_ACTIVITY_PROJECTION_NAME, ProjectionApplyOutcome, ProjectionPosition,
    ProjectionPositions, SectionAvailability, SourceCoverage, SourceFreshness,
};
pub use integration::Customer360SourceEvent;
pub use ports::{
    AuthorizationPort, AuthorizationRequest, Customer360Action, Customer360OwnedRecordQueryPort,
    Customer360OwnedRecordView, Customer360RecordAuthorizationPort, Customer360RecordCommand,
    Customer360RecordReceipt, Customer360RecordUnitOfWork, PortError, ProjectionRepository,
    ProjectionWriteRequest, ReadProjection, ReadRepository, RebuildProjectionRequest,
    SourceProjectionMetadata,
};
