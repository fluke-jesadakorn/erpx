//! Support evidence use cases. Files supplies the current set/link facts; the
//! command transaction owns both the Files and Support mutations.
use crate::domain::support_evidence::{
    EvidenceActor, EvidenceBind, EvidenceBindPlan, EvidenceRevoke, EvidenceRevokePlan,
    EvidenceSetFacts, EvidenceTicket, plan_bind, plan_revoke,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;

pub trait EvidencePorts {
    fn now(&self) -> i64;
    fn authorize<'a>(
        &'a self,
        actor: &'a EvidenceActor,
        manage: bool,
    ) -> impl std::future::Future<Output = Result<(), BusinessError>> + Send + 'a;
    fn replay<'a>(
        &'a self,
        actor: &'a EvidenceActor,
        key: &'a str,
        operation: &'a str,
        record_id: &'a str,
        input: &'a Value,
    ) -> impl std::future::Future<Output = Result<Option<Value>, BusinessError>> + Send + 'a;
    fn ticket<'a>(
        &'a self,
        actor: &'a EvidenceActor,
        id: &'a str,
    ) -> impl std::future::Future<Output = Result<EvidenceTicket, BusinessError>> + Send + 'a;
    fn set<'a>(
        &'a self,
        actor: &'a EvidenceActor,
        id: &'a str,
    ) -> impl std::future::Future<Output = Result<EvidenceSetFacts, BusinessError>> + Send + 'a;
    fn attachment<'a>(
        &'a self,
        actor: &'a EvidenceActor,
        ticket_id: &'a str,
        id: &'a str,
    ) -> impl std::future::Future<Output = Result<bool, BusinessError>> + Send + 'a;
    fn eligible<'a>(
        &'a self,
        actor: &'a EvidenceActor,
    ) -> impl std::future::Future<Output = Result<bool, BusinessError>> + Send + 'a;
    fn commit_bind<'a>(
        &'a self,
        actor: &'a EvidenceActor,
        key: &'a str,
        plan: &'a EvidenceBindPlan,
        input: &'a Value,
    ) -> impl std::future::Future<Output = Result<Value, BusinessError>> + Send + 'a;
    fn commit_revoke<'a>(
        &'a self,
        actor: &'a EvidenceActor,
        key: &'a str,
        plan: &'a EvidenceRevokePlan,
        input: &'a Value,
    ) -> impl std::future::Future<Output = Result<Value, BusinessError>> + Send + 'a;
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

pub async fn bind_evidence<P: EvidencePorts>(
    ports: &P,
    actor: &EvidenceActor,
    ticket_id: &str,
    input: &EvidenceBind,
    key: &str,
) -> Result<Value, BusinessError> {
    ports.authorize(actor, actor.can_manage).await?;
    let wire = serde_json::to_value(input)
        .map_err(|_| invalid("SUPPORT_EVIDENCE_INVALID", "Provide valid evidence."))?;
    if let Some(prior) = ports
        .replay(actor, key, "support.evidence.bind", ticket_id, &wire)
        .await?
    {
        return Ok(prior);
    }
    if !ports.eligible(actor).await? {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SUPPORT_EVIDENCE_ASSIGNMENT",
            "Your current employment and assignment no longer permit changes to this case evidence.",
        ));
    }
    let ticket = ports.ticket(actor, ticket_id).await?;
    let set = ports
        .set(actor, input.attachment_set_ids.first().map(String::as_str).unwrap_or(""))
        .await?;
    let plan = plan_bind(actor, &ticket, &set, input, key, ports.now())?;
    ports.commit_bind(actor, key, &plan, &wire).await
}

pub async fn revoke_evidence<P: EvidencePorts>(
    ports: &P,
    actor: &EvidenceActor,
    ticket_id: &str,
    input: &EvidenceRevoke,
    key: &str,
) -> Result<Value, BusinessError> {
    ports.authorize(actor, actor.can_manage).await?;
    let wire = serde_json::to_value(input)
        .map_err(|_| invalid("SUPPORT_EVIDENCE_INVALID", "Provide valid evidence."))?;
    if let Some(prior) = ports
        .replay(actor, key, "support.evidence.revoke", ticket_id, &wire)
        .await?
    {
        return Ok(prior);
    }
    if !ports.eligible(actor).await? {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SUPPORT_EVIDENCE_ASSIGNMENT",
            "Your current employment and assignment no longer permit changes to this case evidence.",
        ));
    }
    let ticket = ports.ticket(actor, ticket_id).await?;
    if !ports
        .attachment(actor, ticket_id, &input.attachment_id)
        .await?
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "CONFLICT",
            "The evidence is held or no longer withdrawable.",
        ));
    }
    let plan = plan_revoke(actor, &ticket, input, ports.now())?;
    ports.commit_revoke(actor, key, &plan, &wire).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::support_evidence::EvidenceLinkFacts;
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
        ticket: Mutex<Option<EvidenceTicket>>,
        set: Mutex<Option<EvidenceSetFacts>>,
        attachment: Mutex<bool>,
        eligible: Mutex<bool>,
        committed: Mutex<Vec<String>>,
        denied: bool,
    }
    impl EvidencePorts for Fake {
        fn now(&self) -> i64 {
            10
        }
        async fn authorize(&self, _: &EvidenceActor, _: bool) -> Result<(), BusinessError> {
            if self.denied {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "SUPPORT_EVIDENCE_AUTHORITY_REQUIRED",
                    "Current Support evidence authority is required.",
                ));
            }
            Ok(())
        }
        async fn replay(
            &self,
            _: &EvidenceActor,
            key: &str,
            _: &str,
            _: &str,
            _: &Value,
        ) -> Result<Option<Value>, BusinessError> {
            Ok(self.replay.lock().unwrap().get(key).cloned())
        }
        async fn ticket(
            &self,
            _: &EvidenceActor,
            _: &str,
        ) -> Result<EvidenceTicket, BusinessError> {
            self.ticket.lock().unwrap().clone().ok_or_else(|| {
                BusinessError::new(
                    FailureKind::NotFound,
                    "SUPPORT_TICKET_NOT_FOUND",
                    "Ticket not found in this scope.",
                )
            })
        }
        async fn set(
            &self,
            _: &EvidenceActor,
            _: &str,
        ) -> Result<EvidenceSetFacts, BusinessError> {
            self.set.lock().unwrap().clone().ok_or_else(|| {
                BusinessError::new(
                    FailureKind::NotFound,
                    "ATTACHMENT_SET_NOT_FOUND",
                    "File set not found.",
                )
            })
        }
        async fn attachment(
            &self,
            _: &EvidenceActor,
            _: &str,
            _: &str,
        ) -> Result<bool, BusinessError> {
            Ok(*self.attachment.lock().unwrap())
        }
        async fn eligible(&self, _: &EvidenceActor) -> Result<bool, BusinessError> {
            Ok(*self.eligible.lock().unwrap())
        }
        async fn commit_bind(
            &self,
            _: &EvidenceActor,
            key: &str,
            plan: &EvidenceBindPlan,
            _: &Value,
        ) -> Result<Value, BusinessError> {
            self.committed.lock().unwrap().push(format!("bind:{key}"));
            Ok(serde_json::json!({"id": plan.ticket_id, "version": plan.ticket_version}))
        }
        async fn commit_revoke(
            &self,
            _: &EvidenceActor,
            key: &str,
            plan: &EvidenceRevokePlan,
            _: &Value,
        ) -> Result<Value, BusinessError> {
            self.committed.lock().unwrap().push(format!("revoke:{key}"));
            Ok(serde_json::json!({"id": plan.ticket_id, "version": plan.ticket_version}))
        }
    }
    fn actor() -> EvidenceActor {
        EvidenceActor {
            user_id: "agent".into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            can_read: true,
            can_work: true,
            can_manage: false,
            same_person_ids: vec![],
        }
    }
    fn ticket() -> EvidenceTicket {
        EvidenceTicket {
            id: "ticket-1".into(),
            company_id: "company".into(),
            branch_id: Some("branch".into()),
            department_id: None,
            status: "open".into(),
            source_kind: "sales_order".into(),
            owner_id: Some("agent".into()),
            version: 1,
        }
    }
    fn set() -> EvidenceSetFacts {
        EvidenceSetFacts {
            id: "set-1".into(),
            company_id: "company".into(),
            target_type: "support_ticket".into(),
            target_id: "ticket-1".into(),
            component_kind: "action".into(),
            component_ref: Some("support-evidence:internal_evidence".into()),
            purpose_key: "internal_evidence".into(),
            expected_target_version: 1,
            state: "draft".into(),
            required_count: 1,
            links: vec![EvidenceLinkFacts {
                id: "link-1".into(),
                attachment_id: "attachment-1".into(),
                source_state: "available".into(),
                checksum_sha256: "a".repeat(64),
                media_type: "application/pdf".into(),
                bytes: 128,
            }],
        }
    }
    fn bind() -> EvidenceBind {
        EvidenceBind {
            expected_version: 1,
            attachment_set_ids: vec!["set-1".into()],
            reason: "Internal case evidence".into(),
        }
    }

    #[test]
    fn binds_once_and_replays_the_original_result() {
        let fake = Fake::default();
        *fake.ticket.lock().unwrap() = Some(ticket());
        *fake.set.lock().unwrap() = Some(set());
        *fake.eligible.lock().unwrap() = true;
        let first = block_on(bind_evidence(&fake, &actor(), "ticket-1", &bind(), "key")).unwrap();
        assert_eq!(first["version"], 2);
        assert_eq!(fake.committed.lock().unwrap().len(), 1);
        fake.replay.lock().unwrap().insert("key".into(), first.clone());
        let second = block_on(bind_evidence(&fake, &actor(), "ticket-1", &bind(), "key")).unwrap();
        assert_eq!(second, first);
        assert_eq!(fake.committed.lock().unwrap().len(), 1);
    }

    #[test]
    fn revokes_once_and_rejects_held_evidence() {
        let fake = Fake::default();
        *fake.ticket.lock().unwrap() = Some(ticket());
        *fake.attachment.lock().unwrap() = true;
        *fake.eligible.lock().unwrap() = true;
        let result = block_on(revoke_evidence(
            &fake,
            &actor(),
            "ticket-1",
            &EvidenceRevoke {
                expected_version: 1,
                attachment_id: "attachment-1".into(),
                reason: "Wrong screenshot replaced".into(),
            },
            "key",
        ))
        .unwrap();
        assert_eq!(result["version"], 2);
        *fake.attachment.lock().unwrap() = false;
        assert_eq!(
            block_on(revoke_evidence(
                &fake,
                &actor(),
                "ticket-1",
                &EvidenceRevoke {
                    expected_version: 1,
                    attachment_id: "attachment-1".into(),
                    reason: "Wrong screenshot replaced".into(),
                },
                "key2",
            ))
            .unwrap_err()
            .code,
            "CONFLICT"
        );
    }

    #[test]
    fn authority_and_assignment_denials_fail_closed() {
        let denied = Fake {
            denied: true,
            ..Default::default()
        };
        assert_eq!(
            block_on(bind_evidence(&denied, &actor(), "ticket-1", &bind(), "key"))
                .unwrap_err()
                .code,
            "SUPPORT_EVIDENCE_AUTHORITY_REQUIRED"
        );
        let fake = Fake::default();
        *fake.ticket.lock().unwrap() = Some(ticket());
        *fake.set.lock().unwrap() = Some(set());
        *fake.eligible.lock().unwrap() = false;
        assert_eq!(
            block_on(bind_evidence(&fake, &actor(), "ticket-1", &bind(), "key"))
                .unwrap_err()
                .code,
            "SUPPORT_EVIDENCE_ASSIGNMENT"
        );
    }
}
