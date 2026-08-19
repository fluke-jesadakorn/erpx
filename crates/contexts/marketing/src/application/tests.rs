use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use erpx_cqrs::{
    CONTRACT_VERSION, CommandEnvelope, CommandHandler, FreshnessState, IdempotencyKey,
    ProjectionMetadata, ProjectionNotReady, ProjectionTarget, QueryConsistency, QueryEnvelope,
    QueryHandler, ReadYourWrites,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, AuthorityScope, CommandId, CorrelationId,
    ErrorCategory, EventId, FixedClock, QueryId,
};
use uuid::Uuid;

use super::*;
use crate::ports::{LeadCandidateList, MarketingCommit, ReadProjection, StoredCommit};

fn id(v: u128) -> Uuid {
    Uuid::from_u128(v)
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
fn actor() -> ActorAttribution {
    ActorAttribution {
        principal_ref: "marketing-test".into(),
        kind: ActorKind::HoldingUser,
        session_id: None,
        represented_principal_ref: None,
        support_capability_id: None,
    }
}
fn envelope(command: MarketingCommand, expected: Option<u64>) -> CommandEnvelope<MarketingCommand> {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id: CommandId::try_from_uuid(id(10)).unwrap(),
        idempotency_key: IdempotencyKey::try_new("marketing-stable-key").unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(11)).unwrap(),
        causation_id: None,
        actor: actor(),
        active_scope: scope(),
        audit: AuditContext {
            purpose: "test".into(),
            customer_visible: false,
            module: Some("marketing".into()),
        },
        expected_aggregate_version: expected,
        command,
    }
}
fn aggregate() -> LeadCandidate {
    LeadCandidate::create(
        LeadCandidateId::try_from_uuid(id(20)).unwrap(),
        scope(),
        CampaignId::try_from_uuid(id(21)).unwrap(),
        AudienceMemberId::try_from_uuid(id(22)).unwrap(),
        "organic".into(),
        None,
        at(),
    )
    .unwrap()
}

#[derive(Clone)]
struct Auth {
    allowed: bool,
}
#[async_trait]
impl AuthorizationPort for Auth {
    async fn authorize(&self, _: AuthorizationRequest<'_>) -> Result<(), PortError> {
        if self.allowed {
            Ok(())
        } else {
            Err(PortError::Denied)
        }
    }
}
#[derive(Clone)]
struct Entitlement;
#[async_trait]
impl EntitlementPort for Entitlement {
    async fn ensure_marketing_enabled(&self, _: &AuthorityScope, _: bool) -> Result<(), PortError> {
        Ok(())
    }
}
#[derive(Clone)]
struct Write {
    value: LeadCandidate,
}
#[async_trait]
impl WriteRepository for Write {
    async fn load_lead(
        &self,
        _: &AuthorityScope,
        _: LeadCandidateId,
    ) -> Result<Option<LeadCandidate>, PortError> {
        Ok(Some(self.value.clone()))
    }

    async fn load_budget(
        &self,
        _: &AuthorityScope,
        _: CampaignBudgetId,
    ) -> Result<Option<CampaignBudget>, PortError> {
        Ok(None)
    }
}
#[derive(Clone)]
struct Uow {
    commits: Arc<AtomicUsize>,
    request: Arc<Mutex<Option<MarketingCommit>>>,
}
#[async_trait]
impl CommandUnitOfWork for Uow {
    async fn find_receipt(&self, _: &CommandIdentity) -> Result<Option<StoredCommit>, PortError> {
        Ok(None)
    }
    async fn commit(&self, request: MarketingCommit) -> Result<StoredCommit, PortError> {
        self.commits.fetch_add(1, Ordering::SeqCst);
        *self.request.lock().unwrap() = Some(request.clone());
        Ok(StoredCommit {
            command_id: request.identity.command_id,
            correlation_id: request.correlation_id,
            aggregate: request.aggregate,
            committed_events: vec![],
            committed_at: at(),
            replayed: false,
        })
    }
}
fn handler(
    allowed: bool,
    commits: Arc<AtomicUsize>,
) -> MarketingCommandHandler<Write, Uow, Auth, Entitlement, FixedClock> {
    MarketingCommandHandler::new(
        Write { value: aggregate() },
        Uow {
            commits,
            request: Arc::new(Mutex::new(None)),
        },
        Auth { allowed },
        Entitlement,
        FixedClock::new(at()),
    )
}

#[tokio::test]
async fn mutation_requires_expected_version_before_atomic_commit() {
    let commits = Arc::new(AtomicUsize::new(0));
    let result = handler(true, commits.clone())
        .handle(envelope(
            MarketingCommand::Reject(RejectLeadCandidate {
                lead_id: aggregate().id(),
            }),
            None,
        ))
        .await
        .unwrap_err();
    assert_eq!(result.category(), ErrorCategory::Conflict);
    assert_eq!(commits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn authorization_denial_happens_before_commit() {
    let commits = Arc::new(AtomicUsize::new(0));
    let result = handler(false, commits.clone())
        .handle(envelope(
            MarketingCommand::Reject(RejectLeadCandidate {
                lead_id: aggregate().id(),
            }),
            Some(1),
        ))
        .await
        .unwrap_err();
    assert_eq!(result.category(), ErrorCategory::Denied);
    assert_eq!(commits.load(Ordering::SeqCst), 0);
}

struct NotReadyRead;
#[async_trait]
impl ReadRepository for NotReadyRead {
    async fn get_lead(
        &self,
        _: &AuthorityScope,
        _: LeadCandidateId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<LeadCandidateView>>, PortError> {
        let QueryConsistency::ReadYourWrites(required) = consistency else {
            panic!("expected RYW")
        };
        Err(PortError::NotReady(ProjectionNotReady {
            projection: required.projection.clone(),
            partition: required.partition.clone(),
            target: required.target,
            observed: None,
        }))
    }
    async fn list_leads(
        &self,
        _: &AuthorityScope,
        _: &LeadCandidateListFilter,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<LeadCandidateList>, PortError> {
        Ok(ReadProjection {
            value: LeadCandidateList {
                candidates: vec![],
                next_cursor: None,
            },
            metadata: ProjectionMetadata {
                checkpoint: None,
                freshness: FreshnessState::Current,
                lag_millis: None,
            },
        })
    }

    async fn get_budget(
        &self,
        _: &AuthorityScope,
        _: CampaignBudgetId,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<Option<CampaignBudgetView>>, PortError> {
        Ok(ReadProjection {
            value: None,
            metadata: ProjectionMetadata {
                checkpoint: None,
                freshness: FreshnessState::Current,
                lag_millis: None,
            },
        })
    }
}

#[tokio::test]
async fn read_your_writes_not_ready_is_never_served_from_write_state() {
    let target = ProjectionTarget {
        event_id: EventId::try_from_uuid(id(30)).unwrap(),
        global_sequence: 8,
    };
    let query = QueryEnvelope {
        contract_version: CONTRACT_VERSION,
        query_id: QueryId::try_from_uuid(id(31)).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(32)).unwrap(),
        actor: actor(),
        active_scope: scope(),
        audit: AuditContext {
            purpose: "test".into(),
            customer_visible: false,
            module: Some("marketing".into()),
        },
        consistency: QueryConsistency::ReadYourWrites(ReadYourWrites {
            projection: "marketing.leads".into(),
            partition: "holding-1".into(),
            target,
            timeout_millis: 100,
        }),
        query: MarketingQuery::Get(GetLeadCandidate {
            lead_id: aggregate().id(),
        }),
    };
    let result = MarketingQueryHandler::new(NotReadyRead, Auth { allowed: true }, Entitlement)
        .handle(query)
        .await
        .unwrap_err();
    assert_eq!(result.category(), ErrorCategory::NotReady);
}

#[test]
fn stable_identity_excludes_retry_metadata() {
    let fingerprint = CommandFingerprint::Qualify {
        lead_id: aggregate().id(),
        score_basis_points: 8_000,
    };
    let first = CommandIdentity {
        command_id: CommandId::try_from_uuid(id(40)).unwrap(),
        idempotency_key: IdempotencyKey::try_new("same").unwrap(),
        fingerprint: fingerprint.clone(),
    };
    let retry = CommandIdentity {
        command_id: first.command_id,
        idempotency_key: first.idempotency_key.clone(),
        fingerprint,
    };
    assert_eq!(first, retry);
}
