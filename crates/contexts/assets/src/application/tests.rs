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
    CONTRACT_VERSION, CommandEnvelope, CommandHandler, EventPosition, IdempotencyKey,
    ProjectionNotReady, ProjectionTarget, QueryConsistency, QueryEnvelope, QueryHandler,
    ReadYourWrites,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, AuthorityScope, CommandId, CorrelationId,
    ErrorCategory, EventId, FixedClock, QueryId,
};

use super::*;
use crate::ports::{AuthorizationRequest, PortError, ReadProjection};

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

fn asset_scope() -> AssetScope {
    AssetScope::try_new(scope()).unwrap()
}

fn actor() -> ActorAttribution {
    ActorAttribution {
        principal_ref: "assets-test-actor".into(),
        kind: ActorKind::HoldingUser,
        session_id: None,
        represented_principal_ref: None,
        support_capability_id: None,
    }
}

fn command_envelope<C>(sequence: u128, expected: Option<u64>, command: C) -> CommandEnvelope<C> {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id: CommandId::try_from_uuid(id(100 + sequence)).unwrap(),
        idempotency_key: IdempotencyKey::try_new(format!("assets-test-{sequence}")).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(200 + sequence)).unwrap(),
        causation_id: None,
        actor: actor(),
        active_scope: scope(),
        audit: AuditContext {
            purpose: "assets.test".into(),
            customer_visible: false,
            module: Some("assets".into()),
        },
        expected_aggregate_version: expected,
        command,
    }
}

#[derive(Clone)]
struct FakeAuthorization(bool);

#[async_trait]
impl AuthorizationPort for FakeAuthorization {
    async fn authorize(&self, _request: AuthorizationRequest<'_>) -> Result<(), PortError> {
        if self.0 {
            Ok(())
        } else {
            Err(PortError::Denied)
        }
    }
}

#[derive(Clone)]
struct FakeEntitlement(bool);

#[async_trait]
impl EntitlementPort for FakeEntitlement {
    async fn ensure_assets_enabled(
        &self,
        _scope: &AuthorityScope,
        _write: bool,
    ) -> Result<(), PortError> {
        if self.0 {
            Ok(())
        } else {
            Err(PortError::EntitlementDenied)
        }
    }
}

#[derive(Clone)]
struct FakeWriteRepository(Option<Asset>);

#[async_trait]
impl WriteRepository for FakeWriteRepository {
    async fn load(
        &self,
        _authority_scope: &AuthorityScope,
        _asset_id: AssetId,
    ) -> Result<Option<Asset>, PortError> {
        Ok(self.0.clone())
    }
}

#[derive(Clone, Default)]
struct FakeUnitOfWork {
    receipts: Arc<Mutex<Vec<(CommandIdentity, AssetStoredCommit)>>>,
    commits: Arc<AtomicUsize>,
}

#[async_trait]
impl CommandUnitOfWork for FakeUnitOfWork {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<AssetStoredCommit>, PortError> {
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

    async fn commit(&self, request: AssetCommit) -> Result<AssetStoredCommit, PortError> {
        self.commits.fetch_add(1, Ordering::SeqCst);
        let stored = AssetStoredCommit {
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

#[derive(Clone)]
struct NotReadyRead {
    reads: Arc<AtomicUsize>,
}

#[async_trait]
impl ReadRepository for NotReadyRead {
    async fn get(
        &self,
        _authority_scope: &AuthorityScope,
        _asset_id: AssetId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<AssetView>>, PortError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        let QueryConsistency::ReadYourWrites(requirement) = consistency else {
            return Err(PortError::Internal);
        };
        Err(PortError::NotReady(ProjectionNotReady {
            projection: requirement.projection.clone(),
            partition: requirement.partition.clone(),
            target: requirement.target,
            observed: None,
        }))
    }

    async fn list(
        &self,
        _authority_scope: &AuthorityScope,
        _filter: &AssetListFilter,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<AssetList>, PortError> {
        Err(PortError::Internal)
    }
}

fn handler(
    write: FakeWriteRepository,
    uow: FakeUnitOfWork,
    authorization: bool,
    entitlement: bool,
) -> AssetCommandHandler<
    FakeWriteRepository,
    FakeUnitOfWork,
    FakeAuthorization,
    FakeEntitlement,
    FixedClock,
> {
    AssetCommandHandler::new(
        write,
        uow,
        FakeAuthorization(authorization),
        FakeEntitlement(entitlement),
        FixedClock::new(at()),
    )
}

fn create_command() -> CreateAsset {
    CreateAsset {
        asset_id: AssetId::try_from_uuid(id(10)).unwrap(),
        code: "AST-10".into(),
        name: "Packing line".into(),
        scope: asset_scope(),
        location_id: id(11),
    }
}

#[test]
fn stable_command_identity_replays_without_second_commit() {
    let uow = FakeUnitOfWork::default();
    let handler = handler(FakeWriteRepository(None), uow.clone(), true, true);
    let first = block_on(handler.handle(command_envelope(1, None, create_command()))).unwrap();
    let replay = block_on(handler.handle(command_envelope(1, None, create_command()))).unwrap();
    assert!(!first.replayed);
    assert!(replay.replayed);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 1);
}

#[test]
fn authorization_and_entitlement_both_gate_commits() {
    for (authorization, entitlement) in [(false, true), (true, false)] {
        let uow = FakeUnitOfWork::default();
        let error = block_on(
            handler(
                FakeWriteRepository(None),
                uow.clone(),
                authorization,
                entitlement,
            )
            .handle(command_envelope(2, None, create_command())),
        )
        .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Denied);
        assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn expected_version_is_checked_before_atomic_commit() {
    let mut aggregate = Asset::create(
        AssetId::try_from_uuid(id(10)).unwrap(),
        "AST-10",
        "Packing line",
        asset_scope(),
        id(11),
        at(),
    )
    .unwrap();
    aggregate.take_uncommitted_events();
    let uow = FakeUnitOfWork::default();
    let error = block_on(
        handler(
            FakeWriteRepository(Some(aggregate)),
            uow.clone(),
            true,
            true,
        )
        .handle(command_envelope(
            3,
            Some(99),
            PlaceAssetInService {
                asset_id: AssetId::try_from_uuid(id(10)).unwrap(),
            },
        )),
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
}

#[test]
fn read_your_writes_not_ready_is_returned_without_write_fallback() {
    let reads = Arc::new(AtomicUsize::new(0));
    let handler = AssetQueryHandler::new(
        NotReadyRead {
            reads: reads.clone(),
        },
        FakeAuthorization(true),
        FakeEntitlement(true),
    );
    let target = ProjectionTarget {
        event_id: EventId::try_from_uuid(id(901)).unwrap(),
        global_sequence: 7,
    };
    let error = block_on(handler.handle(QueryEnvelope {
        contract_version: CONTRACT_VERSION,
        query_id: QueryId::try_from_uuid(id(500)).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(501)).unwrap(),
        actor: actor(),
        active_scope: scope(),
        audit: AuditContext {
            purpose: "assets.test.query".into(),
            customer_visible: false,
            module: Some("assets".into()),
        },
        consistency: QueryConsistency::ReadYourWrites(ReadYourWrites {
            projection: "assets.asset".into(),
            partition: "company:2".into(),
            target,
            timeout_millis: 10,
        }),
        query: GetAsset {
            asset_id: AssetId::try_from_uuid(id(10)).unwrap(),
        },
    }))
    .unwrap_err();
    assert!(matches!(error, CqrsError::NotReady(value) if value.target == target));
    assert_eq!(reads.load(Ordering::SeqCst), 1);
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
