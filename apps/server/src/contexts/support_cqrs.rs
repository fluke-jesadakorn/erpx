//! Typed, storage-agnostic gRPC inbound adapter for Support CQRS.

use std::sync::Arc;

use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandReceipt, CqrsError, FreshnessState, IdempotencyKey, QueryConsistency,
    QueryEnvelope, QueryMetadata, QueryResult, ReadYourWrites,
};
use erpx_integration_contracts::{customer, identity_access};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    QueryId, SessionId as KernelSessionId, TenantScope,
};
use erpx_proto::{common, erpx::support::v2 as platform};
use erpx_support::{
    AddIncidentTimeline, AddTicketMessage, ApproveSupportSession, BroadcastAudience,
    BroadcastIncident, CapabilityEvidence, CapabilityId, CapabilityRevocationReason, GetIncident,
    GetSupportCapability, GetSupportSession, GetSupportTicket, IncidentId, IncidentStatus,
    IssueSupportCapability, ListIncidents, ListSupportTickets, MessageAudience, MessageId,
    OpenIncident, OpenSupportTicket, RequestSupportSession, RevokeSupportCapability,
    RevokeSupportSession, SessionId, SupportAction, SupportCommand, SupportMode,
    SupportMutationResult, SupportQuery, SupportQueryResponse, SupportTicketId, TicketStatus,
    TransitionIncident, TransitionSupportTicket,
    ports::{
        IncidentListFilter, IncidentView, SupportCapabilityView, SupportSessionView,
        SupportTicketListFilter, SupportTicketView, TicketReadAudience,
    },
};
use platform::support_command_service_server::SupportCommandService;
use platform::support_query_service_server::SupportQueryService;
use tonic::{Request, Response, Status, metadata::MetadataMap};
use uuid::Uuid;

const MAX_ROUTE_LENGTH: usize = 255;
const SUPPORT_CAPABILITY_DELIVERY_METADATA: &str = "x-erpx-support-capability-bin";
const SUPPORT_MFA_REQUIRED_MESSAGE: &str =
    "current MFA verification is required for this Support action";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportTenantRoute {
    pub tenant_id: String,
    pub holding_ref: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportAccessMode {
    Read,
    Write,
}

pub struct SupportAuthenticationRequest {
    transport_metadata: MetadataMap,
    presented_actor: ActorAttribution,
    presented_scope: TenantScope,
    presented_route: SupportTenantRoute,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
    access_mode: SupportAccessMode,
    action: SupportAction,
}
impl SupportAuthenticationRequest {
    pub fn transport_metadata(&self) -> &MetadataMap {
        &self.transport_metadata
    }
    pub const fn presented_actor(&self) -> &ActorAttribution {
        &self.presented_actor
    }
    pub const fn presented_scope(&self) -> &TenantScope {
        &self.presented_scope
    }
    pub const fn presented_route(&self) -> &SupportTenantRoute {
        &self.presented_route
    }
    pub fn subscription_id(&self) -> Option<&str> {
        self.subscription_id.as_deref()
    }
    pub fn mfa_code(&self) -> Option<&str> {
        self.mfa_code.as_deref()
    }
    pub const fn access_mode(&self) -> SupportAccessMode {
        self.access_mode
    }
    pub const fn action(&self) -> SupportAction {
        self.action
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedSupportRequest {
    actor: ActorAttribution,
    tenant_scope: TenantScope,
    route: SupportTenantRoute,
}
impl AuthenticatedSupportRequest {
    pub fn try_new(
        actor: ActorAttribution,
        tenant_scope: TenantScope,
        route: SupportTenantRoute,
    ) -> Result<Self, SupportAuthenticationError> {
        actor
            .validate()
            .map_err(|_| SupportAuthenticationError::Internal)?;
        tenant_scope
            .validate()
            .map_err(|_| SupportAuthenticationError::Internal)?;
        validate_route(&route).map_err(|_| SupportAuthenticationError::Internal)?;
        validate_route_scope(&route, &tenant_scope)
            .map_err(|_| SupportAuthenticationError::Internal)?;
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
    pub const fn route(&self) -> &SupportTenantRoute {
        &self.route
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportAuthenticationError {
    Unauthenticated,
    Denied,
    Unavailable,
    Internal,
}

#[async_trait]
pub trait SupportRequestAuthenticator: Send + Sync + 'static {
    async fn authenticate(
        &self,
        request: SupportAuthenticationRequest,
    ) -> Result<AuthenticatedSupportRequest, SupportAuthenticationError>;
}
#[async_trait]
pub trait SupportCommandApplication: Send + Sync + 'static {
    async fn execute(
        &self,
        route: SupportTenantRoute,
        command: CommandEnvelope<SupportCommand>,
    ) -> Result<CommandReceipt<SupportMutationResult>, CqrsError>;
}
#[async_trait]
pub trait SupportQueryApplication: Send + Sync + 'static {
    async fn execute(
        &self,
        route: SupportTenantRoute,
        query: QueryEnvelope<SupportQuery>,
    ) -> Result<QueryResult<SupportQueryResponse>, CqrsError>;
}

/// Resolves current Customer-owned state after authentication. Client IDs are
/// selectors only and never become authoritative Support input by themselves.
#[async_trait]
pub trait SupportCustomerFactResolver: Send + Sync + 'static {
    async fn resolve_customer(
        &self,
        route: &SupportTenantRoute,
        authority: &TenantScope,
        customer_id: Uuid,
        account_id: Uuid,
    ) -> Result<customer::CustomerProfileFact, Status>;
}

/// Owned one-time material delivered to the authenticated Support agent.
///
/// The bytes deliberately cannot be cloned or serialized. Callers can only
/// borrow the fixed-size value for the duration of a closure, and the owned
/// storage is cleared when it leaves scope.
pub struct SupportCapabilityDeliverySecret(zeroize::Zeroizing<[u8; 32]>);

impl SupportCapabilityDeliverySecret {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(zeroize::Zeroizing::new(bytes))
    }

    pub fn with_bytes<R>(&self, use_bytes: impl FnOnce(&[u8; 32]) -> R) -> R {
        use_bytes(&self.0)
    }
}

impl std::fmt::Debug for SupportCapabilityDeliverySecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SupportCapabilityDeliverySecret(REDACTED)")
    }
}

pub struct SecureCapabilityIssueMaterial {
    pub evidence: CapabilityEvidence,
    pub secret_hash_sha256: [u8; 32],
    pub delivery_secret: SupportCapabilityDeliverySecret,
}
impl std::fmt::Debug for SecureCapabilityIssueMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecureCapabilityIssueMaterial")
            .field("evidence", &self.evidence)
            .field("secret_hash_sha256", &"REDACTED")
            .field("delivery_secret", &"REDACTED")
            .finish()
    }
}
/// Secure Server/agent boundary. It must recheck current Identity session,
/// device, MFA and SoD state and provide only a hash to the core.
#[async_trait]
pub trait SupportCapabilityAuthorityResolver: Send + Sync + 'static {
    // This boundary binds every authority witness explicitly; grouping them
    // would make omission and scope substitution easier at implementations.
    #[allow(clippy::too_many_arguments)]
    async fn resolve_issue_material(
        &self,
        route: &SupportTenantRoute,
        actor: &ActorAttribution,
        authority: &TenantScope,
        command_id: CommandId,
        capability_id: CapabilityId,
        session_id: SessionId,
        device_ref: &str,
    ) -> Result<SecureCapabilityIssueMaterial, Status>;
}

#[derive(Debug, Clone)]
pub struct SupportCommandRpc<A, G, C, I> {
    application: Arc<A>,
    authenticator: Arc<G>,
    customers: Arc<C>,
    capabilities: Arc<I>,
}
impl<A, G, C, I> SupportCommandRpc<A, G, C, I> {
    pub const fn new(
        application: Arc<A>,
        authenticator: Arc<G>,
        customers: Arc<C>,
        capabilities: Arc<I>,
    ) -> Self {
        Self {
            application,
            authenticator,
            customers,
            capabilities,
        }
    }
}
#[derive(Debug, Clone)]
pub struct SupportQueryRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}
impl<A, G> SupportQueryRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

impl<
    A: SupportCommandApplication,
    G: SupportRequestAuthenticator,
    C: SupportCustomerFactResolver,
    I: SupportCapabilityAuthorityResolver,
> SupportCommandRpc<A, G, C, I>
{
    async fn authenticated_metadata(
        &self,
        transport: MetadataMap,
        metadata: Option<common::CommandRequestMetadata>,
        action: SupportAction,
    ) -> Result<(SupportTenantRoute, CommandEnvelope<()>), Status> {
        let decoded = decode_command_metadata(metadata)?;
        authenticate_command(self.authenticator.as_ref(), transport, decoded, action).await
    }
    async fn execute(
        &self,
        route: SupportTenantRoute,
        envelope: CommandEnvelope<()>,
        command: SupportCommand,
    ) -> Result<CommandReceipt<SupportMutationResult>, Status> {
        self.application
            .execute(route, map_command(envelope, command))
            .await
            .map_err(status_from_cqrs)
    }
}

#[async_trait]
impl<
    A: SupportCommandApplication,
    G: SupportRequestAuthenticator,
    C: SupportCustomerFactResolver,
    I: SupportCapabilityAuthorityResolver,
> SupportCommandService for SupportCommandRpc<A, G, C, I>
{
    async fn open_ticket(
        &self,
        request: Request<platform::OpenSupportTicketCommandRequest>,
    ) -> Result<Response<platform::SupportInternalTicketMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let (route, envelope) = self
            .authenticated_metadata(transport, input.metadata, SupportAction::OpenTicket)
            .await?;
        let customer_id = parse_uuid(&input.customer_id, "customer_id")?;
        let account_id = parse_uuid(&input.customer_account_id, "customer_account_id")?;
        let fact = self
            .customers
            .resolve_customer(&route, &envelope.tenant_scope, customer_id, account_id)
            .await?;
        if fact.customer.as_uuid() != customer_id || fact.account.as_uuid() != account_id {
            return Err(Status::permission_denied("customer binding changed"));
        }
        let receipt = self
            .execute(
                route,
                envelope,
                SupportCommand::OpenTicket(OpenSupportTicket {
                    ticket_id: ticket_id(&input.ticket_id)?,
                    customer: fact,
                    case_code: input.case_code,
                    category: input.category,
                    severity: input.severity,
                    description: input.description,
                }),
            )
            .await?;
        internal_ticket_mutation(receipt)
    }
    async fn transition_ticket(
        &self,
        request: Request<platform::TransitionSupportTicketCommandRequest>,
    ) -> Result<Response<platform::SupportInternalTicketMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let i = request.into_inner();
        let (r, e) = self
            .authenticated_metadata(transport, i.metadata, SupportAction::TransitionTicket)
            .await?;
        internal_ticket_mutation(
            self.execute(
                r,
                e,
                SupportCommand::TransitionTicket(TransitionSupportTicket {
                    ticket_id: ticket_id(&i.ticket_id)?,
                    next: decode_ticket_status(i.next)?,
                }),
            )
            .await?,
        )
    }
    async fn add_customer_reply(
        &self,
        request: Request<platform::AddSupportTicketMessageCommandRequest>,
    ) -> Result<Response<platform::SupportCustomerTicketMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let i = request.into_inner();
        let (r, e) = self
            .authenticated_metadata(transport, i.metadata, SupportAction::AddCustomerMessage)
            .await?;
        let receipt = self
            .execute(
                r,
                e,
                SupportCommand::AddTicketMessage(AddTicketMessage {
                    ticket_id: ticket_id(&i.ticket_id)?,
                    message_id: message_id(&i.message_id)?,
                    audience: MessageAudience::CustomerVisible,
                    body: i.body,
                }),
            )
            .await?;
        customer_ticket_mutation(receipt)
    }
    async fn add_internal_note(
        &self,
        request: Request<platform::AddSupportTicketMessageCommandRequest>,
    ) -> Result<Response<platform::SupportInternalTicketMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let i = request.into_inner();
        let (r, e) = self
            .authenticated_metadata(transport, i.metadata, SupportAction::AddInternalMessage)
            .await?;
        internal_ticket_mutation(
            self.execute(
                r,
                e,
                SupportCommand::AddTicketMessage(AddTicketMessage {
                    ticket_id: ticket_id(&i.ticket_id)?,
                    message_id: message_id(&i.message_id)?,
                    audience: MessageAudience::Internal,
                    body: i.body,
                }),
            )
            .await?,
        )
    }
    async fn request_session(
        &self,
        request: Request<platform::RequestTypedSupportSessionCommandRequest>,
    ) -> Result<Response<platform::SupportSessionMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let i = request.into_inner();
        let (r, e) = self
            .authenticated_metadata(transport, i.metadata, SupportAction::RequestSession)
            .await?;
        let agent = identity_access::PrincipalRef::try_from_uuid(parse_uuid(
            &e.actor.principal_ref,
            "authenticated principal_ref",
        )?)
        .map_err(|_| invalid("authenticated principal_ref is invalid"))?;
        let represented = i
            .represented_principal_id
            .as_deref()
            .map(|v| {
                identity_access::PrincipalRef::try_from_uuid(parse_uuid(
                    v,
                    "represented_principal_id",
                )?)
                .map_err(|_| invalid("represented principal is invalid"))
            })
            .transpose()?;
        session_mutation(
            self.execute(
                r,
                e,
                SupportCommand::RequestSession(RequestSupportSession {
                    session_id: session_id(&i.session_id)?,
                    ticket_id: ticket_id(&i.ticket_id)?,
                    agent,
                    represented_principal: represented,
                    mode: decode_mode(i.mode)?,
                    permissions: i.permissions,
                    approved_scope: decode_scope(
                        i.approved_scope
                            .ok_or_else(|| invalid("approved_scope is required"))?,
                    )?,
                    module: i.module,
                    reason: i.reason,
                    duration_seconds: i.duration_seconds,
                }),
            )
            .await?,
        )
    }
    async fn approve_session(
        &self,
        request: Request<platform::SupportSessionIdentityCommandRequest>,
    ) -> Result<Response<platform::SupportSessionMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let i = request.into_inner();
        let (r, e) = self
            .authenticated_metadata(transport, i.metadata, SupportAction::ApproveSession)
            .await?;
        session_mutation(
            self.execute(
                r,
                e,
                SupportCommand::ApproveSession(ApproveSupportSession {
                    session_id: session_id(&i.session_id)?,
                }),
            )
            .await?,
        )
    }
    async fn revoke_session(
        &self,
        request: Request<platform::RevokeTypedSupportSessionCommandRequest>,
    ) -> Result<Response<platform::SupportSessionMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let i = request.into_inner();
        let (r, e) = self
            .authenticated_metadata(transport, i.metadata, SupportAction::RevokeSession)
            .await?;
        session_mutation(
            self.execute(
                r,
                e,
                SupportCommand::RevokeSession(RevokeSupportSession {
                    session_id: session_id(&i.session_id)?,
                    reason: i.reason,
                }),
            )
            .await?,
        )
    }
    async fn issue_capability(
        &self,
        request: Request<platform::IssueSupportCapabilityCommandRequest>,
    ) -> Result<Response<platform::SupportCapabilityMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let i = request.into_inner();
        let (r, e) = self
            .authenticated_metadata(transport, i.metadata, SupportAction::IssueCapability)
            .await?;
        let sid = session_id(&i.session_id)?;
        let cid = capability_id(&i.capability_id)?;
        let SecureCapabilityIssueMaterial {
            evidence,
            secret_hash_sha256,
            delivery_secret,
        } = self
            .capabilities
            .resolve_issue_material(
                &r,
                &e.actor,
                &e.tenant_scope,
                e.command_id,
                cid,
                sid,
                &i.device_ref,
            )
            .await?;
        let response = capability_mutation(
            self.execute(
                r,
                e,
                SupportCommand::IssueCapability(IssueSupportCapability {
                    capability_id: cid,
                    session_id: sid,
                    evidence,
                    secret_hash_sha256,
                }),
            )
            .await?,
        )?;
        Ok(attach_delivery_material(response, delivery_secret))
    }
    async fn revoke_capability(
        &self,
        request: Request<platform::RevokeSupportCapabilityCommandRequest>,
    ) -> Result<Response<platform::SupportCapabilityMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let i = request.into_inner();
        let (r, e) = self
            .authenticated_metadata(transport, i.metadata, SupportAction::RevokeCapability)
            .await?;
        capability_mutation(
            self.execute(
                r,
                e,
                SupportCommand::RevokeCapability(RevokeSupportCapability {
                    capability_id: capability_id(&i.capability_id)?,
                    reason: decode_revocation(i.reason)?,
                }),
            )
            .await?,
        )
    }
    async fn open_incident(
        &self,
        request: Request<platform::OpenSupportIncidentCommandRequest>,
    ) -> Result<Response<platform::SupportIncidentMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let i = request.into_inner();
        let (r, e) = self
            .authenticated_metadata(transport, i.metadata, SupportAction::OpenIncident)
            .await?;
        let accounts = i
            .affected_customer_account_ids
            .iter()
            .map(|v| {
                customer::CustomerAccountRef::try_from_uuid(parse_uuid(v, "customer_account_id")?)
                    .map_err(|_| invalid("customer_account_id is invalid"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        incident_mutation(
            self.execute(
                r,
                e,
                SupportCommand::OpenIncident(OpenIncident {
                    incident_id: incident_id(&i.incident_id)?,
                    incident_code: i.incident_code,
                    severity: i.severity,
                    affected_services: i.affected_services,
                    affected_customer_accounts: accounts,
                }),
            )
            .await?,
        )
    }
    async fn transition_incident(
        &self,
        request: Request<platform::TransitionSupportIncidentCommandRequest>,
    ) -> Result<Response<platform::SupportIncidentMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let i = request.into_inner();
        let (r, e) = self
            .authenticated_metadata(transport, i.metadata, SupportAction::TransitionIncident)
            .await?;
        incident_mutation(
            self.execute(
                r,
                e,
                SupportCommand::TransitionIncident(TransitionIncident {
                    incident_id: incident_id(&i.incident_id)?,
                    next: decode_incident_status(i.next)?,
                }),
            )
            .await?,
        )
    }
    async fn add_incident_timeline(
        &self,
        request: Request<platform::AddSupportIncidentTimelineCommandRequest>,
    ) -> Result<Response<platform::SupportIncidentMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let i = request.into_inner();
        let (r, e) = self
            .authenticated_metadata(transport, i.metadata, SupportAction::AddIncidentTimeline)
            .await?;
        incident_mutation(
            self.execute(
                r,
                e,
                SupportCommand::AddIncidentTimeline(AddIncidentTimeline {
                    incident_id: incident_id(&i.incident_id)?,
                    message: i.message,
                }),
            )
            .await?,
        )
    }
    async fn broadcast_incident(
        &self,
        request: Request<platform::BroadcastSupportIncidentCommandRequest>,
    ) -> Result<Response<platform::SupportIncidentMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let i = request.into_inner();
        let audience = decode_broadcast_audience(i.audience)?;
        let action = match audience {
            BroadcastAudience::CustomerVisible => SupportAction::BroadcastCustomerIncident,
            BroadcastAudience::Internal => SupportAction::BroadcastInternalIncident,
        };
        let (r, e) = self
            .authenticated_metadata(transport, i.metadata, action)
            .await?;
        incident_mutation(
            self.execute(
                r,
                e,
                SupportCommand::BroadcastIncident(BroadcastIncident {
                    incident_id: incident_id(&i.incident_id)?,
                    audience,
                    message: i.message,
                }),
            )
            .await?,
        )
    }
}

impl<A: SupportQueryApplication, G: SupportRequestAuthenticator> SupportQueryRpc<A, G> {
    async fn query(
        &self,
        transport: MetadataMap,
        metadata: Option<common::QueryRequestMetadata>,
        query: SupportQuery,
    ) -> Result<QueryResult<SupportQueryResponse>, Status> {
        let action = query_action(&query);
        let decoded = decode_query(metadata, query)?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), transport, decoded, action).await?;
        self.application
            .execute(route, envelope)
            .await
            .map_err(status_from_cqrs)
    }
}

#[async_trait]
impl<A: SupportQueryApplication, G: SupportRequestAuthenticator> SupportQueryService
    for SupportQueryRpc<A, G>
{
    async fn get_customer_ticket(
        &self,
        request: Request<platform::GetSupportCustomerTicketRecordRequest>,
    ) -> Result<Response<platform::GetSupportCustomerTicketRecordResponse>, Status> {
        let transport = request.metadata().clone();
        let i = request.into_inner();
        let r = self
            .query(
                transport,
                i.metadata,
                SupportQuery::GetTicket(GetSupportTicket {
                    ticket_id: ticket_id(&i.ticket_id)?,
                    audience: TicketReadAudience::Customer,
                }),
            )
            .await?;
        let SupportQueryResponse::Ticket(v) = r.value else {
            return Err(response_mismatch());
        };
        Ok(Response::new(
            platform::GetSupportCustomerTicketRecordResponse {
                ticket: v.as_ref().map(encode_customer_ticket),
                metadata: Some(encode_query_metadata(&r.metadata)),
            },
        ))
    }
    async fn list_customer_tickets(
        &self,
        request: Request<platform::ListSupportCustomerTicketRecordsRequest>,
    ) -> Result<Response<platform::ListSupportCustomerTicketRecordsResponse>, Status> {
        let transport = request.metadata().clone();
        let i = request.into_inner();
        let filter = SupportTicketListFilter {
            status: i.status.map(decode_ticket_status).transpose()?,
            customer: i
                .customer_id
                .as_deref()
                .map(|v| {
                    customer::CustomerRef::try_from_uuid(parse_uuid(v, "customer_id")?)
                        .map_err(|_| invalid("customer_id is invalid"))
                })
                .transpose()?,
            audience: TicketReadAudience::Customer,
            page_size: page_size(i.page_size)?,
            cursor: i.cursor,
        };
        let r = self
            .query(
                transport,
                i.metadata,
                SupportQuery::ListTickets(ListSupportTickets { filter }),
            )
            .await?;
        let SupportQueryResponse::Tickets(v) = r.value else {
            return Err(response_mismatch());
        };
        Ok(Response::new(
            platform::ListSupportCustomerTicketRecordsResponse {
                tickets: v.tickets.iter().map(encode_customer_ticket).collect(),
                next_cursor: v.next_cursor,
                metadata: Some(encode_query_metadata(&r.metadata)),
            },
        ))
    }
    async fn get_internal_ticket(
        &self,
        request: Request<platform::GetSupportInternalTicketRecordRequest>,
    ) -> Result<Response<platform::GetSupportInternalTicketRecordResponse>, Status> {
        let transport = request.metadata().clone();
        let i = request.into_inner();
        let r = self
            .query(
                transport,
                i.metadata,
                SupportQuery::GetTicket(GetSupportTicket {
                    ticket_id: ticket_id(&i.ticket_id)?,
                    audience: TicketReadAudience::Internal,
                }),
            )
            .await?;
        let SupportQueryResponse::Ticket(v) = r.value else {
            return Err(response_mismatch());
        };
        Ok(Response::new(
            platform::GetSupportInternalTicketRecordResponse {
                ticket: v.as_ref().map(encode_internal_ticket),
                metadata: Some(encode_query_metadata(&r.metadata)),
            },
        ))
    }
    async fn list_internal_tickets(
        &self,
        request: Request<platform::ListSupportInternalTicketRecordsRequest>,
    ) -> Result<Response<platform::ListSupportInternalTicketRecordsResponse>, Status> {
        let transport = request.metadata().clone();
        let i = request.into_inner();
        let filter = SupportTicketListFilter {
            status: i.status.map(decode_ticket_status).transpose()?,
            customer: i
                .customer_id
                .as_deref()
                .map(|v| {
                    customer::CustomerRef::try_from_uuid(parse_uuid(v, "customer_id")?)
                        .map_err(|_| invalid("customer_id is invalid"))
                })
                .transpose()?,
            audience: TicketReadAudience::Internal,
            page_size: page_size(i.page_size)?,
            cursor: i.cursor,
        };
        let r = self
            .query(
                transport,
                i.metadata,
                SupportQuery::ListTickets(ListSupportTickets { filter }),
            )
            .await?;
        let SupportQueryResponse::Tickets(v) = r.value else {
            return Err(response_mismatch());
        };
        Ok(Response::new(
            platform::ListSupportInternalTicketRecordsResponse {
                tickets: v.tickets.iter().map(encode_internal_ticket).collect(),
                next_cursor: v.next_cursor,
                metadata: Some(encode_query_metadata(&r.metadata)),
            },
        ))
    }
    async fn get_incident(
        &self,
        request: Request<platform::GetSupportIncidentRecordRequest>,
    ) -> Result<Response<platform::GetSupportIncidentRecordResponse>, Status> {
        let transport = request.metadata().clone();
        let i = request.into_inner();
        let r = self
            .query(
                transport,
                i.metadata,
                SupportQuery::GetIncident(GetIncident {
                    incident_id: incident_id(&i.incident_id)?,
                }),
            )
            .await?;
        let SupportQueryResponse::Incident(v) = r.value else {
            return Err(response_mismatch());
        };
        Ok(Response::new(platform::GetSupportIncidentRecordResponse {
            incident: v.as_ref().map(encode_incident),
            metadata: Some(encode_query_metadata(&r.metadata)),
        }))
    }
    async fn list_incidents(
        &self,
        request: Request<platform::ListSupportIncidentRecordsRequest>,
    ) -> Result<Response<platform::ListSupportIncidentRecordsResponse>, Status> {
        let transport = request.metadata().clone();
        let i = request.into_inner();
        let filter = IncidentListFilter {
            status: i.status.map(decode_incident_status).transpose()?,
            page_size: page_size(i.page_size)?,
            cursor: i.cursor,
        };
        let r = self
            .query(
                transport,
                i.metadata,
                SupportQuery::ListIncidents(ListIncidents { filter }),
            )
            .await?;
        let SupportQueryResponse::Incidents(v) = r.value else {
            return Err(response_mismatch());
        };
        Ok(Response::new(
            platform::ListSupportIncidentRecordsResponse {
                incidents: v.incidents.iter().map(encode_incident).collect(),
                next_cursor: v.next_cursor,
                metadata: Some(encode_query_metadata(&r.metadata)),
            },
        ))
    }
    async fn get_session(
        &self,
        request: Request<platform::GetSupportSessionRecordRequest>,
    ) -> Result<Response<platform::GetSupportSessionRecordResponse>, Status> {
        let transport = request.metadata().clone();
        let i = request.into_inner();
        let r = self
            .query(
                transport,
                i.metadata,
                SupportQuery::GetSession(GetSupportSession {
                    session_id: session_id(&i.session_id)?,
                }),
            )
            .await?;
        let SupportQueryResponse::Session(v) = r.value else {
            return Err(response_mismatch());
        };
        Ok(Response::new(platform::GetSupportSessionRecordResponse {
            session: v.as_ref().map(encode_session),
            metadata: Some(encode_query_metadata(&r.metadata)),
        }))
    }
    async fn get_capability(
        &self,
        request: Request<platform::GetSupportCapabilityRecordRequest>,
    ) -> Result<Response<platform::GetSupportCapabilityRecordResponse>, Status> {
        let transport = request.metadata().clone();
        let i = request.into_inner();
        let r = self
            .query(
                transport,
                i.metadata,
                SupportQuery::GetCapability(GetSupportCapability {
                    capability_id: capability_id(&i.capability_id)?,
                }),
            )
            .await?;
        let SupportQueryResponse::Capability(v) = r.value else {
            return Err(response_mismatch());
        };
        Ok(Response::new(
            platform::GetSupportCapabilityRecordResponse {
                capability: v.as_ref().map(encode_capability),
                metadata: Some(encode_query_metadata(&r.metadata)),
            },
        ))
    }
}

fn internal_ticket_mutation(
    r: CommandReceipt<SupportMutationResult>,
) -> Result<Response<platform::SupportInternalTicketMutationResponse>, Status> {
    let SupportMutationResult::Ticket(v) = &r.response else {
        return Err(response_mismatch());
    };
    Ok(Response::new(
        platform::SupportInternalTicketMutationResponse {
            receipt: Some(encode_receipt(&r)),
            ticket: Some(encode_internal_ticket(v)),
        },
    ))
}
fn customer_ticket_mutation(
    r: CommandReceipt<SupportMutationResult>,
) -> Result<Response<platform::SupportCustomerTicketMutationResponse>, Status> {
    let SupportMutationResult::Ticket(v) = &r.response else {
        return Err(response_mismatch());
    };
    Ok(Response::new(
        platform::SupportCustomerTicketMutationResponse {
            receipt: Some(encode_receipt(&r)),
            ticket: Some(encode_customer_ticket(v)),
        },
    ))
}
fn session_mutation(
    r: CommandReceipt<SupportMutationResult>,
) -> Result<Response<platform::SupportSessionMutationResponse>, Status> {
    let SupportMutationResult::Session(v) = &r.response else {
        return Err(response_mismatch());
    };
    Ok(Response::new(platform::SupportSessionMutationResponse {
        receipt: Some(encode_receipt(&r)),
        session: Some(encode_session(v)),
    }))
}
fn capability_mutation(
    r: CommandReceipt<SupportMutationResult>,
) -> Result<Response<platform::SupportCapabilityMutationResponse>, Status> {
    let SupportMutationResult::Capability(v) = &r.response else {
        return Err(response_mismatch());
    };
    Ok(Response::new(platform::SupportCapabilityMutationResponse {
        receipt: Some(encode_receipt(&r)),
        capability: Some(encode_capability(v)),
    }))
}
fn attach_delivery_material<T>(
    mut response: Response<T>,
    secret: SupportCapabilityDeliverySecret,
) -> Response<T> {
    let value = secret.with_bytes(|bytes| tonic::metadata::MetadataValue::from_bytes(bytes));
    response
        .metadata_mut()
        .insert_bin(SUPPORT_CAPABILITY_DELIVERY_METADATA, value);
    response
}
fn incident_mutation(
    r: CommandReceipt<SupportMutationResult>,
) -> Result<Response<platform::SupportIncidentMutationResponse>, Status> {
    let SupportMutationResult::Incident(v) = &r.response else {
        return Err(response_mismatch());
    };
    Ok(Response::new(platform::SupportIncidentMutationResponse {
        receipt: Some(encode_receipt(&r)),
        incident: Some(encode_incident(v)),
    }))
}

struct DecodedCommand {
    route: SupportTenantRoute,
    envelope: CommandEnvelope<()>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
}
struct DecodedQuery<Q> {
    route: SupportTenantRoute,
    envelope: QueryEnvelope<Q>,
}
fn map_command(v: CommandEnvelope<()>, command: SupportCommand) -> CommandEnvelope<SupportCommand> {
    CommandEnvelope {
        contract_version: v.contract_version,
        command_id: v.command_id,
        idempotency_key: v.idempotency_key,
        correlation_id: v.correlation_id,
        causation_id: v.causation_id,
        actor: v.actor,
        tenant_scope: v.tenant_scope,
        audit: v.audit,
        expected_aggregate_version: v.expected_aggregate_version,
        command,
    }
}
fn query_action(query: &SupportQuery) -> SupportAction {
    match query {
        SupportQuery::GetTicket(query) => match query.audience {
            TicketReadAudience::Customer => SupportAction::ViewCustomerTicket,
            TicketReadAudience::Internal => SupportAction::ViewInternalTicket,
        },
        SupportQuery::ListTickets(query) => match query.filter.audience {
            TicketReadAudience::Customer => SupportAction::ListCustomerTickets,
            TicketReadAudience::Internal => SupportAction::ListInternalTickets,
        },
        SupportQuery::GetIncident(_) => SupportAction::ViewIncident,
        SupportQuery::ListIncidents(_) => SupportAction::ListIncidents,
        SupportQuery::GetSession(_) => SupportAction::ViewSession,
        SupportQuery::GetCapability(_) => SupportAction::ViewCapability,
    }
}
async fn authenticate_command<G: SupportRequestAuthenticator>(
    auth: &G,
    transport: MetadataMap,
    d: DecodedCommand,
    action: SupportAction,
) -> Result<(SupportTenantRoute, CommandEnvelope<()>), Status> {
    require_current_mfa(action, d.mfa_code.as_deref())?;
    let a = auth
        .authenticate(SupportAuthenticationRequest {
            transport_metadata: transport,
            presented_actor: d.envelope.actor.clone(),
            presented_scope: d.envelope.tenant_scope.clone(),
            presented_route: d.route,
            subscription_id: d.subscription_id,
            mfa_code: d.mfa_code,
            access_mode: SupportAccessMode::Write,
            action,
        })
        .await
        .map_err(status_from_auth)?;
    let mut e = d.envelope;
    e.actor = a.actor().clone();
    e.tenant_scope = a.tenant_scope().clone();
    e.validate()
        .map_err(|_| Status::internal("authenticated Support authority is invalid"))?;
    Ok((a.route().clone(), e))
}
async fn authenticate_query<Q, G: SupportRequestAuthenticator>(
    auth: &G,
    transport: MetadataMap,
    d: DecodedQuery<Q>,
    action: SupportAction,
) -> Result<(SupportTenantRoute, QueryEnvelope<Q>), Status> {
    let a = auth
        .authenticate(SupportAuthenticationRequest {
            transport_metadata: transport,
            presented_actor: d.envelope.actor.clone(),
            presented_scope: d.envelope.tenant_scope.clone(),
            presented_route: d.route,
            subscription_id: None,
            mfa_code: None,
            access_mode: SupportAccessMode::Read,
            action,
        })
        .await
        .map_err(status_from_auth)?;
    let mut e = d.envelope;
    e.actor = a.actor().clone();
    e.tenant_scope = a.tenant_scope().clone();
    e.validate()
        .map_err(|_| Status::internal("authenticated Support authority is invalid"))?;
    Ok((a.route().clone(), e))
}
fn decode_command_metadata(
    metadata: Option<common::CommandRequestMetadata>,
) -> Result<DecodedCommand, Status> {
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
        command: (),
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
fn require_current_mfa(action: SupportAction, mfa_code: Option<&str>) -> Result<(), Status> {
    if !action.requires_current_mfa() || mfa_code.is_some_and(|value| !value.trim().is_empty()) {
        Ok(())
    } else {
        Err(Status::failed_precondition(SUPPORT_MFA_REQUIRED_MESSAGE))
    }
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

fn encode_customer_ticket(v: &SupportTicketView) -> platform::SupportCustomerTicketRecord {
    platform::SupportCustomerTicketRecord {
        ticket_id: v.ticket_id.as_uuid().to_string(),
        source_version: v.version,
        scope: Some(encode_scope(&v.scope)),
        customer_id: v.customer.as_uuid().to_string(),
        customer_account_id: v.customer_account.as_uuid().to_string(),
        case_code: v.case_code.clone(),
        category: v.category.clone(),
        severity: v.severity.clone(),
        description: v.description.clone(),
        status: encode_ticket_status(v.status),
        messages: v
            .customer_messages
            .iter()
            .map(|m| platform::SupportCustomerMessageRecord {
                message_id: m.message_id.as_uuid().to_string(),
                author_ref: m.author_ref.clone(),
                body: m.body.clone(),
                created_at: m.created_at.to_rfc3339(),
            })
            .collect(),
        created_at: v.created_at.to_rfc3339(),
        updated_at: v.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}
fn encode_internal_ticket(v: &SupportTicketView) -> platform::SupportInternalTicketRecord {
    platform::SupportInternalTicketRecord {
        ticket_id: v.ticket_id.as_uuid().to_string(),
        source_version: v.version,
        scope: Some(encode_scope(&v.scope)),
        customer_id: v.customer.as_uuid().to_string(),
        customer_account_id: v.customer_account.as_uuid().to_string(),
        case_code: v.case_code.clone(),
        category: v.category.clone(),
        severity: v.severity.clone(),
        description: v.description.clone(),
        status: encode_ticket_status(v.status),
        messages: v
            .internal_messages
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|m| platform::SupportInternalMessageRecord {
                message_id: m.message_id.as_uuid().to_string(),
                author_ref: m.author_ref.clone(),
                audience: match m.audience {
                    MessageAudience::CustomerVisible => {
                        platform::SupportMessageAudience::CustomerVisible as i32
                    }
                    MessageAudience::Internal => platform::SupportMessageAudience::Internal as i32,
                },
                body: m.body.clone(),
                created_at: m.created_at.to_rfc3339(),
            })
            .collect(),
        created_at: v.created_at.to_rfc3339(),
        updated_at: v.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}
fn encode_incident(v: &IncidentView) -> platform::SupportIncidentRecord {
    platform::SupportIncidentRecord {
        incident_id: v.incident_id.as_uuid().to_string(),
        source_version: v.version,
        scope: Some(encode_scope(&v.scope)),
        incident_code: v.incident_code.clone(),
        severity: v.severity.clone(),
        commander_ref: v.commander_ref.clone(),
        affected_services: v.affected_services.clone(),
        affected_customer_accounts: v
            .affected_customer_accounts
            .iter()
            .map(|v| v.as_uuid().to_string())
            .collect(),
        status: encode_incident_status(v.status),
        created_at: v.created_at.to_rfc3339(),
        updated_at: v.updated_at.to_rfc3339(),
        projected_at: String::new(),
        timeline: v
            .timeline
            .iter()
            .map(|e| platform::SupportIncidentTimelineRecord {
                message: e.message.clone(),
                author_ref: e.actor_ref.clone(),
                created_at: e.at.to_rfc3339(),
            })
            .collect(),
        broadcasts: v
            .broadcasts
            .iter()
            .map(|e| platform::SupportIncidentBroadcastRecord {
                audience: match e.audience {
                    BroadcastAudience::CustomerVisible => {
                        platform::SupportBroadcastAudience::CustomerVisible as i32
                    }
                    BroadcastAudience::Internal => {
                        platform::SupportBroadcastAudience::Internal as i32
                    }
                },
                message: e.message.clone(),
                author_ref: e.actor_ref.clone(),
                created_at: e.at.to_rfc3339(),
            })
            .collect(),
    }
}
fn encode_session(v: &SupportSessionView) -> platform::SupportSessionRecord {
    platform::SupportSessionRecord {
        session_id: v.session_id.as_uuid().to_string(),
        source_version: v.version,
        approved_scope: Some(encode_scope(&v.scope)),
        ticket_id: v.ticket_id.as_uuid().to_string(),
        customer_id: v.customer.as_uuid().to_string(),
        customer_account_id: v.customer_account.as_uuid().to_string(),
        agent_principal_id: v.agent.as_uuid().to_string(),
        represented_principal_id: v.represented_principal.map(|v| v.as_uuid().to_string()),
        mode: encode_mode(v.mode),
        permissions: v.permissions.clone(),
        module: v.module.clone(),
        reason: v.reason.clone(),
        status: match v.status {
            erpx_support::SessionStatus::Requested => {
                platform::SupportSessionStatus::Requested as i32
            }
            erpx_support::SessionStatus::Approved => {
                platform::SupportSessionStatus::Approved as i32
            }
            erpx_support::SessionStatus::Revoked => platform::SupportSessionStatus::Revoked as i32,
        },
        requested_at: v.requested_at.to_rfc3339(),
        expires_at: v.expires_at.to_rfc3339(),
        approved_by: v.approved_by.clone(),
        approved_at: v.approved_at.map(|v| v.to_rfc3339()),
        revoked_at: v.revoked_at.map(|v| v.to_rfc3339()),
        revoked_reason: v.revoked_reason.clone(),
        projected_at: String::new(),
    }
}
fn encode_capability(v: &SupportCapabilityView) -> platform::SupportCapabilityRecord {
    let (reason, at) = v
        .revocation
        .map(|(r, a)| (Some(encode_revocation(r)), Some(a.to_rfc3339())))
        .unwrap_or((None, None));
    platform::SupportCapabilityRecord {
        capability_id: v.capability_id.as_uuid().to_string(),
        source_version: v.version,
        session_id: v.session_id.as_uuid().to_string(),
        ticket_id: v.ticket_id.as_uuid().to_string(),
        customer_id: v.customer.as_uuid().to_string(),
        customer_account_id: v.customer_account.as_uuid().to_string(),
        agent_principal_id: v.agent.as_uuid().to_string(),
        device_ref: v.device_ref.clone(),
        mode: encode_mode(v.mode),
        permissions: v.permissions.clone(),
        approved_scope: Some(encode_scope(&v.scope)),
        module: v.module.clone(),
        issued_at: v.issued_at.to_rfc3339(),
        expires_at: v.expires_at.to_rfc3339(),
        revocation_reason: reason,
        revoked_at: at,
        projected_at: String::new(),
    }
}

fn ticket_id(v: &str) -> Result<SupportTicketId, Status> {
    SupportTicketId::try_from_uuid(parse_uuid(v, "ticket_id")?)
        .map_err(|_| invalid("ticket_id is invalid"))
}
fn incident_id(v: &str) -> Result<IncidentId, Status> {
    IncidentId::try_from_uuid(parse_uuid(v, "incident_id")?)
        .map_err(|_| invalid("incident_id is invalid"))
}
fn session_id(v: &str) -> Result<SessionId, Status> {
    SessionId::try_from_uuid(parse_uuid(v, "session_id")?)
        .map_err(|_| invalid("session_id is invalid"))
}
fn capability_id(v: &str) -> Result<CapabilityId, Status> {
    CapabilityId::try_from_uuid(parse_uuid(v, "capability_id")?)
        .map_err(|_| invalid("capability_id is invalid"))
}
fn message_id(v: &str) -> Result<MessageId, Status> {
    MessageId::try_from_uuid(parse_uuid(v, "message_id")?)
        .map_err(|_| invalid("message_id is invalid"))
}
fn page_size(v: u32) -> Result<u16, Status> {
    u16::try_from(v).map_err(|_| invalid("page_size is invalid"))
}
fn decode_ticket_status(v: i32) -> Result<TicketStatus, Status> {
    match platform::SupportTicketStatus::try_from(v).ok() {
        Some(platform::SupportTicketStatus::New) => Ok(TicketStatus::New),
        Some(platform::SupportTicketStatus::Triaged) => Ok(TicketStatus::Triaged),
        Some(platform::SupportTicketStatus::WaitingForCustomer) => {
            Ok(TicketStatus::WaitingForCustomer)
        }
        Some(platform::SupportTicketStatus::AccessRequested) => Ok(TicketStatus::AccessRequested),
        Some(platform::SupportTicketStatus::Approved) => Ok(TicketStatus::Approved),
        Some(platform::SupportTicketStatus::InProgress) => Ok(TicketStatus::InProgress),
        Some(platform::SupportTicketStatus::Resolved) => Ok(TicketStatus::Resolved),
        Some(platform::SupportTicketStatus::CustomerConfirmed) => {
            Ok(TicketStatus::CustomerConfirmed)
        }
        Some(platform::SupportTicketStatus::Closed) => Ok(TicketStatus::Closed),
        _ => Err(invalid("ticket status is invalid")),
    }
}
fn decode_incident_status(v: i32) -> Result<IncidentStatus, Status> {
    match platform::SupportIncidentStatus::try_from(v).ok() {
        Some(platform::SupportIncidentStatus::Investigating) => Ok(IncidentStatus::Investigating),
        Some(platform::SupportIncidentStatus::Identified) => Ok(IncidentStatus::Identified),
        Some(platform::SupportIncidentStatus::Monitoring) => Ok(IncidentStatus::Monitoring),
        Some(platform::SupportIncidentStatus::Resolved) => Ok(IncidentStatus::Resolved),
        Some(platform::SupportIncidentStatus::PostmortemComplete) => {
            Ok(IncidentStatus::PostmortemComplete)
        }
        _ => Err(invalid("incident status is invalid")),
    }
}
fn decode_mode(v: i32) -> Result<SupportMode, Status> {
    match platform::SupportMode::try_from(v).ok() {
        Some(platform::SupportMode::DiagnosticRead) => Ok(SupportMode::DiagnosticRead),
        Some(platform::SupportMode::TenantMutation) => Ok(SupportMode::TenantMutation),
        Some(platform::SupportMode::Impersonation) => Ok(SupportMode::Impersonation),
        _ => Err(invalid("support mode is invalid")),
    }
}
fn decode_revocation(v: i32) -> Result<CapabilityRevocationReason, Status> {
    match platform::SupportCapabilityRevocationReason::try_from(v).ok() {
        Some(platform::SupportCapabilityRevocationReason::CustomerRequested) => {
            Ok(CapabilityRevocationReason::CustomerRequested)
        }
        Some(platform::SupportCapabilityRevocationReason::AgentDisabled) => {
            Ok(CapabilityRevocationReason::AgentDisabled)
        }
        Some(platform::SupportCapabilityRevocationReason::DeviceRevoked) => {
            Ok(CapabilityRevocationReason::DeviceRevoked)
        }
        Some(platform::SupportCapabilityRevocationReason::SessionRevoked) => {
            Ok(CapabilityRevocationReason::SessionRevoked)
        }
        Some(platform::SupportCapabilityRevocationReason::TicketClosed) => {
            Ok(CapabilityRevocationReason::TicketClosed)
        }
        Some(platform::SupportCapabilityRevocationReason::Administrative) => {
            Ok(CapabilityRevocationReason::Administrative)
        }
        _ => Err(invalid("revocation reason is invalid")),
    }
}
fn decode_broadcast_audience(v: i32) -> Result<BroadcastAudience, Status> {
    match platform::SupportBroadcastAudience::try_from(v).ok() {
        Some(platform::SupportBroadcastAudience::CustomerVisible) => {
            Ok(BroadcastAudience::CustomerVisible)
        }
        Some(platform::SupportBroadcastAudience::Internal) => Ok(BroadcastAudience::Internal),
        _ => Err(invalid("broadcast audience is invalid")),
    }
}
fn encode_ticket_status(v: TicketStatus) -> i32 {
    match v {
        TicketStatus::New => platform::SupportTicketStatus::New as i32,
        TicketStatus::Triaged => platform::SupportTicketStatus::Triaged as i32,
        TicketStatus::WaitingForCustomer => {
            platform::SupportTicketStatus::WaitingForCustomer as i32
        }
        TicketStatus::AccessRequested => platform::SupportTicketStatus::AccessRequested as i32,
        TicketStatus::Approved => platform::SupportTicketStatus::Approved as i32,
        TicketStatus::InProgress => platform::SupportTicketStatus::InProgress as i32,
        TicketStatus::Resolved => platform::SupportTicketStatus::Resolved as i32,
        TicketStatus::CustomerConfirmed => platform::SupportTicketStatus::CustomerConfirmed as i32,
        TicketStatus::Closed => platform::SupportTicketStatus::Closed as i32,
    }
}
fn encode_incident_status(v: IncidentStatus) -> i32 {
    match v {
        IncidentStatus::Investigating => platform::SupportIncidentStatus::Investigating as i32,
        IncidentStatus::Identified => platform::SupportIncidentStatus::Identified as i32,
        IncidentStatus::Monitoring => platform::SupportIncidentStatus::Monitoring as i32,
        IncidentStatus::Resolved => platform::SupportIncidentStatus::Resolved as i32,
        IncidentStatus::PostmortemComplete => {
            platform::SupportIncidentStatus::PostmortemComplete as i32
        }
    }
}
fn encode_mode(v: SupportMode) -> i32 {
    match v {
        SupportMode::DiagnosticRead => platform::SupportMode::DiagnosticRead as i32,
        SupportMode::TenantMutation => platform::SupportMode::TenantMutation as i32,
        SupportMode::Impersonation => platform::SupportMode::Impersonation as i32,
    }
}
fn encode_revocation(v: CapabilityRevocationReason) -> i32 {
    match v {
        CapabilityRevocationReason::CustomerRequested => {
            platform::SupportCapabilityRevocationReason::CustomerRequested as i32
        }
        CapabilityRevocationReason::AgentDisabled => {
            platform::SupportCapabilityRevocationReason::AgentDisabled as i32
        }
        CapabilityRevocationReason::DeviceRevoked => {
            platform::SupportCapabilityRevocationReason::DeviceRevoked as i32
        }
        CapabilityRevocationReason::SessionRevoked => {
            platform::SupportCapabilityRevocationReason::SessionRevoked as i32
        }
        CapabilityRevocationReason::TicketClosed => {
            platform::SupportCapabilityRevocationReason::TicketClosed as i32
        }
        CapabilityRevocationReason::Administrative => {
            platform::SupportCapabilityRevocationReason::Administrative as i32
        }
    }
}

fn decode_route(v: Option<common::TenantRoute>) -> Result<SupportTenantRoute, Status> {
    let v = v.ok_or_else(|| invalid("tenant route is required"))?;
    let r = SupportTenantRoute {
        tenant_id: v.tenant_id.trim().into(),
        holding_ref: v.holding_ref.trim().into(),
    };
    validate_route(&r)?;
    Ok(r)
}
fn validate_route(v: &SupportTenantRoute) -> Result<(), Status> {
    if [&v.tenant_id, &v.holding_ref]
        .iter()
        .any(|v| v.is_empty() || v.len() > MAX_ROUTE_LENGTH)
    {
        Err(invalid("tenant route is invalid"))
    } else {
        Ok(())
    }
}
fn validate_route_scope(r: &SupportTenantRoute, s: &TenantScope) -> Result<(), Status> {
    let holding = match s {
        TenantScope::Platform => return Ok(()),
        TenantScope::Holding { holding_id }
        | TenantScope::Company { holding_id, .. }
        | TenantScope::Branch { holding_id, .. }
        | TenantScope::Subtree { holding_id, .. } => holding_id,
    };
    if parse_uuid(&r.holding_ref, "holding_ref")? == *holding {
        Ok(())
    } else {
        Err(invalid("tenant route does not match Support scope"))
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
            .map(KernelSessionId::try_from_uuid)
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
            .map(|p| common::EventPosition {
                event_id: p.event_id.to_string(),
                global_sequence: p.global_sequence,
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
                .map(|c| common::ProjectionCheckpoint {
                    projection: c.projection.clone(),
                    partition: c.partition.clone(),
                    event_id: c.event_id.to_string(),
                    global_sequence: c.global_sequence,
                    updated_at: c.updated_at.to_rfc3339(),
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
    Status::internal("Support response mismatch")
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
fn status_from_auth(v: SupportAuthenticationError) -> Status {
    match v {
        SupportAuthenticationError::Unauthenticated => {
            Status::unauthenticated("authentication required")
        }
        SupportAuthenticationError::Denied => Status::permission_denied("Support access denied"),
        SupportAuthenticationError::Unavailable => {
            Status::unavailable("authentication unavailable")
        }
        SupportAuthenticationError::Internal => Status::internal("authentication failed"),
    }
}
fn invalid(v: &str) -> Status {
    Status::invalid_argument(v.to_owned())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use async_trait::async_trait;
    use erpx_cqrs::{CommandEnvelope, IdempotencyKey};
    use erpx_kernel::{
        ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, TenantScope,
    };
    use erpx_proto::common;
    use erpx_support::SupportAction;
    use tonic::{Response, metadata::MetadataMap};
    use uuid::Uuid;

    use super::{
        AuthenticatedSupportRequest, DecodedCommand, SecureCapabilityIssueMaterial,
        SupportAuthenticationError, SupportAuthenticationRequest, SupportCapabilityDeliverySecret,
        SupportRequestAuthenticator, SupportTenantRoute,
    };

    struct MustNotAuthenticate {
        called: AtomicBool,
    }

    struct ObserveAction {
        expected: SupportAction,
        called: AtomicBool,
    }

    #[async_trait]
    impl SupportRequestAuthenticator for MustNotAuthenticate {
        async fn authenticate(
            &self,
            _request: SupportAuthenticationRequest,
        ) -> Result<AuthenticatedSupportRequest, SupportAuthenticationError> {
            self.called.store(true, Ordering::SeqCst);
            Err(SupportAuthenticationError::Internal)
        }
    }

    #[async_trait]
    impl SupportRequestAuthenticator for ObserveAction {
        async fn authenticate(
            &self,
            request: SupportAuthenticationRequest,
        ) -> Result<AuthenticatedSupportRequest, SupportAuthenticationError> {
            assert_eq!(request.action(), self.expected);
            self.called.store(true, Ordering::SeqCst);
            Err(SupportAuthenticationError::Denied)
        }
    }

    fn decoded_command(mfa_code: Option<&str>) -> DecodedCommand {
        let holding_id = Uuid::from_u128(1);
        DecodedCommand {
            route: SupportTenantRoute {
                tenant_id: "tenant-1".into(),
                holding_ref: holding_id.to_string(),
            },
            envelope: CommandEnvelope {
                contract_version: erpx_cqrs::CONTRACT_VERSION,
                command_id: CommandId::try_from_uuid(Uuid::from_u128(2)).unwrap(),
                idempotency_key: IdempotencyKey::try_new("support-command-1").unwrap(),
                correlation_id: CorrelationId::try_from_uuid(Uuid::from_u128(3)).unwrap(),
                causation_id: None,
                actor: ActorAttribution {
                    principal_ref: Uuid::from_u128(4).to_string(),
                    kind: ActorKind::TenantUser,
                    session_id: None,
                    represented_principal_ref: None,
                    support_capability_id: None,
                },
                tenant_scope: TenantScope::Holding { holding_id },
                audit: AuditContext {
                    purpose: "support transport MFA test".into(),
                    customer_visible: false,
                    module: Some("support".into()),
                },
                expected_aggregate_version: None,
                command: (),
            },
            subscription_id: Some("support".into()),
            mfa_code: mfa_code.map(str::to_owned),
        }
    }

    async fn assert_rejected_before_authentication(action: SupportAction) {
        for mfa_code in [None, Some(" \t")] {
            let authenticator = MustNotAuthenticate {
                called: AtomicBool::new(false),
            };
            let result = super::authenticate_command(
                &authenticator,
                MetadataMap::new(),
                decoded_command(mfa_code),
                action,
            )
            .await;
            let error = match result {
                Ok(_) => panic!("high-risk Support action accepted absent current MFA"),
                Err(error) => error,
            };
            assert_eq!(error.code(), tonic::Code::FailedPrecondition);
            assert_eq!(error.message(), super::SUPPORT_MFA_REQUIRED_MESSAGE);
            assert!(!authenticator.called.load(Ordering::SeqCst));
        }
    }

    #[test]
    fn customer_encoder_cannot_serialize_internal_notes() {
        let source = include_str!("support_cqrs.rs");
        let start = source.find("fn encode_customer_ticket").unwrap();
        let end = source[start..].find("fn encode_internal_ticket").unwrap() + start;
        assert!(!source[start..end].contains("internal_messages"));
    }
    #[test]
    fn capability_transport_has_no_raw_material() {
        let source = include_str!("support_cqrs.rs");
        assert!(!source.contains(concat!("capability_", "secret")));
        assert!(!source.contains(concat!("one_time_", "credential")));
    }

    #[test]
    fn successful_delivery_helper_always_sets_binary_metadata() {
        let mut bytes = [0_u8; 32];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = (index as u8).wrapping_mul(17);
        }

        let response = super::attach_delivery_material(
            Response::new(()),
            SupportCapabilityDeliverySecret::from_bytes(bytes),
        );
        let delivered = response
            .metadata()
            .get_bin(super::SUPPORT_CAPABILITY_DELIVERY_METADATA)
            .expect("successful delivery must carry agent material")
            .to_bytes()
            .expect("binary delivery metadata must decode");

        assert_eq!(delivered.as_ref(), &bytes);
        assert!(delivered.iter().any(|byte| !byte.is_ascii()));
    }

    #[test]
    fn delivery_material_debug_is_redacted_and_uses_zeroizing_storage() {
        let bytes = [0xa5_u8; 32];
        let material = SecureCapabilityIssueMaterial {
            evidence: erpx_support::CapabilityEvidence {
                device_ref: "support-device".into(),
                mfa_verified_at: None,
                segregation_of_duties_passed: true,
                authority_scope: TenantScope::Holding {
                    holding_id: Uuid::from_u128(91),
                },
            },
            secret_hash_sha256: bytes,
            delivery_secret: SupportCapabilityDeliverySecret::from_bytes(bytes),
        };
        let material_debug = format!("{material:?}");
        assert!(material_debug.contains("REDACTED"));
        assert!(!material_debug.contains("165"));

        let secret = SupportCapabilityDeliverySecret::from_bytes(bytes);
        assert_eq!(
            format!("{secret:?}"),
            "SupportCapabilityDeliverySecret(REDACTED)"
        );
        assert!(!format!("{secret:?}").contains("165"));

        assert!(std::any::type_name_of_val(&secret.0).contains("Zeroizing"));
    }

    #[tokio::test]
    async fn approve_session_requires_current_mfa_before_authentication() {
        assert_rejected_before_authentication(SupportAction::ApproveSession).await;
    }

    #[tokio::test]
    async fn issue_capability_requires_current_mfa_before_authentication() {
        assert_rejected_before_authentication(SupportAction::IssueCapability).await;
    }

    #[tokio::test]
    async fn revoke_capability_requires_current_mfa_before_authentication() {
        assert_rejected_before_authentication(SupportAction::RevokeCapability).await;
    }

    #[tokio::test]
    async fn exact_high_risk_action_reaches_authenticator_when_mfa_is_present() {
        let authenticator = ObserveAction {
            expected: SupportAction::RevokeCapability,
            called: AtomicBool::new(false),
        };
        let result = super::authenticate_command(
            &authenticator,
            MetadataMap::new(),
            decoded_command(Some("present")),
            SupportAction::RevokeCapability,
        )
        .await;
        let error = match result {
            Ok(_) => panic!("test authenticator unexpectedly accepted request"),
            Err(error) => error,
        };
        assert_eq!(error.code(), tonic::Code::PermissionDenied);
        assert!(authenticator.called.load(Ordering::SeqCst));
    }

    #[test]
    fn action_and_audience_distinctions_are_not_collapsed() {
        use erpx_support::{
            GetSupportTicket, ListSupportTickets, SupportQuery, SupportTicketId,
            ports::{SupportTicketListFilter, TicketReadAudience},
        };

        let ticket_id = SupportTicketId::try_from_uuid(Uuid::from_u128(8)).unwrap();
        assert_eq!(
            super::query_action(&SupportQuery::GetTicket(GetSupportTicket {
                ticket_id,
                audience: TicketReadAudience::Customer,
            })),
            SupportAction::ViewCustomerTicket
        );
        assert_eq!(
            super::query_action(&SupportQuery::GetTicket(GetSupportTicket {
                ticket_id,
                audience: TicketReadAudience::Internal,
            })),
            SupportAction::ViewInternalTicket
        );

        let customer = SupportQuery::ListTickets(ListSupportTickets {
            filter: SupportTicketListFilter {
                status: None,
                customer: None,
                audience: TicketReadAudience::Customer,
                page_size: 10,
                cursor: None,
            },
        });
        let internal = SupportQuery::ListTickets(ListSupportTickets {
            filter: SupportTicketListFilter {
                status: None,
                customer: None,
                audience: TicketReadAudience::Internal,
                page_size: 10,
                cursor: None,
            },
        });
        assert_eq!(
            super::query_action(&customer),
            SupportAction::ListCustomerTickets
        );
        assert_eq!(
            super::query_action(&internal),
            SupportAction::ListInternalTickets
        );

        let metadata = common::CommandRequestMetadata {
            mfa_code: Some("present".into()),
            ..Default::default()
        };
        assert!(
            super::require_current_mfa(
                SupportAction::IssueCapability,
                metadata.mfa_code.as_deref()
            )
            .is_ok()
        );
    }
}
