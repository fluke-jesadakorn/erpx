#![doc = "Customer 360 composed read-model core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    Customer360ProjectionHandler, Customer360QueryHandler, GetCustomer360, GetCustomer360Result,
};
pub use domain::{
    CONTROL_CUSTOMERS_PROJECTION_NAME, Customer360DomainError, Customer360Projection,
    Customer360ProjectionInput, Customer360ProjectionStream, Customer360SectionAvailability,
    Customer360View, ProjectionApplyOutcome, ProjectionPosition, ProjectionPositions,
    SectionAvailability, SourceCoverage, TENANT_ACTIVITY_PROJECTION_NAME,
};
pub use integration::Customer360SourceEvent;
pub use ports::{
    AuthorizationPort, AuthorizationRequest, Customer360Action, PortError, ProjectionRepository,
    ProjectionWriteRequest, ReadProjection, ReadRepository, RebuildProjectionRequest,
    SourceProjectionMetadata,
};
