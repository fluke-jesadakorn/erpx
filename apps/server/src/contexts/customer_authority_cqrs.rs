//! Typed gRPC inbound adapter for Customer Authority.
//!
//! Runtime composition supplies authentication and application ports. This
//! module is storage agnostic and never exposes sensitive identity fields on
//! query responses.

use std::sync::Arc;

use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandReceipt, CqrsError, FreshnessState, IdempotencyKey,
    ProjectionCheckpoint, ProjectionMetadata, QueryConsistency, QueryEnvelope, QueryMetadata,
    QueryResult, ReadYourWrites,
};
use erpx_customer_authority::{
    ChangeCustomerStatus, CreateCustomer, CreateCustomerResult, CustomerAuthorityMutationResult,
    CustomerAuthorityView, CustomerCode, CustomerId, CustomerIdentity, CustomerList,
    CustomerListFilter, CustomerStatus, GetCustomer, GetCustomerResult, LinkCustomerAuthority,
    ListCustomers, MergeCustomer, UnlinkCustomerAuthority, UpdateCustomerIdentity,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    QueryId, SessionId, TenantScope,
};
use erpx_proto::{common, platform};
use tonic::{Request, Response, Status, metadata::MetadataMap};
use uuid::Uuid;

use platform::customer_authority_command_service_server::CustomerAuthorityCommandService;
use platform::customer_authority_query_service_server::CustomerAuthorityQueryService;

const MAX_ROUTE_LENGTH: usize = 255;
const MAX_SECURITY_INPUT_LENGTH: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerAuthorityRoute {
    pub tenant_id: String,
    pub holding_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomerAuthorityAccessMode {
    Read,
    Write,
}

/// Untrusted transport claims. This type deliberately has no `Debug`
/// implementation because metadata and MFA input may contain credentials.
pub struct CustomerAuthorityAuthenticationRequest {
    transport_metadata: MetadataMap,
    presented_actor: ActorAttribution,
    presented_scope: TenantScope,
    presented_route: CustomerAuthorityRoute,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
    access_mode: CustomerAuthorityAccessMode,
}

impl CustomerAuthorityAuthenticationRequest {
    pub fn transport_metadata(&self) -> &MetadataMap {
        &self.transport_metadata
    }
    pub const fn presented_actor(&self) -> &ActorAttribution {
        &self.presented_actor
    }
    pub const fn presented_scope(&self) -> &TenantScope {
        &self.presented_scope
    }
    pub const fn presented_route(&self) -> &CustomerAuthorityRoute {
        &self.presented_route
    }
    pub fn subscription_id(&self) -> Option<&str> {
        self.subscription_id.as_deref()
    }
    pub fn mfa_code(&self) -> Option<&str> {
        self.mfa_code.as_deref()
    }
    pub const fn access_mode(&self) -> CustomerAuthorityAccessMode {
        self.access_mode
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedCustomerAuthorityRequest {
    actor: ActorAttribution,
    tenant_scope: TenantScope,
    route: CustomerAuthorityRoute,
}

impl AuthenticatedCustomerAuthorityRequest {
    pub fn try_new(
        actor: ActorAttribution,
        tenant_scope: TenantScope,
        route: CustomerAuthorityRoute,
    ) -> Result<Self, CustomerAuthorityAuthenticationError> {
        actor
            .validate()
            .map_err(|_| CustomerAuthorityAuthenticationError::Internal)?;
        tenant_scope
            .validate()
            .map_err(|_| CustomerAuthorityAuthenticationError::Internal)?;
        validate_route(&route).map_err(|_| CustomerAuthorityAuthenticationError::Internal)?;
        validate_route_scope(&route, &tenant_scope)
            .map_err(|_| CustomerAuthorityAuthenticationError::Internal)?;
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
    pub const fn route(&self) -> &CustomerAuthorityRoute {
        &self.route
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomerAuthorityAuthenticationError {
    Unauthenticated,
    Denied,
    Unavailable,
    Internal,
}

#[async_trait]
pub trait CustomerAuthorityRequestAuthenticator: Send + Sync + 'static {
    async fn authenticate(
        &self,
        request: CustomerAuthorityAuthenticationRequest,
    ) -> Result<AuthenticatedCustomerAuthorityRequest, CustomerAuthorityAuthenticationError>;
}

#[async_trait]
pub trait CustomerAuthorityCommandApplication: Send + Sync + 'static {
    async fn create(
        &self,
        route: CustomerAuthorityRoute,
        command: CommandEnvelope<CreateCustomer>,
    ) -> Result<CommandReceipt<CreateCustomerResult>, CqrsError>;
    async fn update_identity(
        &self,
        route: CustomerAuthorityRoute,
        command: CommandEnvelope<UpdateCustomerIdentity>,
    ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, CqrsError>;
    async fn change_status(
        &self,
        route: CustomerAuthorityRoute,
        command: CommandEnvelope<ChangeCustomerStatus>,
    ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, CqrsError>;
    async fn link_authority(
        &self,
        route: CustomerAuthorityRoute,
        command: CommandEnvelope<LinkCustomerAuthority>,
    ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, CqrsError>;
    async fn unlink_authority(
        &self,
        route: CustomerAuthorityRoute,
        command: CommandEnvelope<UnlinkCustomerAuthority>,
    ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, CqrsError>;
    async fn merge(
        &self,
        route: CustomerAuthorityRoute,
        command: CommandEnvelope<MergeCustomer>,
    ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, CqrsError>;
}

#[async_trait]
pub trait CustomerAuthorityQueryApplication: Send + Sync + 'static {
    async fn get(
        &self,
        route: CustomerAuthorityRoute,
        query: QueryEnvelope<GetCustomer>,
    ) -> Result<QueryResult<GetCustomerResult>, CqrsError>;
    async fn list(
        &self,
        route: CustomerAuthorityRoute,
        query: QueryEnvelope<ListCustomers>,
    ) -> Result<QueryResult<CustomerList>, CqrsError>;
}

#[derive(Debug, Clone)]
pub struct CustomerAuthorityCommandRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}

impl<A, G> CustomerAuthorityCommandRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CustomerAuthorityQueryRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}

impl<A, G> CustomerAuthorityQueryRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

#[async_trait]
impl<A, G> CustomerAuthorityCommandService for CustomerAuthorityCommandRpc<A, G>
where
    A: CustomerAuthorityCommandApplication,
    G: CustomerAuthorityRequestAuthenticator,
{
    async fn create(
        &self,
        request: Request<platform::CreateCustomerAuthorityRequest>,
    ) -> Result<Response<platform::CreateCustomerAuthorityResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            CreateCustomer {
                customer_id: decode_customer_id(&input.customer_id)?,
                code: CustomerCode::try_new(input.customer_code)
                    .map_err(|_| invalid("customer_code is invalid"))?,
                identity: decode_identity(input.identity)?,
                primary_authority: decode_scope(
                    input
                        .primary_authority
                        .ok_or_else(|| invalid("primary_authority is required"))?,
                )?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        let receipt = self
            .application
            .create(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::CreateCustomerAuthorityResponse {
            receipt: Some(encode_receipt(&receipt)),
            customer: Some(encode_customer(&receipt.response.customer)),
        }))
    }

    async fn update_identity(
        &self,
        request: Request<platform::UpdateCustomerAuthorityIdentityRequest>,
    ) -> Result<Response<platform::UpdateCustomerAuthorityIdentityResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            UpdateCustomerIdentity {
                customer_id: decode_customer_id(&input.customer_id)?,
                identity: decode_identity(input.identity)?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        let receipt = self
            .application
            .update_identity(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(
            platform::UpdateCustomerAuthorityIdentityResponse {
                receipt: Some(encode_receipt(&receipt)),
                customer: Some(encode_customer(&receipt.response.customer)),
            },
        ))
    }

    async fn change_status(
        &self,
        request: Request<platform::ChangeCustomerAuthorityStatusRequest>,
    ) -> Result<Response<platform::ChangeCustomerAuthorityStatusResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            ChangeCustomerStatus {
                customer_id: decode_customer_id(&input.customer_id)?,
                status: decode_status(input.status)?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        let receipt = self
            .application
            .change_status(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(
            platform::ChangeCustomerAuthorityStatusResponse {
                receipt: Some(encode_receipt(&receipt)),
                customer: Some(encode_customer(&receipt.response.customer)),
            },
        ))
    }

    async fn link_authority(
        &self,
        request: Request<platform::LinkCustomerAuthorityRequest>,
    ) -> Result<Response<platform::LinkCustomerAuthorityResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            LinkCustomerAuthority {
                customer_id: decode_customer_id(&input.customer_id)?,
                scope: decode_scope(
                    input
                        .authority
                        .ok_or_else(|| invalid("authority is required"))?,
                )?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        let receipt = self
            .application
            .link_authority(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::LinkCustomerAuthorityResponse {
            receipt: Some(encode_receipt(&receipt)),
            customer: Some(encode_customer(&receipt.response.customer)),
        }))
    }

    async fn unlink_authority(
        &self,
        request: Request<platform::UnlinkCustomerAuthorityRequest>,
    ) -> Result<Response<platform::UnlinkCustomerAuthorityResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            UnlinkCustomerAuthority {
                customer_id: decode_customer_id(&input.customer_id)?,
                scope: decode_scope(
                    input
                        .authority
                        .ok_or_else(|| invalid("authority is required"))?,
                )?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        let receipt = self
            .application
            .unlink_authority(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::UnlinkCustomerAuthorityResponse {
            receipt: Some(encode_receipt(&receipt)),
            customer: Some(encode_customer(&receipt.response.customer)),
        }))
    }

    async fn merge(
        &self,
        request: Request<platform::MergeCustomerAuthorityRequest>,
    ) -> Result<Response<platform::MergeCustomerAuthorityResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            MergeCustomer {
                retired_customer_id: decode_customer_id(&input.retired_customer_id)?,
                surviving_customer_id: decode_customer_id(&input.surviving_customer_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        let receipt = self
            .application
            .merge(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::MergeCustomerAuthorityResponse {
            receipt: Some(encode_receipt(&receipt)),
            retired_customer: Some(encode_customer(&receipt.response.customer)),
        }))
    }
}

#[async_trait]
impl<A, G> CustomerAuthorityQueryService for CustomerAuthorityQueryRpc<A, G>
where
    A: CustomerAuthorityQueryApplication,
    G: CustomerAuthorityRequestAuthenticator,
{
    async fn get(
        &self,
        request: Request<platform::GetCustomerAuthorityRequest>,
    ) -> Result<Response<platform::GetCustomerAuthorityResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_query(
            input.metadata,
            GetCustomer {
                customer_id: decode_customer_id(&input.customer_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), transport, decoded).await?;
        let result = self
            .application
            .get(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::GetCustomerAuthorityResponse {
            customer: result.value.customer.as_ref().map(encode_customer),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }

    async fn list(
        &self,
        request: Request<platform::ListCustomerAuthoritiesRequest>,
    ) -> Result<Response<platform::ListCustomerAuthoritiesResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let filter = input.filter.unwrap_or_default();
        let decoded = decode_query(
            input.metadata,
            ListCustomers {
                filter: CustomerListFilter {
                    search: optional_text(filter.search),
                    status: filter.status.map(decode_status).transpose()?,
                    include_merged: filter.include_merged,
                    page_size: u16::try_from(filter.page_size)
                        .map_err(|_| invalid("page_size is invalid"))?,
                    after_code: filter
                        .after_customer_code
                        .map(CustomerCode::try_new)
                        .transpose()
                        .map_err(|_| invalid("after_customer_code is invalid"))?,
                },
            },
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), transport, decoded).await?;
        let result = self
            .application
            .list(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::ListCustomerAuthoritiesResponse {
            customers: result.value.customers.iter().map(encode_customer).collect(),
            next_cursor: result.value.next_cursor.map(|code| code.as_str().into()),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }
}

struct DecodedCommand<C> {
    presented_route: CustomerAuthorityRoute,
    envelope: CommandEnvelope<C>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
}

struct DecodedQuery<Q> {
    presented_route: CustomerAuthorityRoute,
    envelope: QueryEnvelope<Q>,
}

async fn authenticate_command<C, G: CustomerAuthorityRequestAuthenticator>(
    authenticator: &G,
    transport_metadata: MetadataMap,
    decoded: DecodedCommand<C>,
) -> Result<(CustomerAuthorityRoute, CommandEnvelope<C>), Status> {
    let authentication = authenticator
        .authenticate(CustomerAuthorityAuthenticationRequest {
            transport_metadata,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.tenant_scope.clone(),
            presented_route: decoded.presented_route,
            subscription_id: decoded.subscription_id,
            mfa_code: decoded.mfa_code,
            access_mode: CustomerAuthorityAccessMode::Write,
        })
        .await
        .map_err(status_from_authentication)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authentication.actor().clone();
    envelope.tenant_scope = authentication.tenant_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated Customer Authority is invalid"))?;
    Ok((authentication.route().clone(), envelope))
}

async fn authenticate_query<Q, G: CustomerAuthorityRequestAuthenticator>(
    authenticator: &G,
    transport_metadata: MetadataMap,
    decoded: DecodedQuery<Q>,
) -> Result<(CustomerAuthorityRoute, QueryEnvelope<Q>), Status> {
    let authentication = authenticator
        .authenticate(CustomerAuthorityAuthenticationRequest {
            transport_metadata,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.tenant_scope.clone(),
            presented_route: decoded.presented_route,
            subscription_id: None,
            mfa_code: None,
            access_mode: CustomerAuthorityAccessMode::Read,
        })
        .await
        .map_err(status_from_authentication)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authentication.actor().clone();
    envelope.tenant_scope = authentication.tenant_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated Customer Authority is invalid"))?;
    Ok((authentication.route().clone(), envelope))
}

fn decode_command<C>(
    metadata: Option<common::CommandRequestMetadata>,
    command: C,
) -> Result<DecodedCommand<C>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("command metadata is required"))?;
    let route = decode_route(metadata.tenant_route)?;
    let correlation_id = parse_correlation_id(&metadata.correlation_id)?;
    let subscription_id = normalize_security_input(metadata.subscription_id)?;
    let mfa_code = normalize_security_input(metadata.mfa_code)?;
    let envelope = CommandEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract_version is invalid"))?,
        command_id: CommandId::try_from_uuid(parse_uuid(&metadata.command_id, "command_id")?)
            .map_err(|_| invalid("command_id is invalid"))?,
        idempotency_key: IdempotencyKey::try_new(metadata.idempotency_key)
            .map_err(|_| invalid("idempotency_key is invalid"))?,
        correlation_id,
        causation_id: metadata
            .causation_id
            .map(|value| parse_event_id(&value))
            .transpose()?,
        actor: decode_actor(metadata.actor)?,
        tenant_scope: decode_scope(
            metadata
                .tenant_scope
                .ok_or_else(|| invalid("tenant_scope is required"))?,
        )?,
        audit: decode_audit(metadata.audit, correlation_id)?,
        expected_aggregate_version: metadata.expected_aggregate_version,
        command,
    };
    envelope
        .validate()
        .map_err(|_| invalid("command metadata is invalid"))?;
    validate_route_scope(&route, &envelope.tenant_scope)?;
    Ok(DecodedCommand {
        presented_route: route,
        envelope,
        subscription_id,
        mfa_code,
    })
}

fn decode_query<Q>(
    metadata: Option<common::QueryRequestMetadata>,
    query: Q,
) -> Result<DecodedQuery<Q>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("query metadata is required"))?;
    let route = decode_route(metadata.tenant_route)?;
    let correlation_id = parse_correlation_id(&metadata.correlation_id)?;
    let envelope = QueryEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract_version is invalid"))?,
        query_id: QueryId::try_from_uuid(parse_uuid(&metadata.query_id, "query_id")?)
            .map_err(|_| invalid("query_id is invalid"))?,
        correlation_id,
        actor: decode_actor(metadata.actor)?,
        tenant_scope: decode_scope(
            metadata
                .tenant_scope
                .ok_or_else(|| invalid("tenant_scope is required"))?,
        )?,
        audit: decode_audit(metadata.audit, correlation_id)?,
        consistency: decode_consistency(metadata.consistency)?,
        query,
    };
    envelope
        .validate()
        .map_err(|_| invalid("query metadata is invalid"))?;
    validate_route_scope(&route, &envelope.tenant_scope)?;
    Ok(DecodedQuery {
        presented_route: route,
        envelope,
    })
}

fn decode_route(route: Option<common::TenantRoute>) -> Result<CustomerAuthorityRoute, Status> {
    let route = route.ok_or_else(|| invalid("tenant route is required"))?;
    let route = CustomerAuthorityRoute {
        tenant_id: route.tenant_id.trim().to_owned(),
        holding_ref: route.holding_ref.trim().to_owned(),
    };
    validate_route(&route)?;
    Ok(route)
}

fn validate_route(route: &CustomerAuthorityRoute) -> Result<(), Status> {
    for value in [&route.tenant_id, &route.holding_ref] {
        if value.trim().is_empty() || value.len() > MAX_ROUTE_LENGTH {
            return Err(invalid("tenant route is invalid"));
        }
    }
    Ok(())
}

fn validate_route_scope(route: &CustomerAuthorityRoute, scope: &TenantScope) -> Result<(), Status> {
    let expected_holding = match scope {
        TenantScope::Platform => return Ok(()),
        TenantScope::Holding { holding_id }
        | TenantScope::Company { holding_id, .. }
        | TenantScope::Branch { holding_id, .. }
        | TenantScope::Subtree { holding_id, .. } => holding_id,
    };
    let route_holding = parse_uuid(&route.holding_ref, "tenant route holding_ref")?;
    if &route_holding != expected_holding {
        return Err(invalid("tenant route does not match tenant scope"));
    }
    Ok(())
}

fn decode_actor(actor: Option<common::ActorAttribution>) -> Result<ActorAttribution, Status> {
    let actor = actor.ok_or_else(|| invalid("actor is required"))?;
    let kind = match common::ActorKind::try_from(actor.kind).ok() {
        Some(common::ActorKind::PlatformUser) => ActorKind::PlatformUser,
        Some(common::ActorKind::TenantUser) => ActorKind::TenantUser,
        Some(common::ActorKind::Service) => ActorKind::Service,
        Some(common::ActorKind::SystemWorker) => ActorKind::SystemWorker,
        _ => return Err(invalid("actor kind is invalid")),
    };
    Ok(ActorAttribution {
        principal_ref: actor.principal_ref,
        kind,
        session_id: parse_optional_uuid(&actor.session_id, "session_id")?
            .map(SessionId::try_from_uuid)
            .transpose()
            .map_err(|_| invalid("session_id is invalid"))?,
        represented_principal_ref: optional_nonempty(actor.represented_principal_ref),
        support_capability_id: parse_optional_uuid(
            &actor.support_capability_id,
            "support_capability_id",
        )?,
    })
}

fn decode_audit(
    audit: Option<common::AuditContext>,
    correlation_id: CorrelationId,
) -> Result<AuditContext, Status> {
    let audit = audit.ok_or_else(|| invalid("audit context is required"))?;
    if !audit.correlation_id.trim().is_empty()
        && parse_correlation_id(&audit.correlation_id)? != correlation_id
    {
        return Err(invalid("audit correlation does not match request"));
    }
    Ok(AuditContext {
        purpose: audit.purpose,
        customer_visible: audit.customer_visible,
        module: optional_nonempty(audit.module),
    })
}

fn decode_scope(scope: common::TenantScope) -> Result<TenantScope, Status> {
    use common::tenant_scope::Scope;
    let scope = match scope
        .scope
        .ok_or_else(|| invalid("tenant scope is empty"))?
    {
        Scope::Platform(_) => TenantScope::Platform,
        Scope::Holding(value) => TenantScope::Holding {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
        },
        Scope::Company(value) => TenantScope::Company {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_id: parse_uuid(&value.company_id, "company_id")?,
        },
        Scope::Branch(value) => TenantScope::Branch {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_id: parse_uuid(&value.company_id, "company_id")?,
            branch_id: parse_uuid(&value.branch_id, "branch_id")?,
        },
        Scope::Subtree(value) => TenantScope::Subtree {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_ids: value
                .company_ids
                .iter()
                .map(|value| parse_uuid(value, "company_id"))
                .collect::<Result<_, _>>()?,
            branch_ids: value
                .branch_ids
                .iter()
                .map(|value| parse_uuid(value, "branch_id"))
                .collect::<Result<_, _>>()?,
        },
    };
    scope
        .validate()
        .map_err(|_| invalid("tenant scope is invalid"))?;
    Ok(scope)
}

fn decode_consistency(
    consistency: Option<common::QueryConsistency>,
) -> Result<QueryConsistency, Status> {
    use common::query_consistency::Requirement;
    match consistency.and_then(|value| value.requirement) {
        None | Some(Requirement::Eventual(_)) => Ok(QueryConsistency::Eventual),
        Some(Requirement::ReadYourWrites(value)) => {
            Ok(QueryConsistency::ReadYourWrites(ReadYourWrites {
                projection: value.projection,
                partition: value.partition,
                target: erpx_cqrs::ProjectionTarget {
                    event_id: parse_event_id(&value.event_id)?,
                    global_sequence: value.global_sequence,
                },
                timeout_millis: value.timeout_millis,
            }))
        }
        Some(Requirement::AggregateVersion(_)) => Err(invalid(
            "aggregate consistency is not supported by Customer Authority queries",
        )),
    }
}

fn decode_identity(
    identity: Option<platform::CustomerAuthorityIdentityInput>,
) -> Result<CustomerIdentity, Status> {
    let identity = identity.ok_or_else(|| invalid("identity is required"))?;
    CustomerIdentity::try_new(
        identity.trading_name,
        identity.legal_name,
        identity.registration_number,
        identity.tax_identifier,
        identity.country,
    )
    .map_err(|_| invalid("customer identity is invalid"))
}

fn decode_status(value: i32) -> Result<CustomerStatus, Status> {
    match platform::CustomerAuthorityStatus::try_from(value).ok() {
        Some(platform::CustomerAuthorityStatus::Prospect) => Ok(CustomerStatus::Prospect),
        Some(platform::CustomerAuthorityStatus::Onboarding) => Ok(CustomerStatus::Onboarding),
        Some(platform::CustomerAuthorityStatus::Active) => Ok(CustomerStatus::Active),
        Some(platform::CustomerAuthorityStatus::Suspended) => Ok(CustomerStatus::Suspended),
        Some(platform::CustomerAuthorityStatus::Churned) => Ok(CustomerStatus::Churned),
        _ => Err(invalid("customer status is invalid")),
    }
}

fn encode_receipt<R>(receipt: &CommandReceipt<R>) -> common::CommandReceipt {
    common::CommandReceipt {
        command_id: receipt.command_id.to_string(),
        correlation_id: receipt.correlation_id.to_string(),
        aggregate: Some(common::AggregateVersion {
            context: receipt.aggregate.context.clone(),
            aggregate_type: receipt.aggregate.aggregate_type.clone(),
            aggregate_id: receipt.aggregate.aggregate_id.to_string(),
            version: receipt.aggregate.version,
        }),
        committed_events: receipt
            .committed_events
            .iter()
            .map(|event| common::EventPosition {
                event_id: event.event_id.to_string(),
                global_sequence: event.global_sequence,
            })
            .collect(),
        committed_at: receipt.committed_at.to_rfc3339(),
        replayed: receipt.replayed,
    }
}

fn encode_query_metadata(metadata: &QueryMetadata) -> common::QueryMetadata {
    common::QueryMetadata {
        correlation_id: metadata.correlation_id.to_string(),
        projection: Some(encode_projection_metadata(&metadata.projection)),
        consistency: Some(encode_consistency(&metadata.consistency)),
    }
}

fn encode_projection_metadata(metadata: &ProjectionMetadata) -> common::ProjectionMetadata {
    common::ProjectionMetadata {
        checkpoint: metadata.checkpoint.as_ref().map(encode_checkpoint),
        freshness: match metadata.freshness {
            FreshnessState::Current => common::ProjectionFreshnessState::Current as i32,
            FreshnessState::Stale => common::ProjectionFreshnessState::Stale as i32,
            FreshnessState::Rebuilding => common::ProjectionFreshnessState::Rebuilding as i32,
            FreshnessState::Unavailable => common::ProjectionFreshnessState::Unavailable as i32,
        },
        lag_millis: metadata.lag_millis,
    }
}

fn encode_checkpoint(checkpoint: &ProjectionCheckpoint) -> common::ProjectionCheckpoint {
    common::ProjectionCheckpoint {
        projection: checkpoint.projection.clone(),
        partition: checkpoint.partition.clone(),
        event_id: checkpoint.event_id.to_string(),
        global_sequence: checkpoint.global_sequence,
        updated_at: checkpoint.updated_at.to_rfc3339(),
    }
}

fn encode_consistency(consistency: &QueryConsistency) -> common::QueryConsistency {
    use common::query_consistency::Requirement;
    let requirement = match consistency {
        QueryConsistency::Eventual => Requirement::Eventual(common::EventualConsistency {}),
        QueryConsistency::ReadYourWrites(value) =>
        {
            #[allow(deprecated)]
            Requirement::ReadYourWrites(common::ReadYourWrites {
                command_id: String::new(),
                projection: value.projection.clone(),
                partition: value.partition.clone(),
                event_id: value.target.event_id.to_string(),
                global_sequence: value.target.global_sequence,
                timeout_millis: value.timeout_millis,
            })
        }
    };
    common::QueryConsistency {
        requirement: Some(requirement),
    }
}

fn encode_customer(view: &CustomerAuthorityView) -> platform::CustomerAuthorityRecord {
    platform::CustomerAuthorityRecord {
        customer_id: view.customer_id.as_uuid().to_string(),
        aggregate_version: view.version,
        customer_code: view.code.as_str().into(),
        trading_name: view.trading_name.clone(),
        legal_name: view.legal_name.clone(),
        country: view.country.clone(),
        status: match view.status {
            CustomerStatus::Prospect => platform::CustomerAuthorityStatus::Prospect as i32,
            CustomerStatus::Onboarding => platform::CustomerAuthorityStatus::Onboarding as i32,
            CustomerStatus::Active => platform::CustomerAuthorityStatus::Active as i32,
            CustomerStatus::Suspended => platform::CustomerAuthorityStatus::Suspended as i32,
            CustomerStatus::Churned => platform::CustomerAuthorityStatus::Churned as i32,
        },
        primary_authority: Some(encode_scope(&view.primary_authority)),
        authority_links: view.authority_links.iter().map(encode_scope).collect(),
        merged_into_customer_id: view.merged_into.map(|id| id.as_uuid().to_string()),
        created_at: view.created_at.to_rfc3339(),
        updated_at: view.updated_at.to_rfc3339(),
    }
}

fn encode_scope(scope: &TenantScope) -> common::TenantScope {
    use common::tenant_scope::Scope;
    let scope = match scope {
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
    };
    common::TenantScope { scope: Some(scope) }
}

fn parse_uuid(value: &str, field: &'static str) -> Result<Uuid, Status> {
    let value = Uuid::parse_str(value.trim()).map_err(|_| invalid(field))?;
    if value.is_nil() {
        return Err(invalid(field));
    }
    Ok(value)
}

fn parse_optional_uuid(value: &str, field: &'static str) -> Result<Option<Uuid>, Status> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        parse_uuid(value, field).map(Some)
    }
}

fn parse_event_id(value: &str) -> Result<EventId, Status> {
    EventId::try_from_uuid(parse_uuid(value, "event_id")?)
        .map_err(|_| invalid("event_id is invalid"))
}

fn parse_correlation_id(value: &str) -> Result<CorrelationId, Status> {
    CorrelationId::try_from_uuid(parse_uuid(value, "correlation_id")?)
        .map_err(|_| invalid("correlation_id is invalid"))
}

fn decode_customer_id(value: &str) -> Result<CustomerId, Status> {
    CustomerId::try_from_uuid(parse_uuid(value, "customer_id")?)
        .map_err(|_| invalid("customer_id is invalid"))
}

fn optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_owned();
        (!value.is_empty()).then_some(value)
    })
}

fn optional_nonempty(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn normalize_security_input(value: Option<String>) -> Result<Option<String>, Status> {
    let value = optional_text(value);
    if value
        .as_ref()
        .is_some_and(|value| value.len() > MAX_SECURITY_INPUT_LENGTH)
    {
        return Err(invalid("security metadata is invalid"));
    }
    Ok(value)
}

fn invalid(message: &'static str) -> Status {
    Status::invalid_argument(message)
}

fn status_from_cqrs(error: CqrsError) -> Status {
    match error.category() {
        ErrorCategory::Invalid => Status::invalid_argument("Customer Authority request is invalid"),
        ErrorCategory::Denied => Status::permission_denied("Customer Authority request is denied"),
        ErrorCategory::NotFound => Status::not_found("Customer was not found"),
        ErrorCategory::Conflict => Status::aborted("Customer has changed"),
        ErrorCategory::NotReady => Status::unavailable("Customer projection is not ready"),
        ErrorCategory::Unavailable => Status::unavailable("Customer Authority is unavailable"),
        ErrorCategory::Internal => Status::internal("Customer Authority failed"),
    }
}

fn status_from_authentication(error: CustomerAuthorityAuthenticationError) -> Status {
    match error {
        CustomerAuthorityAuthenticationError::Unauthenticated => {
            Status::unauthenticated("authentication is required")
        }
        CustomerAuthorityAuthenticationError::Denied => {
            Status::permission_denied("Customer Authority request is denied")
        }
        CustomerAuthorityAuthenticationError::Unavailable => {
            Status::unavailable("authentication is unavailable")
        }
        CustomerAuthorityAuthenticationError::Internal => Status::internal("authentication failed"),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    #[test]
    fn query_record_is_access_safe_by_construction() {
        let view = CustomerAuthorityView {
            customer_id: CustomerId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            version: 1,
            code: CustomerCode::try_new("CUS-1").unwrap(),
            trading_name: "Acme".into(),
            legal_name: Some("Acme Limited".into()),
            country: "TH".into(),
            status: CustomerStatus::Active,
            primary_authority: TenantScope::Company {
                holding_id: Uuid::from_u128(2),
                company_id: Uuid::from_u128(3),
            },
            authority_links: vec![TenantScope::Company {
                holding_id: Uuid::from_u128(2),
                company_id: Uuid::from_u128(3),
            }],
            merged_into: None,
            created_at: Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap(),
            updated_at: Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap(),
        };
        let encoded = encode_customer(&view);
        assert_eq!(encoded.customer_id, Uuid::from_u128(1).to_string());
        let rendered = format!("{encoded:?}");
        assert!(!rendered.contains("registration-secret"));
        assert!(!rendered.contains("tax-secret"));
    }

    #[test]
    fn identity_input_is_validated_at_transport_boundary() {
        let invalid_identity = platform::CustomerAuthorityIdentityInput {
            trading_name: String::new(),
            legal_name: None,
            registration_number: None,
            tax_identifier: None,
            country: "TH".into(),
        };
        assert!(decode_identity(Some(invalid_identity)).is_err());
    }

    #[test]
    fn source_is_storage_agnostic() {
        let source = include_str!("customer_authority_cqrs.rs");
        let production = source
            .split_once(concat!("#[cfg", "(test)]"))
            .map_or(source, |(production, _)| production);
        for forbidden in [
            concat!("sql", "x"),
            concat!("Command", "Store"),
            concat!("Pg", "Pool"),
        ] {
            assert!(
                !production.contains(forbidden),
                "production transport contains {forbidden}"
            );
        }
    }
}
