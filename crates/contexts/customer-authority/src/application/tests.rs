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
    QueryEnvelope, QueryHandler, ReadYourWrites,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    FixedClock, QueryId, TenantScope,
};
use uuid::Uuid;

use super::*;
use crate::{
    domain::{CustomerAccount, CustomerCode, CustomerId, CustomerIdentity, CustomerStatus},
    ports::{
        AuthorizationRequest, CustomerAuthorityCommit, PortError, ReadProjection, StoredCommit,
    },
};

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn at() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
}

fn company() -> TenantScope {
    TenantScope::Company {
        holding_id: id(1),
        company_id: id(2),
    }
}

fn actor() -> ActorAttribution {
    ActorAttribution {
        principal_ref: "customer-authority-test".into(),
        kind: ActorKind::PlatformUser,
        session_id: None,
        represented_principal_ref: None,
        support_capability_id: None,
    }
}

fn account(customer_id: u128, code: &str) -> CustomerAccount {
    account_in_scope(customer_id, code, company())
}

fn account_in_scope(customer_id: u128, code: &str, scope: TenantScope) -> CustomerAccount {
    let mut value = CustomerAccount::create(
        CustomerId::try_from_uuid(id(customer_id)).unwrap(),
        CustomerCode::try_new(code).unwrap(),
        CustomerIdentity::try_new(
            format!("Customer {customer_id}"),
            Some(format!("Customer {customer_id} Limited")),
            Some("registration-sensitive".into()),
            Some("tax-sensitive".into()),
            "TH",
        )
        .unwrap(),
        scope,
        at(),
    )
    .unwrap();
    value.take_uncommitted_events();
    value
}

fn command_envelope<C>(
    sequence: u128,
    scope: TenantScope,
    expected: Option<u64>,
    command: C,
) -> CommandEnvelope<C> {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id: CommandId::try_from_uuid(id(100 + sequence)).unwrap(),
        idempotency_key: IdempotencyKey::try_new(format!("customer-authority-{sequence}")).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(200 + sequence)).unwrap(),
        causation_id: None,
        actor: actor(),
        tenant_scope: scope,
        audit: AuditContext {
            purpose: "customer_authority.test".into(),
            customer_visible: true,
            module: Some("customer_authority".into()),
        },
        expected_aggregate_version: expected,
        command,
    }
}

fn query_envelope<Q>(sequence: u128, scope: TenantScope, query: Q) -> QueryEnvelope<Q> {
    QueryEnvelope {
        contract_version: CONTRACT_VERSION,
        query_id: QueryId::try_from_uuid(id(300 + sequence)).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(400 + sequence)).unwrap(),
        actor: actor(),
        tenant_scope: scope,
        audit: AuditContext {
            purpose: "customer_authority.test.query".into(),
            customer_visible: true,
            module: Some("customer_authority".into()),
        },
        consistency: QueryConsistency::Eventual,
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
        self.allow.then_some(()).ok_or(PortError::Denied)
    }
}

#[derive(Clone)]
struct FakeEntitlement {
    allow: bool,
}

#[async_trait]
impl EntitlementPort for FakeEntitlement {
    async fn ensure_customer_authority_enabled(
        &self,
        _scope: &TenantScope,
        _write: bool,
    ) -> Result<(), PortError> {
        self.allow.then_some(()).ok_or(PortError::EntitlementDenied)
    }
}

#[derive(Clone)]
struct FakeWriteRepository {
    aggregates: Arc<Mutex<Vec<CustomerAccount>>>,
    loads: Arc<AtomicUsize>,
}

#[async_trait]
impl WriteRepository for FakeWriteRepository {
    async fn load(
        &self,
        _authority_scope: &TenantScope,
        customer_id: CustomerId,
    ) -> Result<Option<CustomerAccount>, PortError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self
            .aggregates
            .lock()
            .unwrap()
            .iter()
            .find(|aggregate| aggregate.id() == customer_id)
            .cloned())
    }
}

#[derive(Clone, Default)]
struct FakeUnitOfWork {
    receipts: Arc<Mutex<Vec<(CommandIdentity, StoredCommit)>>>,
    commits: Arc<AtomicUsize>,
    merge_target: Arc<Mutex<Option<MergeTargetVersion>>>,
}

#[async_trait]
impl CommandUnitOfWork for FakeUnitOfWork {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
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

    async fn commit(&self, request: CustomerAuthorityCommit) -> Result<StoredCommit, PortError> {
        self.commits.fetch_add(1, Ordering::SeqCst);
        *self.merge_target.lock().unwrap() = request.merge_target;
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
    view: CustomerAuthorityView,
    reads: Arc<AtomicUsize>,
    not_ready: bool,
}

#[async_trait]
impl ReadRepository for FakeReadRepository {
    async fn get(
        &self,
        _authority_scope: &TenantScope,
        _customer_id: CustomerId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<CustomerAuthorityView>>, PortError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        if self.not_ready {
            let QueryConsistency::ReadYourWrites(requirement) = consistency else {
                return Err(PortError::Internal);
            };
            return Err(PortError::NotReady(ProjectionNotReady {
                projection: requirement.projection.clone(),
                partition: requirement.partition.clone(),
                target: requirement.target,
                observed: None,
            }));
        }
        Ok(ReadProjection {
            value: Some(self.view.clone()),
            metadata: projection_metadata(),
        })
    }

    async fn list(
        &self,
        _authority_scope: &TenantScope,
        _filter: &CustomerListFilter,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<CustomerList>, PortError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(ReadProjection {
            value: CustomerList {
                customers: vec![self.view.clone()],
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
    aggregates: Vec<CustomerAccount>,
    allow: bool,
) -> (
    CustomerAuthorityCommandHandler<
        FakeWriteRepository,
        FakeUnitOfWork,
        FakeAuthorization,
        FakeEntitlement,
        FixedClock,
    >,
    FakeUnitOfWork,
    Arc<AtomicUsize>,
) {
    let unit_of_work = FakeUnitOfWork::default();
    let loads = Arc::new(AtomicUsize::new(0));
    let handler = CustomerAuthorityCommandHandler::new(
        FakeWriteRepository {
            aggregates: Arc::new(Mutex::new(aggregates)),
            loads: loads.clone(),
        },
        unit_of_work.clone(),
        FakeAuthorization {
            allow,
            calls: Arc::new(AtomicUsize::new(0)),
        },
        FakeEntitlement { allow: true },
        FixedClock::new(at()),
    );
    (handler, unit_of_work, loads)
}

#[test]
fn stable_idempotency_replays_without_loading_write_store() {
    let aggregate = account(10, "CUS-10");
    let (handler, unit_of_work, loads) = command_handler(vec![aggregate], true);
    let envelope = command_envelope(
        1,
        TenantScope::Platform,
        Some(1),
        ChangeCustomerStatus {
            customer_id: CustomerId::try_from_uuid(id(10)).unwrap(),
            status: CustomerStatus::Active,
        },
    );
    let first = block_on(handler.handle(envelope.clone())).unwrap();
    let second = block_on(handler.handle(envelope)).unwrap();
    assert!(!first.replayed);
    assert!(second.replayed);
    assert_eq!(unit_of_work.commits.load(Ordering::SeqCst), 1);
    assert_eq!(loads.load(Ordering::SeqCst), 1);
}

#[test]
fn changed_payload_under_same_identity_conflicts() {
    let aggregate = account(10, "CUS-10");
    let (handler, _unit_of_work, _loads) = command_handler(vec![aggregate], true);
    let first = command_envelope(
        2,
        TenantScope::Platform,
        Some(1),
        ChangeCustomerStatus {
            customer_id: CustomerId::try_from_uuid(id(10)).unwrap(),
            status: CustomerStatus::Active,
        },
    );
    let mut changed = first.clone();
    changed.command.status = CustomerStatus::Suspended;
    block_on(handler.handle(first)).unwrap();
    let error = block_on(handler.handle(changed)).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
}

#[test]
fn mutations_require_expected_version_and_contained_scope() {
    let aggregate = account(10, "CUS-10");
    let (handler, _unit_of_work, _loads) = command_handler(vec![aggregate], true);
    let missing_version = command_envelope(
        3,
        TenantScope::Platform,
        None,
        ChangeCustomerStatus {
            customer_id: CustomerId::try_from_uuid(id(10)).unwrap(),
            status: CustomerStatus::Active,
        },
    );
    assert_eq!(
        block_on(handler.handle(missing_version))
            .unwrap_err()
            .category(),
        ErrorCategory::Invalid
    );

    let unrelated_scope = TenantScope::Company {
        holding_id: id(91),
        company_id: id(92),
    };
    let denied = command_envelope(
        4,
        unrelated_scope,
        Some(1),
        ChangeCustomerStatus {
            customer_id: CustomerId::try_from_uuid(id(10)).unwrap(),
            status: CustomerStatus::Active,
        },
    );
    assert_eq!(
        block_on(handler.handle(denied)).unwrap_err().category(),
        ErrorCategory::Denied
    );
}

#[test]
fn merge_carries_survivor_version_into_atomic_guard() {
    let retired = account(10, "CUS-10");
    let survivor = account(11, "CUS-11");
    let (handler, unit_of_work, _loads) = command_handler(vec![retired, survivor], true);
    let receipt = block_on(handler.handle(command_envelope(
        5,
        TenantScope::Platform,
        Some(1),
        MergeCustomer {
            retired_customer_id: CustomerId::try_from_uuid(id(10)).unwrap(),
            surviving_customer_id: CustomerId::try_from_uuid(id(11)).unwrap(),
        },
    )))
    .unwrap();
    assert_eq!(
        receipt.response.customer.merged_into,
        Some(CustomerId::try_from_uuid(id(11)).unwrap())
    );
    assert_eq!(
        *unit_of_work.merge_target.lock().unwrap(),
        Some(MergeTargetVersion {
            customer_id: CustomerId::try_from_uuid(id(11)).unwrap(),
            version: 1,
        })
    );
}

#[test]
fn merge_rejects_cross_company_target_even_for_platform_authority() {
    let retired = account(10, "CUS-10");
    let survivor = account_in_scope(
        11,
        "CUS-11",
        TenantScope::Company {
            holding_id: id(1),
            company_id: id(99),
        },
    );
    let (handler, unit_of_work, _loads) = command_handler(vec![retired, survivor], true);
    let error = block_on(handler.handle(command_envelope(
        51,
        TenantScope::Platform,
        Some(1),
        MergeCustomer {
            retired_customer_id: CustomerId::try_from_uuid(id(10)).unwrap(),
            surviving_customer_id: CustomerId::try_from_uuid(id(11)).unwrap(),
        },
    )))
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Denied);
    assert_eq!(unit_of_work.commits.load(Ordering::SeqCst), 0);
}

#[test]
fn customer_record_reference_is_stable_and_uses_the_singular_route() {
    let customer_id = Uuid::new_v4();
    let value = CustomerAccount::create(
        CustomerId::try_from_uuid(customer_id).unwrap(),
        CustomerCode::try_new("CUS-REF").unwrap(),
        CustomerIdentity::try_new("Reference Customer", None, None, None, "TH").unwrap(),
        company(),
        at(),
    )
    .unwrap();
    let reference = value.record_ref().unwrap();
    assert_eq!(reference.id().as_uuid(), customer_id);
    assert_eq!(
        reference.canonical_detail_route(),
        format!("/customer/account/{customer_id}")
    );
    assert_eq!(
        reference.copied_reference_route(),
        format!("/reference/item/{customer_id}")
    );
}

#[test]
fn queries_use_only_access_safe_read_projection() {
    let view = CustomerAuthorityView::from(&account(10, "CUS-10"));
    let reads = Arc::new(AtomicUsize::new(0));
    let handler = CustomerAuthorityQueryHandler::new(
        FakeReadRepository {
            view,
            reads: reads.clone(),
            not_ready: false,
        },
        FakeAuthorization {
            allow: true,
            calls: Arc::new(AtomicUsize::new(0)),
        },
        FakeEntitlement { allow: true },
    );
    let result = block_on(handler.handle(query_envelope(
        6,
        TenantScope::Platform,
        GetCustomer {
            customer_id: CustomerId::try_from_uuid(id(10)).unwrap(),
        },
    )))
    .unwrap();
    let rendered = format!("{:?}", result.value.customer.unwrap());
    assert!(!rendered.contains("registration-sensitive"));
    assert!(!rendered.contains("tax-sensitive"));
    assert_eq!(reads.load(Ordering::SeqCst), 1);
}

#[test]
fn read_your_writes_not_ready_never_falls_back_to_write_store() {
    let view = CustomerAuthorityView::from(&account(10, "CUS-10"));
    let handler = CustomerAuthorityQueryHandler::new(
        FakeReadRepository {
            view,
            reads: Arc::new(AtomicUsize::new(0)),
            not_ready: true,
        },
        FakeAuthorization {
            allow: true,
            calls: Arc::new(AtomicUsize::new(0)),
        },
        FakeEntitlement { allow: true },
    );
    let mut envelope = query_envelope(
        7,
        TenantScope::Platform,
        GetCustomer {
            customer_id: CustomerId::try_from_uuid(id(10)).unwrap(),
        },
    );
    envelope.consistency = QueryConsistency::ReadYourWrites(ReadYourWrites {
        projection: "customer_authority".into(),
        partition: "platform".into(),
        target: ProjectionTarget {
            event_id: EventId::try_from_uuid(id(999)).unwrap(),
            global_sequence: 9,
        },
        timeout_millis: 10,
    });
    assert!(matches!(
        block_on(handler.handle(envelope)),
        Err(CqrsError::NotReady(_))
    ));
}

struct ThreadWake(std::thread::Thread);

impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::park(),
        }
    }
}
