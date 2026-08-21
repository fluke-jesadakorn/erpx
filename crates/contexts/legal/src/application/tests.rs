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
use erpx_integration_contracts::{BoundedContext, IntegrationEvent, RecordRef};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    FixedClock, QueryId, SessionId,
};
use uuid::Uuid;

use super::*;
use crate::domain::ContractStatus;
use crate::ports::{
    CommandIdentity, ContractListFilter, DocumentListFilter, LegalCommit, ReadProjection,
};

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}
fn at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
}
fn scope() -> AuthorityScope {
    AuthorityScope::Company {
        holding_id: id(1),
        company_id: id(2),
    }
}
fn actor(name: &str) -> ActorAttribution {
    ActorAttribution {
        principal_ref: name.into(),
        kind: ActorKind::HoldingUser,
        session_id: Some(SessionId::try_from_uuid(id(102)).unwrap()),
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
fn matter_id() -> LegalMatterId {
    LegalMatterId::try_from_uuid(id(40)).unwrap()
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
        active_scope: scope(),
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

fn review_contract() -> LegalContract {
    let mut value = contract();
    value
        .transition(ContractTransition::SubmitForReview, "creator", at())
        .unwrap();
    value
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

fn matter() -> LegalMatter {
    LegalMatter::create(
        matter_id(),
        scope(),
        "MAT-40".into(),
        "Customer dispute".into(),
        "legal-owner".into(),
        at(),
    )
    .unwrap()
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

#[derive(Clone, Default)]
struct FakeWrite {
    contract: Option<LegalContract>,
    document: Option<LegalDocument>,
    matter: Option<LegalMatter>,
    contract_loads: Arc<AtomicUsize>,
}

#[async_trait]
impl WriteRepository for FakeWrite {
    async fn load_contract(
        &self,
        _scope: &AuthorityScope,
        _id: LegalContractId,
    ) -> Result<Option<LegalContract>, PortError> {
        self.contract_loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.contract.clone())
    }

    async fn load_document(
        &self,
        _scope: &AuthorityScope,
        _id: LegalDocumentId,
    ) -> Result<Option<LegalDocument>, PortError> {
        Ok(self.document.clone())
    }

    async fn load_matter(
        &self,
        _scope: &AuthorityScope,
        _id: LegalMatterId,
    ) -> Result<Option<LegalMatter>, PortError> {
        Ok(self.matter.clone())
    }
}

#[derive(Clone, Default)]
struct FakeUnitOfWork {
    commits: Arc<AtomicUsize>,
    receipt_lookups: Arc<AtomicUsize>,
    request: Arc<Mutex<Option<LegalCommit>>>,
    stored: Arc<Mutex<Option<(CommandIdentity, StoredCommit)>>>,
}

#[async_trait]
impl CommandUnitOfWork for FakeUnitOfWork {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        self.receipt_lookups.fetch_add(1, Ordering::SeqCst);
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
            matter: None,
            contract_loads: Default::default(),
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
async fn contract_approval_rejects_non_humans_before_receipt_or_contract_repository_access() {
    for kind in [
        ActorKind::Agent,
        ActorKind::Service,
        ActorKind::SystemWorker,
    ] {
        let contract_loads = Arc::new(AtomicUsize::new(0));
        let uow = FakeUnitOfWork::default();
        let contract = review_contract();
        let mut request = envelope(
            LegalCommand::TransitionContract(TransitionContract {
                contract_id: contract_id(),
                transition: ContractTransition::Approve,
            }),
            Some(contract.version()),
            "non-human",
        );
        request.actor.kind = kind;

        let error = handler(
            FakeWrite {
                contract: Some(contract),
                document: None,
                matter: None,
                contract_loads: contract_loads.clone(),
            },
            uow.clone(),
            true,
        )
        .handle(request)
        .await
        .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Denied);
        assert_eq!(uow.receipt_lookups.load(Ordering::SeqCst), 0);
        assert_eq!(contract_loads.load(Ordering::SeqCst), 0);
        assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn contract_approval_derives_human_approver_and_enforces_creator_separation() {
    let review = review_contract();
    let creator_uow = FakeUnitOfWork::default();
    let creator_error = handler(
        FakeWrite {
            contract: Some(review.clone()),
            document: None,
            matter: None,
            contract_loads: Default::default(),
        },
        creator_uow.clone(),
        true,
    )
    .handle(envelope(
        LegalCommand::TransitionContract(TransitionContract {
            contract_id: contract_id(),
            transition: ContractTransition::Approve,
        }),
        Some(review.version()),
        "creator",
    ))
    .await
    .unwrap_err();
    assert_eq!(creator_error.category(), ErrorCategory::Invalid);
    assert_eq!(creator_uow.commits.load(Ordering::SeqCst), 0);

    let reviewer_uow = FakeUnitOfWork::default();
    handler(
        FakeWrite {
            contract: Some(review.clone()),
            document: None,
            matter: None,
            contract_loads: Default::default(),
        },
        reviewer_uow.clone(),
        true,
    )
    .handle(envelope(
        LegalCommand::TransitionContract(TransitionContract {
            contract_id: contract_id(),
            transition: ContractTransition::Approve,
        }),
        Some(review.version()),
        "reviewer",
    ))
    .await
    .unwrap();
    let request = reviewer_uow.request.lock().unwrap().clone().unwrap();
    let LegalAggregate::Contract(approved) = request.aggregate else {
        panic!("contract commit expected")
    };
    assert_eq!(approved.status(), ContractStatus::Approved);
    assert_eq!(approved.approved_by(), Some("reviewer"));
    assert_eq!(request.actor.principal_ref, "reviewer");
}

#[tokio::test]
async fn non_approval_contract_transition_preserves_existing_agent_behavior() {
    let contract = contract();
    let uow = FakeUnitOfWork::default();
    let mut request = envelope(
        LegalCommand::TransitionContract(TransitionContract {
            contract_id: contract_id(),
            transition: ContractTransition::SubmitForReview,
        }),
        Some(contract.version()),
        "workflow-agent",
    );
    request.actor.kind = ActorKind::Agent;
    handler(
        FakeWrite {
            contract: Some(contract),
            document: None,
            matter: None,
            contract_loads: Default::default(),
        },
        uow.clone(),
        true,
    )
    .handle(request)
    .await
    .unwrap();
    let request = uow.request.lock().unwrap().clone().unwrap();
    let LegalAggregate::Contract(review) = request.aggregate else {
        panic!("contract commit expected")
    };
    assert_eq!(review.status(), ContractStatus::Review);
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
async fn create_matter_publishes_typed_graph_fact_without_copying_source_records() {
    let uow = FakeUnitOfWork::default();
    handler(FakeWrite::default(), uow.clone(), true)
        .handle(envelope(
            LegalCommand::CreateMatter(CreateMatter {
                matter_id: matter_id(),
                scope: scope(),
                reference: "MAT-40".into(),
                title: "Customer dispute".into(),
            }),
            None,
            "legal-owner",
        ))
        .await
        .unwrap();
    let request = uow.request.lock().unwrap().clone().unwrap();
    assert_eq!(request.aggregate.aggregate_type(), "legal_matter");
    assert_eq!(request.integration_events.len(), 1);
    assert_eq!(
        request.integration_events[0].event_type(),
        "legal.matter_changed"
    );
}

#[tokio::test]
async fn company_authority_cannot_link_a_sibling_company_record() {
    let uow = FakeUnitOfWork::default();
    let error = handler(
        FakeWrite {
            contract: None,
            document: None,
            matter: Some(matter()),
            contract_loads: Default::default(),
        },
        uow.clone(),
        true,
    )
    .handle(envelope(
        LegalCommand::LinkMatterRecord(LinkMatterRecord {
            matter_id: matter_id(),
            relation: RecordRelation::Disputes,
            target: RecordRef::try_new(BoundedContext::Finance, "customer_invoice", id(41), 3)
                .unwrap(),
            target_scope: AuthorityScope::Company {
                holding_id: id(1),
                company_id: id(3),
            },
            target_digest: digest('c'),
        }),
        Some(1),
        "legal-owner",
    ))
    .await
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Denied);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
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
            matter: None,
            contract_loads: Default::default(),
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
            matter: None,
            contract_loads: Default::default(),
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
        _scope: &AuthorityScope,
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
        _scope: &AuthorityScope,
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
        _scope: &AuthorityScope,
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
        _scope: &AuthorityScope,
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
        _scope: &AuthorityScope,
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

    async fn get_matter(
        &self,
        _scope: &AuthorityScope,
        _id: LegalMatterId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<MatterView>>, PortError> {
        *self.consistency.lock().unwrap() = Some(consistency.clone());
        Ok(ReadProjection {
            value: None,
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
        active_scope: scope(),
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
        partition: "holding-1".into(),
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
