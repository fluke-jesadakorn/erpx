use std::{
    future::Future,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use erpx_cqrs::{
    CONTRACT_VERSION, CommandEnvelope, CommandHandler, EventPosition, FreshnessState,
    IdempotencyKey, ProjectionMetadata, ProjectionNotReady, ProjectionTarget, QueryConsistency,
    QueryEnvelope, QueryHandler,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    FixedClock, QueryId,
};
use uuid::Uuid;

use super::*;
use crate::{
    domain::{ArchivedBrandRevision, BrandDefinition, BrandProfile, BrandScope},
    ports::{
        AuthorizationRequest, BrandCommit, BrandHistory, BrandStoredCommit, PortError,
        ReadProjection, ResolvedBrandView,
    },
};

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn at() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
}

fn scope() -> TenantScope {
    TenantScope::Platform
}

fn actor() -> ActorAttribution {
    ActorAttribution {
        principal_ref: "brand-admin".into(),
        kind: ActorKind::PlatformUser,
        session_id: None,
        represented_principal_ref: None,
        support_capability_id: None,
    }
}

fn profile_id() -> BrandProfileId {
    BrandProfileId::try_from_uuid(id(10)).unwrap()
}

fn draft_profile() -> BrandProfile {
    BrandProfile::create_draft(
        profile_id(),
        BrandScope::Platform,
        BrandDefinition::neutral_platform(),
        None,
        "brand-admin".into(),
        at(),
    )
    .unwrap()
}

fn published_profile() -> BrandProfile {
    let mut value = draft_profile();
    value
        .publish_draft(
            None,
            "brand-admin".into(),
            Utc.with_ymd_and_hms(2026, 8, 5, 10, 0, 0).single().unwrap(),
        )
        .unwrap();
    value.take_uncommitted_events();
    value
}

fn command_envelope<C>(sequence: u128, expected: Option<u64>, command: C) -> CommandEnvelope<C> {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id: CommandId::try_from_uuid(id(100 + sequence)).unwrap(),
        idempotency_key: IdempotencyKey::try_new(format!("brand-test-{sequence}")).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(200 + sequence)).unwrap(),
        causation_id: None,
        actor: actor(),
        tenant_scope: scope(),
        audit: AuditContext {
            purpose: "brand.test".into(),
            customer_visible: false,
            module: Some("brand".into()),
        },
        expected_aggregate_version: expected,
        command,
    }
}

fn query_envelope<Q>(sequence: u128, consistency: QueryConsistency, query: Q) -> QueryEnvelope<Q> {
    QueryEnvelope {
        contract_version: CONTRACT_VERSION,
        query_id: QueryId::try_from_uuid(id(300 + sequence)).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(400 + sequence)).unwrap(),
        actor: actor(),
        tenant_scope: scope(),
        audit: AuditContext {
            purpose: "brand.test.query".into(),
            customer_visible: false,
            module: Some("brand".into()),
        },
        consistency,
        query,
    }
}

#[derive(Clone)]
struct FakeAuthorization {
    allow: bool,
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl AuthorizationPort for FakeAuthorization {
    async fn authorize(&self, _request: AuthorizationRequest<'_>) -> Result<(), PortError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.allow {
            Ok(())
        } else {
            Err(PortError::Denied)
        }
    }
}

#[derive(Clone)]
struct FakeEntitlement {
    allow: bool,
}

#[async_trait]
impl EntitlementPort for FakeEntitlement {
    async fn ensure_brand_enabled(
        &self,
        _scope: &TenantScope,
        _write: bool,
    ) -> Result<(), PortError> {
        if self.allow {
            Ok(())
        } else {
            Err(PortError::EntitlementDenied)
        }
    }
}

#[derive(Clone, Default)]
struct FakeParent;

#[async_trait]
impl ParentAuthorityPort for FakeParent {
    async fn current_parent(
        &self,
        _child: &BrandScope,
    ) -> Result<Option<ParentBrandWitness>, PortError> {
        Ok(None)
    }
}

#[derive(Clone)]
struct FakeWriteRepository {
    aggregate: Arc<Mutex<Option<BrandProfile>>>,
    archived: Arc<Mutex<Option<ArchivedBrandRevision>>>,
    loads: Arc<AtomicUsize>,
    archive_loads: Arc<AtomicUsize>,
}

#[async_trait]
impl WriteRepository for FakeWriteRepository {
    async fn load(
        &self,
        _authority_scope: &TenantScope,
        _profile_id: BrandProfileId,
    ) -> Result<Option<BrandProfile>, PortError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.aggregate.lock().unwrap().clone())
    }

    async fn load_archived_revision(
        &self,
        _authority_scope: &TenantScope,
        _profile_id: BrandProfileId,
        _revision: u64,
    ) -> Result<Option<ArchivedBrandRevision>, PortError> {
        self.archive_loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.archived.lock().unwrap().clone())
    }
}

#[derive(Clone, Default)]
struct FakeUnitOfWork {
    receipts: Arc<Mutex<Vec<(CommandIdentity, BrandStoredCommit)>>>,
    commits: Arc<Mutex<Vec<BrandCommit>>>,
}

#[async_trait]
impl CommandUnitOfWork for FakeUnitOfWork {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<BrandStoredCommit>, PortError> {
        for (stored_identity, receipt) in self.receipts.lock().unwrap().iter() {
            let reused = stored_identity.command_id == identity.command_id
                || stored_identity.idempotency_key == identity.idempotency_key;
            if reused {
                return if stored_identity == identity {
                    Ok(Some(receipt.clone()))
                } else {
                    Err(PortError::IdempotencyConflict)
                };
            }
        }
        Ok(None)
    }

    async fn commit(&self, request: BrandCommit) -> Result<BrandStoredCommit, PortError> {
        let stored = BrandStoredCommit {
            command_id: request.identity.command_id,
            correlation_id: request.correlation_id,
            aggregate: request.aggregate.clone(),
            committed_events: vec![EventPosition {
                event_id: EventId::try_from_uuid(id(900)).unwrap(),
                global_sequence: 1,
            }],
            committed_at: at(),
            replayed: false,
        };
        self.receipts
            .lock()
            .unwrap()
            .push((request.identity.clone(), stored.clone()));
        self.commits.lock().unwrap().push(request);
        Ok(stored)
    }
}

#[derive(Clone)]
struct FakeReadRepository {
    draft: Option<BrandProfileView>,
    not_ready: Option<ProjectionNotReady>,
    reads: Arc<AtomicUsize>,
}

#[async_trait]
impl ReadRepository for FakeReadRepository {
    async fn get_draft(
        &self,
        _authority_scope: &TenantScope,
        _scope: &BrandScope,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<BrandProfileView>>, PortError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        if let Some(not_ready) = &self.not_ready {
            return Err(PortError::NotReady(not_ready.clone()));
        }
        Ok(ReadProjection {
            value: self.draft.clone(),
            metadata: projection_metadata(),
        })
    }

    async fn get_resolved(
        &self,
        _authority_scope: &TenantScope,
        _scope: &BrandScope,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<ResolvedBrandView>>, PortError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(ReadProjection {
            value: None,
            metadata: projection_metadata(),
        })
    }

    async fn list_history(
        &self,
        _authority_scope: &TenantScope,
        _scope: &BrandScope,
        _page_size: u16,
        _cursor: Option<&str>,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<BrandHistory>, PortError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(ReadProjection {
            value: BrandHistory {
                entries: Vec::new(),
                next_cursor: None,
            },
            metadata: projection_metadata(),
        })
    }
}

fn projection_metadata() -> ProjectionMetadata {
    ProjectionMetadata {
        checkpoint: None,
        freshness: FreshnessState::Current,
        lag_millis: Some(0),
    }
}

fn handler(
    aggregate: Option<BrandProfile>,
    archived: Option<ArchivedBrandRevision>,
    uow: FakeUnitOfWork,
    allow: bool,
) -> BrandCommandHandler<
    FakeWriteRepository,
    FakeUnitOfWork,
    FakeAuthorization,
    FakeEntitlement,
    FakeParent,
    FixedClock,
> {
    BrandCommandHandler::new(
        FakeWriteRepository {
            aggregate: Arc::new(Mutex::new(aggregate)),
            archived: Arc::new(Mutex::new(archived)),
            loads: Arc::new(AtomicUsize::new(0)),
            archive_loads: Arc::new(AtomicUsize::new(0)),
        },
        uow,
        FakeAuthorization {
            allow,
            calls: Arc::new(AtomicUsize::new(0)),
        },
        FakeEntitlement { allow: true },
        FakeParent,
        FixedClock::new(Utc.with_ymd_and_hms(2026, 8, 5, 11, 0, 0).single().unwrap()),
    )
}

#[test]
fn create_draft_commits_scope_guard_without_publishing_integration_event() {
    let uow = FakeUnitOfWork::default();
    let receipt = block_on(
        handler(None, None, uow.clone(), true).handle(command_envelope(
            1,
            None,
            CreateBrandDraft {
                profile_id: profile_id(),
                scope: BrandScope::Platform,
                definition: BrandDefinition::neutral_platform(),
                reason: "initial neutral brand".into(),
            },
        )),
    )
    .unwrap();
    assert_eq!(receipt.aggregate.version, 1);
    let commits = uow.commits.lock().unwrap();
    assert!(commits[0].ensure_scope_vacant);
    assert!(commits[0].integration_events.is_empty());
    assert_eq!(commits[0].reason, "initial neutral brand");
}

#[test]
fn retry_returns_stored_receipt_without_duplicate_commit() {
    let uow = FakeUnitOfWork::default();
    let envelope = command_envelope(
        2,
        None,
        CreateBrandDraft {
            profile_id: profile_id(),
            scope: BrandScope::Platform,
            definition: BrandDefinition::neutral_platform(),
            reason: "initial neutral brand".into(),
        },
    );
    let service = handler(None, None, uow.clone(), true);
    block_on(service.handle(envelope.clone())).unwrap();
    let replay = block_on(service.handle(envelope)).unwrap();
    assert!(replay.replayed);
    assert_eq!(uow.commits.lock().unwrap().len(), 1);
}

#[test]
fn mutation_requires_expected_aggregate_version() {
    let error = block_on(
        handler(Some(draft_profile()), None, FakeUnitOfWork::default(), true).handle(
            command_envelope(
                3,
                None,
                PublishBrandDraft {
                    profile_id: profile_id(),
                    reason: "publish".into(),
                },
            ),
        ),
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Invalid);
}

#[test]
fn authorization_denial_stops_after_authoritative_scope_resolution() {
    let write = FakeWriteRepository {
        aggregate: Arc::new(Mutex::new(Some(draft_profile()))),
        archived: Arc::new(Mutex::new(None)),
        loads: Arc::new(AtomicUsize::new(0)),
        archive_loads: Arc::new(AtomicUsize::new(0)),
    };
    let loads = write.loads.clone();
    let service = BrandCommandHandler::new(
        write,
        FakeUnitOfWork::default(),
        FakeAuthorization {
            allow: false,
            calls: Arc::new(AtomicUsize::new(0)),
        },
        FakeEntitlement { allow: true },
        FakeParent,
        FixedClock::new(at()),
    );
    let error = block_on(service.handle(command_envelope(
        4,
        Some(1),
        PublishBrandDraft {
            profile_id: profile_id(),
            reason: "publish".into(),
        },
    )))
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Denied);
    // Existing commands resolve scope from the authoritative aggregate before
    // authorization, but no commit or archived revision read can occur.
    assert_eq!(loads.load(Ordering::SeqCst), 1);
}

#[test]
fn publish_emits_owner_contract_event() {
    let uow = FakeUnitOfWork::default();
    let receipt = block_on(
        handler(Some(draft_profile()), None, uow.clone(), true).handle(command_envelope(
            5,
            Some(1),
            PublishBrandDraft {
                profile_id: profile_id(),
                reason: "approved for publication".into(),
            },
        )),
    )
    .unwrap();
    assert_eq!(receipt.aggregate.version, 2);
    assert_eq!(uow.commits.lock().unwrap()[0].integration_events.len(), 1);
}

#[test]
fn rollback_loads_owned_archive_and_creates_new_revision() {
    let aggregate = published_profile();
    let source = ArchivedBrandRevision {
        profile_id: aggregate.id(),
        scope: BrandScope::Platform,
        source_revision: 1,
        definition: BrandDefinition::neutral_platform(),
    };
    let receipt = block_on(
        handler(
            Some(aggregate),
            Some(source),
            FakeUnitOfWork::default(),
            true,
        )
        .handle(command_envelope(
            6,
            Some(2),
            RollbackBrand {
                profile_id: profile_id(),
                source_revision: 1,
                reason: "restore prior approved identity".into(),
            },
        )),
    )
    .unwrap();
    assert_eq!(receipt.response.profile.published.unwrap().revision(), 2);
}

#[test]
fn query_uses_read_projection_and_preserves_not_ready() {
    let reads = Arc::new(AtomicUsize::new(0));
    let service = BrandQueryHandler::new(
        FakeReadRepository {
            draft: Some(BrandProfileView::from(&draft_profile())),
            not_ready: None,
            reads: reads.clone(),
        },
        FakeAuthorization {
            allow: true,
            calls: Arc::new(AtomicUsize::new(0)),
        },
        FakeEntitlement { allow: true },
    );
    let result = block_on(service.handle(query_envelope(
        1,
        QueryConsistency::Eventual,
        GetBrandDraft {
            scope: BrandScope::Platform,
        },
    )))
    .unwrap();
    assert!(result.value.profile.is_some());
    assert_eq!(reads.load(Ordering::SeqCst), 1);

    let target = ProjectionTarget {
        event_id: EventId::try_from_uuid(id(999)).unwrap(),
        global_sequence: 42,
    };
    let not_ready_service = BrandQueryHandler::new(
        FakeReadRepository {
            draft: None,
            not_ready: Some(ProjectionNotReady {
                projection: "brand_profiles".into(),
                partition: "control".into(),
                target,
                observed: None,
            }),
            reads: Arc::new(AtomicUsize::new(0)),
        },
        FakeAuthorization {
            allow: true,
            calls: Arc::new(AtomicUsize::new(0)),
        },
        FakeEntitlement { allow: true },
    );
    let error = block_on(not_ready_service.handle(query_envelope(
        2,
        QueryConsistency::Eventual,
        GetBrandDraft {
            scope: BrandScope::Platform,
        },
    )))
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::NotReady);
}

#[test]
fn history_rejects_unbounded_page_before_read() {
    let reads = Arc::new(AtomicUsize::new(0));
    let service = BrandQueryHandler::new(
        FakeReadRepository {
            draft: None,
            not_ready: None,
            reads: reads.clone(),
        },
        FakeAuthorization {
            allow: true,
            calls: Arc::new(AtomicUsize::new(0)),
        },
        FakeEntitlement { allow: true },
    );
    let error = block_on(service.handle(query_envelope(
        3,
        QueryConsistency::Eventual,
        ListBrandHistory {
            scope: BrandScope::Platform,
            page_size: 101,
            cursor: None,
        },
    )))
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Invalid);
    assert_eq!(reads.load(Ordering::SeqCst), 0);
}

struct NoopWake;

impl Wake for NoopWake {
    fn wake(self: Arc<Self>) {}
}

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(NoopWake));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
