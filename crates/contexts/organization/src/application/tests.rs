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
    IdempotencyKey, ProjectionMetadata, QueryConsistency, QueryEnvelope, QueryHandler,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    FixedClock, QueryId, TenantScope,
};
use uuid::Uuid;

use super::*;
use crate::{
    domain::{OrganizationNodeKind, OrganizationNodeStatus},
    ports::{AuthorizationRequest, PortError, ReadProjection, StoredCommit},
};

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn node_id(value: u128) -> OrganizationNodeId {
    OrganizationNodeId::try_from_uuid(id(value)).unwrap()
}

fn at() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
}

fn authority_scope() -> TenantScope {
    TenantScope::Holding { holding_id: id(1) }
}

fn branch_scope() -> TenantScope {
    TenantScope::Branch {
        holding_id: id(1),
        company_id: id(2),
        branch_id: id(3),
    }
}

fn branch_placement() -> OrganizationPlacement {
    OrganizationPlacement::Branch {
        holding_id: id(1),
        company_id: id(2),
    }
}

fn actor() -> ActorAttribution {
    ActorAttribution {
        principal_ref: "organization-test-actor".into(),
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
        idempotency_key: IdempotencyKey::try_new(format!("organization-test-{sequence}")).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(200 + sequence)).unwrap(),
        causation_id: None,
        actor: actor(),
        tenant_scope: authority_scope(),
        audit: AuditContext {
            purpose: "organization.test".into(),
            customer_visible: false,
            module: Some("organization".into()),
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
        tenant_scope: authority_scope(),
        audit: AuditContext {
            purpose: "organization.test.query".into(),
            customer_visible: false,
            module: Some("organization".into()),
        },
        consistency: QueryConsistency::Eventual,
        query,
    }
}

fn aggregate() -> OrganizationNode {
    let mut aggregate = OrganizationNode::create(
        node_id(3),
        branch_placement(),
        "BKK".into(),
        "Bangkok".into(),
        at(),
    )
    .unwrap();
    aggregate.take_uncommitted_events();
    aggregate
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
    async fn ensure_organization_enabled(
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

#[derive(Clone)]
struct FakeHierarchy {
    error: Arc<Mutex<Option<PortError>>>,
    placement_checks: Arc<AtomicUsize>,
    archive_checks: Arc<AtomicUsize>,
}

impl FakeHierarchy {
    fn allowing() -> Self {
        Self {
            error: Arc::new(Mutex::new(None)),
            placement_checks: Arc::new(AtomicUsize::new(0)),
            archive_checks: Arc::new(AtomicUsize::new(0)),
        }
    }
}

#[async_trait]
impl HierarchyPort for FakeHierarchy {
    async fn validate_placement(
        &self,
        _authority_scope: &TenantScope,
        _node_id: OrganizationNodeId,
        _placement: &OrganizationPlacement,
    ) -> Result<(), PortError> {
        self.placement_checks.fetch_add(1, Ordering::SeqCst);
        match self.error.lock().unwrap().clone() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    async fn ensure_can_archive(
        &self,
        _authority_scope: &TenantScope,
        _node_id: OrganizationNodeId,
    ) -> Result<(), PortError> {
        self.archive_checks.fetch_add(1, Ordering::SeqCst);
        match self.error.lock().unwrap().clone() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

#[derive(Clone)]
struct FakeWriteRepository {
    aggregate: Arc<Mutex<Option<OrganizationNode>>>,
    loads: Arc<AtomicUsize>,
}

#[async_trait]
impl WriteRepository for FakeWriteRepository {
    async fn load(
        &self,
        _authority_scope: &TenantScope,
        _node_id: OrganizationNodeId,
    ) -> Result<Option<OrganizationNode>, PortError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.aggregate.lock().unwrap().clone())
    }
}

#[derive(Clone, Default)]
struct FakeUnitOfWork {
    receipts: Arc<Mutex<Vec<(CommandIdentity, StoredCommit)>>>,
    commits: Arc<AtomicUsize>,
}

#[async_trait]
impl CommandUnitOfWork for FakeUnitOfWork {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        let receipts = self.receipts.lock().unwrap();
        for (stored_identity, receipt) in receipts.iter() {
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

    async fn commit(&self, request: OrganizationCommit) -> Result<StoredCommit, PortError> {
        self.commits.fetch_add(1, Ordering::SeqCst);
        let stored = StoredCommit {
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
struct FakeReadRepository {
    view: NodeView,
    reads: Arc<AtomicUsize>,
}

#[async_trait]
impl ReadRepository for FakeReadRepository {
    async fn get(
        &self,
        _authority_scope: &TenantScope,
        _node_id: OrganizationNodeId,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<NodeView>>, PortError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(ReadProjection {
            value: Some(self.view.clone()),
            metadata: projection_metadata(),
        })
    }

    async fn list(
        &self,
        _authority_scope: &TenantScope,
        _filter: &NodeListFilter,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<NodeList>, PortError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(ReadProjection {
            value: NodeList {
                nodes: vec![self.view.clone()],
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

fn command_handler(
    write: FakeWriteRepository,
    uow: FakeUnitOfWork,
    hierarchy: FakeHierarchy,
    allow: bool,
) -> OrganizationCommandHandler<
    FakeWriteRepository,
    FakeUnitOfWork,
    FakeAuthorization,
    FakeEntitlement,
    FakeHierarchy,
    FixedClock,
> {
    OrganizationCommandHandler::new(
        write,
        uow,
        FakeAuthorization {
            allow,
            calls: Arc::new(AtomicUsize::new(0)),
        },
        FakeEntitlement { allow: true },
        hierarchy,
        FixedClock::new(at()),
    )
}

fn empty_write_repository() -> FakeWriteRepository {
    FakeWriteRepository {
        aggregate: Arc::new(Mutex::new(None)),
        loads: Arc::new(AtomicUsize::new(0)),
    }
}

fn populated_write_repository() -> FakeWriteRepository {
    FakeWriteRepository {
        aggregate: Arc::new(Mutex::new(Some(aggregate()))),
        loads: Arc::new(AtomicUsize::new(0)),
    }
}

fn create_command() -> CreateOrganizationNode {
    CreateOrganizationNode {
        node_id: node_id(3),
        placement: branch_placement(),
        code: "BKK".into(),
        name: "Bangkok".into(),
    }
}

#[test]
fn create_replays_stable_identity_without_a_second_hierarchy_check_or_commit() {
    let uow = FakeUnitOfWork::default();
    let hierarchy = FakeHierarchy::allowing();
    let handler = command_handler(
        empty_write_repository(),
        uow.clone(),
        hierarchy.clone(),
        true,
    );
    let envelope = command_envelope(1, None, create_command());
    let first = block_on(handler.handle(envelope.clone())).unwrap();
    let replay = block_on(handler.handle(envelope)).unwrap();
    assert!(!first.replayed);
    assert!(replay.replayed);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 1);
    assert_eq!(hierarchy.placement_checks.load(Ordering::SeqCst), 1);
}

#[test]
fn reused_idempotency_identity_with_changed_payload_conflicts() {
    let handler = command_handler(
        empty_write_repository(),
        FakeUnitOfWork::default(),
        FakeHierarchy::allowing(),
        true,
    );
    let original = command_envelope(2, None, create_command());
    block_on(handler.handle(original.clone())).unwrap();
    let mut changed = original;
    changed.command.name = "Changed".into();
    let error = block_on(handler.handle(changed)).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
}

#[test]
fn expected_version_is_checked_before_mutation_and_commit() {
    let uow = FakeUnitOfWork::default();
    let handler = command_handler(
        populated_write_repository(),
        uow.clone(),
        FakeHierarchy::allowing(),
        true,
    );
    let error = block_on(handler.handle(command_envelope(
        3,
        Some(99),
        RenameOrganizationNode {
            node_id: node_id(3),
            name: "New name".into(),
        },
    )))
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
}

#[test]
fn authorization_denial_prevents_hierarchy_check_and_commit() {
    let uow = FakeUnitOfWork::default();
    let hierarchy = FakeHierarchy::allowing();
    let handler = command_handler(
        empty_write_repository(),
        uow.clone(),
        hierarchy.clone(),
        false,
    );
    let error = block_on(handler.handle(command_envelope(4, None, create_command()))).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Denied);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
    assert_eq!(hierarchy.placement_checks.load(Ordering::SeqCst), 0);
}

#[test]
fn reparent_cannot_escape_command_authority() {
    let uow = FakeUnitOfWork::default();
    let hierarchy = FakeHierarchy::allowing();
    let handler = command_handler(
        populated_write_repository(),
        uow.clone(),
        hierarchy.clone(),
        true,
    );
    let error = block_on(handler.handle(command_envelope(
        5,
        Some(1),
        ReparentOrganizationNode {
            node_id: node_id(3),
            placement: OrganizationPlacement::Branch {
                holding_id: id(9),
                company_id: id(10),
            },
        },
    )))
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Denied);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
    assert_eq!(hierarchy.placement_checks.load(Ordering::SeqCst), 0);
}

#[test]
fn authoritative_parent_validation_prevents_commit() {
    let uow = FakeUnitOfWork::default();
    let hierarchy = FakeHierarchy::allowing();
    *hierarchy.error.lock().unwrap() = Some(PortError::InvalidParent);
    let handler = command_handler(
        empty_write_repository(),
        uow.clone(),
        hierarchy.clone(),
        true,
    );
    let error = block_on(handler.handle(command_envelope(6, None, create_command()))).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Invalid);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
    assert_eq!(hierarchy.placement_checks.load(Ordering::SeqCst), 1);
}

#[test]
fn archive_requires_no_active_children() {
    let uow = FakeUnitOfWork::default();
    let hierarchy = FakeHierarchy::allowing();
    *hierarchy.error.lock().unwrap() = Some(PortError::ActiveChildren);
    let handler = command_handler(
        populated_write_repository(),
        uow.clone(),
        hierarchy.clone(),
        true,
    );
    let error = block_on(handler.handle(command_envelope(
        7,
        Some(1),
        ArchiveOrganizationNode {
            node_id: node_id(3),
        },
    )))
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Invalid);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
    assert_eq!(hierarchy.archive_checks.load(Ordering::SeqCst), 1);
}

#[test]
fn query_handler_reads_projection_and_preserves_metadata() {
    let reads = Arc::new(AtomicUsize::new(0));
    let view = NodeView::from(&aggregate());
    assert_eq!(view.kind, OrganizationNodeKind::Branch);
    assert_eq!(view.status, OrganizationNodeStatus::Active);
    assert_eq!(view.scope, branch_scope());
    let handler = OrganizationQueryHandler::new(
        FakeReadRepository {
            view: view.clone(),
            reads: reads.clone(),
        },
        FakeAuthorization {
            allow: true,
            calls: Arc::new(AtomicUsize::new(0)),
        },
        FakeEntitlement { allow: true },
    );
    let result = block_on(handler.handle(query_envelope(
        1,
        GetOrganizationNode {
            node_id: view.node_id,
        },
    )))
    .unwrap();
    assert_eq!(result.value.node, Some(view));
    assert_eq!(
        result.metadata.projection.freshness,
        FreshnessState::Current
    );
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
