use erpx_integration_contracts::{DisplayName, ReferenceCode, legal};

use crate::domain::{DomainEvent, LegalAggregate, MatterStatus, RecordRelation};

/// Public Legal graph facts are emitted only by the Legal matter aggregate.
/// Contract documents and access history stay private Legal evidence.
pub(crate) fn published_events(
    aggregate: &LegalAggregate,
    events: &[DomainEvent],
) -> Vec<legal::Event> {
    let LegalAggregate::Matter(matter) = aggregate else {
        return Vec::new();
    };
    let matter_ref = legal::LegalMatterRef::try_from_uuid(matter.id().as_uuid())
        .expect("validated Legal matter identifier");
    events
        .iter()
        .filter_map(|event| match event {
            DomainEvent::MatterChanged => {
                Some(legal::Event::MatterChanged(legal::MatterChangedFact {
                    matter: matter_ref,
                    matter_reference: ReferenceCode::try_new(matter.reference().to_owned())
                        .expect("domain validated matter reference"),
                    title: DisplayName::try_new(matter.title().to_owned())
                        .expect("domain validated matter title"),
                    status: match matter.status() {
                        MatterStatus::Open => legal::MatterStatus::Open,
                        MatterStatus::Closed => legal::MatterStatus::Closed,
                    },
                    link_count: u64::try_from(matter.links().len())
                        .expect("bounded matter link count"),
                    legal_hold_active: matter.legal_hold().is_some_and(|hold| hold.active),
                }))
            }
            DomainEvent::RecordLinked(link) => {
                Some(legal::Event::RecordLinked(legal::RecordLinkedFact {
                    matter: matter_ref,
                    relation: match link.relation {
                        RecordRelation::GovernedBy => legal::RecordRelation::GovernedBy,
                        RecordRelation::Supports => legal::RecordRelation::Supports,
                        RecordRelation::BilledUnder => legal::RecordRelation::BilledUnder,
                        RecordRelation::AttributedTo => legal::RecordRelation::AttributedTo,
                        RecordRelation::Disputes => legal::RecordRelation::Disputes,
                        RecordRelation::SettlementFor => legal::RecordRelation::SettlementFor,
                        RecordRelation::Supersedes => legal::RecordRelation::Supersedes,
                        RecordRelation::AccountingFor => legal::RecordRelation::AccountingFor,
                    },
                    target: link.target.clone(),
                    target_scope: link.target_scope.clone(),
                    target_digest: ReferenceCode::try_new(link.target_digest.clone())
                        .expect("domain validated record digest"),
                }))
            }
            DomainEvent::LegalHoldChanged(hold) => {
                let changed_at = hold.released_at.unwrap_or(hold.placed_at);
                Some(legal::Event::LegalHoldChanged(
                    legal::LegalHoldChangedFact {
                        matter: matter_ref,
                        active: hold.active,
                        reason: DisplayName::try_new(hold.reason.clone())
                            .expect("domain validated Legal hold reason"),
                        changed_at,
                    },
                ))
            }
            DomainEvent::ContractChanged
            | DomainEvent::DocumentChanged
            | DomainEvent::AccessRecorded => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_document_lifecycle_is_not_published() {
        let aggregate = LegalAggregate::AccessEntry(
            crate::domain::AccessEntry::record(
                crate::domain::AccessEntryId::try_from_uuid(uuid::Uuid::from_u128(1)).unwrap(),
                erpx_kernel::AuthorityScope::Company {
                    holding_id: uuid::Uuid::from_u128(2),
                    company_id: uuid::Uuid::from_u128(3),
                },
                crate::domain::LegalDocumentId::try_from_uuid(uuid::Uuid::from_u128(4)).unwrap(),
                1,
                "user:1".into(),
                "view".into(),
                "review".into(),
                chrono::Utc::now(),
                chrono::Utc::now(),
            )
            .unwrap(),
        );
        assert!(published_events(&aggregate, &[DomainEvent::AccessRecorded]).is_empty());
    }
}
