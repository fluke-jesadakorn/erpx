use std::{
    future::Future,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

use async_trait::async_trait;
use chrono::{Duration, TimeZone, Utc};
use erpx_cqrs::{
    CONTRACT_VERSION, CommandEnvelope, CommandHandler, EventPosition, FreshnessState,
    IdempotencyKey, ProjectionMetadata, QueryConsistency, QueryEnvelope, QueryHandler,
};
use erpx_integration_contracts::CurrencyCode;
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    FixedClock, QueryId, TenantScope,
};

use super::*;
use crate::{
    domain::{BillingSignal, CatalogPlanStatus, SubscriptionStatus},
    ports::{
        BillingEventEvidence, CommercialCommit, EntitlementChangeEvidence, Page, PageFilter,
        ReadProjection, VerifiedBillingEvent, VerifiedEntitlementChange,
    },
};

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}
fn at() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
}
fn scope() -> TenantScope {
    TenantScope::Holding { holding_id: id(1) }
}
fn currency() -> CurrencyCode {
    CurrencyCode::try_new("THB").unwrap()
}

fn actor() -> ActorAttribution {
    ActorAttribution {
        principal_ref: "commercial-test-actor".into(),
        kind: ActorKind::TenantUser,
        session_id: None,
        represented_principal_ref: None,
        support_capability_id: None,
    }
}

fn command_envelope<C>(sequence: u128, expected: Option<u64>, command: C) -> CommandEnvelope<C> {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id: CommandId::try_from_uuid(id(100 + sequence)).unwrap(),
        idempotency_key: IdempotencyKey::try_new(format!("commercial-test-{sequence}")).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(200 + sequence)).unwrap(),
        causation_id: None,
        actor: actor(),
        tenant_scope: scope(),
        audit: AuditContext {
            purpose: "commercial.test".into(),
            customer_visible: false,
            module: Some("commercial".into()),
        },
        expected_aggregate_version: expected,
        command,
    }
}

fn query_envelope<Q>(sequence: u128, query: Q) -> QueryEnvelope<Q> {
    QueryEnvelope {
        contract_version: CONTRACT_VERSION,
        query_id: QueryId::try_from_uuid(id(300 + sequence)).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(400 + sequence)).unwrap(),
        actor: actor(),
        tenant_scope: scope(),
        audit: AuditContext {
            purpose: "commercial.test.query".into(),
            customer_visible: false,
            module: Some("commercial".into()),
        },
        consistency: QueryConsistency::Eventual,
        query,
    }
}

fn subscription() -> Subscription {
    let mut value = Subscription::create(
        SubscriptionId::try_from_uuid(id(10)).unwrap(),
        scope(),
        id(11),
        CatalogPlanId::try_from_uuid(id(12)).unwrap(),
        50_000,
        currency(),
        true,
        at(),
    )
    .unwrap();
    value.take_uncommitted_events();
    value
}

#[derive(Clone)]
struct FakeAuthorization {
    allow: bool,
}
#[async_trait]
impl AuthorizationPort for FakeAuthorization {
    async fn authorize(&self, _request: AuthorizationRequest<'_>) -> Result<(), PortError> {
        if self.allow {
            Ok(())
        } else {
            Err(PortError::Denied)
        }
    }
}

#[derive(Clone)]
struct FakeScopeAuthority {
    allow: bool,
    calls: Arc<AtomicUsize>,
}
#[async_trait]
impl ScopeAuthorityPort for FakeScopeAuthority {
    async fn ensure_customer_in_scope(
        &self,
        _customer_id: Uuid,
        _target_scope: &TenantScope,
    ) -> Result<(), PortError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.allow {
            Ok(())
        } else {
            Err(PortError::ScopeMismatch)
        }
    }
}

#[derive(Clone)]
struct FakeProvider {
    allow: bool,
    calls: Arc<AtomicUsize>,
}
#[async_trait]
impl ProviderVerificationPort for FakeProvider {
    async fn verify_billing_event(
        &self,
        evidence: &BillingEventEvidence,
    ) -> Result<VerifiedBillingEvent, PortError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if !self.allow {
            return Err(PortError::ProviderRejected);
        }
        Ok(VerifiedBillingEvent {
            provider_event_id: evidence.provider_event_id.clone(),
            occurred_at: evidence.occurred_at,
            signal: evidence.signal,
        })
    }

    async fn verify_entitlement_change(
        &self,
        evidence: &EntitlementChangeEvidence,
    ) -> Result<VerifiedEntitlementChange, PortError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if !self.allow {
            return Err(PortError::ProviderRejected);
        }
        Ok(VerifiedEntitlementChange {
            provider_event_id: evidence.provider_event_id.clone(),
            occurred_at: evidence.occurred_at,
            capability: evidence.capability.clone(),
            enabled: evidence.enabled,
            capacity: evidence.capacity,
        })
    }
}

#[derive(Clone, Default)]
struct FakeWriteRepository {
    subscription: Arc<Mutex<Option<Subscription>>>,
    loads: Arc<AtomicUsize>,
}

#[async_trait]
impl WriteRepository for FakeWriteRepository {
    async fn load_catalog_plan(
        &self,
        _scope: &TenantScope,
        _id: CatalogPlanId,
    ) -> Result<Option<CatalogPlan>, PortError> {
        Ok(None)
    }
    async fn load_subscription(
        &self,
        _scope: &TenantScope,
        _id: SubscriptionId,
    ) -> Result<Option<Subscription>, PortError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.subscription.lock().unwrap().clone())
    }
    async fn load_entitlement(
        &self,
        _scope: &TenantScope,
        _id: EntitlementId,
    ) -> Result<Option<Entitlement>, PortError> {
        Ok(None)
    }
}

#[derive(Clone, Default)]
struct FakeUnitOfWork {
    receipts: Arc<Mutex<Vec<(CommandIdentity, CommercialStoredCommit)>>>,
    commits: Arc<AtomicUsize>,
}

#[async_trait]
impl CommandUnitOfWork for FakeUnitOfWork {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<CommercialStoredCommit>, PortError> {
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

    async fn commit(&self, request: CommercialCommit) -> Result<CommercialStoredCommit, PortError> {
        self.commits.fetch_add(1, Ordering::SeqCst);
        let stored = CommercialStoredCommit {
            command_id: request.identity.command_id,
            correlation_id: request.correlation_id,
            aggregate: request.aggregate,
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
            .push((request.identity, stored.clone()));
        Ok(stored)
    }
}

type Handler = CommercialCommandHandler<
    FakeWriteRepository,
    FakeUnitOfWork,
    FakeAuthorization,
    FakeScopeAuthority,
    FakeProvider,
    FixedClock,
>;

fn handler(
    write: FakeWriteRepository,
    uow: FakeUnitOfWork,
    authorize: bool,
    scope_allowed: bool,
    provider_allowed: bool,
    provider_calls: Arc<AtomicUsize>,
) -> Handler {
    CommercialCommandHandler::new(
        write,
        uow,
        FakeAuthorization { allow: authorize },
        FakeScopeAuthority {
            allow: scope_allowed,
            calls: Arc::new(AtomicUsize::new(0)),
        },
        FakeProvider {
            allow: provider_allowed,
            calls: provider_calls,
        },
        FixedClock::new(at()),
    )
}

fn create_subscription() -> CreateSubscription {
    CreateSubscription {
        subscription_id: SubscriptionId::try_from_uuid(id(10)).unwrap(),
        scope: scope(),
        customer_id: id(11),
        plan_id: CatalogPlanId::try_from_uuid(id(12)).unwrap(),
        amount_minor: 50_000,
        currency: currency(),
        starts_in_trial: true,
    }
}

#[test]
fn create_replays_stable_identity_once() {
    let uow = FakeUnitOfWork::default();
    let handler = handler(
        FakeWriteRepository::default(),
        uow.clone(),
        true,
        true,
        true,
        Arc::new(AtomicUsize::new(0)),
    );
    let envelope = command_envelope(1, None, create_subscription());
    let first = block_on(handler.handle(envelope.clone())).unwrap();
    let replay = block_on(handler.handle(envelope)).unwrap();
    assert!(!first.replayed);
    assert!(replay.replayed);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 1);
}

#[test]
fn changed_payload_with_reused_idempotency_identity_conflicts() {
    let uow = FakeUnitOfWork::default();
    let handler = handler(
        FakeWriteRepository::default(),
        uow,
        true,
        true,
        true,
        Arc::new(AtomicUsize::new(0)),
    );
    let original = command_envelope(2, None, create_subscription());
    block_on(handler.handle(original.clone())).unwrap();
    let mut changed = original;
    changed.command.amount_minor += 1;
    assert_eq!(
        block_on(handler.handle(changed)).unwrap_err().category(),
        ErrorCategory::Conflict
    );
}

#[test]
fn expected_version_conflict_prevents_provider_call_and_commit() {
    let provider_calls = Arc::new(AtomicUsize::new(0));
    let uow = FakeUnitOfWork::default();
    let handler = handler(
        FakeWriteRepository {
            subscription: Arc::new(Mutex::new(Some(subscription()))),
            loads: Arc::new(AtomicUsize::new(0)),
        },
        uow.clone(),
        true,
        true,
        true,
        provider_calls.clone(),
    );
    let error = block_on(handler.handle(command_envelope(
        3,
        Some(99),
        ApplyBillingEvent {
            subscription_id: SubscriptionId::try_from_uuid(id(10)).unwrap(),
            evidence: BillingEventEvidence {
                provider: "stripe".into(),
                provider_event_id: "evt-1".into(),
                occurred_at: at() + Duration::seconds(1),
                signal: BillingSignal::TrialEnded,
                verification_token: "signed".into(),
            },
        },
    )))
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(provider_calls.load(Ordering::SeqCst), 0);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
}

#[test]
fn rejected_provider_evidence_never_commits() {
    let uow = FakeUnitOfWork::default();
    let handler = handler(
        FakeWriteRepository {
            subscription: Arc::new(Mutex::new(Some(subscription()))),
            loads: Arc::new(AtomicUsize::new(0)),
        },
        uow.clone(),
        true,
        true,
        false,
        Arc::new(AtomicUsize::new(0)),
    );
    let error = block_on(handler.handle(command_envelope(
        4,
        Some(1),
        ApplyBillingEvent {
            subscription_id: SubscriptionId::try_from_uuid(id(10)).unwrap(),
            evidence: BillingEventEvidence {
                provider: "stripe".into(),
                provider_event_id: "evt-2".into(),
                occurred_at: at() + Duration::seconds(1),
                signal: BillingSignal::TrialEnded,
                verification_token: "bad".into(),
            },
        },
    )))
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Denied);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
}

#[test]
fn customer_scope_denial_prevents_creation() {
    let uow = FakeUnitOfWork::default();
    let handler = handler(
        FakeWriteRepository::default(),
        uow.clone(),
        true,
        false,
        true,
        Arc::new(AtomicUsize::new(0)),
    );
    let error =
        block_on(handler.handle(command_envelope(5, None, create_subscription()))).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Denied);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
}

#[derive(Clone)]
struct FakeReadRepository {
    subscription: SubscriptionView,
    reads: Arc<AtomicUsize>,
}

fn projection<T>(value: T) -> ReadProjection<T> {
    ReadProjection {
        value,
        metadata: ProjectionMetadata {
            checkpoint: None,
            freshness: FreshnessState::Current,
            lag_millis: Some(0),
        },
    }
}

#[async_trait]
impl ReadRepository for FakeReadRepository {
    async fn get_catalog_plan(
        &self,
        _scope: &TenantScope,
        _id: CatalogPlanId,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<CatalogPlanView>>, PortError> {
        Ok(projection(None))
    }
    async fn list_catalog_plans(
        &self,
        _scope: &TenantScope,
        _filter: &PageFilter,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Page<CatalogPlanView>>, PortError> {
        Ok(projection(Page {
            records: vec![],
            next_cursor: None,
        }))
    }
    async fn get_subscription(
        &self,
        _scope: &TenantScope,
        _id: SubscriptionId,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<SubscriptionView>>, PortError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(projection(Some(self.subscription.clone())))
    }
    async fn list_subscriptions(
        &self,
        _scope: &TenantScope,
        _filter: &PageFilter,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Page<SubscriptionView>>, PortError> {
        Ok(projection(Page {
            records: vec![self.subscription.clone()],
            next_cursor: None,
        }))
    }
    async fn get_entitlement(
        &self,
        _scope: &TenantScope,
        _id: EntitlementId,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<EntitlementView>>, PortError> {
        Ok(projection(None))
    }
    async fn list_entitlements(
        &self,
        _scope: &TenantScope,
        _filter: &PageFilter,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Page<EntitlementView>>, PortError> {
        Ok(projection(Page {
            records: vec![],
            next_cursor: None,
        }))
    }
}

#[test]
fn query_uses_disposable_read_projection_and_preserves_freshness() {
    let reads = Arc::new(AtomicUsize::new(0));
    let view = SubscriptionView::from(&subscription());
    let handler = CommercialQueryHandler::new(
        FakeReadRepository {
            subscription: view.clone(),
            reads: reads.clone(),
        },
        FakeAuthorization { allow: true },
    );
    let result = block_on(handler.handle(query_envelope(
        1,
        GetSubscription {
            subscription_id: view.id,
        },
    )))
    .unwrap();
    assert_eq!(result.value.subscription, Some(view));
    assert_eq!(
        result.metadata.projection.freshness,
        FreshnessState::Current
    );
    assert_eq!(reads.load(Ordering::SeqCst), 1);
}

#[test]
fn invalid_page_size_fails_before_read() {
    let reads = Arc::new(AtomicUsize::new(0));
    let handler = CommercialQueryHandler::new(
        FakeReadRepository {
            subscription: SubscriptionView::from(&subscription()),
            reads: reads.clone(),
        },
        FakeAuthorization { allow: true },
    );
    let error = block_on(handler.handle(query_envelope(
        2,
        ListSubscriptions {
            filter: PageFilter {
                page_size: 0,
                cursor: None,
            },
        },
    )))
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Invalid);
    assert_eq!(reads.load(Ordering::SeqCst), 0);
}

#[test]
fn catalog_view_status_is_typed() {
    let plan = CatalogPlan::create(
        CatalogPlanId::try_from_uuid(id(50)).unwrap(),
        TenantScope::Platform,
        "growth".into(),
        "Growth".into(),
        10,
        currency(),
        at(),
    )
    .unwrap();
    assert_eq!(
        CatalogPlanView::from(&plan).status,
        CatalogPlanStatus::Draft
    );
    assert_eq!(subscription().status(), SubscriptionStatus::Trial);
}

struct ThreadWaker;
impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {}
}

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(ThreadWaker));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
