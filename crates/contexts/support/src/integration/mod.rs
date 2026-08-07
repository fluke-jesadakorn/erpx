use erpx_integration_contracts::{ReferenceCode, support};

use crate::domain::{DomainEvent, IncidentStatus, TicketStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IntegrationMappingError {
    InvalidContractValue,
}

pub(crate) fn published_events(
    events: &[DomainEvent],
) -> Result<Vec<support::Event>, IntegrationMappingError> {
    events.iter().filter_map(map_event).collect()
}

fn map_event(event: &DomainEvent) -> Option<Result<support::Event, IntegrationMappingError>> {
    match event {
        DomainEvent::TicketChanged {
            ticket_id,
            case_code,
            customer,
            status,
            category,
            at,
        } => Some(
            support::SupportCaseRef::try_from_uuid(ticket_id.as_uuid())
                .and_then(|support_case| {
                    Ok(support::SupportCaseFact {
                        support_case,
                        case_code: ReferenceCode::try_new(case_code.clone())?,
                        customer: *customer,
                        status: map_ticket_status(*status),
                        category: ReferenceCode::try_new(category.clone())?,
                        updated_at: *at,
                    })
                })
                .map(support::Event::SupportCaseChanged)
                .map_err(|_| IntegrationMappingError::InvalidContractValue),
        ),
        DomainEvent::IncidentChanged {
            incident_id,
            incident_code,
            status,
            severity,
            affected_services,
            affected_customer_accounts,
            at,
        } => Some(
            support::SupportIncidentRef::try_from_uuid(incident_id.as_uuid())
                .and_then(|incident| {
                    Ok(support::SupportIncidentFact {
                        incident,
                        incident_code: ReferenceCode::try_new(incident_code.clone())?,
                        status: map_incident_status(*status),
                        severity: ReferenceCode::try_new(severity.clone())?,
                        affected_services: affected_services
                            .iter()
                            .cloned()
                            .map(ReferenceCode::try_new)
                            .collect::<Result<Vec<_>, _>>()?,
                        affected_customer_accounts: affected_customer_accounts.clone(),
                        updated_at: *at,
                    })
                })
                .map(support::Event::IncidentChanged)
                .map_err(|_| IntegrationMappingError::InvalidContractValue),
        ),
        DomainEvent::Session | DomainEvent::CapabilityChanged => None,
    }
}

const fn map_incident_status(value: IncidentStatus) -> support::SupportIncidentStatus {
    match value {
        IncidentStatus::Investigating => support::SupportIncidentStatus::Investigating,
        IncidentStatus::Identified => support::SupportIncidentStatus::Identified,
        IncidentStatus::Monitoring => support::SupportIncidentStatus::Monitoring,
        IncidentStatus::Resolved => support::SupportIncidentStatus::Resolved,
        IncidentStatus::PostmortemComplete => support::SupportIncidentStatus::PostmortemComplete,
    }
}

const fn map_ticket_status(value: TicketStatus) -> support::SupportCaseStatus {
    match value {
        TicketStatus::New | TicketStatus::Triaged | TicketStatus::InProgress => {
            support::SupportCaseStatus::Open
        }
        TicketStatus::WaitingForCustomer => support::SupportCaseStatus::PendingCustomer,
        TicketStatus::AccessRequested | TicketStatus::Approved => {
            support::SupportCaseStatus::PendingInternal
        }
        TicketStatus::Resolved | TicketStatus::CustomerConfirmed => {
            support::SupportCaseStatus::Resolved
        }
        TicketStatus::Closed => support::SupportCaseStatus::Closed,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use erpx_integration_contracts::{IntegrationEvent, customer, support};
    use erpx_kernel::TenantScope;
    use uuid::Uuid;

    use super::*;
    use crate::domain::{Incident, IncidentId, SupportTicket, SupportTicketId};

    fn customer_fact() -> customer::CustomerProfileFact {
        customer::CustomerProfileFact {
            customer: customer::CustomerRef::try_from_uuid(Uuid::from_u128(10)).unwrap(),
            account: customer::CustomerAccountRef::try_from_uuid(Uuid::from_u128(11)).unwrap(),
            customer_code: erpx_integration_contracts::ReferenceCode::try_new("CUS-10").unwrap(),
            display_name: erpx_integration_contracts::DisplayName::try_new("Acme").unwrap(),
            owning_company: erpx_integration_contracts::organization::CompanyRef::try_from_uuid(
                Uuid::from_u128(2),
            )
            .unwrap(),
            status: customer::CustomerStatus::Active,
        }
    }

    #[test]
    fn ticket_fact_exposes_no_messages_or_capability_material() {
        let at = Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap();
        let mut ticket = SupportTicket::open(
            SupportTicketId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            TenantScope::Company {
                holding_id: Uuid::from_u128(1),
                company_id: Uuid::from_u128(2),
            },
            customer_fact(),
            "SUP-1".into(),
            "access".into(),
            "high".into(),
            "cannot sign in".into(),
            at,
        )
        .unwrap();
        let events = published_events(&ticket.take_uncommitted_events()).unwrap();
        assert!(
            matches!(events.as_slice(), [support::Event::SupportCaseChanged(fact)] if fact.updated_at == at)
        );
        assert!(events[0].validate().is_ok());
        let wire = format!("{events:?}");
        assert!(!wire.contains("message"));
        assert!(!wire.contains("capability"));
    }

    #[test]
    fn incident_fact_contains_routing_state_but_no_private_timeline_or_broadcasts() {
        let at = Utc.with_ymd_and_hms(2026, 8, 5, 10, 0, 0).single().unwrap();
        let account = customer::CustomerAccountRef::try_from_uuid(Uuid::from_u128(11)).unwrap();
        let mut incident = Incident::open(
            IncidentId::try_from_uuid(Uuid::from_u128(20)).unwrap(),
            TenantScope::Company {
                holding_id: Uuid::from_u128(1),
                company_id: Uuid::from_u128(2),
            },
            "INC-20".into(),
            "critical".into(),
            "commander".into(),
            vec!["api".into(), "checkout".into()],
            vec![account],
            at,
        )
        .unwrap();

        let events = published_events(&incident.take_uncommitted_events()).unwrap();
        assert!(matches!(
            events.as_slice(),
            [support::Event::IncidentChanged(fact)]
                if fact.updated_at == at
                    && fact.affected_services.len() == 2
                    && fact.affected_customer_accounts == vec![account]
        ));
        assert!(events[0].validate().is_ok());
        let wire = format!("{events:?}");
        assert!(!wire.contains("timeline"));
        assert!(!wire.contains("broadcast"));
        assert!(!wire.contains("commander"));
    }
}
