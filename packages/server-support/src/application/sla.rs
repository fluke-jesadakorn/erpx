//! Support SLA use cases. Ports supply current authority, replay and the owning
//! transaction; this layer never opens a transaction or reads HTTP.
use crate::domain::sla::{
    AlertAcknowledge, AlertAcknowledgePlan, ClockAttach, ClockAttachPlan, PolicyChange,
    PolicyChangePlan, PolicyCreate, PolicyCreatePlan, PolicySnapshot, SlaActor,
    SlaPolicy, TicketSnapshot, plan_alert_acknowledge, plan_attach, plan_policy_change,
    plan_policy_create,
};
use erpx_server_shared::public::BusinessError;
use serde_json::Value;

pub trait SlaPorts {
    fn now(&self) -> i64;
    fn authorize<'a>(
        &'a self,
        actor: &'a SlaActor,
        manage: bool,
    ) -> impl std::future::Future<Output = Result<(), BusinessError>> + Send + 'a;
    fn replay<'a>(
        &'a self,
        actor: &'a SlaActor,
        key: &'a str,
        operation: &'a str,
        record_id: &'a str,
        input: &'a Value,
    ) -> impl std::future::Future<Output = Result<Option<Value>, BusinessError>> + Send + 'a;
    fn policy<'a>(
        &'a self,
        actor: &'a SlaActor,
        id: &'a str,
    ) -> impl std::future::Future<Output = Result<(PolicySnapshot, SlaPolicy), BusinessError>> + Send + 'a;
    fn ticket<'a>(
        &'a self,
        actor: &'a SlaActor,
        id: &'a str,
    ) -> impl std::future::Future<Output = Result<TicketSnapshot, BusinessError>> + Send + 'a;
    fn alert<'a>(
        &'a self,
        actor: &'a SlaActor,
        id: &'a str,
    ) -> impl std::future::Future<Output = Result<crate::domain::sla::AlertSnapshot, BusinessError>> + Send + 'a;
    fn commit_create<'a>(
        &'a self,
        actor: &'a SlaActor,
        key: &'a str,
        plan: &'a PolicyCreatePlan,
        input: &'a Value,
    ) -> impl std::future::Future<Output = Result<Value, BusinessError>> + Send + 'a;
    fn commit_change<'a>(
        &'a self,
        actor: &'a SlaActor,
        key: &'a str,
        plan: &'a PolicyChangePlan,
        input: &'a Value,
    ) -> impl std::future::Future<Output = Result<Value, BusinessError>> + Send + 'a;
    fn commit_attach<'a>(
        &'a self,
        actor: &'a SlaActor,
        key: &'a str,
        plan: &'a ClockAttachPlan,
        input: &'a Value,
    ) -> impl std::future::Future<Output = Result<Value, BusinessError>> + Send + 'a;
    fn commit_ack<'a>(
        &'a self,
        actor: &'a SlaActor,
        key: &'a str,
        plan: &'a AlertAcknowledgePlan,
        input: &'a Value,
    ) -> impl std::future::Future<Output = Result<Value, BusinessError>> + Send + 'a;
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

pub async fn create_policy<P: SlaPorts>(
    ports: &P,
    actor: &SlaActor,
    input: &PolicyCreate,
    key: &str,
) -> Result<Value, BusinessError> {
    ports.authorize(actor, true).await?;
    let wire = serde_json::to_value(input)
        .map_err(|_| invalid("SUPPORT_SLA_INPUT_INVALID", "Provide a valid policy."))?;
    let id = format!("support-policy-{key}");
    if let Some(prior) = ports
        .replay(actor, key, "support.sla.create", &id, &wire)
        .await?
    {
        return Ok(prior);
    }
    let plan = plan_policy_create(actor, key, input, ports.now())?;
    ports.commit_create(actor, key, &plan, &wire).await
}

pub async fn change_policy<P: SlaPorts>(
    ports: &P,
    actor: &SlaActor,
    id: &str,
    input: &PolicyChange,
    key: &str,
) -> Result<Value, BusinessError> {
    ports.authorize(actor, true).await?;
    let wire = serde_json::to_value(input)
        .map_err(|_| invalid("SUPPORT_SLA_INPUT_INVALID", "Provide a valid action."))?;
    let operation = format!("support.sla.{}", input.action());
    if let Some(prior) = ports.replay(actor, key, &operation, id, &wire).await? {
        return Ok(prior);
    }
    let (snapshot, _) = ports.policy(actor, id).await?;
    let plan = plan_policy_change(actor, &snapshot, input, ports.now())?;
    ports.commit_change(actor, key, &plan, &wire).await
}

pub async fn attach_clock<P: SlaPorts>(
    ports: &P,
    actor: &SlaActor,
    id: &str,
    input: &ClockAttach,
    key: &str,
) -> Result<Value, BusinessError> {
    ports.authorize(actor, true).await?;
    let wire = serde_json::to_value(input)
        .map_err(|_| invalid("SUPPORT_SLA_INPUT_INVALID", "Provide a valid attachment."))?;
    if let Some(prior) = ports
        .replay(actor, key, "support.sla.attach", id, &wire)
        .await?
    {
        return Ok(prior);
    }
    let ticket = ports.ticket(actor, id).await?;
    let (policy, value) = ports.policy(actor, &input.policy_id).await?;
    let plan = plan_attach(actor, &ticket, &policy, &value, input, ports.now())?;
    ports.commit_attach(actor, key, &plan, &wire).await
}

pub async fn acknowledge_alert<P: SlaPorts>(
    ports: &P,
    actor: &SlaActor,
    id: &str,
    input: &AlertAcknowledge,
    key: &str,
) -> Result<Value, BusinessError> {
    ports.authorize(actor, true).await?;
    let wire = serde_json::to_value(input)
        .map_err(|_| invalid("SUPPORT_SLA_INPUT_INVALID", "Provide a valid acknowledgement."))?;
    if let Some(prior) = ports
        .replay(actor, key, "support.sla.acknowledge", id, &wire)
        .await?
    {
        return Ok(prior);
    }
    let snapshot = ports.alert(actor, id).await?;
    let plan = plan_alert_acknowledge(actor, &snapshot, input, ports.now())?;
    ports.commit_ack(actor, key, &plan, &wire).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::business_time::BusinessCalendar;
    use crate::domain::sla::{AlertSnapshot, PolicyStatus, SlaPermissions};
    use erpx_server_shared::public::FailureKind;
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
        policy: Mutex<Option<(PolicySnapshot, SlaPolicy)>>,
        ticket: Mutex<Option<TicketSnapshot>>,
        alert: Mutex<Option<AlertSnapshot>>,
        committed: Mutex<Vec<String>>,
        denied: bool,
    }
    impl SlaPorts for Fake {
        fn now(&self) -> i64 {
            1_793_757_600_000
        }
        async fn authorize(&self, _: &SlaActor, _: bool) -> Result<(), BusinessError> {
            if self.denied {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "SUPPORT_SLA_AUTHORITY_REQUIRED",
                    "Current Support SLA authority is required.",
                ));
            }
            Ok(())
        }
        async fn replay(
            &self,
            _: &SlaActor,
            key: &str,
            _: &str,
            _: &str,
            _: &Value,
        ) -> Result<Option<Value>, BusinessError> {
            Ok(self.replay.lock().unwrap().get(key).cloned())
        }
        async fn policy(
            &self,
            _: &SlaActor,
            _: &str,
        ) -> Result<(PolicySnapshot, SlaPolicy), BusinessError> {
            self.policy.lock().unwrap().clone().ok_or_else(|| {
                BusinessError::new(
                    FailureKind::NotFound,
                    "SUPPORT_POLICY_NOT_FOUND",
                    "Policy not found in this Company.",
                )
            })
        }
        async fn ticket(
            &self,
            _: &SlaActor,
            _: &str,
        ) -> Result<TicketSnapshot, BusinessError> {
            self.ticket.lock().unwrap().clone().ok_or_else(|| {
                BusinessError::new(
                    FailureKind::NotFound,
                    "SUPPORT_TICKET_NOT_FOUND",
                    "Ticket not found in this scope.",
                )
            })
        }
        async fn alert(
            &self,
            _: &SlaActor,
            _: &str,
        ) -> Result<AlertSnapshot, BusinessError> {
            self.alert.lock().unwrap().clone().ok_or_else(|| {
                BusinessError::new(
                    FailureKind::NotFound,
                    "SUPPORT_ALERT_NOT_FOUND",
                    "Target escalation is not available in this scope.",
                )
            })
        }
        async fn commit_create(
            &self,
            _: &SlaActor,
            key: &str,
            plan: &PolicyCreatePlan,
            _: &Value,
        ) -> Result<Value, BusinessError> {
            self.committed.lock().unwrap().push(format!("create:{key}"));
            Ok(serde_json::json!({"id": plan.id, "version": 1}))
        }
        async fn commit_change(
            &self,
            _: &SlaActor,
            key: &str,
            plan: &PolicyChangePlan,
            _: &Value,
        ) -> Result<Value, BusinessError> {
            self.committed.lock().unwrap().push(format!("change:{key}"));
            Ok(serde_json::json!({"id": plan.id, "version": plan.version}))
        }
        async fn commit_attach(
            &self,
            _: &SlaActor,
            key: &str,
            plan: &ClockAttachPlan,
            _: &Value,
        ) -> Result<Value, BusinessError> {
            self.committed.lock().unwrap().push(format!("attach:{key}"));
            Ok(serde_json::json!({"id": plan.ticket_id, "version": plan.ticket_version}))
        }
        async fn commit_ack(
            &self,
            _: &SlaActor,
            key: &str,
            plan: &AlertAcknowledgePlan,
            _: &Value,
        ) -> Result<Value, BusinessError> {
            self.committed.lock().unwrap().push(format!("ack:{key}"));
            Ok(serde_json::json!({"id": plan.id, "version": plan.version}))
        }
    }
    fn policy() -> SlaPolicy {
        SlaPolicy {
            calendar: BusinessCalendar {
                timezone: "Asia/Bangkok".into(),
                weekdays: vec![1, 2, 3, 4, 5],
                opens_at_minute: 540,
                closes_at_minute: 1020,
                holidays: vec![],
            },
            target_minutes: 60,
            pause_while_waiting: true,
        }
    }
    fn actor() -> SlaActor {
        SlaActor {
            user_id: "manager".into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            permissions: SlaPermissions {
                can_read: true,
                can_manage: true,
            },
            same_person_ids: vec![],
        }
    }
    fn snapshot(status: PolicyStatus) -> PolicySnapshot {
        PolicySnapshot {
            id: "support-policy-1".into(),
            company_id: "company".into(),
            status,
            created_by: "creator".into(),
            approved_by: None,
            version: 1,
        }
    }

    #[test]
    fn creates_once_and_replays_the_original_result() {
        let fake = Fake::default();
        let input = PolicyCreate {
            name: "Standard support".into(),
            policy: policy(),
            reason: "Draft the standard support target".into(),
        };
        let first = block_on(create_policy(&fake, &actor(), &input, "key")).unwrap();
        assert_eq!(first["id"], "support-policy-key");
        assert_eq!(fake.committed.lock().unwrap().len(), 1);
        fake.replay.lock().unwrap().insert("key".into(), first.clone());
        let second = block_on(create_policy(&fake, &actor(), &input, "key")).unwrap();
        assert_eq!(second, first);
        assert_eq!(fake.committed.lock().unwrap().len(), 1);
    }

    #[test]
    fn approves_an_independent_policy_and_attaches_it() {
        let fake = Fake::default();
        *fake.policy.lock().unwrap() = Some((snapshot(PolicyStatus::Draft), policy()));
        let approve = PolicyChange::Approve {
            expected_version: 1,
            reason: "Independent approval".into(),
        };
        block_on(change_policy(
            &fake,
            &actor(),
            "support-policy-1",
            &approve,
            "approve",
        ))
        .unwrap();
        *fake.policy.lock().unwrap() = Some((snapshot(PolicyStatus::Approved), policy()));
        *fake.ticket.lock().unwrap() = Some(TicketSnapshot {
            id: "ticket-1".into(),
            company_id: "company".into(),
            status: "open".into(),
            version: 1,
            sla_policy_id: None,
            clock: None,
        });
        let attach = ClockAttach {
            expected_version: 1,
            policy_id: "support-policy-1".into(),
            policy_version: 1,
            reason: "Attach the standard target".into(),
        };
        let result = block_on(attach_clock(&fake, &actor(), "ticket-1", &attach, "attach")).unwrap();
        assert_eq!(result["version"], 2);
    }

    #[test]
    fn acknowledges_once_and_denies_missing_authority() {
        let fake = Fake::default();
        *fake.alert.lock().unwrap() = Some(AlertSnapshot {
            id: "support-breach-ticket-1".into(),
            ticket_id: "ticket-1".into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            version: 1,
            acknowledged_by: None,
        });
        let input = AlertAcknowledge {
            expected_version: 1,
            reason: "Take responsibility and follow the recovery plan".into(),
        };
        let result = block_on(acknowledge_alert(
            &fake,
            &actor(),
            "support-breach-ticket-1",
            &input,
            "ack",
        ))
        .unwrap();
        assert_eq!(result["version"], 2);
        let denied = Fake {
            denied: true,
            ..Default::default()
        };
        assert_eq!(
            block_on(acknowledge_alert(
                &denied,
                &actor(),
                "support-breach-ticket-1",
                &input,
                "ack"
            ))
            .unwrap_err()
            .code,
            "SUPPORT_SLA_AUTHORITY_REQUIRED"
        );
    }
}
