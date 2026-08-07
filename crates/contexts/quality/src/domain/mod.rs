mod inspection;

pub use inspection::{
    InspectionId, InspectionKind, InspectionStatus, QualityDomainError, QualityInspection,
    deterministic_reference,
};

pub(crate) use inspection::DomainEvent;
