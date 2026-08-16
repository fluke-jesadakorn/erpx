use erpx_integration_contracts::{ReferenceCode, quality};

use crate::domain::DomainEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IntegrationMappingError {
    InspectionReference,
    InspectionIdentifier,
    SubjectIdentifier,
}

pub(crate) fn published_events(
    events: &[DomainEvent],
) -> Result<Vec<quality::Event>, IntegrationMappingError> {
    events.iter().filter_map(map_event).collect()
}

fn map_event(event: &DomainEvent) -> Option<Result<quality::Event, IntegrationMappingError>> {
    match event {
        DomainEvent::ResultRecorded {
            inspection_id,
            product_id,
            reference,
            passed_quantity,
            failed_quantity,
            note,
            at,
        } => {
            let _ = (note, at);
            if *failed_quantity == 0 {
                // A positive eligibility fact is published only after the
                // explicit Quality-owned release transition.
                return None;
            }
            let inspection = quality::InspectionRef::try_from_uuid(inspection_id.as_uuid())
                .map_err(|_| IntegrationMappingError::InspectionIdentifier);
            let subject = quality::QualitySubjectRef::try_from_uuid(*product_id)
                .map_err(|_| IntegrationMappingError::SubjectIdentifier);
            let inspection_code = ReferenceCode::try_new(reference.clone())
                .map_err(|_| IntegrationMappingError::InspectionReference);
            let outcome = if *passed_quantity == 0 {
                quality::InspectionOutcome::Failed
            } else {
                quality::InspectionOutcome::PassedWithDeviation
            };
            Some(
                inspection
                    .and_then(|inspection| subject.map(|subject| (inspection, subject)))
                    .and_then(|(inspection, subject)| {
                        inspection_code.map(|inspection_code| {
                            quality::Event::InspectionCompleted(quality::InspectionFact {
                                inspection,
                                subject,
                                inspection_code,
                                outcome,
                            })
                        })
                    }),
            )
        }
        DomainEvent::Created {
            inspection_id,
            scope,
            product_id,
            reference,
            at,
        } => {
            let _ = (inspection_id, scope, product_id, reference, at);
            None
        }
        DomainEvent::Released {
            inspection_id,
            product_id,
            reference,
            at,
        } => {
            let _ = at;
            let inspection = quality::InspectionRef::try_from_uuid(inspection_id.as_uuid())
                .map_err(|_| IntegrationMappingError::InspectionIdentifier);
            let subject = quality::QualitySubjectRef::try_from_uuid(*product_id)
                .map_err(|_| IntegrationMappingError::SubjectIdentifier);
            let inspection_code = ReferenceCode::try_new(reference.clone())
                .map_err(|_| IntegrationMappingError::InspectionReference);
            Some(
                inspection
                    .and_then(|inspection| subject.map(|subject| (inspection, subject)))
                    .and_then(|(inspection, subject)| {
                        inspection_code.map(|inspection_code| {
                            quality::Event::InspectionCompleted(quality::InspectionFact {
                                inspection,
                                subject,
                                inspection_code,
                                outcome: quality::InspectionOutcome::Passed,
                            })
                        })
                    }),
            )
        }
        DomainEvent::Started { inspection_id, at }
        | DomainEvent::Cancelled { inspection_id, at } => {
            let _ = (inspection_id, at);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use erpx_integration_contracts::quality::{Event, InspectionOutcome};
    use uuid::Uuid;

    use super::*;
    use crate::domain::InspectionId;

    #[test]
    fn completed_result_maps_to_owner_contract_without_exposing_domain_event() {
        let event = DomainEvent::ResultRecorded {
            inspection_id: InspectionId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            product_id: Uuid::from_u128(2),
            reference: "QI-0001".into(),
            passed_quantity: 4,
            failed_quantity: 1,
            note: None,
            at: Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap(),
        };
        let mapped = published_events(&[event]).unwrap();
        assert!(matches!(
            mapped.as_slice(),
            [Event::InspectionCompleted(fact)]
                if fact.outcome == InspectionOutcome::PassedWithDeviation
        ));
    }

    #[test]
    fn passing_result_waits_for_explicit_release_before_publication() {
        let result = DomainEvent::ResultRecorded {
            inspection_id: InspectionId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            product_id: Uuid::from_u128(2),
            reference: "QI-0001".into(),
            passed_quantity: 5,
            failed_quantity: 0,
            note: None,
            at: Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap(),
        };
        assert!(published_events(&[result]).unwrap().is_empty());

        let released = DomainEvent::Released {
            inspection_id: InspectionId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            product_id: Uuid::from_u128(2),
            reference: "QI-0001".into(),
            at: Utc.with_ymd_and_hms(2026, 8, 5, 9, 1, 0).single().unwrap(),
        };
        assert!(matches!(
            published_events(&[released]).unwrap().as_slice(),
            [Event::InspectionCompleted(fact)]
                if fact.outcome == InspectionOutcome::Passed
        ));
    }
}
