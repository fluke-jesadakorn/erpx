use erpx_integration_contracts::legal;

use crate::domain::DomainEvent;

/// Legal contracts, document metadata, and access history are private context facts.
/// The existing cross-context Legal contract describes case obligations, so these
/// lifecycle events must not be coerced into that unrelated public fact.
pub(crate) fn published_events(events: &[DomainEvent]) -> Vec<legal::Event> {
    events
        .iter()
        .filter_map(|event| match event {
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
    fn private_document_lifecycle_is_not_misrepresented_as_a_legal_obligation() {
        assert!(
            published_events(&[
                DomainEvent::ContractChanged,
                DomainEvent::DocumentChanged,
                DomainEvent::AccessRecorded,
            ])
            .is_empty()
        );
    }
}
