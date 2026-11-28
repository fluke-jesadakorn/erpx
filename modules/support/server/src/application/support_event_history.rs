//! Support ticket/incident event-history queries. Ports supply owner rows; this
//! layer owns authorization, selector validation and response projection and
//! opens no transaction or HTTP request.
use crate::domain::support_event_history::{
    incident_events, ticket_events, IncidentEventRow, SupportEventHistoryScope, TicketEventRow,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::future::Future;
use std::pin::Pin;

pub type SupportEventHistoryFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Both ports are reads; no receipt, audit or outbox ledger is involved and no
/// caller SQL crosses the boundary.
pub trait SupportEventHistoryPorts: Send {
    fn ticket_events<'a>(&'a mut self, id: &'a str)
        -> SupportEventHistoryFuture<'a, Vec<TicketEventRow>>;
    fn incident_events<'a>(
        &'a mut self,
        id: &'a str,
    ) -> SupportEventHistoryFuture<'a, Vec<IncidentEventRow>>;
}

fn authorize(scope: &SupportEventHistoryScope) -> Result<(), BusinessError> {
    if !scope.can_read() {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "SUPPORT_READ_REQUIRED",
            "A current Support read assignment is required.",
        ));
    }
    if scope.company_id.is_empty() || scope.user_id.is_empty() {
        return Err(BusinessError::invalid(
            "SUPPORT_HISTORY_SCOPE_INVALID",
            "Provide a selected Company and actor.",
        ));
    }
    Ok(())
}

fn selector(id: &str, code: &'static str) -> Result<(), BusinessError> {
    if id.is_empty() || id.len() > 200 || id.chars().any(char::is_control) {
        return Err(BusinessError::invalid(
            code,
            "Use the current Support event selector.",
        ));
    }
    Ok(())
}

/// One bounded ticket-history read: every immutable event row, newest first.
pub async fn read_ticket_history<P: SupportEventHistoryPorts>(
    ports: &mut P,
    scope: &SupportEventHistoryScope,
    id: &str,
) -> Result<Vec<TicketEventRow>, BusinessError> {
    authorize(scope)?;
    selector(id, "SUPPORT_TICKET_HISTORY_ID_INVALID")?;
    ticket_events(ports.ticket_events(id).await?)
}

/// One bounded incident-history read: every immutable event row, newest first.
pub async fn read_incident_history<P: SupportEventHistoryPorts>(
    ports: &mut P,
    scope: &SupportEventHistoryScope,
    id: &str,
) -> Result<Vec<IncidentEventRow>, BusinessError> {
    authorize(scope)?;
    selector(id, "SUPPORT_INCIDENT_HISTORY_ID_INVALID")?;
    incident_events(ports.incident_events(id).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn scope() -> SupportEventHistoryScope {
        SupportEventHistoryScope {
            company_id: "company".into(),
            user_id: "reader".into(),
            support_read: true,
        }
    }

    fn ticket() -> TicketEventRow {
        TicketEventRow {
            id: "support-ticket-1:1".into(),
            ticket_id: "support-ticket-1".into(),
            version: 1,
            action: "create".into(),
            actor_id: "creator".into(),
            reason: "Reported".into(),
            owner_id: Some("owner".into()),
            due_date: Some("2026-11-30".into()),
            status: "open".into(),
            created_at: 1,
            evidence_json: json!({"kind":"report"}),
        }
    }

    fn incident() -> IncidentEventRow {
        IncidentEventRow {
            id: "support-incident-1:1".into(),
            incident_id: "support-incident-1".into(),
            version: 1,
            action: "create".into(),
            actor_id: "creator".into(),
            reason: "Reported".into(),
            created_at: 1,
            evidence_json: json!({"kind":"outage"}),
        }
    }

    #[derive(Default)]
    struct Fake {
        tickets: Vec<TicketEventRow>,
        incidents: Vec<IncidentEventRow>,
        calls: BTreeMap<&'static str, usize>,
    }

    impl SupportEventHistoryPorts for Fake {
        fn ticket_events<'a>(
            &'a mut self,
            id: &'a str,
        ) -> SupportEventHistoryFuture<'a, Vec<TicketEventRow>> {
            *self.calls.entry("ticket").or_insert(0) += 1;
            let rows = self
                .tickets
                .iter()
                .filter(|row| row.ticket_id == id)
                .cloned()
                .collect();
            Box::pin(async move { Ok(rows) })
        }
        fn incident_events<'a>(
            &'a mut self,
            id: &'a str,
        ) -> SupportEventHistoryFuture<'a, Vec<IncidentEventRow>> {
            *self.calls.entry("incident").or_insert(0) += 1;
            let rows = self
                .incidents
                .iter()
                .filter(|row| row.incident_id == id)
                .cloned()
                .collect();
            Box::pin(async move { Ok(rows) })
        }
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        let mut future = Box::pin(future);
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(value) => value,
            std::task::Poll::Pending => panic!("test future unexpectedly pending"),
        }
    }

    #[test]
    fn both_histories_read_once_with_full_payload() {
        let mut ports = Fake {
            tickets: vec![ticket()],
            incidents: vec![incident()],
            ..Default::default()
        };
        let tickets = block_on(read_ticket_history(
            &mut ports,
            &scope(),
            "support-ticket-1",
        ))
        .unwrap();
        assert_eq!(tickets.len(), 1);
        assert_eq!(ports.calls["ticket"], 1);
        let incidents = block_on(read_incident_history(
            &mut ports,
            &scope(),
            "support-incident-1",
        ))
        .unwrap();
        assert_eq!(incidents.len(), 1);
        assert_eq!(ports.calls["incident"], 1);
        assert_eq!(
            serde_json::to_value(&incidents).unwrap()[0]["evidence_json"]["kind"],
            "outage"
        );
    }

    #[test]
    fn denied_reader_and_bad_selector_fail_closed() {
        let mut ports = Fake::default();
        let mut denied = scope();
        denied.support_read = false;
        assert_eq!(
            block_on(read_ticket_history(&mut ports, &denied, "support-ticket-1"))
                .unwrap_err()
                .code,
            "SUPPORT_READ_REQUIRED"
        );
        assert!(ports.calls.is_empty());
        assert_eq!(
            block_on(read_incident_history(&mut ports, &scope(), ""))
                .unwrap_err()
                .code,
            "SUPPORT_INCIDENT_HISTORY_ID_INVALID"
        );
        assert!(ports.calls.is_empty());
    }
}
