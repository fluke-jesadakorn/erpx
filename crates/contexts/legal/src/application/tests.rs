use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use chrono::{Duration, TimeZone, Utc};
use erpx_cqrs::{
    CONTRACT_VERSION, CommandEnvelope, CommandHandler, FreshnessState, IdempotencyKey,
    ProjectionMetadata, ProjectionNotReady, ProjectionTarget, QueryConsistency, QueryEnvelope,
    QueryHandler, ReadYourWrites,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    FixedClock, QueryId,
};
use uuid::Uuid;

use super::*;
use crate::ports::{
    CommandIdentity, ContractListFilter, DocumentListFilter, LegalCommit, ReadProjection,
};

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}
fn at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
}
fn scope() -> TenantScope {
    TenantScope::Company {
        holding_id: id(1),
        company_id: id(2),
    }
}
fn actor(name: &str) -> ActorAttribution {
    ActorAttribution {
        principal_ref: name.into(),
        kind: ActorKind::TenantUser,
        session_id: None,
        represented_principal_ref: None,
        support_capability_id: None,
    }
}
fn contract_id() -> LegalContractId {
    LegalContractId::try_from_uuid(id(10)).unwrap()
}
fn document_id() -> LegalDocumentId {
    LegalDocumentId::try_from_uuid(id(20)).unwrap()
}
fn access_id() -> AccessEntryId {
    AccessEntryId::try_from_uuid(id(30)).unwrap()
}
fn digest(byte: char) -> String {
    format!("sha256:{}", byte.to_string().repeat(64))
}

fn envelope(
    command: LegalCommand,
    expected: Option<u64>,
    principal: &str,
) -> CommandEnvelope<LegalCommand> {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id: CommandId::try_from_uuid(id(100)).unwrap(),
        idempotency_key: IdempotencyKey::try_new("legal-stable-key").unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(101)).unwrap(),
        causation_id: None,
        actor: actor(principal),
        tenant_scope: scope(),
        audit: AuditContext {
            purpose: "legal.test".into(),
            customer_visible: false,
            module: Some("legal".into()),
        },
        expected_aggregate_version: expected,
        command,
    }
}

fn contract() -> LegalContract {
    LegalContract::create(
        contract_id(),
        scope(),
        "CTR-10".into(),
        "Agreement".into(),
        "legal/contracts/10/v1".into(),
        digest('a'),
        at() + Duration::days(30),
        "creator".into(),
        at(),
    )
    .unwrap()
}

fn document() -> LegalDocument {
    let mut value = LegalDocument::create(document_id(), scope(), "Opinion".into(), at()).unwrap();
    value
        .add_version(
            "legal/documents/20/v1".into(),
            digest('b'),
            "author".into(),
            at() + Duration::minutes(1),
        )
        .unwrap();
    value
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
    async fn ensure_legal_enabled(
        &self,
        _scope: &TenantScope,
        _write: bool,
    ) -> Result<(), PortError> {
        if self.0 {
            Ok(())
        } else {
            Err(PortError::EntitlementDenied)
        }
    }
}

#[derive(Clone, Default)]
struct FakeWrite {
    contract: Option<LegalContract>,
    document: Option<LegalDocument>,
}

#[async_trait]
impl WriteRepository for FakeWrite {
    async fn load_contract(
        &self,
        _scope: &TenantScope,
        _id: LegalContractId,
    ) -> Result<Option<LegalContract>, PortError> {
        Ok(self.contract.clone())
    }

    async fn load_document(
        &self,
        _scope: &TenantScope,
        _id: LegalDocumentId,
    ) -> Result<Option<LegalDocument>, PortError> {
        Ok(self.document.clone())
    }
}

#[derive(Clone, Default)]
struct FakeUnitOfWork {
    commits: Arc<AtomicUsize>,
    request: Arc<Mutex<Option<LegalCommit>>>,
    stored: Arc<Mutex<Option<(CommandIdentity, StoredCommit)>>>,
}

#[async_trait]
impl CommandUnitOfWork for FakeUnitOfWork {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        let stored = self.stored.lock().unwrap();
        let Some((stored_identity, receipt)) = stored.as_ref() else {
            return Ok(None);
        };
        if stored_identity == identity {
            Ok(Some(receipt.clone()))
        } else if stored_identity.command_id == identity.command_id
            || stored_identity.idempotency_key == identity.idempotency_key
        {
            Err(PortError::IdempotencyConflict)
        } else {
            Ok(None)
        }
    }

    async fn commit(&self, request: LegalCommit) -> Result<StoredCommit, PortError> {
        self.commits.fetch_add(1, Ordering::SeqCst);
        *self.request.lock().unwrap() = Some(request.clone());
        let receipt = StoredCommit {
            command_id: request.identity.command_id,
            correlation_id: request.correlation_id,
            aggregate: request.aggregate,
            committed_events: Vec::new(),
            committed_at: at(),
            replayed: false,
        };
        *self.stored.lock().unwrap() = Some((request.identity, receipt.clone()));
        Ok(receipt)
    }
}

fn handler(
    write: FakeWrite,
    uow: FakeUnitOfWork,
    allowed: bool,
) -> LegalCommandHandler<FakeWrite, FakeUnitOfWork, FakeAuthorization, FakeEntitlement, FixedClock>
{
    LegalCommandHandler::new(
        write,
        uow,
        FakeAuthorization(allowed),
        FakeEntitlement(true),
        FixedClock::new(at()),
    )
}

#[tokio::test]
async fn mutation_requires_expected_version_before_atomic_commit() {
    let uow = FakeUnitOfWork::default();
    let error = handler(
        FakeWrite {
            contract: Some(contract()),
            document: None,
        },
        uow.clone(),
        true,
    )
    .handle(envelope(
        LegalCommand::AddContractParty(AddContractParty {
            contract_id: contract_id(),
            party: ContractParty::try_new(id(11), "supplier".into()).unwrap(),
        }),
        None,
        "legal-admin",
    ))
    .await
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn authorization_denial_prevents_any_commit() {
    let uow = FakeUnitOfWork::default();
    let error = handler(FakeWrite::default(), uow.clone(), false)
        .handle(envelope(
            LegalCommand::CreateDocument(CreateDocument {
                document_id: document_id(),
                scope: scope(),
                title: "Opinion".into(),
            }),
            None,
            "legal-admin",
        ))
        .await
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Denied);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn create_contract_commits_snapshot_receipt_audit_and_outbox_as_one_request() {
    let uow = FakeUnitOfWork::default();
    let receipt = handler(FakeWrite::default(), uow.clone(), true)
        .handle(envelope(
            LegalCommand::CreateContract(CreateContract {
                contract_id: contract_id(),
                scope: scope(),
                reference: "CTR-10".into(),
                title: "Agreement".into(),
                object_ref: "legal/contracts/10/v1".into(),
                content_sha256: digest('a'),
                retention_until: at() + Duration::days(30),
            }),
            None,
            "creator",
        ))
        .await
        .unwrap();
    assert_eq!(receipt.aggregate.context, "legal");
    assert_eq!(receipt.aggregate.aggregate_type, "legal_contract");
    let request = uow.request.lock().unwrap().clone().unwrap();
    assert_eq!(request.audit.purpose, "legal.test");
    assert_eq!(request.authority_scope, scope());
    assert!(request.integration_events.is_empty());
    assert_eq!(request.expected_version, None);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn stable_idempotency_replays_without_a_second_commit() {
    let uow = FakeUnitOfWork::default();
    let command = LegalCommand::CreateDocument(CreateDocument {
        document_id: document_id(),
        scope: scope(),
        title: "Opinion".into(),
    });
    let first = handler(FakeWrite::default(), uow.clone(), true)
        .handle(envelope(command.clone(), None, "legal-admin"))
        .await
        .unwrap();
    let second = handler(FakeWrite::default(), uow.clone(), true)
        .handle(envelope(command, None, "legal-admin"))
        .await
        .unwrap();
    assert!(!first.replayed);
    assert!(second.replayed);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn access_audit_is_bound_to_existing_document_version_and_committed_append_only() {
    let uow = FakeUnitOfWork::default();
    handler(
        FakeWrite {
            contract: None,
            document: Some(document()),
        },
        uow.clone(),
        true,
    )
    .handle(envelope(
        LegalCommand::RecordDocumentAccess(RecordDocumentAccess {
            access_entry_id: access_id(),
            document_id: document_id(),
            document_version: 1,
            action: "download".into(),
            purpose: "litigation review".into(),
            observed_at: at(),
        }),
        None,
        "reader",
    ))
    .await
    .unwrap();
    let request = uow.request.lock().unwrap().clone().unwrap();
    let LegalAggregate::AccessEntry(entry) = request.aggregate else {
        panic!("access entry")
    };
    assert_eq!(entry.version(), 1);
    assert_eq!(entry.actor_ref(), "reader");
    assert_eq!(entry.document_version(), 1);
}

#[tokio::test]
async fn access_audit_rejects_a_document_version_that_is_not_authoritative() {
    let uow = FakeUnitOfWork::default();
    let error = handler(
        FakeWrite {
            contract: None,
            document: Some(document()),
        },
        uow.clone(),
        true,
    )
    .handle(envelope(
        LegalCommand::RecordDocumentAccess(RecordDocumentAccess {
            access_entry_id: access_id(),
            document_id: document_id(),
            document_version: 2,
            action: "download".into(),
            purpose: "review".into(),
            observed_at: at(),
        }),
        None,
        "reader",
    ))
    .await
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Invalid);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
}

#[derive(Clone)]
struct FakeRead {
    consistency: Arc<Mutex<Option<QueryConsistency>>>,
    not_ready: bool,
}

impl Default for FakeRead {
    fn default() -> Self {
        Self {
            consistency: Arc::new(Mutex::new(None)),
            not_ready: false,
        }
    }
}

fn metadata() -> ProjectionMetadata {
    ProjectionMetadata {
        checkpoint: None,
        freshness: FreshnessState::Current,
        lag_millis: Some(0),
    }
}

#[async_trait]
impl ReadRepository for FakeRead {
    async fn get_contract(
        &self,
        _scope: &TenantScope,
        _id: LegalContractId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<ContractView>>, PortError> {
        *self.consistency.lock().unwrap() = Some(consistency.clone());
        Ok(ReadProjection {
            value: None,
            metadata: metadata(),
        })
    }
    async fn list_contracts(
        &self,
        _scope: &TenantScope,
        _filter: &ContractListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<ContractList>, PortError> {
        *self.consistency.lock().unwrap() = Some(consistency.clone());
        Ok(ReadProjection {
            value: ContractList {
                contracts: Vec::new(),
                next_cursor: None,
            },
            metadata: metadata(),
        })
    }
    async fn get_document(
        &self,
        _scope: &TenantScope,
        _id: LegalDocumentId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<DocumentView>>, PortError> {
        *self.consistency.lock().unwrap() = Some(consistency.clone());
        Ok(ReadProjection {
            value: None,
            metadata: metadata(),
        })
    }
    async fn list_documents(
        &self,
        _scope: &TenantScope,
        _filter: &DocumentListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<DocumentList>, PortError> {
        *self.consistency.lock().unwrap() = Some(consistency.clone());
        Ok(ReadProjection {
            value: DocumentList {
                documents: Vec::new(),
                next_cursor: None,
            },
            metadata: metadata(),
        })
    }
    async fn list_access_history(
        &self,
        _scope: &TenantScope,
        _filter: &AccessHistoryFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<AccessHistoryList>, PortError> {
        *self.consistency.lock().unwrap() = Some(consistency.clone());
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
            value: AccessHistoryList {
                entries: Vec::new(),
                next_cursor: None,
            },
            metadata: metadata(),
        })
    }
}

fn query_envelope(query: LegalQuery, consistency: QueryConsistency) -> QueryEnvelope<LegalQuery> {
    QueryEnvelope {
        contract_version: CONTRACT_VERSION,
        query_id: QueryId::try_from_uuid(id(200)).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(201)).unwrap(),
        actor: actor("legal-reader"),
        tenant_scope: scope(),
        audit: AuditContext {
            purpose: "legal.query".into(),
            customer_visible: false,
            module: Some("legal".into()),
        },
        consistency,
        query,
    }
}

#[tokio::test]
async fn read_your_writes_is_forwarded_to_projection_and_not_ready_never_falls_back() {
    let consistency = QueryConsistency::ReadYourWrites(ReadYourWrites {
        projection: "legal_access_history".into(),
        partition: "tenant-1".into(),
        target: ProjectionTarget {
            event_id: EventId::try_from_uuid(id(300)).unwrap(),
            global_sequence: 8,
        },
        timeout_millis: 100,
    });
    let read = FakeRead {
        consistency: Arc::new(Mutex::new(None)),
        not_ready: true,
    };
    let error =
        LegalQueryHandler::new(read.clone(), FakeAuthorization(true), FakeEntitlement(true))
            .handle(query_envelope(
                LegalQuery::ListAccessHistory(ListAccessHistory {
                    filter: AccessHistoryFilter {
                        document_id: Some(document_id()),
                        page_size: 20,
                        cursor: None,
                    },
                }),
                consistency.clone(),
            ))
            .await
            .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::NotReady);
    assert_eq!(*read.consistency.lock().unwrap(), Some(consistency));
}

#[tokio::test]
async fn invalid_page_size_is_rejected_before_projection_access() {
    let read = FakeRead::default();
    let error =
        LegalQueryHandler::new(read.clone(), FakeAuthorization(true), FakeEntitlement(true))
            .handle(query_envelope(
                LegalQuery::ListDocuments(ListDocuments {
                    filter: DocumentListFilter {
                        archived: None,
                        page_size: 0,
                        cursor: None,
                    },
                }),
                QueryConsistency::Eventual,
            ))
            .await
            .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Invalid);
    assert_eq!(*read.consistency.lock().unwrap(), None);
}
