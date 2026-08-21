use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use erpx_cqrs::{
    CONTRACT_VERSION, CommandEnvelope, CommandHandler, IdempotencyKey, ProjectionNotReady,
    ProjectionTarget, QueryConsistency, QueryEnvelope, QueryHandler, ReadYourWrites,
};
use erpx_integration_contracts::{DisplayName, ReferenceCode, customer, organization};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, AuthorityScope, CommandId, CorrelationId,
    ErrorCategory, EventId, FixedClock, QueryId, SessionId as KernelSessionId,
};
use uuid::Uuid;

use super::*;
use crate::{
    domain::{CapabilityId, IncidentId, SessionId, SupportCapability, SupportTicketId},
    ports::{
        AuthorizationRequest, CommandIdentity, CommandUnitOfWork, PortError, StoredCommit,
        SupportCommit, TicketReadAudience,
    },
};

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}
fn at() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
}
fn scope() -> AuthorityScope {
    AuthorityScope::Company {
        holding_id: id(1),
        company_id: id(2),
    }
}
fn customer_fact() -> customer::CustomerProfileFact {
    customer::CustomerProfileFact {
        customer: customer::CustomerRef::try_from_uuid(id(10)).unwrap(),
        account: customer::CustomerAccountRef::try_from_uuid(id(11)).unwrap(),
        customer_code: ReferenceCode::try_new("CUS-10").unwrap(),
        display_name: DisplayName::try_new("Acme").unwrap(),
        owning_company: organization::CompanyRef::try_from_uuid(id(2)).unwrap(),
        status: customer::CustomerStatus::Active,
    }
}
fn principal(value: u128) -> identity_access::PrincipalRef {
    identity_access::PrincipalRef::try_from_uuid(id(value)).unwrap()
}
fn requested_session() -> SupportSession {
    let opened_at = at() - chrono::Duration::minutes(2);
    let mut ticket = SupportTicket::open(
        SupportTicketId::try_from_uuid(id(20)).unwrap(),
        scope(),
        customer_fact(),
        "SUP-20".into(),
        "identity".into(),
        "high".into(),
        "cannot authenticate".into(),
        opened_at,
    )
    .unwrap();
    ticket
        .transition(
            TicketStatus::Triaged,
            opened_at + chrono::Duration::seconds(30),
        )
        .unwrap();
    ticket
        .transition(
            TicketStatus::AccessRequested,
            opened_at + chrono::Duration::minutes(1),
        )
        .unwrap();
    SupportSession::request(
        SessionId::try_from_uuid(id(30)).unwrap(),
        &ticket,
        principal(32),
        None,
        SupportMode::DiagnosticRead,
        vec!["holding.read".into()],
        scope(),
        "support".into(),
        "customer requested diagnostics".into(),
        chrono::Duration::minutes(30),
        opened_at + chrono::Duration::minutes(1),
    )
    .unwrap()
}
fn actor(value: &str) -> ActorAttribution {
    ActorAttribution {
        principal_ref: value.into(),
        kind: ActorKind::PlatformUser,
        session_id: None,
        represented_principal_ref: None,
        support_capability_id: None,
    }
}
fn approve_envelope(kind: ActorKind) -> CommandEnvelope<SupportCommand> {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id: CommandId::try_from_uuid(id(510)).unwrap(),
        idempotency_key: IdempotencyKey::try_new("support-approve-session-stable-key").unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(511)).unwrap(),
        causation_id: None,
        actor: ActorAttribution {
            principal_ref: principal(33).as_uuid().to_string(),
            kind,
            session_id: Some(KernelSessionId::try_from_uuid(id(512)).unwrap()),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        active_scope: scope(),
        audit: AuditContext {
            purpose: "customer approves Support session".into(),
            customer_visible: true,
            module: Some("support".into()),
        },
        expected_aggregate_version: Some(1),
        command: SupportCommand::ApproveSession(ApproveSupportSession {
            session_id: SessionId::try_from_uuid(id(30)).unwrap(),
        }),
    }
}
fn envelope(principal: &str, correlation: u128) -> CommandEnvelope<SupportCommand> {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id: CommandId::try_from_uuid(id(100)).unwrap(),
        idempotency_key: IdempotencyKey::try_new("support-open-ticket-stable-key").unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(correlation)).unwrap(),
        causation_id: None,
        actor: actor(principal),
        active_scope: scope(),
        audit: AuditContext {
            purpose: "support.test".into(),
            customer_visible: false,
            module: Some("support".into()),
        },
        expected_aggregate_version: None,
        command: SupportCommand::OpenTicket(OpenSupportTicket {
            ticket_id: SupportTicketId::try_from_uuid(id(20)).unwrap(),
            customer: customer_fact(),
            case_code: "SUP-20".into(),
            category: "identity".into(),
            severity: "high".into(),
            description: "cannot authenticate".into(),
        }),
    }
}

#[derive(Clone, Default)]
struct FakeWrite;
#[async_trait]
impl WriteRepository for FakeWrite {
    async fn load_ticket(
        &self,
        _: &AuthorityScope,
        _: SupportTicketId,
    ) -> Result<Option<SupportTicket>, PortError> {
        Ok(None)
    }
    async fn load_incident(
        &self,
        _: &AuthorityScope,
        _: IncidentId,
    ) -> Result<Option<Incident>, PortError> {
        Ok(None)
    }
    async fn load_session(
        &self,
        _: &AuthorityScope,
        _: SessionId,
    ) -> Result<Option<SupportSession>, PortError> {
        Ok(None)
    }
    async fn load_capability(
        &self,
        _: &AuthorityScope,
        _: CapabilityId,
    ) -> Result<Option<SupportCapability>, PortError> {
        Ok(None)
    }
}

#[derive(Clone)]
struct SessionWrite {
    session: SupportSession,
    loads: Arc<AtomicUsize>,
}
#[async_trait]
impl WriteRepository for SessionWrite {
    async fn load_ticket(
        &self,
        _: &AuthorityScope,
        _: SupportTicketId,
    ) -> Result<Option<SupportTicket>, PortError> {
        Ok(None)
    }
    async fn load_incident(
        &self,
        _: &AuthorityScope,
        _: IncidentId,
    ) -> Result<Option<Incident>, PortError> {
        Ok(None)
    }
    async fn load_session(
        &self,
        _: &AuthorityScope,
        _: SessionId,
    ) -> Result<Option<SupportSession>, PortError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(Some(self.session.clone()))
    }
    async fn load_capability(
        &self,
        _: &AuthorityScope,
        _: CapabilityId,
    ) -> Result<Option<SupportCapability>, PortError> {
        Ok(None)
    }
}

#[derive(Clone)]
struct Allow;
#[async_trait]
impl AuthorizationPort for Allow {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError> {
        assert!(request.authority_scope.contains(request.target_scope));
        Ok(())
    }
}
#[async_trait]
impl EntitlementPort for Allow {
    async fn ensure_support_enabled(&self, _: &AuthorityScope, _: bool) -> Result<(), PortError> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct FakeUow {
    commits: Arc<Mutex<Vec<SupportCommit>>>,
    stored: Arc<Mutex<Option<(CommandIdentity, StoredCommit)>>>,
    receipt_reads: Arc<AtomicUsize>,
}
#[async_trait]
impl CommandUnitOfWork for FakeUow {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        self.receipt_reads.fetch_add(1, Ordering::SeqCst);
        Ok(self
            .stored
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|(found, receipt)| (found == identity).then(|| receipt.clone())))
    }
    async fn commit(&self, request: SupportCommit) -> Result<StoredCommit, PortError> {
        let stored = StoredCommit {
            command_id: request.identity.command_id,
            correlation_id: request.correlation_id,
            aggregate: request.aggregate.clone(),
            committed_events: vec![erpx_cqrs::EventPosition {
                event_id: EventId::try_from_uuid(id(200)).unwrap(),
                global_sequence: 1,
            }],
            committed_at: at(),
            replayed: false,
        };
        *self.stored.lock().unwrap() = Some((request.identity.clone(), stored.clone()));
        self.commits.lock().unwrap().push(request);
        Ok(stored)
    }
}

#[tokio::test]
async fn command_commit_is_atomic_shape_and_stable_retry_does_not_commit_twice() {
    let uow = FakeUow::default();
    let handler =
        SupportCommandHandler::new(FakeWrite, uow.clone(), Allow, Allow, FixedClock::new(at()));
    let first = handler.handle(envelope("agent-one", 101)).await.unwrap();
    assert!(!first.replayed);
    assert_eq!(uow.commits.lock().unwrap().len(), 1);
    {
        let commits = uow.commits.lock().unwrap();
        let commit = &commits[0];
        assert_eq!(commit.aggregate.version(), 1);
        assert!(commit.expected_version.is_none());
        assert!(commit.dependency.is_none());
        assert!(commit.human_decision.is_none());
        assert_eq!(commit.integration_events.len(), 1);
        assert_eq!(commit.audit.purpose, "support.test");
    }
    let retry = handler.handle(envelope("agent-two", 102)).await.unwrap();
    assert!(retry.replayed);
    assert_eq!(retry.correlation_id, first.correlation_id);
    assert_eq!(uow.commits.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn non_humans_are_denied_before_support_repository_access() {
    for kind in [
        ActorKind::Agent,
        ActorKind::Service,
        ActorKind::SystemWorker,
    ] {
        let loads = Arc::new(AtomicUsize::new(0));
        let write = SessionWrite {
            session: requested_session(),
            loads: loads.clone(),
        };
        let uow = FakeUow::default();
        let handler =
            SupportCommandHandler::new(write, uow.clone(), Allow, Allow, FixedClock::new(at()));
        let error = handler.handle(approve_envelope(kind)).await.unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Denied);
        assert_eq!(loads.load(Ordering::SeqCst), 0);
        assert_eq!(uow.receipt_reads.load(Ordering::SeqCst), 0);
        assert!(uow.commits.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn human_kind_without_authenticated_session_is_denied_before_repository_access() {
    let loads = Arc::new(AtomicUsize::new(0));
    let write = SessionWrite {
        session: requested_session(),
        loads: loads.clone(),
    };
    let uow = FakeUow::default();
    let handler =
        SupportCommandHandler::new(write, uow.clone(), Allow, Allow, FixedClock::new(at()));
    let mut command = approve_envelope(ActorKind::HoldingUser);
    command.actor.session_id = None;
    let error = handler.handle(command).await.unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Denied);
    assert_eq!(loads.load(Ordering::SeqCst), 0);
    assert_eq!(uow.receipt_reads.load(Ordering::SeqCst), 0);
    assert!(uow.commits.lock().unwrap().is_empty());
}

#[tokio::test]
async fn approve_session_commits_exact_authenticated_human_decision() {
    let loads = Arc::new(AtomicUsize::new(0));
    let write = SessionWrite {
        session: requested_session(),
        loads: loads.clone(),
    };
    let uow = FakeUow::default();
    let handler =
        SupportCommandHandler::new(write, uow.clone(), Allow, Allow, FixedClock::new(at()));
    handler
        .handle(approve_envelope(ActorKind::HoldingUser))
        .await
        .unwrap();
    assert_eq!(loads.load(Ordering::SeqCst), 1);
    let commits = uow.commits.lock().unwrap();
    let commit = commits.first().expect("one Support approval commit");
    let decision = commit
        .human_decision
        .as_ref()
        .expect("approval carries human decision evidence");
    let session_id = SessionId::try_from_uuid(id(30)).unwrap();
    assert_eq!(decision.actor(), &commit.actor);
    assert_eq!(decision.actor().kind, ActorKind::HoldingUser);
    assert_eq!(
        decision.actor().session_id.map(|session| session.as_uuid()),
        Some(id(512))
    );
    assert_eq!(decision.authority_scope(), &scope());
    assert_eq!(
        decision.decision_ref(),
        support_session_approval_decision_ref(session_id)
    );
    assert_eq!(decision.decided_at(), at());
    let SupportAggregate::Session(session) = &commit.aggregate else {
        panic!("approval must commit a Support session");
    };
    assert_eq!(session.approved_by(), Some(decision.principal_ref()));
    assert_eq!(session.approved_at(), Some(decision.decided_at()));
}

#[test]
fn fingerprint_is_stable_across_actor_correlation_and_retry_time() {
    let first = envelope("agent-one", 101);
    let second = envelope("agent-two", 102);
    let first_identity = CommandIdentity {
        command_id: first.command_id,
        idempotency_key: first.idempotency_key,
        fingerprint: first.command.fingerprint(),
    };
    let second_identity = CommandIdentity {
        command_id: second.command_id,
        idempotency_key: second.idempotency_key,
        fingerprint: second.command.fingerprint(),
    };
    assert_eq!(first_identity, second_identity);
    assert_eq!(QueryConsistency::Eventual, QueryConsistency::default());
}

#[test]
fn capability_fingerprint_excludes_rotating_authority_evidence_and_secret_hash() {
    let base = IssueSupportCapability {
        capability_id: CapabilityId::try_from_uuid(id(510)).unwrap(),
        session_id: SessionId::try_from_uuid(id(511)).unwrap(),
        evidence: CapabilityEvidence {
            device_ref: "device-511".into(),
            mfa_verified_at: Some(at()),
            segregation_of_duties_passed: true,
            authority_scope: scope(),
        },
        secret_hash_sha256: [7; 32],
    };
    let mut retried = base.clone();
    retried.evidence.mfa_verified_at = Some(at() + chrono::Duration::minutes(1));
    retried.secret_hash_sha256 = [8; 32];
    assert_eq!(
        SupportCommand::IssueCapability(base).fingerprint(),
        SupportCommand::IssueCapability(retried).fingerprint()
    );
}

#[test]
fn closure_and_session_revocation_emit_atomic_termination_work() {
    let ticket_id = SupportTicketId::try_from_uuid(id(20)).unwrap();
    assert_eq!(
        authority_effects(
            &SupportCommand::TransitionTicket(TransitionSupportTicket {
                ticket_id,
                next: TicketStatus::Closed,
            }),
            at(),
        ),
        vec![SupportAuthorityEffect::TerminateTicketAccess {
            ticket_id,
            effective_at: at(),
        }]
    );

    let session_id = SessionId::try_from_uuid(id(30)).unwrap();
    assert_eq!(
        authority_effects(
            &SupportCommand::RevokeSession(RevokeSupportSession {
                session_id,
                reason: "customer requested".into(),
            }),
            at(),
        ),
        vec![SupportAuthorityEffect::TerminateSessionCapabilities {
            session_id,
            effective_at: at(),
        }]
    );
}

#[derive(Clone)]
struct NotReadyRead;

#[allow(clippy::result_large_err)]
fn not_ready<T>() -> Result<T, PortError> {
    Err(PortError::NotReady(ProjectionNotReady {
        projection: "support_tickets".into(),
        partition: "holding-1".into(),
        target: ProjectionTarget {
            event_id: EventId::try_from_uuid(id(300)).unwrap(),
            global_sequence: 42,
        },
        observed: None,
    }))
}

#[async_trait]
impl ReadRepository for NotReadyRead {
    async fn get_ticket(
        &self,
        _: &AuthorityScope,
        _: SupportTicketId,
        _: TicketReadAudience,
        _: &QueryConsistency,
    ) -> Result<crate::ports::ReadProjection<Option<SupportTicketView>>, PortError> {
        not_ready()
    }
    async fn list_tickets(
        &self,
        _: &AuthorityScope,
        _: &SupportTicketListFilter,
        _: &QueryConsistency,
    ) -> Result<crate::ports::ReadProjection<SupportTicketList>, PortError> {
        not_ready()
    }
    async fn get_incident(
        &self,
        _: &AuthorityScope,
        _: IncidentId,
        _: &QueryConsistency,
    ) -> Result<crate::ports::ReadProjection<Option<IncidentView>>, PortError> {
        not_ready()
    }
    async fn list_incidents(
        &self,
        _: &AuthorityScope,
        _: &IncidentListFilter,
        _: &QueryConsistency,
    ) -> Result<crate::ports::ReadProjection<IncidentList>, PortError> {
        not_ready()
    }
    async fn get_session(
        &self,
        _: &AuthorityScope,
        _: SessionId,
        _: &QueryConsistency,
    ) -> Result<crate::ports::ReadProjection<Option<SupportSessionView>>, PortError> {
        not_ready()
    }
    async fn get_capability(
        &self,
        _: &AuthorityScope,
        _: CapabilityId,
        _: &QueryConsistency,
    ) -> Result<crate::ports::ReadProjection<Option<SupportCapabilityView>>, PortError> {
        not_ready()
    }
}

#[tokio::test]
async fn read_your_writes_not_ready_is_returned_without_write_store_fallback() {
    let consistency = QueryConsistency::ReadYourWrites(ReadYourWrites {
        projection: "support_tickets".into(),
        partition: "holding-1".into(),
        target: ProjectionTarget {
            event_id: EventId::try_from_uuid(id(300)).unwrap(),
            global_sequence: 42,
        },
        timeout_millis: 50,
    });
    let query = QueryEnvelope {
        contract_version: CONTRACT_VERSION,
        query_id: QueryId::try_from_uuid(id(400)).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(401)).unwrap(),
        actor: actor("customer-user"),
        active_scope: scope(),
        audit: AuditContext {
            purpose: "support.read".into(),
            customer_visible: true,
            module: Some("support".into()),
        },
        consistency,
        query: SupportQuery::GetTicket(GetSupportTicket {
            ticket_id: SupportTicketId::try_from_uuid(id(20)).unwrap(),
            audience: TicketReadAudience::Customer,
        }),
    };
    let handler = SupportQueryHandler::new(NotReadyRead, Allow, Allow);
    let error = handler.handle(query).await.unwrap_err();
    assert!(matches!(error, CqrsError::NotReady(value) if value.target.global_sequence == 42));
}
