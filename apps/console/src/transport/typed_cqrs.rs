//! Shared client-side boundary for the typed CQRS v1 services.
//!
//! Feature gateways depend on the neutral types and service traits in this
//! module. Context-specific transport adapters are the only callers allowed to
//! unwrap [`WireCommand`] and [`WireQuery`], which keeps generated protobuf and
//! Tonic types out of presenters and views.

// This foundation is registered before context adapters migrate to it during
// the coordinated v1 cutover, so portions of its crate-private surface are
// intentionally dormant at the registration barrier.
#![allow(dead_code)]

use std::{fmt, sync::Arc};

use async_trait::async_trait;
use erpx_cqrs::{
    CONTRACT_VERSION, MAX_IDEMPOTENCY_KEY_LENGTH, MAX_PARTITION_KEY_LENGTH,
    MAX_PROJECTION_NAME_LENGTH, MAX_READ_YOUR_WRITES_WAIT,
};
use erpx_proto::common::v1::{
    ActorAttribution, ActorKind, AuditContext, BranchScope, CommandReceipt, CommandRequestMetadata,
    CompanyScope, EventualConsistency, HoldingScope, PlatformScope, ProjectionFreshnessState,
    QueryConsistency, QueryMetadata, QueryRequestMetadata, ReadYourWrites, SubtreeScope,
    TenantRoute as WireTenantRoute, TenantScope, query_consistency, tenant_scope,
};
use tonic::metadata::MetadataValue;
use uuid::Uuid;

const MAX_ACCESS_TOKEN_LENGTH: usize = 16_384;
const MAX_ENDPOINT_LENGTH: usize = 2_048;
const MAX_REFERENCE_LENGTH: usize = 256;
const MAX_AUDIT_PURPOSE_LENGTH: usize = 256;
const MAX_MFA_CODE_LENGTH: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransportErrorKind {
    InvalidRequest,
    InvalidResponse,
    AuthenticationRequired,
    AuthorizationDenied,
    NotFound,
    NotReady,
    Conflict,
    Rejected,
    Throttled,
    Offline,
    Service,
}

/// A deliberately bounded error that is safe to pass to a presenter.
///
/// Server messages and endpoint details are never retained because they can
/// contain implementation or deployment information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TransportError {
    kind: TransportErrorKind,
    operation: &'static str,
}

impl TransportError {
    pub(crate) const fn new(kind: TransportErrorKind, operation: &'static str) -> Self {
        Self { kind, operation }
    }

    pub(crate) const fn kind(&self) -> TransportErrorKind {
        self.kind
    }

    pub(crate) const fn operation(&self) -> &'static str {
        self.operation
    }

    pub(in crate::transport) fn from_status(
        operation: &'static str,
        status: tonic::Status,
    ) -> Self {
        let kind = match status.code() {
            tonic::Code::Unauthenticated => TransportErrorKind::AuthenticationRequired,
            tonic::Code::PermissionDenied => TransportErrorKind::AuthorizationDenied,
            tonic::Code::NotFound => TransportErrorKind::NotFound,
            tonic::Code::Aborted | tonic::Code::AlreadyExists => TransportErrorKind::Conflict,
            tonic::Code::InvalidArgument | tonic::Code::OutOfRange => TransportErrorKind::Rejected,
            tonic::Code::FailedPrecondition
                if matches!(status.message(), "not_ready" | "projection_not_ready")
                    || status.message().ends_with("projection is not ready") =>
            {
                TransportErrorKind::NotReady
            }
            tonic::Code::FailedPrecondition => TransportErrorKind::Rejected,
            tonic::Code::ResourceExhausted => TransportErrorKind::Throttled,
            tonic::Code::Unavailable | tonic::Code::DeadlineExceeded | tonic::Code::Cancelled => {
                TransportErrorKind::Offline
            }
            tonic::Code::Unknown
            | tonic::Code::Internal
            | tonic::Code::DataLoss
            | tonic::Code::Unimplemented
            | tonic::Code::Ok => TransportErrorKind::Service,
        };
        Self::new(kind, operation)
    }
}

impl fmt::Display for TransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} transport failed ({:?})",
            self.operation, self.kind
        )
    }
}

impl std::error::Error for TransportError {}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct AuthenticatedSession {
    access_token: String,
    principal_ref: String,
    session_id: Uuid,
    actor_kind: ConsoleActorKind,
    represented_principal_ref: Option<String>,
    support_capability_id: Option<Uuid>,
}

impl fmt::Debug for AuthenticatedSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthenticatedSession")
            .field("access_token", &"[redacted]")
            .field("principal_ref", &self.principal_ref)
            .field("session_id", &self.session_id)
            .field("actor_kind", &self.actor_kind)
            .field("represented_principal_ref", &self.represented_principal_ref)
            .field("support_capability_id", &self.support_capability_id)
            .finish()
    }
}

impl AuthenticatedSession {
    pub(crate) fn try_new(
        access_token: impl Into<String>,
        principal_ref: impl Into<String>,
        session_id: impl AsRef<str>,
        actor_kind: ConsoleActorKind,
    ) -> Result<Self, TransportError> {
        let access_token = access_token.into();
        let principal_ref = principal_ref.into();
        validate_text(
            &access_token,
            MAX_ACCESS_TOKEN_LENGTH,
            "build authenticated session",
        )?;
        validate_reference(&principal_ref, "build authenticated session")?;
        let session_id = parse_uuid(session_id.as_ref(), "build authenticated session")?;
        Ok(Self {
            access_token,
            principal_ref,
            session_id,
            actor_kind,
            represented_principal_ref: None,
            support_capability_id: None,
        })
    }

    pub(crate) fn try_with_representation(
        mut self,
        represented_principal_ref: impl Into<String>,
        support_capability_id: impl AsRef<str>,
    ) -> Result<Self, TransportError> {
        if self.actor_kind != ConsoleActorKind::PlatformUser {
            return Err(TransportError::new(
                TransportErrorKind::InvalidRequest,
                "build represented authenticated session",
            ));
        }
        let represented_principal_ref = represented_principal_ref.into();
        validate_reference(
            &represented_principal_ref,
            "build represented authenticated session",
        )?;
        self.represented_principal_ref = Some(represented_principal_ref);
        self.support_capability_id = Some(parse_uuid(
            support_capability_id.as_ref(),
            "build represented authenticated session",
        )?);
        Ok(self)
    }

    fn actor(&self) -> ActorAttribution {
        ActorAttribution {
            principal_ref: self.principal_ref.clone(),
            kind: self.actor_kind.into(),
            session_id: self.session_id.to_string(),
            represented_principal_ref: self.represented_principal_ref.clone().unwrap_or_default(),
            support_capability_id: self
                .support_capability_id
                .map(|value| value.to_string())
                .unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConsoleActorKind {
    PlatformUser,
    TenantUser,
}

impl From<ConsoleActorKind> for i32 {
    fn from(value: ConsoleActorKind) -> Self {
        match value {
            ConsoleActorKind::PlatformUser => ActorKind::PlatformUser as i32,
            ConsoleActorKind::TenantUser => ActorKind::TenantUser as i32,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AuthorityScope {
    Platform,
    Holding {
        holding_id: Uuid,
    },
    Company {
        holding_id: Uuid,
        company_id: Uuid,
    },
    Branch {
        holding_id: Uuid,
        company_id: Uuid,
        branch_id: Uuid,
    },
    Subtree {
        holding_id: Uuid,
        company_ids: Vec<Uuid>,
        branch_ids: Vec<Uuid>,
    },
}

impl AuthorityScope {
    pub(crate) const fn platform() -> Self {
        Self::Platform
    }

    pub(crate) fn try_holding(holding_id: impl AsRef<str>) -> Result<Self, TransportError> {
        Ok(Self::Holding {
            holding_id: parse_uuid(holding_id.as_ref(), "build authority scope")?,
        })
    }

    pub(crate) fn try_company(
        holding_id: impl AsRef<str>,
        company_id: impl AsRef<str>,
    ) -> Result<Self, TransportError> {
        Ok(Self::Company {
            holding_id: parse_uuid(holding_id.as_ref(), "build authority scope")?,
            company_id: parse_uuid(company_id.as_ref(), "build authority scope")?,
        })
    }

    pub(crate) fn try_branch(
        holding_id: impl AsRef<str>,
        company_id: impl AsRef<str>,
        branch_id: impl AsRef<str>,
    ) -> Result<Self, TransportError> {
        Ok(Self::Branch {
            holding_id: parse_uuid(holding_id.as_ref(), "build authority scope")?,
            company_id: parse_uuid(company_id.as_ref(), "build authority scope")?,
            branch_id: parse_uuid(branch_id.as_ref(), "build authority scope")?,
        })
    }

    pub(crate) fn try_subtree<Companies, Company, Branches, Branch>(
        holding_id: impl AsRef<str>,
        company_ids: Companies,
        branch_ids: Branches,
    ) -> Result<Self, TransportError>
    where
        Companies: IntoIterator<Item = Company>,
        Company: AsRef<str>,
        Branches: IntoIterator<Item = Branch>,
        Branch: AsRef<str>,
    {
        let company_ids = company_ids
            .into_iter()
            .map(|value| parse_uuid(value.as_ref(), "build authority scope"))
            .collect::<Result<Vec<_>, _>>()?;
        let branch_ids = branch_ids
            .into_iter()
            .map(|value| parse_uuid(value.as_ref(), "build authority scope"))
            .collect::<Result<Vec<_>, _>>()?;
        if company_ids.is_empty() && branch_ids.is_empty() {
            return Err(TransportError::new(
                TransportErrorKind::InvalidRequest,
                "build authority scope",
            ));
        }
        let mut unique_companies = company_ids.clone();
        unique_companies.sort_unstable();
        unique_companies.dedup();
        let mut unique_branches = branch_ids.clone();
        unique_branches.sort_unstable();
        unique_branches.dedup();
        if unique_companies.len() != company_ids.len() || unique_branches.len() != branch_ids.len()
        {
            return Err(TransportError::new(
                TransportErrorKind::InvalidRequest,
                "build authority scope",
            ));
        }
        Ok(Self::Subtree {
            holding_id: parse_uuid(holding_id.as_ref(), "build authority scope")?,
            company_ids,
            branch_ids,
        })
    }

    fn to_wire(&self) -> TenantScope {
        let scope = match self {
            Self::Platform => tenant_scope::Scope::Platform(PlatformScope {}),
            Self::Holding { holding_id } => tenant_scope::Scope::Holding(HoldingScope {
                holding_id: holding_id.to_string(),
            }),
            Self::Company {
                holding_id,
                company_id,
            } => tenant_scope::Scope::Company(CompanyScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
            }),
            Self::Branch {
                holding_id,
                company_id,
                branch_id,
            } => tenant_scope::Scope::Branch(BranchScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
                branch_id: branch_id.to_string(),
            }),
            Self::Subtree {
                holding_id,
                company_ids,
                branch_ids,
            } => tenant_scope::Scope::Subtree(SubtreeScope {
                holding_id: holding_id.to_string(),
                company_ids: company_ids.iter().map(ToString::to_string).collect(),
                branch_ids: branch_ids.iter().map(ToString::to_string).collect(),
            }),
        };
        TenantScope { scope: Some(scope) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TenantRoute {
    tenant_id: Uuid,
    holding_ref: String,
}

impl TenantRoute {
    pub(crate) fn try_new(
        tenant_id: impl AsRef<str>,
        holding_ref: impl Into<String>,
    ) -> Result<Self, TransportError> {
        let holding_ref = holding_ref.into();
        validate_reference(&holding_ref, "build tenant route")?;
        Ok(Self {
            tenant_id: parse_uuid(tenant_id.as_ref(), "build tenant route")?,
            holding_ref,
        })
    }

    fn to_wire(&self) -> WireTenantRoute {
        WireTenantRoute {
            tenant_id: self.tenant_id.to_string(),
            holding_ref: self.holding_ref.clone(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct RequestContext {
    session: Arc<AuthenticatedSession>,
    authority: AuthorityScope,
    tenant_route: Option<TenantRoute>,
    module: String,
}

impl RequestContext {
    pub(crate) fn try_new(
        session: AuthenticatedSession,
        authority: AuthorityScope,
        tenant_route: Option<TenantRoute>,
        module: impl Into<String>,
    ) -> Result<Self, TransportError> {
        let module = module.into();
        validate_reference(&module, "build request context")?;
        if !matches!(authority, AuthorityScope::Platform) && tenant_route.is_none() {
            return Err(TransportError::new(
                TransportErrorKind::InvalidRequest,
                "build request context",
            ));
        }
        Ok(Self {
            session: Arc::new(session),
            authority,
            tenant_route,
            module,
        })
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct CommandOptions {
    idempotency_key: String,
    purpose: String,
    customer_visible: bool,
    expected_aggregate_version: Option<u64>,
    causation_id: Option<Uuid>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
}

impl fmt::Debug for CommandOptions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CommandOptions")
            .field("idempotency_key", &self.idempotency_key)
            .field("purpose", &self.purpose)
            .field("customer_visible", &self.customer_visible)
            .field(
                "expected_aggregate_version",
                &self.expected_aggregate_version,
            )
            .field("causation_id", &self.causation_id)
            .field("subscription_id", &self.subscription_id)
            .field("mfa_code", &self.mfa_code.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}

impl CommandOptions {
    pub(crate) fn try_new(
        idempotency_key: impl Into<String>,
        purpose: impl Into<String>,
    ) -> Result<Self, TransportError> {
        let idempotency_key = idempotency_key.into();
        let purpose = purpose.into();
        validate_text(
            &idempotency_key,
            MAX_IDEMPOTENCY_KEY_LENGTH,
            "build command metadata",
        )?;
        validate_text(&purpose, MAX_AUDIT_PURPOSE_LENGTH, "build command metadata")?;
        Ok(Self {
            idempotency_key,
            purpose,
            customer_visible: false,
            expected_aggregate_version: None,
            causation_id: None,
            subscription_id: None,
            mfa_code: None,
        })
    }

    pub(crate) const fn customer_visible(mut self, value: bool) -> Self {
        self.customer_visible = value;
        self
    }

    pub(crate) const fn expected_version(mut self, value: u64) -> Self {
        self.expected_aggregate_version = Some(value);
        self
    }

    pub(crate) fn try_causation_id(
        mut self,
        value: impl AsRef<str>,
    ) -> Result<Self, TransportError> {
        self.causation_id = Some(parse_uuid(value.as_ref(), "build command metadata")?);
        Ok(self)
    }

    pub(crate) fn try_subscription_id(
        mut self,
        value: impl Into<String>,
    ) -> Result<Self, TransportError> {
        let value = value.into();
        validate_reference(&value, "build command metadata")?;
        self.subscription_id = Some(value);
        Ok(self)
    }

    pub(crate) fn try_mfa_code(mut self, value: impl Into<String>) -> Result<Self, TransportError> {
        let value = value.into();
        validate_text(&value, MAX_MFA_CODE_LENGTH, "build command metadata")?;
        self.mfa_code = Some(value);
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QueryOptions {
    purpose: String,
    customer_visible: bool,
    consistency: ReadConsistency,
}

impl QueryOptions {
    pub(crate) fn eventual(purpose: impl Into<String>) -> Result<Self, TransportError> {
        let purpose = purpose.into();
        validate_text(&purpose, MAX_AUDIT_PURPOSE_LENGTH, "build query metadata")?;
        Ok(Self {
            purpose,
            customer_visible: false,
            consistency: ReadConsistency::Eventual,
        })
    }

    pub(crate) fn read_your_writes(
        purpose: impl Into<String>,
        receipt: &CommandReceiptView,
        projection: impl Into<String>,
        partition: impl Into<String>,
        timeout_millis: u64,
    ) -> Result<Self, TransportError> {
        let mut options = Self::eventual(purpose)?;
        options.consistency = receipt.read_your_writes(projection, partition, timeout_millis)?;
        Ok(options)
    }

    pub(crate) const fn customer_visible(mut self, value: bool) -> Self {
        self.customer_visible = value;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ReadConsistency {
    Eventual,
    ReadYourWrites {
        projection: String,
        partition: String,
        event_id: Uuid,
        global_sequence: u64,
        timeout_millis: u64,
    },
}

impl ReadConsistency {
    fn to_wire(&self) -> QueryConsistency {
        let requirement = match self {
            Self::Eventual => query_consistency::Requirement::Eventual(EventualConsistency {}),
            Self::ReadYourWrites {
                projection,
                partition,
                event_id,
                global_sequence,
                timeout_millis,
            } => query_consistency::Requirement::ReadYourWrites(ReadYourWrites {
                projection: projection.clone(),
                partition: partition.clone(),
                event_id: event_id.to_string(),
                global_sequence: *global_sequence,
                timeout_millis: *timeout_millis,
                ..Default::default()
            }),
        };
        QueryConsistency {
            requirement: Some(requirement),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AggregateRevision {
    pub(crate) context: String,
    pub(crate) aggregate_type: String,
    pub(crate) aggregate_id: String,
    pub(crate) version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EventReceipt {
    pub(crate) event_id: String,
    pub(crate) global_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandReceiptView {
    pub(crate) command_id: String,
    pub(crate) correlation_id: String,
    pub(crate) aggregate: AggregateRevision,
    pub(crate) committed_events: Vec<EventReceipt>,
    pub(crate) committed_at: String,
    pub(crate) replayed: bool,
}

impl CommandReceiptView {
    fn read_your_writes(
        &self,
        projection: impl Into<String>,
        partition: impl Into<String>,
        timeout_millis: u64,
    ) -> Result<ReadConsistency, TransportError> {
        let projection = projection.into();
        let partition = partition.into();
        validate_text(
            &projection,
            MAX_PROJECTION_NAME_LENGTH,
            "build read-your-writes query",
        )?;
        validate_text(
            &partition,
            MAX_PARTITION_KEY_LENGTH,
            "build read-your-writes query",
        )?;
        if std::time::Duration::from_millis(timeout_millis) > MAX_READ_YOUR_WRITES_WAIT {
            return Err(TransportError::new(
                TransportErrorKind::InvalidRequest,
                "build read-your-writes query",
            ));
        }
        let target = self.committed_events.last().ok_or_else(|| {
            TransportError::new(
                TransportErrorKind::InvalidResponse,
                "build read-your-writes query",
            )
        })?;
        if target.global_sequence == 0 {
            return Err(TransportError::new(
                TransportErrorKind::InvalidResponse,
                "build read-your-writes query",
            ));
        }
        Ok(ReadConsistency::ReadYourWrites {
            projection,
            partition,
            event_id: parse_response_uuid(&target.event_id, "decode command receipt")?,
            global_sequence: target.global_sequence,
            timeout_millis,
        })
    }
}

impl TryFrom<CommandReceipt> for CommandReceiptView {
    type Error = TransportError;

    fn try_from(receipt: CommandReceipt) -> Result<Self, Self::Error> {
        parse_response_uuid(&receipt.command_id, "decode command receipt")?;
        parse_response_uuid(&receipt.correlation_id, "decode command receipt")?;
        validate_text(
            &receipt.committed_at,
            MAX_REFERENCE_LENGTH,
            "decode command receipt",
        )?;
        chrono::DateTime::parse_from_rfc3339(&receipt.committed_at).map_err(|_| {
            TransportError::new(
                TransportErrorKind::InvalidResponse,
                "decode command receipt",
            )
        })?;
        let aggregate = receipt.aggregate.ok_or_else(|| {
            TransportError::new(
                TransportErrorKind::InvalidResponse,
                "decode command receipt",
            )
        })?;
        validate_reference_response(&aggregate.context, "decode command receipt")?;
        validate_reference_response(&aggregate.aggregate_type, "decode command receipt")?;
        parse_response_uuid(&aggregate.aggregate_id, "decode command receipt")?;
        if aggregate.version == 0 {
            return Err(TransportError::new(
                TransportErrorKind::InvalidResponse,
                "decode command receipt",
            ));
        }
        let committed_events = receipt
            .committed_events
            .into_iter()
            .map(|event| {
                parse_response_uuid(&event.event_id, "decode command receipt")?;
                if event.global_sequence == 0 {
                    return Err(TransportError::new(
                        TransportErrorKind::InvalidResponse,
                        "decode command receipt",
                    ));
                }
                Ok(EventReceipt {
                    event_id: event.event_id,
                    global_sequence: event.global_sequence,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            command_id: receipt.command_id,
            correlation_id: receipt.correlation_id,
            aggregate: AggregateRevision {
                context: aggregate.context,
                aggregate_type: aggregate.aggregate_type,
                aggregate_id: aggregate.aggregate_id,
                version: aggregate.version,
            },
            committed_events,
            committed_at: receipt.committed_at,
            replayed: receipt.replayed,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Freshness {
    Current,
    Stale,
    Rebuilding,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QueryMetadataView {
    pub(crate) correlation_id: String,
    pub(crate) freshness: Freshness,
    pub(crate) lag_millis: Option<u64>,
    pub(crate) checkpoint: Option<EventReceipt>,
}

impl TryFrom<QueryMetadata> for QueryMetadataView {
    type Error = TransportError;

    fn try_from(metadata: QueryMetadata) -> Result<Self, Self::Error> {
        parse_response_uuid(&metadata.correlation_id, "decode query metadata")?;
        let projection = metadata.projection.ok_or_else(|| {
            TransportError::new(TransportErrorKind::InvalidResponse, "decode query metadata")
        })?;
        let freshness = match ProjectionFreshnessState::try_from(projection.freshness) {
            Ok(ProjectionFreshnessState::Current) => Freshness::Current,
            Ok(ProjectionFreshnessState::Stale) => Freshness::Stale,
            Ok(ProjectionFreshnessState::Rebuilding) => Freshness::Rebuilding,
            Ok(ProjectionFreshnessState::Unavailable) => Freshness::Unavailable,
            Ok(ProjectionFreshnessState::Unspecified) | Err(_) => {
                return Err(TransportError::new(
                    TransportErrorKind::InvalidResponse,
                    "decode query metadata",
                ));
            }
        };
        let checkpoint = projection
            .checkpoint
            .map(|checkpoint| {
                validate_text(
                    &checkpoint.projection,
                    MAX_PROJECTION_NAME_LENGTH,
                    "decode query metadata",
                )?;
                validate_text(
                    &checkpoint.partition,
                    MAX_PARTITION_KEY_LENGTH,
                    "decode query metadata",
                )?;
                parse_response_uuid(&checkpoint.event_id, "decode query metadata")?;
                if checkpoint.global_sequence == 0 {
                    return Err(TransportError::new(
                        TransportErrorKind::InvalidResponse,
                        "decode query metadata",
                    ));
                }
                Ok(EventReceipt {
                    event_id: checkpoint.event_id,
                    global_sequence: checkpoint.global_sequence,
                })
            })
            .transpose()?;
        Ok(Self {
            correlation_id: metadata.correlation_id,
            freshness,
            lag_millis: projection.lag_millis,
            checkpoint,
        })
    }
}

pub(crate) trait RequestIdSource {
    fn next_id(&mut self) -> Result<Uuid, TransportError>;
}

#[derive(Debug, Default)]
pub(crate) struct RandomRequestIds;

impl RequestIdSource for RandomRequestIds {
    fn next_id(&mut self) -> Result<Uuid, TransportError> {
        Ok(Uuid::new_v4())
    }
}

pub(crate) struct CommandInvocation<C> {
    command: C,
    context: RequestContext,
    options: CommandOptions,
    command_id: Uuid,
    correlation_id: Uuid,
}

pub(crate) struct QueryInvocation<Q> {
    query: Q,
    context: RequestContext,
    options: QueryOptions,
    query_id: Uuid,
    correlation_id: Uuid,
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub(crate) trait TypedCommandService<C> {
    type Response;

    async fn execute_command(
        &mut self,
        invocation: CommandInvocation<C>,
    ) -> Result<Self::Response, TransportError>;
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub(crate) trait TypedQueryService<Q> {
    type Response;

    async fn execute_query(
        &mut self,
        invocation: QueryInvocation<Q>,
    ) -> Result<Self::Response, TransportError>;
}

/// Context-agnostic wrapper used by feature gateways and their fake services.
pub(crate) struct TypedServiceClient<S, I = RandomRequestIds> {
    service: S,
    context: RequestContext,
    ids: I,
}

impl<S> TypedServiceClient<S> {
    pub(crate) fn new(service: S, context: RequestContext) -> Self {
        Self {
            service,
            context,
            ids: RandomRequestIds,
        }
    }
}

impl<S, I> TypedServiceClient<S, I>
where
    I: RequestIdSource,
{
    #[cfg(test)]
    fn with_id_source(service: S, context: RequestContext, ids: I) -> Self {
        Self {
            service,
            context,
            ids,
        }
    }

    pub(crate) async fn command<C>(
        &mut self,
        options: CommandOptions,
        command: C,
    ) -> Result<<S as TypedCommandService<C>>::Response, TransportError>
    where
        S: TypedCommandService<C>,
    {
        let command_id = self.ids.next_id()?;
        let correlation_id = self.ids.next_id()?;
        self.service
            .execute_command(CommandInvocation {
                command,
                context: self.context.clone(),
                options,
                command_id,
                correlation_id,
            })
            .await
    }

    pub(crate) async fn query<Q>(
        &mut self,
        options: QueryOptions,
        query: Q,
    ) -> Result<<S as TypedQueryService<Q>>::Response, TransportError>
    where
        S: TypedQueryService<Q>,
    {
        let query_id = self.ids.next_id()?;
        let correlation_id = self.ids.next_id()?;
        self.service
            .execute_query(QueryInvocation {
                query,
                context: self.context.clone(),
                options,
                query_id,
                correlation_id,
            })
            .await
    }

    pub(crate) fn service(&self) -> &S {
        &self.service
    }

    pub(crate) fn into_service(self) -> S {
        self.service
    }
}

pub(in crate::transport) struct WireCommand<C> {
    pub(in crate::transport) command: C,
    pub(in crate::transport) metadata: CommandRequestMetadata,
    session: Arc<AuthenticatedSession>,
}

impl<C> WireCommand<C> {
    pub(in crate::transport) fn request<T>(
        &self,
        message: T,
    ) -> Result<tonic::Request<T>, TransportError> {
        authenticated_request(message, &self.session)
    }

    pub(in crate::transport) fn map_request<T>(
        self,
        build: impl FnOnce(C, CommandRequestMetadata) -> T,
    ) -> Result<tonic::Request<T>, TransportError> {
        let message = build(self.command, self.metadata);
        authenticated_request(message, &self.session)
    }
}

impl<C> CommandInvocation<C> {
    pub(in crate::transport) fn into_wire(self) -> WireCommand<C> {
        let correlation_id = self.correlation_id.to_string();
        let metadata = CommandRequestMetadata {
            contract_version: u32::from(CONTRACT_VERSION),
            command_id: self.command_id.to_string(),
            idempotency_key: self.options.idempotency_key,
            correlation_id: correlation_id.clone(),
            causation_id: self.options.causation_id.map(|value| value.to_string()),
            actor: Some(self.context.session.actor()),
            tenant_scope: Some(self.context.authority.to_wire()),
            audit: Some(AuditContext {
                purpose: self.options.purpose,
                customer_visible: self.options.customer_visible,
                module: self.context.module,
                correlation_id,
            }),
            expected_aggregate_version: self.options.expected_aggregate_version,
            subscription_id: self.options.subscription_id,
            mfa_code: self.options.mfa_code,
            tenant_route: self.context.tenant_route.map(|route| route.to_wire()),
        };
        WireCommand {
            command: self.command,
            metadata,
            session: self.context.session,
        }
    }
}

pub(in crate::transport) struct WireQuery<Q> {
    pub(in crate::transport) query: Q,
    pub(in crate::transport) metadata: QueryRequestMetadata,
    session: Arc<AuthenticatedSession>,
}

impl<Q> WireQuery<Q> {
    pub(in crate::transport) fn request<T>(
        &self,
        message: T,
    ) -> Result<tonic::Request<T>, TransportError> {
        authenticated_request(message, &self.session)
    }

    pub(in crate::transport) fn map_request<T>(
        self,
        build: impl FnOnce(Q, QueryRequestMetadata) -> T,
    ) -> Result<tonic::Request<T>, TransportError> {
        let message = build(self.query, self.metadata);
        authenticated_request(message, &self.session)
    }
}

impl<Q> QueryInvocation<Q> {
    pub(in crate::transport) fn into_wire(self) -> WireQuery<Q> {
        let correlation_id = self.correlation_id.to_string();
        let metadata = QueryRequestMetadata {
            contract_version: u32::from(CONTRACT_VERSION),
            query_id: self.query_id.to_string(),
            correlation_id: correlation_id.clone(),
            actor: Some(self.context.session.actor()),
            tenant_scope: Some(self.context.authority.to_wire()),
            audit: Some(AuditContext {
                purpose: self.options.purpose,
                customer_visible: self.options.customer_visible,
                module: self.context.module,
                correlation_id,
            }),
            consistency: Some(self.options.consistency.to_wire()),
            tenant_route: self.context.tenant_route.map(|route| route.to_wire()),
        };
        WireQuery {
            query: self.query,
            metadata,
            session: self.context.session,
        }
    }
}

fn authenticated_request<T>(
    message: T,
    session: &AuthenticatedSession,
) -> Result<tonic::Request<T>, TransportError> {
    let mut request = tonic::Request::new(message);
    insert_metadata(&mut request, "x-erpx-access-token", &session.access_token)?;
    insert_metadata(&mut request, "x-erpx-actor", &session.principal_ref)?;
    insert_metadata(
        &mut request,
        "x-erpx-session",
        &session.session_id.to_string(),
    )?;
    if let Some(capability_id) = session.support_capability_id {
        insert_metadata(
            &mut request,
            "x-erpx-support-capability-id",
            &capability_id.to_string(),
        )?;
    }
    Ok(request)
}

fn insert_metadata<T>(
    request: &mut tonic::Request<T>,
    key: &'static str,
    value: &str,
) -> Result<(), TransportError> {
    let value = MetadataValue::try_from(value).map_err(|_| {
        TransportError::new(
            TransportErrorKind::InvalidRequest,
            "build authenticated request",
        )
    })?;
    request.metadata_mut().insert(key, value);
    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub(in crate::transport) type GrpcChannel = tonic_web_wasm_client::Client;

#[cfg(not(target_arch = "wasm32"))]
pub(in crate::transport) type GrpcChannel = tonic::transport::Channel;

#[cfg(target_arch = "wasm32")]
pub(in crate::transport) async fn connect_grpc_channel(
    endpoint: &str,
) -> Result<GrpcChannel, TransportError> {
    let endpoint = validated_endpoint(endpoint)?;
    Ok(tonic_web_wasm_client::Client::new(endpoint))
}

#[cfg(not(target_arch = "wasm32"))]
pub(in crate::transport) async fn connect_grpc_channel(
    endpoint: &str,
) -> Result<GrpcChannel, TransportError> {
    let endpoint = validated_endpoint(endpoint)?;
    tonic::transport::Endpoint::from_shared(endpoint)
        .map_err(|_| TransportError::new(TransportErrorKind::InvalidRequest, "connect gRPC"))?
        .connect()
        .await
        .map_err(|_| TransportError::new(TransportErrorKind::Offline, "connect gRPC"))
}

fn validated_endpoint(endpoint: &str) -> Result<String, TransportError> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty()
        || endpoint.len() > MAX_ENDPOINT_LENGTH
        || !(endpoint.starts_with("https://") || endpoint.starts_with("http://"))
    {
        return Err(TransportError::new(
            TransportErrorKind::InvalidRequest,
            "connect gRPC",
        ));
    }
    Ok(endpoint.to_owned())
}

fn validate_text(
    value: &str,
    maximum: usize,
    operation: &'static str,
) -> Result<(), TransportError> {
    if value.trim().is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        return Err(TransportError::new(
            TransportErrorKind::InvalidRequest,
            operation,
        ));
    }
    Ok(())
}

fn validate_reference(value: &str, operation: &'static str) -> Result<(), TransportError> {
    validate_text(value, MAX_REFERENCE_LENGTH, operation)
}

fn validate_reference_response(value: &str, operation: &'static str) -> Result<(), TransportError> {
    if validate_reference(value, operation).is_err() {
        return Err(TransportError::new(
            TransportErrorKind::InvalidResponse,
            operation,
        ));
    }
    Ok(())
}

fn parse_uuid(value: &str, operation: &'static str) -> Result<Uuid, TransportError> {
    Uuid::parse_str(value)
        .ok()
        .filter(|value| !value.is_nil())
        .ok_or_else(|| TransportError::new(TransportErrorKind::InvalidRequest, operation))
}

fn parse_response_uuid(value: &str, operation: &'static str) -> Result<Uuid, TransportError> {
    Uuid::parse_str(value)
        .ok()
        .filter(|value| !value.is_nil())
        .ok_or_else(|| TransportError::new(TransportErrorKind::InvalidResponse, operation))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        future::Future,
        pin::pin,
        sync::Arc,
        task::{Context, Poll, Wake, Waker},
        thread,
    };

    use erpx_proto::common::v1::{AggregateVersion, EventPosition};

    use super::*;

    const SESSION_ID: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    const HOLDING_ID: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    const COMPANY_ID: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
    const TENANT_ID: &str = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";
    const COMMAND_ID: &str = "11111111-1111-4111-8111-111111111111";
    const CORRELATION_ID: &str = "22222222-2222-4222-8222-222222222222";
    const QUERY_ID: &str = "33333333-3333-4333-8333-333333333333";
    const QUERY_CORRELATION_ID: &str = "44444444-4444-4444-8444-444444444444";
    const AGGREGATE_ID: &str = "55555555-5555-4555-8555-555555555555";
    const EVENT_ID: &str = "66666666-6666-4666-8666-666666666666";

    struct FixedRequestIds(VecDeque<Uuid>);

    impl FixedRequestIds {
        fn new(values: &[&str]) -> Self {
            Self(
                values
                    .iter()
                    .map(|value| Uuid::parse_str(value).expect("valid test ID"))
                    .collect(),
            )
        }
    }

    impl RequestIdSource for FixedRequestIds {
        fn next_id(&mut self) -> Result<Uuid, TransportError> {
            self.0.pop_front().ok_or_else(|| {
                TransportError::new(TransportErrorKind::Service, "allocate request identity")
            })
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct CreateWidget {
        label: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct GetWidget;

    #[derive(Default)]
    struct FakeWidgetService {
        commands: Vec<CommandRequestMetadata>,
        queries: Vec<QueryRequestMetadata>,
    }

    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    impl TypedCommandService<CreateWidget> for FakeWidgetService {
        type Response = String;

        async fn execute_command(
            &mut self,
            invocation: CommandInvocation<CreateWidget>,
        ) -> Result<Self::Response, TransportError> {
            let wire = invocation.into_wire();
            let mut metadata = None;
            let mut request = wire.map_request(|command, value| {
                metadata = Some(value);
                command.label
            })?;
            assert_eq!(
                request
                    .metadata_mut()
                    .get("x-erpx-access-token")
                    .and_then(|value| value.to_str().ok()),
                Some("access-token")
            );
            self.commands
                .push(metadata.expect("command metadata is mapped"));
            Ok(request.into_inner())
        }
    }

    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    impl TypedQueryService<GetWidget> for FakeWidgetService {
        type Response = usize;

        async fn execute_query(
            &mut self,
            invocation: QueryInvocation<GetWidget>,
        ) -> Result<Self::Response, TransportError> {
            let wire = invocation.into_wire();
            let mut metadata = None;
            let request = wire.map_request(|query, value| {
                metadata = Some(value);
                query
            })?;
            self.queries
                .push(metadata.expect("query metadata is mapped"));
            Ok(std::mem::size_of_val(request.get_ref()))
        }
    }

    fn context() -> RequestContext {
        RequestContext::try_new(
            AuthenticatedSession::try_new(
                "access-token",
                "principal@example.com",
                SESSION_ID,
                ConsoleActorKind::TenantUser,
            )
            .expect("session is valid"),
            AuthorityScope::try_company(HOLDING_ID, COMPANY_ID).expect("scope is valid"),
            Some(TenantRoute::try_new(TENANT_ID, HOLDING_ID).expect("route is valid")),
            "widgets",
        )
        .expect("context is valid")
    }

    #[test]
    fn fake_command_service_receives_canonical_metadata_without_wire_leaking_to_caller() {
        let mut client = TypedServiceClient::with_id_source(
            FakeWidgetService::default(),
            context(),
            FixedRequestIds::new(&[COMMAND_ID, CORRELATION_ID]),
        );
        let output = block_on(
            client.command(
                CommandOptions::try_new("widget-create-1", "widget_console_create")
                    .expect("options are valid")
                    .expected_version(7),
                CreateWidget {
                    label: "Widget A".into(),
                },
            ),
        )
        .expect("fake command succeeds");

        assert_eq!(output, "Widget A");
        let metadata = &client.service().commands[0];
        assert_eq!(metadata.contract_version, u32::from(CONTRACT_VERSION));
        assert_eq!(metadata.command_id, COMMAND_ID);
        assert_eq!(metadata.correlation_id, CORRELATION_ID);
        assert_eq!(metadata.expected_aggregate_version, Some(7));
        assert_eq!(
            metadata
                .tenant_route
                .as_ref()
                .map(|route| route.tenant_id.as_str()),
            Some(TENANT_ID)
        );
        assert_eq!(
            metadata
                .audit
                .as_ref()
                .map(|audit| audit.correlation_id.as_str()),
            Some(CORRELATION_ID)
        );
    }

    #[test]
    fn command_receipt_drives_exact_read_your_writes_target() {
        let receipt = CommandReceiptView::try_from(CommandReceipt {
            command_id: COMMAND_ID.into(),
            correlation_id: CORRELATION_ID.into(),
            aggregate: Some(AggregateVersion {
                context: "widgets".into(),
                aggregate_type: "widget".into(),
                aggregate_id: AGGREGATE_ID.into(),
                version: 1,
            }),
            committed_events: vec![EventPosition {
                event_id: EVENT_ID.into(),
                global_sequence: 42,
            }],
            committed_at: "2026-08-05T09:00:00Z".into(),
            replayed: false,
        })
        .expect("receipt is valid");
        let mut client = TypedServiceClient::with_id_source(
            FakeWidgetService::default(),
            context(),
            FixedRequestIds::new(&[QUERY_ID, QUERY_CORRELATION_ID]),
        );
        block_on(
            client.query(
                QueryOptions::read_your_writes(
                    "widget_console_read",
                    &receipt,
                    "widget_projection",
                    TENANT_ID,
                    1_500,
                )
                .expect("query options are valid"),
                GetWidget,
            ),
        )
        .expect("fake query succeeds");

        let consistency = client.service().queries[0]
            .consistency
            .as_ref()
            .and_then(|value| value.requirement.as_ref());
        let Some(query_consistency::Requirement::ReadYourWrites(target)) = consistency else {
            panic!("expected read-your-writes consistency");
        };
        assert_eq!(target.projection, "widget_projection");
        assert_eq!(target.partition, TENANT_ID);
        assert_eq!(target.event_id, EVENT_ID);
        assert_eq!(target.global_sequence, 42);
        assert_eq!(target.timeout_millis, 1_500);
    }

    #[test]
    fn rpc_error_is_bounded_and_does_not_echo_server_details() {
        let error = TransportError::from_status(
            "list widgets",
            tonic::Status::unavailable("postgres://secret@example.invalid/database"),
        );

        assert_eq!(error.kind(), TransportErrorKind::Offline);
        assert_eq!(error.operation(), "list widgets");
        assert!(!error.to_string().contains("postgres"));
        assert!(!format!("{error:?}").contains("secret"));

        let not_ready = TransportError::from_status(
            "list operations",
            tonic::Status::failed_precondition("Operations projection is not ready"),
        );
        assert_eq!(not_ready.kind(), TransportErrorKind::NotReady);
    }

    #[test]
    fn invalid_scope_and_unbounded_wait_are_rejected_before_rpc() {
        let scope_error =
            AuthorityScope::try_subtree(HOLDING_ID, Vec::<String>::new(), Vec::<String>::new());
        assert_eq!(
            scope_error.expect_err("empty subtree is invalid").kind(),
            TransportErrorKind::InvalidRequest
        );

        let receipt = CommandReceiptView {
            command_id: COMMAND_ID.into(),
            correlation_id: CORRELATION_ID.into(),
            aggregate: AggregateRevision {
                context: "widgets".into(),
                aggregate_type: "widget".into(),
                aggregate_id: AGGREGATE_ID.into(),
                version: 1,
            },
            committed_events: vec![EventReceipt {
                event_id: EVENT_ID.into(),
                global_sequence: 42,
            }],
            committed_at: "2026-08-05T09:00:00Z".into(),
            replayed: false,
        };
        let query_error = QueryOptions::read_your_writes(
            "widget_console_read",
            &receipt,
            "widget_projection",
            TENANT_ID,
            MAX_READ_YOUR_WRITES_WAIT.as_millis() as u64 + 1,
        );
        assert_eq!(
            query_error.expect_err("wait must be bounded").kind(),
            TransportErrorKind::InvalidRequest
        );
    }

    struct ThreadWaker(thread::Thread);

    impl Wake for ThreadWaker {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.0.unpark();
        }
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
        let mut context = Context::from_waker(&waker);
        let mut future = pin!(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(output) => return output,
                Poll::Pending => thread::park(),
            }
        }
    }
}
