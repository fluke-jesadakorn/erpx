use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use erpx_cqrs::{
    CONTRACT_VERSION, CommandEnvelope, CommandHandler, FreshnessState, IdempotencyKey,
    ProjectionMetadata, ProjectionNotReady, ProjectionTarget, QueryConsistency, QueryEnvelope,
    QueryHandler, ReadYourWrites,
};
use erpx_integration_contracts::{ReferenceCode, customer, marketing};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    FixedClock, QueryId, TenantScope,
};
use uuid::Uuid;

use super::*;
use crate::ports::{CrmCommit, ReadProjection, StoredCommit};

fn id(v: u128) -> Uuid {
    Uuid::from_u128(v)
}
fn at() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
}
fn scope() -> TenantScope {
    TenantScope::Company {
        holding_id: id(1),
        company_id: id(2),
    }
}
fn actor() -> ActorAttribution {
    ActorAttribution {
        principal_ref: "crm-test".into(),
        kind: ActorKind::TenantUser,
        session_id: None,
        represented_principal_ref: None,
        support_capability_id: None,
    }
}
fn envelope(command: CrmCommand, expected: Option<u64>) -> CommandEnvelope<CrmCommand> {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id: CommandId::try_from_uuid(id(10)).unwrap(),
        idempotency_key: IdempotencyKey::try_new("crm-stable-key").unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(11)).unwrap(),
        causation_id: None,
        actor: actor(),
        tenant_scope: scope(),
        audit: AuditContext {
            purpose: "test".into(),
            customer_visible: false,
            module: Some("crm".into()),
        },
        expected_aggregate_version: expected,
        command,
    }
}
fn lead() -> CrmLead {
    CrmLead::accept(
        CrmLeadId::try_from_uuid(id(20)).unwrap(),
        scope(),
        id(21),
        id(23),
        id(24),
        7,
        Some(id(22)),
        "campaign".into(),
        8_000,
        at(),
    )
    .unwrap()
}
fn opportunity() -> CrmOpportunity {
    CrmOpportunity::create(
        OpportunityId::try_from_uuid(id(30)).unwrap(),
        scope(),
        lead().id(),
        id(22),
        "Expansion".into(),
        50_000,
        "THB".into(),
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
    async fn ensure_crm_enabled(&self, _: &TenantScope, _: bool) -> Result<(), PortError> {
        Ok(())
    }
}
#[derive(Clone, Copy)]
struct QualifiedLeads;
#[async_trait]
impl QualifiedLeadResolverPort for QualifiedLeads {
    async fn resolve_qualified_lead(
        &self,
        request: QualifiedLeadResolveRequest<'_>,
    ) -> Result<QualifiedLeadEvidence, PortError> {
        Ok(QualifiedLeadEvidence {
            fact: marketing::QualifiedLeadFact {
                candidate: request.candidate,
                campaign: marketing::CampaignRef::try_from_uuid(id(51)).unwrap(),
                audience_member: marketing::AudienceMemberRef::try_from_uuid(id(52)).unwrap(),
                existing_customer: Some(customer::CustomerRef::try_from_uuid(id(22)).unwrap()),
                source: ReferenceCode::try_new("web").unwrap(),
                qualification_score_basis_points: 9_000,
            },
            source_version: 12,
        })
    }
}
#[derive(Clone)]
struct Write;
#[async_trait]
impl WriteRepository for Write {
    async fn load_lead(&self, _: &TenantScope, _: CrmLeadId) -> Result<Option<CrmLead>, PortError> {
        Ok(Some(lead()))
    }
    async fn load_opportunity(
        &self,
        _: &TenantScope,
        _: OpportunityId,
    ) -> Result<Option<CrmOpportunity>, PortError> {
        Ok(Some(opportunity()))
    }
}
#[derive(Clone)]
struct Uow {
    commits: Arc<AtomicUsize>,
}
#[async_trait]
impl CommandUnitOfWork for Uow {
    async fn find_receipt(&self, _: &CommandIdentity) -> Result<Option<StoredCommit>, PortError> {
        Ok(None)
    }
    async fn commit(&self, request: CrmCommit) -> Result<StoredCommit, PortError> {
        self.commits.fetch_add(1, Ordering::SeqCst);
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
) -> CrmCommandHandler<Write, Uow, Auth, Entitlement, FixedClock, QualifiedLeads> {
    CrmCommandHandler::new(
        Write,
        Uow { commits },
        Auth { allowed },
        Entitlement,
        FixedClock::new(at()),
    )
    .with_qualified_lead_resolver(QualifiedLeads)
}

#[tokio::test]
async fn opportunity_transition_requires_expected_version() {
    let commits = Arc::new(AtomicUsize::new(0));
    let result = handler(true, commits.clone())
        .handle(envelope(
            CrmCommand::TransitionOpportunity(TransitionOpportunity {
                opportunity_id: opportunity().id(),
                stage: OpportunityStage::Discovery,
            }),
            None,
        ))
        .await
        .unwrap_err();
    assert_eq!(result.category(), ErrorCategory::Conflict);
    assert_eq!(commits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn marketing_handoff_consumes_published_fact_without_marketing_core() {
    let commits = Arc::new(AtomicUsize::new(0));
    let fact = marketing::QualifiedLeadFact {
        candidate: marketing::LeadCandidateRef::try_from_uuid(id(50)).unwrap(),
        campaign: marketing::CampaignRef::try_from_uuid(id(51)).unwrap(),
        audience_member: marketing::AudienceMemberRef::try_from_uuid(id(52)).unwrap(),
        existing_customer: Some(customer::CustomerRef::try_from_uuid(id(22)).unwrap()),
        source: ReferenceCode::try_new("web").unwrap(),
        qualification_score_basis_points: 9_000,
    };
    let receipt = handler(true, commits.clone())
        .handle(envelope(
            CrmCommand::AcceptMarketingLead(AcceptMarketingLead {
                lead_id: CrmLeadId::try_from_uuid(id(53)).unwrap(),
                qualified: fact,
            }),
            None,
        ))
        .await
        .unwrap();
    assert!(matches!(receipt.response, CrmMutationResult::Lead(ref lead)
            if lead.marketing_candidate_id == id(50)
                && lead.campaign_id == id(51)
                && lead.audience_member_id == id(52)
                && lead.marketing_source_version == 12));
    assert_eq!(commits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn marketing_handoff_is_unavailable_without_owner_evidence_resolver() {
    let commits = Arc::new(AtomicUsize::new(0));
    let fact = marketing::QualifiedLeadFact {
        candidate: marketing::LeadCandidateRef::try_from_uuid(id(50)).unwrap(),
        campaign: marketing::CampaignRef::try_from_uuid(id(999)).unwrap(),
        audience_member: marketing::AudienceMemberRef::try_from_uuid(id(998)).unwrap(),
        existing_customer: None,
        source: ReferenceCode::try_new("untrusted-client").unwrap(),
        qualification_score_basis_points: 10_000,
    };
    let result = CrmCommandHandler::new(
        Write,
        Uow {
            commits: commits.clone(),
        },
        Auth { allowed: true },
        Entitlement,
        FixedClock::new(at()),
    )
    .handle(envelope(
        CrmCommand::AcceptMarketingLead(AcceptMarketingLead {
            lead_id: CrmLeadId::try_from_uuid(id(53)).unwrap(),
            qualified: fact,
        }),
        None,
    ))
    .await
    .unwrap_err();
    assert_eq!(result.category(), ErrorCategory::Unavailable);
    assert_eq!(commits.load(Ordering::SeqCst), 0);
}

struct NotReadyRead;
#[async_trait]
impl ReadRepository for NotReadyRead {
    async fn get_lead(
        &self,
        _: &TenantScope,
        _: CrmLeadId,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<Option<CrmLeadView>>, PortError> {
        unreachable!()
    }
    async fn get_opportunity(
        &self,
        _: &TenantScope,
        _: OpportunityId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<OpportunityView>>, PortError> {
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
    async fn list_opportunities(
        &self,
        _: &TenantScope,
        _: &OpportunityListFilter,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<OpportunityList>, PortError> {
        Ok(ReadProjection {
            value: OpportunityList {
                opportunities: vec![],
                next_cursor: None,
            },
            metadata: ProjectionMetadata {
                checkpoint: None,
                freshness: FreshnessState::Current,
                lag_millis: None,
            },
        })
    }
}

#[tokio::test]
async fn projection_not_ready_does_not_fall_back_to_write_repository() {
    let target = ProjectionTarget {
        event_id: EventId::try_from_uuid(id(60)).unwrap(),
        global_sequence: 10,
    };
    let query = QueryEnvelope {
        contract_version: CONTRACT_VERSION,
        query_id: QueryId::try_from_uuid(id(61)).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(62)).unwrap(),
        actor: actor(),
        tenant_scope: scope(),
        audit: AuditContext {
            purpose: "test".into(),
            customer_visible: false,
            module: Some("crm".into()),
        },
        consistency: QueryConsistency::ReadYourWrites(ReadYourWrites {
            projection: "crm.opportunities".into(),
            partition: "tenant-1".into(),
            target,
            timeout_millis: 100,
        }),
        query: CrmQuery::GetOpportunity(GetOpportunity {
            opportunity_id: opportunity().id(),
        }),
    };
    let result = CrmQueryHandler::new(NotReadyRead, Auth { allowed: true }, Entitlement)
        .handle(query)
        .await
        .unwrap_err();
    assert_eq!(result.category(), ErrorCategory::NotReady);
}

#[tokio::test]
async fn authorization_is_checked_before_commit() {
    let commits = Arc::new(AtomicUsize::new(0));
    let result = handler(false, commits.clone())
        .handle(envelope(
            CrmCommand::TransitionOpportunity(TransitionOpportunity {
                opportunity_id: opportunity().id(),
                stage: OpportunityStage::Discovery,
            }),
            Some(1),
        ))
        .await
        .unwrap_err();
    assert_eq!(result.category(), ErrorCategory::Denied);
    assert_eq!(commits.load(Ordering::SeqCst), 0);
}
