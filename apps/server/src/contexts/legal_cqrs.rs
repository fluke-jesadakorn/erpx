//! Typed, storage-agnostic gRPC inbound adapter for Legal CQRS.
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandEnvelope, CommandReceipt, CqrsError, FreshnessState, IdempotencyKey, QueryConsistency,
    QueryEnvelope, QueryMetadata, QueryResult, ReadYourWrites,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    QueryId, SessionId, TenantScope,
};
use erpx_legal::ports::{
    AccessEntryView, AccessHistoryFilter, ContractListFilter, ContractView, DocumentListFilter,
    DocumentView,
};
use erpx_legal::{
    AccessEntryId, AddContractParty, AddDocumentVersion, ArchiveDocument, ContractParty,
    ContractStatus, ContractTransition, CreateContract, CreateDocument, GetContract, GetDocument,
    LegalCommand, LegalContractId, LegalDocumentId, LegalMutationResult, LegalQuery,
    LegalQueryResponse, ListAccessHistory, ListContracts, ListDocuments, RecordDocumentAccess,
    TransitionContract,
};
use erpx_proto::{common, erpx::legal::v2 as platform};
use platform::{
    legal_command_service_server::LegalCommandService,
    legal_query_service_server::LegalQueryService,
};
use std::sync::Arc;
use tonic::{Request, Response, Status, metadata::MetadataMap};
use uuid::Uuid;
const MAX_ROUTE_LENGTH: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalTenantRoute {
    pub tenant_id: String,
    pub holding_ref: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegalAccessMode {
    Read,
    Write,
}
pub struct LegalAuthenticationRequest {
    transport_metadata: MetadataMap,
    presented_actor: ActorAttribution,
    presented_scope: TenantScope,
    presented_route: LegalTenantRoute,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
    access_mode: LegalAccessMode,
}
impl LegalAuthenticationRequest {
    pub fn transport_metadata(&self) -> &MetadataMap {
        &self.transport_metadata
    }
    pub const fn presented_actor(&self) -> &ActorAttribution {
        &self.presented_actor
    }
    pub const fn presented_scope(&self) -> &TenantScope {
        &self.presented_scope
    }
    pub const fn presented_route(&self) -> &LegalTenantRoute {
        &self.presented_route
    }
    pub fn subscription_id(&self) -> Option<&str> {
        self.subscription_id.as_deref()
    }
    pub fn mfa_code(&self) -> Option<&str> {
        self.mfa_code.as_deref()
    }
    pub const fn access_mode(&self) -> LegalAccessMode {
        self.access_mode
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedLegalRequest {
    actor: ActorAttribution,
    tenant_scope: TenantScope,
    route: LegalTenantRoute,
}
impl AuthenticatedLegalRequest {
    pub fn try_new(
        actor: ActorAttribution,
        tenant_scope: TenantScope,
        route: LegalTenantRoute,
    ) -> Result<Self, LegalAuthenticationError> {
        actor
            .validate()
            .map_err(|_| LegalAuthenticationError::Internal)?;
        tenant_scope
            .validate()
            .map_err(|_| LegalAuthenticationError::Internal)?;
        validate_route(&route).map_err(|_| LegalAuthenticationError::Internal)?;
        validate_route_scope(&route, &tenant_scope)
            .map_err(|_| LegalAuthenticationError::Internal)?;
        Ok(Self {
            actor,
            tenant_scope,
            route,
        })
    }
    pub const fn actor(&self) -> &ActorAttribution {
        &self.actor
    }
    pub const fn tenant_scope(&self) -> &TenantScope {
        &self.tenant_scope
    }
    pub const fn route(&self) -> &LegalTenantRoute {
        &self.route
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegalAuthenticationError {
    Unauthenticated,
    Denied,
    Unavailable,
    Internal,
}
#[async_trait]
pub trait LegalRequestAuthenticator: Send + Sync + 'static {
    async fn authenticate(
        &self,
        request: LegalAuthenticationRequest,
    ) -> Result<AuthenticatedLegalRequest, LegalAuthenticationError>;
}
#[async_trait]
pub trait LegalCommandApplication: Send + Sync + 'static {
    async fn execute(
        &self,
        route: LegalTenantRoute,
        command: CommandEnvelope<LegalCommand>,
    ) -> Result<CommandReceipt<LegalMutationResult>, CqrsError>;
}
#[async_trait]
pub trait LegalQueryApplication: Send + Sync + 'static {
    async fn execute(
        &self,
        route: LegalTenantRoute,
        query: QueryEnvelope<LegalQuery>,
    ) -> Result<QueryResult<LegalQueryResponse>, CqrsError>;
}
#[derive(Debug, Clone)]
pub struct LegalCommandRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}
impl<A, G> LegalCommandRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}
#[derive(Debug, Clone)]
pub struct LegalQueryRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}
impl<A, G> LegalQueryRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

#[async_trait]
impl<A: LegalCommandApplication, G: LegalRequestAuthenticator> LegalCommandService
    for LegalCommandRpc<A, G>
{
    async fn create_contract(
        &self,
        request: Request<platform::CreateLegalContractRequest>,
    ) -> Result<Response<platform::LegalContractMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let v = request.into_inner();
        let receipt = self
            .command(
                transport,
                v.metadata,
                LegalCommand::CreateContract(CreateContract {
                    contract_id: contract_id(&v.contract_id)?,
                    scope: decode_scope(
                        v.target_scope
                            .ok_or_else(|| invalid("target_scope is required"))?,
                    )?,
                    reference: v.reference,
                    title: v.title,
                    object_ref: v.object_ref,
                    content_sha256: v.content_sha256,
                    retention_until: timestamp(&v.retention_until, "retention_until")?,
                }),
            )
            .await?;
        contract_mutation(receipt)
    }
    async fn add_contract_party(
        &self,
        request: Request<platform::AddLegalContractPartyRequest>,
    ) -> Result<Response<platform::LegalContractMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let v = request.into_inner();
        let p = v.party.ok_or_else(|| invalid("party is required"))?;
        let receipt = self
            .command(
                transport,
                v.metadata,
                LegalCommand::AddContractParty(AddContractParty {
                    contract_id: contract_id(&v.contract_id)?,
                    party: ContractParty::try_new(parse_uuid(&p.party_id, "party_id")?, p.role)
                        .map_err(|_| invalid("party is invalid"))?,
                }),
            )
            .await?;
        contract_mutation(receipt)
    }
    async fn transition_contract(
        &self,
        request: Request<platform::TransitionLegalContractRequest>,
    ) -> Result<Response<platform::LegalContractMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let v = request.into_inner();
        let receipt = self
            .command(
                transport,
                v.metadata,
                LegalCommand::TransitionContract(TransitionContract {
                    contract_id: contract_id(&v.contract_id)?,
                    transition: decode_transition(v.transition)?,
                }),
            )
            .await?;
        contract_mutation(receipt)
    }
    async fn create_document(
        &self,
        request: Request<platform::CreateLegalDocumentRequest>,
    ) -> Result<Response<platform::LegalDocumentMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let v = request.into_inner();
        let receipt = self
            .command(
                transport,
                v.metadata,
                LegalCommand::CreateDocument(CreateDocument {
                    document_id: document_id(&v.document_id)?,
                    scope: decode_scope(
                        v.target_scope
                            .ok_or_else(|| invalid("target_scope is required"))?,
                    )?,
                    title: v.title,
                }),
            )
            .await?;
        document_mutation(receipt)
    }
    async fn add_document_version(
        &self,
        request: Request<platform::AddLegalDocumentVersionRequest>,
    ) -> Result<Response<platform::LegalDocumentMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let v = request.into_inner();
        let receipt = self
            .command(
                transport,
                v.metadata,
                LegalCommand::AddDocumentVersion(AddDocumentVersion {
                    document_id: document_id(&v.document_id)?,
                    object_ref: v.object_ref,
                    content_sha256: v.content_sha256,
                }),
            )
            .await?;
        document_mutation(receipt)
    }
    async fn archive_document(
        &self,
        request: Request<platform::ArchiveLegalDocumentRequest>,
    ) -> Result<Response<platform::LegalDocumentMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let v = request.into_inner();
        let receipt = self
            .command(
                transport,
                v.metadata,
                LegalCommand::ArchiveDocument(ArchiveDocument {
                    document_id: document_id(&v.document_id)?,
                }),
            )
            .await?;
        document_mutation(receipt)
    }
    async fn record_document_access(
        &self,
        request: Request<platform::RecordLegalDocumentAccessRequest>,
    ) -> Result<Response<platform::LegalAccessMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let v = request.into_inner();
        let receipt = self
            .command(
                transport,
                v.metadata,
                LegalCommand::RecordDocumentAccess(RecordDocumentAccess {
                    access_entry_id: access_id(&v.access_entry_id)?,
                    document_id: document_id(&v.document_id)?,
                    document_version: v.document_version,
                    action: v.action,
                    purpose: v.purpose,
                    observed_at: timestamp(&v.observed_at, "observed_at")?,
                }),
            )
            .await?;
        access_mutation(receipt)
    }
}
impl<A: LegalCommandApplication, G: LegalRequestAuthenticator> LegalCommandRpc<A, G> {
    async fn command(
        &self,
        transport: MetadataMap,
        metadata: Option<common::CommandRequestMetadata>,
        command: LegalCommand,
    ) -> Result<CommandReceipt<LegalMutationResult>, Status> {
        let decoded = decode_command(metadata, command)?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        self.application
            .execute(route, envelope)
            .await
            .map_err(status_from_cqrs)
    }
}

#[async_trait]
impl<A: LegalQueryApplication, G: LegalRequestAuthenticator> LegalQueryService
    for LegalQueryRpc<A, G>
{
    async fn get_contract(
        &self,
        request: Request<platform::GetLegalContractRequest>,
    ) -> Result<Response<platform::GetLegalContractResponse>, Status> {
        let t = request.metadata().clone();
        let v = request.into_inner();
        let r = self
            .query(
                t,
                v.metadata,
                LegalQuery::GetContract(GetContract {
                    contract_id: contract_id(&v.contract_id)?,
                }),
            )
            .await?;
        let LegalQueryResponse::Contract(value) = r.value else {
            return Err(response_mismatch());
        };
        Ok(Response::new(platform::GetLegalContractResponse {
            contract: value.as_ref().map(encode_contract),
            metadata: Some(encode_query_metadata(&r.metadata)),
        }))
    }
    async fn list_contracts(
        &self,
        request: Request<platform::ListTypedLegalContractsRequest>,
    ) -> Result<Response<platform::ListTypedLegalContractsResponse>, Status> {
        let t = request.metadata().clone();
        let v = request.into_inner();
        let r = self
            .query(
                t,
                v.metadata,
                LegalQuery::ListContracts(ListContracts {
                    filter: ContractListFilter {
                        status: v.status.map(decode_status).transpose()?,
                        page_size: page_size(v.page_size)?,
                        cursor: v.cursor,
                    },
                }),
            )
            .await?;
        let LegalQueryResponse::Contracts(value) = r.value else {
            return Err(response_mismatch());
        };
        Ok(Response::new(platform::ListTypedLegalContractsResponse {
            contracts: value.contracts.iter().map(encode_contract).collect(),
            next_cursor: value.next_cursor,
            metadata: Some(encode_query_metadata(&r.metadata)),
        }))
    }
    async fn get_document(
        &self,
        request: Request<platform::GetLegalDocumentRequest>,
    ) -> Result<Response<platform::GetLegalDocumentResponse>, Status> {
        let t = request.metadata().clone();
        let v = request.into_inner();
        let r = self
            .query(
                t,
                v.metadata,
                LegalQuery::GetDocument(GetDocument {
                    document_id: document_id(&v.document_id)?,
                }),
            )
            .await?;
        let LegalQueryResponse::Document(value) = r.value else {
            return Err(response_mismatch());
        };
        Ok(Response::new(platform::GetLegalDocumentResponse {
            document: value.as_ref().map(encode_document),
            metadata: Some(encode_query_metadata(&r.metadata)),
        }))
    }
    async fn list_documents(
        &self,
        request: Request<platform::ListTypedLegalDocumentsRequest>,
    ) -> Result<Response<platform::ListTypedLegalDocumentsResponse>, Status> {
        let t = request.metadata().clone();
        let v = request.into_inner();
        let r = self
            .query(
                t,
                v.metadata,
                LegalQuery::ListDocuments(ListDocuments {
                    filter: DocumentListFilter {
                        archived: v.archived,
                        page_size: page_size(v.page_size)?,
                        cursor: v.cursor,
                    },
                }),
            )
            .await?;
        let LegalQueryResponse::Documents(value) = r.value else {
            return Err(response_mismatch());
        };
        Ok(Response::new(platform::ListTypedLegalDocumentsResponse {
            documents: value.documents.iter().map(encode_document).collect(),
            next_cursor: value.next_cursor,
            metadata: Some(encode_query_metadata(&r.metadata)),
        }))
    }
    async fn list_access_history(
        &self,
        request: Request<platform::ListTypedLegalAccessHistoryRequest>,
    ) -> Result<Response<platform::ListTypedLegalAccessHistoryResponse>, Status> {
        let t = request.metadata().clone();
        let v = request.into_inner();
        let r = self
            .query(
                t,
                v.metadata,
                LegalQuery::ListAccessHistory(ListAccessHistory {
                    filter: AccessHistoryFilter {
                        document_id: v.document_id.as_deref().map(document_id).transpose()?,
                        page_size: page_size(v.page_size)?,
                        cursor: v.cursor,
                    },
                }),
            )
            .await?;
        let LegalQueryResponse::AccessHistory(value) = r.value else {
            return Err(response_mismatch());
        };
        Ok(Response::new(
            platform::ListTypedLegalAccessHistoryResponse {
                access_entries: value.entries.iter().map(encode_access).collect(),
                next_cursor: value.next_cursor,
                metadata: Some(encode_query_metadata(&r.metadata)),
            },
        ))
    }
}
impl<A: LegalQueryApplication, G: LegalRequestAuthenticator> LegalQueryRpc<A, G> {
    async fn query(
        &self,
        transport: MetadataMap,
        metadata: Option<common::QueryRequestMetadata>,
        query: LegalQuery,
    ) -> Result<QueryResult<LegalQueryResponse>, Status> {
        let decoded = decode_query(metadata, query)?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), transport, decoded).await?;
        self.application
            .execute(route, envelope)
            .await
            .map_err(status_from_cqrs)
    }
}

fn contract_mutation(
    r: CommandReceipt<LegalMutationResult>,
) -> Result<Response<platform::LegalContractMutationResponse>, Status> {
    let LegalMutationResult::Contract(v) = &r.response else {
        return Err(response_mismatch());
    };
    Ok(Response::new(platform::LegalContractMutationResponse {
        receipt: Some(encode_receipt(&r)),
        contract: Some(encode_contract(v)),
    }))
}
fn document_mutation(
    r: CommandReceipt<LegalMutationResult>,
) -> Result<Response<platform::LegalDocumentMutationResponse>, Status> {
    let LegalMutationResult::Document(v) = &r.response else {
        return Err(response_mismatch());
    };
    Ok(Response::new(platform::LegalDocumentMutationResponse {
        receipt: Some(encode_receipt(&r)),
        document: Some(encode_document(v)),
    }))
}
fn access_mutation(
    r: CommandReceipt<LegalMutationResult>,
) -> Result<Response<platform::LegalAccessMutationResponse>, Status> {
    let LegalMutationResult::AccessEntry(v) = &r.response else {
        return Err(response_mismatch());
    };
    Ok(Response::new(platform::LegalAccessMutationResponse {
        receipt: Some(encode_receipt(&r)),
        access_entry: Some(encode_access(v)),
    }))
}

struct DecodedCommand<C> {
    route: LegalTenantRoute,
    envelope: CommandEnvelope<C>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
}
struct DecodedQuery<Q> {
    route: LegalTenantRoute,
    envelope: QueryEnvelope<Q>,
}
async fn authenticate_command<C, G: LegalRequestAuthenticator>(
    auth: &G,
    transport: MetadataMap,
    decoded: DecodedCommand<C>,
) -> Result<(LegalTenantRoute, CommandEnvelope<C>), Status> {
    let authenticated = auth
        .authenticate(LegalAuthenticationRequest {
            transport_metadata: transport,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.tenant_scope.clone(),
            presented_route: decoded.route,
            subscription_id: decoded.subscription_id,
            mfa_code: decoded.mfa_code,
            access_mode: LegalAccessMode::Write,
        })
        .await
        .map_err(status_from_auth)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.tenant_scope = authenticated.tenant_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated Legal authority is invalid"))?;
    Ok((authenticated.route().clone(), envelope))
}
async fn authenticate_query<Q, G: LegalRequestAuthenticator>(
    auth: &G,
    transport: MetadataMap,
    decoded: DecodedQuery<Q>,
) -> Result<(LegalTenantRoute, QueryEnvelope<Q>), Status> {
    let authenticated = auth
        .authenticate(LegalAuthenticationRequest {
            transport_metadata: transport,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.tenant_scope.clone(),
            presented_route: decoded.route,
            subscription_id: None,
            mfa_code: None,
            access_mode: LegalAccessMode::Read,
        })
        .await
        .map_err(status_from_auth)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.tenant_scope = authenticated.tenant_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated Legal authority is invalid"))?;
    Ok((authenticated.route().clone(), envelope))
}
fn decode_command<C>(
    metadata: Option<common::CommandRequestMetadata>,
    command: C,
) -> Result<DecodedCommand<C>, Status> {
    let m = metadata.ok_or_else(|| invalid("command metadata is required"))?;
    let route = decode_route(m.tenant_route)?;
    let envelope = CommandEnvelope {
        contract_version: u16::try_from(m.contract_version)
            .map_err(|_| invalid("contract_version is invalid"))?,
        command_id: CommandId::try_from_uuid(parse_uuid(&m.command_id, "command_id")?)
            .map_err(|_| invalid("command_id is invalid"))?,
        idempotency_key: IdempotencyKey::try_new(m.idempotency_key)
            .map_err(|_| invalid("idempotency_key is invalid"))?,
        correlation_id: correlation(&m.correlation_id)?,
        causation_id: m.causation_id.as_deref().map(event_id).transpose()?,
        actor: decode_actor(m.actor)?,
        tenant_scope: decode_scope(
            m.tenant_scope
                .ok_or_else(|| invalid("tenant_scope is required"))?,
        )?,
        audit: decode_audit(m.audit)?,
        expected_aggregate_version: m.expected_aggregate_version,
        command,
    };
    envelope
        .validate()
        .map_err(|_| invalid("command metadata is invalid"))?;
    Ok(DecodedCommand {
        route,
        envelope,
        subscription_id: clean(m.subscription_id),
        mfa_code: clean(m.mfa_code),
    })
}
fn decode_query<Q>(
    metadata: Option<common::QueryRequestMetadata>,
    query: Q,
) -> Result<DecodedQuery<Q>, Status> {
    let m = metadata.ok_or_else(|| invalid("query metadata is required"))?;
    let route = decode_route(m.tenant_route)?;
    let envelope = QueryEnvelope {
        contract_version: u16::try_from(m.contract_version)
            .map_err(|_| invalid("contract_version is invalid"))?,
        query_id: QueryId::try_from_uuid(parse_uuid(&m.query_id, "query_id")?)
            .map_err(|_| invalid("query_id is invalid"))?,
        correlation_id: correlation(&m.correlation_id)?,
        actor: decode_actor(m.actor)?,
        tenant_scope: decode_scope(
            m.tenant_scope
                .ok_or_else(|| invalid("tenant_scope is required"))?,
        )?,
        audit: decode_audit(m.audit)?,
        consistency: decode_consistency(m.consistency)?,
        query,
    };
    envelope
        .validate()
        .map_err(|_| invalid("query metadata is invalid"))?;
    Ok(DecodedQuery { route, envelope })
}

fn encode_contract(v: &ContractView) -> platform::LegalContractRecord {
    platform::LegalContractRecord {
        contract_id: v.contract_id.as_uuid().to_string(),
        source_version: v.version,
        scope: Some(encode_scope(&v.scope)),
        reference: v.reference.clone(),
        title: v.title.clone(),
        object_ref: v.object_ref.clone(),
        content_sha256: v.content_sha256.clone(),
        retention_until: v.retention_until.to_rfc3339(),
        status: encode_status(v.status),
        parties: v
            .parties
            .iter()
            .map(|p| platform::LegalContractPartyRecord {
                party_id: p.party_id.to_string(),
                role: p.role.clone(),
            })
            .collect(),
        created_by: v.created_by.clone(),
        approved_by: v.approved_by.clone(),
        created_at: v.created_at.to_rfc3339(),
        updated_at: v.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}
fn encode_document(v: &DocumentView) -> platform::LegalDocumentRecord {
    platform::LegalDocumentRecord {
        document_id: v.document_id.as_uuid().to_string(),
        source_version: v.version,
        scope: Some(encode_scope(&v.scope)),
        title: v.title.clone(),
        versions: v
            .versions
            .iter()
            .map(|x| platform::LegalDocumentVersionRecord {
                version: x.version,
                object_ref: x.object_ref.clone(),
                content_sha256: x.content_sha256.clone(),
                created_at: x.created_at.to_rfc3339(),
                created_by: x.created_by.clone(),
            })
            .collect(),
        current_version: v.current_version,
        archived: v.archived,
        created_at: v.created_at.to_rfc3339(),
        updated_at: v.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}
fn encode_access(v: &AccessEntryView) -> platform::LegalAccessEntryRecord {
    platform::LegalAccessEntryRecord {
        access_entry_id: v.access_entry_id.as_uuid().to_string(),
        source_version: v.version,
        scope: Some(encode_scope(&v.scope)),
        document_id: v.document_id.as_uuid().to_string(),
        document_version: v.document_version,
        actor_ref: v.actor_ref.clone(),
        action: v.action.clone(),
        purpose: v.purpose.clone(),
        observed_at: v.observed_at.to_rfc3339(),
        projected_at: String::new(),
    }
}
fn decode_transition(v: i32) -> Result<ContractTransition, Status> {
    match platform::LegalContractTransition::try_from(v).ok() {
        Some(platform::LegalContractTransition::SubmitForReview) => {
            Ok(ContractTransition::SubmitForReview)
        }
        Some(platform::LegalContractTransition::Approve) => Ok(ContractTransition::Approve),
        Some(platform::LegalContractTransition::Activate) => Ok(ContractTransition::Activate),
        Some(platform::LegalContractTransition::Expire) => Ok(ContractTransition::Expire),
        Some(platform::LegalContractTransition::Terminate) => Ok(ContractTransition::Terminate),
        Some(platform::LegalContractTransition::Archive) => Ok(ContractTransition::Archive),
        _ => Err(invalid("contract transition is invalid")),
    }
}
fn decode_status(v: i32) -> Result<ContractStatus, Status> {
    match platform::LegalContractStatus::try_from(v).ok() {
        Some(platform::LegalContractStatus::Draft) => Ok(ContractStatus::Draft),
        Some(platform::LegalContractStatus::Review) => Ok(ContractStatus::Review),
        Some(platform::LegalContractStatus::Approved) => Ok(ContractStatus::Approved),
        Some(platform::LegalContractStatus::Active) => Ok(ContractStatus::Active),
        Some(platform::LegalContractStatus::Expired) => Ok(ContractStatus::Expired),
        Some(platform::LegalContractStatus::Terminated) => Ok(ContractStatus::Terminated),
        Some(platform::LegalContractStatus::Archived) => Ok(ContractStatus::Archived),
        _ => Err(invalid("contract status is invalid")),
    }
}
fn encode_status(v: ContractStatus) -> i32 {
    match v {
        ContractStatus::Draft => platform::LegalContractStatus::Draft as i32,
        ContractStatus::Review => platform::LegalContractStatus::Review as i32,
        ContractStatus::Approved => platform::LegalContractStatus::Approved as i32,
        ContractStatus::Active => platform::LegalContractStatus::Active as i32,
        ContractStatus::Expired => platform::LegalContractStatus::Expired as i32,
        ContractStatus::Terminated => platform::LegalContractStatus::Terminated as i32,
        ContractStatus::Archived => platform::LegalContractStatus::Archived as i32,
    }
}
fn contract_id(v: &str) -> Result<LegalContractId, Status> {
    LegalContractId::try_from_uuid(parse_uuid(v, "contract_id")?)
        .map_err(|_| invalid("contract_id is invalid"))
}
fn document_id(v: &str) -> Result<LegalDocumentId, Status> {
    LegalDocumentId::try_from_uuid(parse_uuid(v, "document_id")?)
        .map_err(|_| invalid("document_id is invalid"))
}
fn access_id(v: &str) -> Result<AccessEntryId, Status> {
    AccessEntryId::try_from_uuid(parse_uuid(v, "access_entry_id")?)
        .map_err(|_| invalid("access_entry_id is invalid"))
}
fn timestamp(v: &str, field: &str) -> Result<DateTime<Utc>, Status> {
    DateTime::parse_from_rfc3339(v)
        .map(|x| x.with_timezone(&Utc))
        .map_err(|_| invalid(&format!("{field} is invalid")))
}
fn page_size(v: u32) -> Result<u16, Status> {
    u16::try_from(v).map_err(|_| invalid("page_size is invalid"))
}

fn encode_receipt<R>(v: &CommandReceipt<R>) -> common::CommandReceipt {
    common::CommandReceipt {
        command_id: v.command_id.to_string(),
        correlation_id: v.correlation_id.to_string(),
        aggregate: Some(common::AggregateVersion {
            context: v.aggregate.context.clone(),
            aggregate_type: v.aggregate.aggregate_type.clone(),
            aggregate_id: v.aggregate.aggregate_id.to_string(),
            version: v.aggregate.version,
        }),
        committed_events: v
            .committed_events
            .iter()
            .map(|x| common::EventPosition {
                event_id: x.event_id.to_string(),
                global_sequence: x.global_sequence,
            })
            .collect(),
        committed_at: v.committed_at.to_rfc3339(),
        replayed: v.replayed,
    }
}
fn encode_query_metadata(v: &QueryMetadata) -> common::QueryMetadata {
    common::QueryMetadata {
        correlation_id: v.correlation_id.to_string(),
        projection: Some(common::ProjectionMetadata {
            checkpoint: v
                .projection
                .checkpoint
                .as_ref()
                .map(|x| common::ProjectionCheckpoint {
                    projection: x.projection.clone(),
                    partition: x.partition.clone(),
                    event_id: x.event_id.to_string(),
                    global_sequence: x.global_sequence,
                    updated_at: x.updated_at.to_rfc3339(),
                }),
            freshness: match v.projection.freshness {
                FreshnessState::Current => common::ProjectionFreshnessState::Current as i32,
                FreshnessState::Stale => common::ProjectionFreshnessState::Stale as i32,
                FreshnessState::Rebuilding => common::ProjectionFreshnessState::Rebuilding as i32,
                FreshnessState::Unavailable => common::ProjectionFreshnessState::Unavailable as i32,
            },
            lag_millis: v.projection.lag_millis,
        }),
        consistency: Some(encode_consistency(&v.consistency)),
    }
}
fn decode_consistency(v: Option<common::QueryConsistency>) -> Result<QueryConsistency, Status> {
    use common::query_consistency::Requirement;
    match v.and_then(|v| v.requirement) {
        None | Some(Requirement::Eventual(_)) => Ok(QueryConsistency::Eventual),
        Some(Requirement::ReadYourWrites(v)) => {
            Ok(QueryConsistency::ReadYourWrites(ReadYourWrites {
                projection: v.projection,
                partition: v.partition,
                target: erpx_cqrs::ProjectionTarget {
                    event_id: event_id(&v.event_id)?,
                    global_sequence: v.global_sequence,
                },
                timeout_millis: v.timeout_millis,
            }))
        }
        Some(Requirement::AggregateVersion(_)) => {
            Err(invalid("aggregate-version reads are unsupported"))
        }
    }
}
fn encode_consistency(v: &QueryConsistency) -> common::QueryConsistency {
    use common::query_consistency::Requirement;
    common::QueryConsistency {
        requirement: Some(match v {
            QueryConsistency::Eventual => Requirement::Eventual(common::EventualConsistency {}),
            QueryConsistency::ReadYourWrites(v) => {
                Requirement::ReadYourWrites(common::ReadYourWrites {
                    projection: v.projection.clone(),
                    partition: v.partition.clone(),
                    event_id: v.target.event_id.to_string(),
                    global_sequence: v.target.global_sequence,
                    timeout_millis: v.timeout_millis,
                    ..Default::default()
                })
            }
        }),
    }
}
fn decode_route(v: Option<common::TenantRoute>) -> Result<LegalTenantRoute, Status> {
    let v = v.ok_or_else(|| invalid("tenant route is required"))?;
    let route = LegalTenantRoute {
        tenant_id: v.tenant_id.trim().into(),
        holding_ref: v.holding_ref.trim().into(),
    };
    validate_route(&route)?;
    Ok(route)
}
fn validate_route(v: &LegalTenantRoute) -> Result<(), Status> {
    if [&v.tenant_id, &v.holding_ref]
        .iter()
        .any(|x| x.is_empty() || x.len() > MAX_ROUTE_LENGTH)
    {
        Err(invalid("tenant route is invalid"))
    } else {
        Ok(())
    }
}
fn validate_route_scope(route: &LegalTenantRoute, scope: &TenantScope) -> Result<(), Status> {
    let holding = match scope {
        TenantScope::Platform => return Ok(()),
        TenantScope::Holding { holding_id }
        | TenantScope::Company { holding_id, .. }
        | TenantScope::Branch { holding_id, .. }
        | TenantScope::Subtree { holding_id, .. } => holding_id,
    };
    if parse_uuid(&route.holding_ref, "holding_ref")? == *holding {
        Ok(())
    } else {
        Err(invalid("tenant route does not match Legal scope"))
    }
}
fn decode_actor(v: Option<common::ActorAttribution>) -> Result<ActorAttribution, Status> {
    let v = v.ok_or_else(|| invalid("actor is required"))?;
    let kind = match common::ActorKind::try_from(v.kind).ok() {
        Some(common::ActorKind::PlatformUser) => ActorKind::PlatformUser,
        Some(common::ActorKind::TenantUser) => ActorKind::TenantUser,
        Some(common::ActorKind::Service) => ActorKind::Service,
        Some(common::ActorKind::SystemWorker) => ActorKind::SystemWorker,
        _ => return Err(invalid("actor kind is invalid")),
    };
    Ok(ActorAttribution {
        principal_ref: v.principal_ref,
        kind,
        session_id: optional_uuid(&v.session_id)?
            .map(SessionId::try_from_uuid)
            .transpose()
            .map_err(|_| invalid("session_id is invalid"))?,
        represented_principal_ref: nonempty(v.represented_principal_ref),
        support_capability_id: optional_uuid(&v.support_capability_id)?,
    })
}
fn decode_audit(v: Option<common::AuditContext>) -> Result<AuditContext, Status> {
    let v = v.ok_or_else(|| invalid("audit is required"))?;
    Ok(AuditContext {
        purpose: v.purpose,
        customer_visible: v.customer_visible,
        module: nonempty(v.module),
    })
}
fn decode_scope(v: common::TenantScope) -> Result<TenantScope, Status> {
    use common::tenant_scope::Scope;
    match v.scope.ok_or_else(|| invalid("scope is required"))? {
        Scope::Platform(_) => Ok(TenantScope::Platform),
        Scope::Holding(v) => Ok(TenantScope::Holding {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
        }),
        Scope::Company(v) => Ok(TenantScope::Company {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
            company_id: parse_uuid(&v.company_id, "company_id")?,
        }),
        Scope::Branch(v) => Ok(TenantScope::Branch {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
            company_id: parse_uuid(&v.company_id, "company_id")?,
            branch_id: parse_uuid(&v.branch_id, "branch_id")?,
        }),
        Scope::Subtree(v) => Ok(TenantScope::Subtree {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
            company_ids: v
                .company_ids
                .iter()
                .map(|x| parse_uuid(x, "company_id"))
                .collect::<Result<_, _>>()?,
            branch_ids: v
                .branch_ids
                .iter()
                .map(|x| parse_uuid(x, "branch_id"))
                .collect::<Result<_, _>>()?,
        }),
    }
}
fn encode_scope(v: &TenantScope) -> common::TenantScope {
    use common::tenant_scope::Scope;
    common::TenantScope {
        scope: Some(match v {
            TenantScope::Platform => Scope::Platform(common::PlatformScope {}),
            TenantScope::Holding { holding_id } => Scope::Holding(common::HoldingScope {
                holding_id: holding_id.to_string(),
            }),
            TenantScope::Company {
                holding_id,
                company_id,
            } => Scope::Company(common::CompanyScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
            }),
            TenantScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Scope::Branch(common::BranchScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
                branch_id: branch_id.to_string(),
            }),
            TenantScope::Subtree {
                holding_id,
                company_ids,
                branch_ids,
            } => Scope::Subtree(common::SubtreeScope {
                holding_id: holding_id.to_string(),
                company_ids: company_ids.iter().map(ToString::to_string).collect(),
                branch_ids: branch_ids.iter().map(ToString::to_string).collect(),
            }),
        }),
    }
}
fn parse_uuid(v: &str, field: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(v)
        .map_err(|_| invalid(&format!("{field} is invalid")))
        .and_then(|id| {
            if id.is_nil() {
                Err(invalid(&format!("{field} is invalid")))
            } else {
                Ok(id)
            }
        })
}
fn correlation(v: &str) -> Result<CorrelationId, Status> {
    CorrelationId::try_from_uuid(parse_uuid(v, "correlation_id")?)
        .map_err(|_| invalid("correlation_id is invalid"))
}
fn event_id(v: &str) -> Result<EventId, Status> {
    EventId::try_from_uuid(parse_uuid(v, "event_id")?).map_err(|_| invalid("event_id is invalid"))
}
fn optional_uuid(v: &str) -> Result<Option<Uuid>, Status> {
    if v.trim().is_empty() {
        Ok(None)
    } else {
        parse_uuid(v, "optional UUID").map(Some)
    }
}
fn nonempty(v: String) -> Option<String> {
    if v.trim().is_empty() { None } else { Some(v) }
}
fn clean(v: Option<String>) -> Option<String> {
    v.and_then(nonempty)
}
fn response_mismatch() -> Status {
    Status::internal("Legal response mismatch")
}
fn status_from_cqrs(v: CqrsError) -> Status {
    match v.category() {
        ErrorCategory::Invalid => Status::invalid_argument(v.to_string()),
        ErrorCategory::Denied => Status::permission_denied(v.to_string()),
        ErrorCategory::NotFound => Status::not_found(v.to_string()),
        ErrorCategory::Conflict => Status::aborted(v.to_string()),
        ErrorCategory::NotReady => Status::failed_precondition(v.to_string()),
        ErrorCategory::Unavailable => Status::unavailable(v.to_string()),
        ErrorCategory::Internal => Status::internal(v.to_string()),
    }
}
fn status_from_auth(v: LegalAuthenticationError) -> Status {
    match v {
        LegalAuthenticationError::Unauthenticated => {
            Status::unauthenticated("authentication required")
        }
        LegalAuthenticationError::Denied => Status::permission_denied("Legal access denied"),
        LegalAuthenticationError::Unavailable => Status::unavailable("authentication unavailable"),
        LegalAuthenticationError::Internal => Status::internal("authentication failed"),
    }
}
fn invalid(v: &str) -> Status {
    Status::invalid_argument(v.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn actor() -> ActorAttribution {
        ActorAttribution {
            principal_ref: "legal-route-test".into(),
            kind: ActorKind::TenantUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        }
    }
    #[test]
    fn status_and_scope_round_trip() {
        assert_eq!(
            decode_status(encode_status(ContractStatus::Archived)).unwrap(),
            ContractStatus::Archived
        );
        let scope = TenantScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        };
        assert_eq!(decode_scope(encode_scope(&scope)).unwrap(), scope)
    }
    #[test]
    fn secret_bearing_auth_request_has_no_debug() {
        let source = include_str!("legal_cqrs.rs");
        let declaration = source
            .find(concat!("pub struct Legal", "AuthenticationRequest"))
            .unwrap();
        let attributes = source[..declaration]
            .lines()
            .rev()
            .take_while(|line| line.trim_start().starts_with("#["))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!attributes.contains("Debug"));
    }
    #[test]
    fn authenticated_route_must_match_holding() {
        let holding = Uuid::from_u128(11);
        let scope = TenantScope::Company {
            holding_id: holding,
            company_id: Uuid::from_u128(12),
        };
        assert!(
            AuthenticatedLegalRequest::try_new(
                actor(),
                scope.clone(),
                LegalTenantRoute {
                    tenant_id: "tenant-a".into(),
                    holding_ref: holding.to_string()
                }
            )
            .is_ok()
        );
        assert_eq!(
            AuthenticatedLegalRequest::try_new(
                actor(),
                scope,
                LegalTenantRoute {
                    tenant_id: "tenant-a".into(),
                    holding_ref: Uuid::from_u128(99).to_string()
                }
            ),
            Err(LegalAuthenticationError::Internal)
        );
    }
}
