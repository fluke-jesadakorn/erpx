//! Support session use cases. Ports supply current authority, replay and the
//! owning transaction; this layer never opens a transaction or reads HTTP.
use crate::domain::session::{
    SessionAction, SessionActor, SessionChangePlan, SessionPlan, SessionRequest, SessionRequestPlan,
    SessionSnapshot, plan_change, plan_request,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionTicketSnapshot {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub source_kind: String,
    pub owner_id: String,
    pub status: String,
    pub version: i64,
}

impl SessionTicketSnapshot {
    pub fn active(&self) -> bool {
        matches!(
            self.status.as_str(),
            "open" | "in_progress" | "waiting_customer"
        )
    }
}

pub trait SessionPorts {
    fn now(&self) -> i64;
    fn authorize<'a>(
        &'a self,
        actor: &'a SessionActor,
        manage: bool,
    ) -> impl std::future::Future<Output = Result<(), BusinessError>> + Send + 'a;
    fn replay<'a>(
        &'a self,
        actor: &'a SessionActor,
        key: &'a str,
        operation: &'a str,
        record_id: &'a str,
        input: &'a Value,
    ) -> impl std::future::Future<Output = Result<Option<Value>, BusinessError>> + Send + 'a;
    fn ticket<'a>(
        &'a self,
        actor: &'a SessionActor,
        id: &'a str,
    ) -> impl std::future::Future<Output = Result<SessionTicketSnapshot, BusinessError>> + Send + 'a;
    fn session<'a>(
        &'a self,
        actor: &'a SessionActor,
        id: &'a str,
    ) -> impl std::future::Future<Output = Result<SessionSnapshot, BusinessError>> + Send + 'a;
    fn target_eligible<'a>(
        &'a self,
        actor: &'a SessionActor,
        requested_for: &'a str,
        ticket: &'a SessionTicketSnapshot,
    ) -> impl std::future::Future<Output = Result<bool, BusinessError>> + Send + 'a;
    fn capability_active<'a>(
        &'a self,
        actor: &'a SessionActor,
        session_id: &'a str,
        capability_id: &'a str,
        version: i64,
    ) -> impl std::future::Future<Output = Result<bool, BusinessError>> + Send + 'a;
    fn commit_request<'a>(
        &'a self,
        actor: &'a SessionActor,
        key: &'a str,
        plan: &'a SessionRequestPlan,
        input: &'a Value,
    ) -> impl std::future::Future<Output = Result<Value, BusinessError>> + Send + 'a;
    fn commit_change<'a>(
        &'a self,
        actor: &'a SessionActor,
        key: &'a str,
        plan: &'a SessionChangePlan,
        input: &'a Value,
    ) -> impl std::future::Future<Output = Result<Value, BusinessError>> + Send + 'a;
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "SUPPORT_SESSION_NOT_FOUND",
        "Support session not found in this scope.",
    )
}

pub async fn request_session<P: SessionPorts>(
    ports: &P,
    actor: &SessionActor,
    input: &SessionRequest,
    key: &str,
) -> Result<Value, BusinessError> {
    ports.authorize(actor, false).await?;
    let wire = serde_json::to_value(input)
        .map_err(|_| invalid("SUPPORT_SESSION_INPUT_INVALID", "Provide a valid request."))?;
    let id = format!("support-session-{key}");
    if let Some(prior) = ports
        .replay(actor, key, "support.session.request", &id, &wire)
        .await?
    {
        return Ok(prior);
    }
    let ticket = ports.ticket(actor, &input.ticket_id).await?;
    if ticket.company_id != actor.company_id {
        return Err(not_found());
    }
    if ticket.source_kind != "sales_order" {
        return Err(invalid(
            "SUPPORT_SESSION_SOURCE_REQUIRED",
            "Temporary customer context requires a linked customer order.",
        ));
    }
    if ticket.version != input.ticket_version {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SUPPORT_SESSION_SOURCE_CHANGED",
            "The linked customer case changed. Reload before requesting access.",
        ));
    }
    if ticket.owner_id != actor.user_id {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "SUPPORT_SESSION_TICKET_OWNER_REQUIRED",
            "Request access only for a case assigned to you.",
        ));
    }
    if !ticket.active() {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SUPPORT_SESSION_TICKET_CLOSED",
            "The linked customer case is no longer active.",
        ));
    }
    if !ports
        .target_eligible(actor, &input.requested_for, &ticket)
        .await?
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SUPPORT_SESSION_TARGET_INELIGIBLE",
            "Choose a currently eligible support colleague.",
        ));
    }
    let SessionPlan::Request(plan) = plan_request(actor, key, input, ports.now())? else {
        return Err(invalid(
            "SUPPORT_SESSION_INPUT_INVALID",
            "Provide a valid request.",
        ));
    };
    ports.commit_request(actor, key, &plan, &wire).await
}

pub async fn change_session<P: SessionPorts>(
    ports: &P,
    actor: &SessionActor,
    id: &str,
    input: &SessionAction,
    key: &str,
) -> Result<Value, BusinessError> {
    ports.authorize(actor, true).await?;
    let wire = serde_json::to_value(input)
        .map_err(|_| invalid("SUPPORT_SESSION_INPUT_INVALID", "Provide a valid action."))?;
    let operation = format!("support.session.{}", input.action());
    if let Some(prior) = ports.replay(actor, key, &operation, id, &wire).await? {
        return Ok(prior);
    }
    let snapshot = ports.session(actor, id).await?;
    let SessionPlan::Change(plan) = plan_change(actor, &snapshot, input, ports.now())? else {
        return Err(invalid(
            "SUPPORT_SESSION_INPUT_INVALID",
            "Provide a valid action.",
        ));
    };
    match input {
        SessionAction::Approve { .. } | SessionAction::Reject { .. } => {
            let ticket = ports.ticket(actor, &snapshot.ticket_id).await?;
            if !ticket.active() {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "SUPPORT_SESSION_TICKET_CLOSED",
                    "The linked customer case is no longer active.",
                ));
            }
            if !ports
                .target_eligible(actor, &snapshot.requested_for, &ticket)
                .await?
            {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "SUPPORT_SESSION_TARGET_INELIGIBLE",
                    "The requesting colleague is no longer eligible.",
                ));
            }
        }
        SessionAction::IssueCapability { .. } => {
            let ticket = ports.ticket(actor, &snapshot.ticket_id).await?;
            if !ticket.active() {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "SUPPORT_SESSION_TICKET_CLOSED",
                    "The linked customer case is no longer active.",
                ));
            }
            if !ports
                .target_eligible(actor, &snapshot.requested_for, &ticket)
                .await?
            {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "SUPPORT_SESSION_TARGET_INELIGIBLE",
                    "The requesting colleague is no longer eligible.",
                ));
            }
        }
        SessionAction::RevokeCapability {
            capability_id,
            expected_capability_version,
            ..
        } => {
            if !ports
                .capability_active(actor, id, capability_id, *expected_capability_version)
                .await?
            {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "SUPPORT_SESSION_CAPABILITY_CHANGED",
                    "Review the current capability state.",
                ));
            }
        }
        SessionAction::Revoke { .. } => {}
    }
    ports.commit_change(actor, key, &plan, &wire).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::session::{CapabilityCode, SessionPermissions, SessionStatus};
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
        ticket: Mutex<Option<SessionTicketSnapshot>>,
        session: Mutex<Option<SessionSnapshot>>,
        eligible: Mutex<bool>,
        capability: Mutex<bool>,
        committed: Mutex<Vec<String>>,
        denied: bool,
    }
    impl SessionPorts for Fake {
        fn now(&self) -> i64 {
            100
        }
        async fn authorize(&self, _: &SessionActor, _: bool) -> Result<(), BusinessError> {
            if self.denied {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "SUPPORT_SESSION_AUTHORITY_REQUIRED",
                    "Current Support session authority is required.",
                ));
            }
            Ok(())
        }
        async fn replay(
            &self,
            _: &SessionActor,
            key: &str,
            _: &str,
            _: &str,
            _: &Value,
        ) -> Result<Option<Value>, BusinessError> {
            Ok(self.replay.lock().unwrap().get(key).cloned())
        }
        async fn ticket(
            &self,
            _: &SessionActor,
            _: &str,
        ) -> Result<SessionTicketSnapshot, BusinessError> {
            self.ticket.lock().unwrap().clone().ok_or_else(not_found)
        }
        async fn session(
            &self,
            _: &SessionActor,
            _: &str,
        ) -> Result<SessionSnapshot, BusinessError> {
            self.session.lock().unwrap().clone().ok_or_else(not_found)
        }
        async fn target_eligible(
            &self,
            _: &SessionActor,
            _: &str,
            _: &SessionTicketSnapshot,
        ) -> Result<bool, BusinessError> {
            Ok(*self.eligible.lock().unwrap())
        }
        async fn capability_active(
            &self,
            _: &SessionActor,
            _: &str,
            _: &str,
            _: i64,
        ) -> Result<bool, BusinessError> {
            Ok(*self.capability.lock().unwrap())
        }
        async fn commit_request(
            &self,
            _: &SessionActor,
            key: &str,
            plan: &SessionRequestPlan,
            _: &Value,
        ) -> Result<Value, BusinessError> {
            self.committed.lock().unwrap().push(format!("request:{key}"));
            Ok(serde_json::json!({"id": plan.id, "status": "pending", "version": 1}))
        }
        async fn commit_change(
            &self,
            _: &SessionActor,
            key: &str,
            plan: &SessionChangePlan,
            _: &Value,
        ) -> Result<Value, BusinessError> {
            self.committed.lock().unwrap().push(format!("change:{key}"));
            Ok(serde_json::json!({"id": plan.id, "status": plan.status.as_str(), "version": plan.version}))
        }
    }
    fn actor() -> SessionActor {
        SessionActor {
            user_id: "agent".into(),
            company_id: "company".into(),
            branch_id: Some("branch".into()),
            department_id: None,
            permissions: SessionPermissions {
                can_read: true,
                can_work: true,
                can_manage: true,
            },
            same_person_ids: vec![],
        }
    }
    fn ticket() -> SessionTicketSnapshot {
        SessionTicketSnapshot {
            id: "ticket-1".into(),
            company_id: "company".into(),
            branch_id: Some("branch".into()),
            department_id: None,
            source_kind: "sales_order".into(),
            owner_id: "agent".into(),
            status: "open".into(),
            version: 1,
        }
    }
    fn request() -> SessionRequest {
        SessionRequest {
            ticket_id: "ticket-1".into(),
            ticket_version: 1,
            requested_for: "agent".into(),
            expires_at: 900,
            reason: "Temporary customer context".into(),
        }
    }
    fn snapshot() -> SessionSnapshot {
        SessionSnapshot {
            id: "support-session-1".into(),
            company_id: "company".into(),
            branch_id: Some("branch".into()),
            department_id: None,
            ticket_id: "ticket-1".into(),
            ticket_version: 1,
            requested_for: "agent".into(),
            status: SessionStatus::Pending,
            created_by: "agent".into(),
            reviewed_by: None,
            expires_at: 900,
            version: 1,
        }
    }

    #[test]
    fn requests_once_and_replays_the_original_result() {
        let fake = Fake::default();
        *fake.ticket.lock().unwrap() = Some(ticket());
        *fake.eligible.lock().unwrap() = true;
        let first = block_on(request_session(&fake, &actor(), &request(), "key")).unwrap();
        assert_eq!(first["id"], "support-session-key");
        assert_eq!(fake.committed.lock().unwrap().len(), 1);
        fake.replay.lock().unwrap().insert("key".into(), first.clone());
        let second = block_on(request_session(&fake, &actor(), &request(), "key")).unwrap();
        assert_eq!(second, first);
        assert_eq!(fake.committed.lock().unwrap().len(), 1);
    }

    #[test]
    fn rejects_a_general_ticket_a_foreign_owner_and_an_ineligible_target() {
        let fake = Fake::default();
        *fake.ticket.lock().unwrap() = Some(SessionTicketSnapshot {
            source_kind: "general".into(),
            ..ticket()
        });
        assert_eq!(
            block_on(request_session(&fake, &actor(), &request(), "key"))
                .unwrap_err()
                .code,
            "SUPPORT_SESSION_SOURCE_REQUIRED"
        );
        *fake.ticket.lock().unwrap() = Some(SessionTicketSnapshot {
            owner_id: "other".into(),
            ..ticket()
        });
        assert_eq!(
            block_on(request_session(&fake, &actor(), &request(), "key"))
                .unwrap_err()
                .code,
            "SUPPORT_SESSION_TICKET_OWNER_REQUIRED"
        );
        *fake.ticket.lock().unwrap() = Some(ticket());
        *fake.eligible.lock().unwrap() = false;
        assert_eq!(
            block_on(request_session(&fake, &actor(), &request(), "key"))
                .unwrap_err()
                .code,
            "SUPPORT_SESSION_TARGET_INELIGIBLE"
        );
    }

    #[test]
    fn approves_and_revokes_a_capability() {
        let fake = Fake::default();
        *fake.ticket.lock().unwrap() = Some(ticket());
        *fake.eligible.lock().unwrap() = true;
        *fake.capability.lock().unwrap() = true;
        let mut approved = snapshot();
        approved.status = SessionStatus::Approved;
        approved.reviewed_by = Some("manager".into());
        *fake.session.lock().unwrap() = Some(approved);
        let result = block_on(change_session(
            &fake,
            &actor(),
            "support-session-1",
            &SessionAction::IssueCapability {
                expected_version: 1,
                code: CapabilityCode::CustomerOrderContext,
                reason: "Issue the customer order context".into(),
            },
            "key",
        ))
        .unwrap();
        assert_eq!(result["status"], "approved");
        assert_eq!(result["version"], 2);
        assert_eq!(fake.committed.lock().unwrap().len(), 1);
    }

    #[test]
    fn authority_denial_and_stale_capability_fail_closed() {
        let fake = Fake {
            denied: true,
            ..Default::default()
        };
        assert_eq!(
            block_on(request_session(&fake, &actor(), &request(), "key"))
                .unwrap_err()
                .code,
            "SUPPORT_SESSION_AUTHORITY_REQUIRED"
        );
        let fake = Fake::default();
        *fake.session.lock().unwrap() = Some(snapshot());
        *fake.capability.lock().unwrap() = false;
        assert_eq!(
            block_on(change_session(
                &fake,
                &actor(),
                "support-session-1",
                &SessionAction::RevokeCapability {
                    expected_version: 1,
                    capability_id: "capability-1".into(),
                    expected_capability_version: 1,
                    reason: "Revoke the capability".into(),
                },
                "key",
            ))
            .unwrap_err()
            .code,
            "SUPPORT_SESSION_CAPABILITY_CHANGED"
        );
    }
}
