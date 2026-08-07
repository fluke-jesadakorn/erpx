use serde::{Deserialize, Serialize};

use crate::{BoundedContext, ReferenceCode};

owned_uuid_reference!(InspectionRef);
owned_uuid_reference!(NonconformanceRef);
owned_uuid_reference!(QualitySubjectRef);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectionOutcome {
    Passed,
    PassedWithDeviation,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InspectionFact {
    pub inspection: InspectionRef,
    pub subject: QualitySubjectRef,
    pub inspection_code: ReferenceCode,
    pub outcome: InspectionOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    InspectionCompleted(InspectionFact),
    NonconformanceRaised {
        nonconformance: NonconformanceRef,
        inspection: InspectionRef,
    },
    NonconformanceResolved {
        nonconformance: NonconformanceRef,
    },
}

impl_integration_event!(Event, BoundedContext::Quality, {
    Self::InspectionCompleted(_) => "quality.inspection_completed",
    Self::NonconformanceRaised { .. } => "quality.nonconformance_raised",
    Self::NonconformanceResolved { .. } => "quality.nonconformance_resolved",
});
