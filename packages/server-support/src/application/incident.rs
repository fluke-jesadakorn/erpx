//! Support incident use cases. Ports supply current authority, replay and the
//! owning transaction; this layer never opens a transaction or reads HTTP.
use crate::domain::incident::{
    IncidentActor, IncidentChange, IncidentChangePlan, IncidentCreate, IncidentCreatePlan,
    IncidentPlan, IncidentSnapshot, IncidentWorkItem, plan_change, plan_create, work_items,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IncidentTicketSnapshot {
    pub id: String,
    pub company_id: String,
    pub source_kind: String,
    pub version: i64,
}

pub trait IncidentPorts {
    fn now(&self) -> i64;
    fn authorize<'a>(
        &'a self,
        actor: &'a IncidentActor,
        manage: bool,
    ) -> impl std::future::Future<Output = Result<(), BusinessError>> + Send + 'a;
    fn replay<'a>(
        &'a self,
        actor: &'a IncidentActor,
        key: &'a str,
        operation: &'a str,
        record_id: &'a str,
        input: &'a Value,
    ) -> impl std::future::Future<Output = Result<Option<Value>, BusinessError>> + Send + 'a;
    fn incident<'a>(
        &'a self,
        actor: &'a IncidentActor,
        id: &'a str,
    ) -> impl std::future::Future<Output = Result<IncidentSnapshot, BusinessError>> + Send + 'a;
    fn ticket<'a>(
        &'a self,
        actor: &'a IncidentActor,
        id: &'a str,
    ) -> impl std::future::Future<Output = Result<IncidentTicketSnapshot, BusinessError>> + Send + 'a;
    fn link_count<'a>(
        &'a self,
        actor: &'a IncidentActor,
        id: &'a str,
    ) -> impl std::future::Future<Output = Result<i64, BusinessError>> + Send + 'a;
    fn commit_create<'a>(
        &'a self,
        actor: &'a IncidentActor,
        key: &'a str,
        plan: &'a IncidentCreatePlan,
        input: &'a Value,
    ) -> impl std::future::Future<Output = Result<Value, BusinessError>> + Send + 'a;
    fn commit_change<'a>(
        &'a self,
        actor: &'a IncidentActor,
        key: &'a str,
        plan: &'a IncidentChangePlan,
        input: &'a Value,
    ) -> impl std::future::Future<Output = Result<Value, BusinessError>> + Send + 'a;
    fn workspace_rows<'a>(
        &'a self,
        actor: &'a IncidentActor,
    ) -> impl std::future::Future<Output = Result<Vec<(IncidentSnapshot, String)>, BusinessError>> + Send + 'a;
}

pub async fn create_incident<P: IncidentPorts>(
    ports: &P,
    actor: &IncidentActor,
    input: &IncidentCreate,
    key: &str,
) -> Result<Value, BusinessError> {
    ports.authorize(actor, true).await?;
    let wire = serde_json::to_value(input)
        .map_err(|_| BusinessError::invalid("SUPPORT_INCIDENT_INPUT_INVALID", "Provide a valid incident."))?;
    let id = format!("incident-{key}");
    if let Some(prior) = ports
        .replay(actor, key, "support.incident.create", &id, &wire)
        .await?
    {
        return Ok(prior);
    }
    let source = ports.ticket(actor, &input.ticket_id).await?;
    if source.source_kind != "sales_order" || source.company_id != actor.company_id {
        return Err(BusinessError::invalid(
            "SUPPORT_INCIDENT_SOURCE_REQUIRED",
            "Shared customer incidents require a linked customer order.",
        ));
    }
    if source.version != input.ticket_version {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SUPPORT_INCIDENT_SOURCE_CHANGED",
            "The linked customer case changed. Reload before declaring the incident.",
        ));
    }
    let IncidentPlan::Create(plan) = plan_create(actor, key, input, ports.now())? else {
        return Err(BusinessError::invalid(
            "SUPPORT_INCIDENT_INPUT_INVALID",
            "Provide a valid incident.",
        ));
    };
    ports.commit_create(actor, key, &plan, &wire).await
}

pub async fn change_incident<P: IncidentPorts>(
    ports: &P,
    actor: &IncidentActor,
    id: &str,
    input: &IncidentChange,
    key: &str,
) -> Result<Value, BusinessError> {
    ports.authorize(actor, input.manage()).await?;
    let wire = serde_json::to_value(input)
        .map_err(|_| BusinessError::invalid("SUPPORT_INCIDENT_INPUT_INVALID", "Provide a valid action."))?;
    let operation = format!("support.incident.{}", input.action());
    if let Some(prior) = ports.replay(actor, key, &operation, id, &wire).await? {
        return Ok(prior);
    }
    let snapshot = ports.incident(actor, id).await?;
    if let IncidentChange::Link { ticket_id, ticket_version, .. } = input {
        let ticket = ports.ticket(actor, ticket_id).await?;
        if ticket.company_id != actor.company_id || ticket.version != *ticket_version {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "SUPPORT_INCIDENT_SOURCE_CHANGED",
                "The linked customer case changed. Reload before linking it.",
            ));
        }
    }
    if let IncidentChange::Unlink { .. } = input
        && ports.link_count(actor, id).await? <= 1
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SUPPORT_INCIDENT_LAST_LINK",
            "Keep at least one affected customer case linked to the incident.",
        ));
    }
    let IncidentPlan::Change(plan) = plan_change(actor, &snapshot, input, ports.now())? else {
        return Err(BusinessError::invalid(
            "SUPPORT_INCIDENT_INPUT_INVALID",
            "Provide a valid action.",
        ));
    };
    ports.commit_change(actor, key, &plan, &wire).await
}

pub async fn get_incident<P: IncidentPorts>(
    ports: &P,
    actor: &IncidentActor,
    id: &str,
) -> Result<IncidentSnapshot, BusinessError> {
    ports.authorize(actor, false).await?;
    ports.incident(actor, id).await
}

pub async fn workspace<P: IncidentPorts>(
    ports: &P,
    actor: &IncidentActor,
) -> Result<Vec<(IncidentSnapshot, String)>, BusinessError> {
    ports.authorize(actor, false).await?;
    let rows = ports.workspace_rows(actor).await?;
    Ok(rows
        .into_iter()
        .filter(|(row, _)| row.company_id == actor.company_id)
        .collect())
}

pub async fn work<P: IncidentPorts>(
    ports: &P,
    actor: &IncidentActor,
) -> Result<Vec<IncidentWorkItem>, BusinessError> {
    if !actor.permissions.can_read && !actor.permissions.can_work {
        return Ok(Vec::new());
    }
    let rows = ports.workspace_rows(actor).await?;
    Ok(work_items(actor, &rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::incident::{
        IncidentPermissions, IncidentSeverity, IncidentStatus,
    };
    use std::sync::Mutex;

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        let mut future = Box::pin(future);
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(value) => value,
            std::task::Poll::Pending => panic!("test future unexpectedly pending"),
        }
    }

    #[derive(Default)]
    struct Fake {
        replay: Mutex<std::collections::BTreeMap<String, Value>>,
        incident: Mutex<Option<IncidentSnapshot>>,
        ticket: Mutex<Option<IncidentTicketSnapshot>>,
        links: Mutex<i64>,
        committed: Mutex<Vec<String>>,
        denied: bool,
    }
    impl IncidentPorts for Fake {
        fn now(&self) -> i64 {
            10
        }
        async fn authorize(&self, _: &IncidentActor, _: bool) -> Result<(), BusinessError> {
            if self.denied {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "SUPPORT_INCIDENT_AUTHORITY_REQUIRED",
                    "Current Support incident authority is required.",
                ));
            }
            Ok(())
        }
        async fn replay(
            &self,
            _: &IncidentActor,
            key: &str,
            _: &str,
            _: &str,
            _: &Value,
        ) -> Result<Option<Value>, BusinessError> {
            Ok(self.replay.lock().unwrap().get(key).cloned())
        }
        async fn incident(
            &self,
            _: &IncidentActor,
            _: &str,
        ) -> Result<IncidentSnapshot, BusinessError> {
            self.incident.lock().unwrap().clone().ok_or_else(|| {
                BusinessError::new(
                    FailureKind::NotFound,
                    "SUPPORT_INCIDENT_NOT_FOUND",
                    "Incident not found in this scope.",
                )
            })
        }
        async fn ticket(
            &self,
            _: &IncidentActor,
            _: &str,
        ) -> Result<IncidentTicketSnapshot, BusinessError> {
            self.ticket.lock().unwrap().clone().ok_or_else(|| {
                BusinessError::new(
                    FailureKind::NotFound,
                    "SUPPORT_TICKET_NOT_FOUND",
                    "Ticket not found in this scope.",
                )
            })
        }
        async fn link_count(&self, _: &IncidentActor, _: &str) -> Result<i64, BusinessError> {
            Ok(*self.links.lock().unwrap())
        }
        async fn commit_create(
            &self,
            _: &IncidentActor,
            key: &str,
            plan: &IncidentCreatePlan,
            _: &Value,
        ) -> Result<Value, BusinessError> {
            self.committed.lock().unwrap().push(format!("create:{key}"));
            Ok(serde_json::json!({"id": plan.id, "status": "open", "version": 1}))
        }
        async fn commit_change(
            &self,
            _: &IncidentActor,
            key: &str,
            plan: &IncidentChangePlan,
            _: &Value,
        ) -> Result<Value, BusinessError> {
            self.committed.lock().unwrap().push(format!("change:{key}"));
            Ok(serde_json::json!({"id": plan.id, "status": plan.status.as_str(), "version": plan.version}))
        }
        async fn workspace_rows(
            &self,
            _: &IncidentActor,
        ) -> Result<Vec<(IncidentSnapshot, String)>, BusinessError> {
            Ok(Vec::new())
        }
    }
    fn actor() -> IncidentActor {
        IncidentActor {
            user_id: "manager".into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            permissions: IncidentPermissions {
                can_read: true,
                can_work: true,
                can_manage: true,
            },
            same_person_ids: vec![],
        }
    }
    fn ticket() -> IncidentTicketSnapshot {
        IncidentTicketSnapshot {
            id: "ticket-1".into(),
            company_id: "company".into(),
            source_kind: "sales_order".into(),
            version: 1,
        }
    }
    fn snapshot() -> IncidentSnapshot {
        IncidentSnapshot {
            id: "incident-1".into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            status: IncidentStatus::Open,
            severity: IncidentSeverity::High,
            owner_id: "agent".into(),
            mitigated_by: None,
            created_by: "manager".into(),
            version: 1,
        }
    }

    #[test]
    fn creates_once_and_replays_the_original_result() {
        let fake = Fake::default();
        *fake.ticket.lock().unwrap() = Some(ticket());
        let input = IncidentCreate {
            ticket_id: "ticket-1".into(),
            ticket_version: 1,
            title: "Checkout outage".into(),
            impact: "Customers cannot check out.".into(),
            severity: IncidentSeverity::Critical,
            owner_id: "agent".into(),
            due_date: "2026-11-10".into(),
        };
        let first = block_on(create_incident(&fake, &actor(), &input, "key")).unwrap();
        assert_eq!(first["id"], "incident-key");
        assert_eq!(fake.committed.lock().unwrap().len(), 1);
        fake.replay.lock().unwrap().insert("key".into(), first.clone());
        let second = block_on(create_incident(&fake, &actor(), &input, "key")).unwrap();
        assert_eq!(second, first);
        assert_eq!(fake.committed.lock().unwrap().len(), 1);
    }

    #[test]
    fn rejects_a_non_order_source_and_stale_ticket() {
        let fake = Fake::default();
        *fake.ticket.lock().unwrap() = Some(IncidentTicketSnapshot {
            source_kind: "general".into(),
            ..ticket()
        });
        let input = IncidentCreate {
            ticket_id: "ticket-1".into(),
            ticket_version: 1,
            title: "Checkout outage".into(),
            impact: "Customers cannot check out.".into(),
            severity: IncidentSeverity::Normal,
            owner_id: "agent".into(),
            due_date: "2026-11-10".into(),
        };
        assert_eq!(
            block_on(create_incident(&fake, &actor(), &input, "key"))
                .unwrap_err()
                .code,
            "SUPPORT_INCIDENT_SOURCE_REQUIRED"
        );
        *fake.ticket.lock().unwrap() = Some(IncidentTicketSnapshot { version: 2, ..ticket() });
        assert_eq!(
            block_on(create_incident(&fake, &actor(), &input, "key"))
                .unwrap_err()
                .code,
            "SUPPORT_INCIDENT_SOURCE_CHANGED"
        );
    }

    #[test]
    fn change_loads_the_current_incident_and_commits_the_plan() {
        let fake = Fake::default();
        *fake.incident.lock().unwrap() = Some(snapshot());
        let mut agent = actor();
        agent.user_id = "agent".into();
        let result = block_on(change_incident(
            &fake,
            &agent,
            "incident-1",
            &IncidentChange::Start {
                expected_version: 1,
                reason: "Start investigation".into(),
            },
            "key",
        ))
        .unwrap();
        assert_eq!(result["status"], "investigating");
        assert_eq!(fake.committed.lock().unwrap().len(), 1);
    }

    #[test]
    fn authority_denial_and_last_unlink_fail_closed() {
        let fake = Fake {
            denied: true,
            ..Default::default()
        };
        let input = IncidentCreate {
            ticket_id: "ticket-1".into(),
            ticket_version: 1,
            title: "Checkout outage".into(),
            impact: "Customers cannot check out.".into(),
            severity: IncidentSeverity::Normal,
            owner_id: "agent".into(),
            due_date: "2026-11-10".into(),
        };
        assert_eq!(
            block_on(create_incident(&fake, &actor(), &input, "key"))
                .unwrap_err()
                .code,
            "SUPPORT_INCIDENT_AUTHORITY_REQUIRED"
        );
        let fake = Fake::default();
        *fake.incident.lock().unwrap() = Some(snapshot());
        *fake.links.lock().unwrap() = 1;
        assert_eq!(
            block_on(change_incident(
                &fake,
                &actor(),
                "incident-1",
                &IncidentChange::Unlink {
                    expected_version: 1,
                    reason: "Correct the link".into(),
                    ticket_id: "ticket-1".into(),
                },
                "key",
            ))
            .unwrap_err()
            .code,
            "SUPPORT_INCIDENT_LAST_LINK"
        );
    }
}
