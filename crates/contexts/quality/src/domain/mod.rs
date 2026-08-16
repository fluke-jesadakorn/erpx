mod inspection;
mod records;

pub use inspection::{
    InspectionId, InspectionKind, InspectionStatus, QualityDomainError, QualityInspection,
    QualityInspectionState, deterministic_reference,
};

pub(crate) use inspection::DomainEvent;
pub use records::{
    QualityLifecycleRecord, QualityLifecycleRecordError, QualityLifecycleRecordKind,
    QualityLifecycleRecordStatus,
};
