//! Support ticket use cases. Ports supply current authority, replay and the
//! owning transaction; this layer never opens a transaction or reads HTTP.
use crate::domain::ticket::{
    TicketActor, TicketChange, TicketChangePlan, TicketCreate, TicketCreatePlan, TicketPlan,
    TicketSnapshot, TicketStatus, TicketWorkItem, plan_change, plan_create, work_items,
};
use erpx_server_shared::public::BusinessError;
use serde_json::Value;

pub trait TicketPorts {
    fn now(&self) -> i64;
    fn authorize<'a>(
        &'a self,
        actor: &'a TicketActor,
        write: bool,
    ) -> impl std::future::Future<Output = Result<(), BusinessError>> + Send + 'a;
    fn replay<'a>(
        &'a self,
        actor: &'a TicketActor,
        key: &'a str,
        operation: &'a str,
        record_id: &'a str,
        input: &'a Value,
    ) -> impl std::future::Future<Output = Result<Option<Value>, BusinessError>> + Send + 'a;
    fn ticket<'a>(
        &'a self,
        actor: &'a TicketActor,
        id: &'a str,
    ) -> impl std::future::Future<Output = Result<TicketSnapshot, BusinessError>> + Send + 'a;
    fn commit_create<'a>(
        &'a self,
        actor: &'a TicketActor,
        key: &'a str,
        plan: &'a TicketCreatePlan,
        input: &'a Value,
    ) -> impl std::future::Future<Output = Result<Value, BusinessError>> + Send + 'a;
    fn commit_change<'a>(
        &'a self,
        actor: &'a TicketActor,
        key: &'a str,
        plan: &'a TicketChangePlan,
        input: &'a Value,
    ) -> impl std::future::Future<Output = Result<Value, BusinessError>> + Send + 'a;
    fn workspace_rows<'a>(
        &'a self,
        actor: &'a TicketActor,
    ) -> impl std::future::Future<Output = Result<Vec<(TicketSnapshot, String)>, BusinessError>> + Send + 'a;
}

pub async fn create_ticket<P: TicketPorts>(
    ports: &P,
    actor: &TicketActor,
    input: &TicketCreate,
    key: &str,
) -> Result<Value, BusinessError> {
    ports.authorize(actor, true).await?;
    let wire = serde_json::to_value(input)
        .map_err(|_| BusinessError::invalid("SUPPORT_INPUT_INVALID", "Provide a valid ticket."))?;
    let id = format!("ticket-{key}");
    if let Some(prior) = ports
        .replay(actor, key, "support.create", &id, &wire)
        .await?
    {
        return Ok(prior);
    }
    let TicketPlan::Create(plan) = plan_create(actor, key, input, ports.now())? else {
        return Err(BusinessError::invalid(
            "SUPPORT_INPUT_INVALID",
            "Provide a valid ticket.",
        ));
    };
    ports.commit_create(actor, key, &plan, &wire).await
}

pub async fn change_ticket<P: TicketPorts>(
    ports: &P,
    actor: &TicketActor,
    id: &str,
    input: &TicketChange,
    key: &str,
) -> Result<Value, BusinessError> {
    let manage = matches!(
        input,
        TicketChange::Triage { .. }
            | TicketChange::Reassign { .. }
            | TicketChange::Confirm { .. }
            | TicketChange::ConfirmGeneral { .. }
            | TicketChange::RejectResolution { .. }
            | TicketChange::Reopen { .. }
            | TicketChange::Cancel { .. }
    );
    let reporting = matches!(
        input,
        TicketChange::EditReport { .. }
            | TicketChange::WithdrawReport { .. }
            | TicketChange::AddInformation { .. }
    );
    ports.authorize(actor, reporting || !manage && !actor.permissions.can_work).await?;
    let wire = serde_json::to_value(input)
        .map_err(|_| BusinessError::invalid("SUPPORT_INPUT_INVALID", "Provide a valid action."))?;
    let operation = format!("support.{}", input.action());
    if let Some(prior) = ports
        .replay(actor, key, &operation, id, &wire)
        .await?
    {
        return Ok(prior);
    }
    let snapshot = ports.ticket(actor, id).await?;
    let TicketPlan::Change(plan) = plan_change(actor, &snapshot, input, ports.now())? else {
        return Err(BusinessError::invalid(
            "SUPPORT_INPUT_INVALID",
            "Provide a valid action.",
        ));
    };
    ports.commit_change(actor, key, &plan, &wire).await
}

pub async fn get_ticket<P: TicketPorts>(
    ports: &P,
    actor: &TicketActor,
    id: &str,
) -> Result<TicketSnapshot, BusinessError> {
    ports.authorize(actor, false).await?;
    ports.ticket(actor, id).await
}

pub async fn workspace<P: TicketPorts>(
    ports: &P,
    actor: &TicketActor,
) -> Result<Vec<(TicketSnapshot, String)>, BusinessError> {
    ports.authorize(actor, false).await?;
    let rows = ports.workspace_rows(actor).await?;
    Ok(rows
        .into_iter()
        .filter(|(row, _)| row.company_id == actor.company_id)
        .collect())
}

pub async fn work<P: TicketPorts>(
    ports: &P,
    actor: &TicketActor,
) -> Result<Vec<TicketWorkItem>, BusinessError> {
    if !actor.permissions.can_read && !actor.permissions.can_work {
        return Ok(Vec::new());
    }
    let rows = ports.workspace_rows(actor).await?;
    Ok(work_items(actor, &rows))
}

pub async fn closed_rows<P: TicketPorts>(
    ports: &P,
    actor: &TicketActor,
) -> Result<Vec<(TicketSnapshot, String)>, BusinessError> {
    ports.authorize(actor, false).await?;
    Ok(ports
        .workspace_rows(actor)
        .await?
        .into_iter()
        .filter(|(row, _)| row.status.terminal())
        .collect())
}

pub fn status_is_terminal(status: TicketStatus) -> bool {
    status.terminal()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ticket::{
        SourceKind, TicketChangePlan, TicketCreatePlan, TicketPermissions,
    };
    use std::sync::Mutex;

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        // The fake ports complete without suspension; a pending future is a test
        // failure rather than an effectful wait.
        let mut future = Box::pin(future);
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(value) => value,
            std::task::Poll::Pending => panic!("test future unexpectedly pending"),
        }
    }

    fn result_value(id: &str, status: &str, version: i64) -> Value {
        let mut map = serde_json::Map::new();
        map.insert("id".into(), Value::String(id.into()));
        map.insert("status".into(), Value::String(status.into()));
        map.insert("version".into(), Value::from(version));
        Value::Object(map)
    }
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Fake {
        replay: Mutex<BTreeMap<String, Value>>,
        ticket: Mutex<Option<TicketSnapshot>>,
        committed: Mutex<Vec<String>>,
        denied: bool,
    }
    impl TicketPorts for Fake {
        fn now(&self) -> i64 {
            10
        }
        async fn authorize(
            &self,
            _: &TicketActor,
            _: bool,
        ) -> Result<(), BusinessError> {
            if self.denied {
                return Err(BusinessError::new(
                    erpx_server_shared::public::FailureKind::Forbidden,
                    "SUPPORT_AUTHORITY_REQUIRED",
                    "Current Support authority is required.",
                ));
            }
            Ok(())
        }
        async fn replay(
            &self,
            _: &TicketActor,
            key: &str,
            _: &str,
            _: &str,
            _: &Value,
        ) -> Result<Option<Value>, BusinessError> {
            Ok(self.replay.lock().unwrap().get(key).cloned())
        }
        async fn ticket(
            &self,
            _: &TicketActor,
            _: &str,
        ) -> Result<TicketSnapshot, BusinessError> {
            self.ticket.lock().unwrap().clone().ok_or_else(|| {
                BusinessError::new(
                    erpx_server_shared::public::FailureKind::NotFound,
                    "SUPPORT_TICKET_NOT_FOUND",
                    "Ticket not found in this scope.",
                )
            })
        }
        async fn commit_create(
            &self,
            _: &TicketActor,
            key: &str,
            plan: &TicketCreatePlan,
            _: &Value,
        ) -> Result<Value, BusinessError> {
            self.committed.lock().unwrap().push(format!("create:{key}"));
            Ok(result_value(&plan.id, plan.status.as_str(), 1))
        }
        async fn commit_change(
            &self,
            _: &TicketActor,
            key: &str,
            plan: &TicketChangePlan,
            _: &Value,
        ) -> Result<Value, BusinessError> {
            self.committed.lock().unwrap().push(format!("change:{key}"));
            Ok(result_value(&plan.id, plan.status.as_str(), plan.version))
        }
        async fn workspace_rows(
            &self,
            _: &TicketActor,
        ) -> Result<Vec<(TicketSnapshot, String)>, BusinessError> {
            Ok(Vec::new())
        }
    }
    fn actor() -> TicketActor {
        TicketActor {
            user_id: "agent".into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            permissions: TicketPermissions {
                can_read: true,
                can_create: true,
                can_write: true,
                can_manage: true,
                can_work: true,
            },
            same_person_ids: vec![],
        }
    }
    fn snapshot() -> TicketSnapshot {
        TicketSnapshot {
            id: "ticket-1".into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            source_kind: SourceKind::General,
            status: TicketStatus::Open,
            owner_id: Some("agent".into()),
            due_date: Some("2026-11-10".into()),
            resolved_by: None,
            created_by: "reporter".into(),
            confirmation_note_id: None,
            version: 1,
        }
    }

    #[test]
    fn create_commits_once_and_replays_the_original_result() { block_on(async {
        let fake = Fake::default();
        let input = TicketCreate::General {
            title: "Printer offline".into(),
            issue: "The front desk printer is offline.".into(),
        };
        let first = create_ticket(&fake, &actor(), &input, "key").await.unwrap();
        assert_eq!(first["id"], "ticket-key");
        assert_eq!(fake.committed.lock().unwrap().len(), 1);
        fake.replay
            .lock()
            .unwrap()
            .insert("key".into(), first.clone());
        let second = create_ticket(&fake, &actor(), &input, "key").await.unwrap();
        assert_eq!(second, first);
        assert_eq!(fake.committed.lock().unwrap().len(), 1);
    }) }

    #[test]
    fn change_loads_the_current_ticket_and_commits_the_plan() { block_on(async {
        let fake = Fake::default();
        *fake.ticket.lock().unwrap() = Some(snapshot());
        let result = change_ticket(
            &fake,
            &actor(),
            "ticket-1",
            &TicketChange::Start {
                expected_version: 1,
                reason: "Start assistance".into(),
            },
            "key",
        )
        .await
        .unwrap();
        assert_eq!(result["status"], "in_progress");
        assert_eq!(fake.committed.lock().unwrap().len(), 1);
    }) }

    #[test]
    fn authority_denial_and_missing_ticket_fail_closed() { block_on(async {
        let fake = Fake {
            denied: true,
            ..Default::default()
        };
        assert_eq!(
            create_ticket(
                &fake,
                &actor(),
                &TicketCreate::General {
                    title: "Title".into(),
                    issue: "Issue".into()
                },
                "key"
            )
            .await
            .unwrap_err()
            .code,
            "SUPPORT_AUTHORITY_REQUIRED"
        );
        let fake = Fake::default();
        assert_eq!(
            get_ticket(&fake, &actor(), "missing").await.unwrap_err().code,
            "SUPPORT_TICKET_NOT_FOUND"
        );
    }) }
}
