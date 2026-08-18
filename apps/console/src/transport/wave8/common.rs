use chrono::{DateTime, Utc};
use erpx_cqrs::{
    AggregateVersion, CommandEnvelope, CommandReceipt, EventPosition, FreshnessState,
    ProjectionCheckpoint, ProjectionMetadata, QueryConsistency, QueryEnvelope, QueryMetadata,
    QueryResult,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AggregateId, CommandId, CorrelationId, EventId, TenantScope,
};
use erpx_proto::common::v2 as wire;
use erpx_ui_foundation::{UiError, UiErrorKind};
use tonic::metadata::MetadataValue;
use uuid::Uuid;

use super::super::typed_cqrs::{TransportError, TransportErrorKind};

const MAX_ACCESS_TOKEN_LENGTH: usize = 16_384;
const MAX_ROUTE_REFERENCE_LENGTH: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Wave8Route {
    tenant_id: Uuid,
    holding_ref: String,
}

impl Wave8Route {
    pub(crate) fn try_new(
        tenant_id: impl AsRef<str>,
        holding_ref: impl Into<String>,
    ) -> Result<Self, UiError> {
        let tenant_id = parse_uuid(tenant_id.as_ref(), "build tenant route")?;
        let holding_ref = holding_ref.into();
        if holding_ref.trim().is_empty()
            || holding_ref.len() > MAX_ROUTE_REFERENCE_LENGTH
            || holding_ref.chars().any(char::is_control)
        {
            return Err(invalid_ui("build tenant route"));
        }
        Ok(Self {
            tenant_id,
            holding_ref,
        })
    }

    fn to_wire(&self) -> wire::TenantRoute {
        wire::TenantRoute {
            tenant_id: self.tenant_id.to_string(),
            holding_ref: self.holding_ref.clone(),
        }
    }
}

#[derive(Clone)]
pub(super) struct AuthenticatedRoute {
    token: String,
    route: Wave8Route,
}

impl AuthenticatedRoute {
    pub(super) fn try_new(token: impl Into<String>, route: Wave8Route) -> Result<Self, UiError> {
        let token = token.into();
        if token.trim().is_empty()
            || token.len() > MAX_ACCESS_TOKEN_LENGTH
            || token.chars().any(char::is_control)
        {
            return Err(invalid_ui("build authenticated transport"));
        }
        Ok(Self { token, route })
    }

    pub(super) fn command<C>(
        &self,
        envelope: &CommandEnvelope<C>,
    ) -> Result<wire::CommandRequestMetadata, UiError> {
        envelope
            .validate()
            .map_err(|_| invalid_ui("build command metadata"))?;
        Ok(wire::CommandRequestMetadata {
            contract_version: u32::from(envelope.contract_version),
            command_id: envelope.command_id.as_uuid().to_string(),
            idempotency_key: envelope.idempotency_key.as_str().to_owned(),
            correlation_id: envelope.correlation_id.as_uuid().to_string(),
            causation_id: envelope
                .causation_id
                .map(|value| value.as_uuid().to_string()),
            actor: Some(encode_actor(&envelope.actor)),
            tenant_scope: Some(encode_scope(&envelope.tenant_scope)),
            audit: Some(wire::AuditContext {
                purpose: envelope.audit.purpose.clone(),
                customer_visible: envelope.audit.customer_visible,
                module: envelope.audit.module.clone().unwrap_or_default(),
                correlation_id: envelope.correlation_id.as_uuid().to_string(),
            }),
            expected_aggregate_version: envelope.expected_aggregate_version,
            subscription_id: None,
            mfa_code: None,
            tenant_route: Some(self.route.to_wire()),
        })
    }

    pub(super) fn query<Q>(
        &self,
        envelope: &QueryEnvelope<Q>,
    ) -> Result<wire::QueryRequestMetadata, UiError> {
        envelope
            .validate()
            .map_err(|_| invalid_ui("build query metadata"))?;
        Ok(wire::QueryRequestMetadata {
            contract_version: u32::from(envelope.contract_version),
            query_id: envelope.query_id.as_uuid().to_string(),
            correlation_id: envelope.correlation_id.as_uuid().to_string(),
            actor: Some(encode_actor(&envelope.actor)),
            tenant_scope: Some(encode_scope(&envelope.tenant_scope)),
            audit: Some(wire::AuditContext {
                purpose: envelope.audit.purpose.clone(),
                customer_visible: envelope.audit.customer_visible,
                module: envelope.audit.module.clone().unwrap_or_default(),
                correlation_id: envelope.correlation_id.as_uuid().to_string(),
            }),
            consistency: Some(encode_consistency(&envelope.consistency)),
            tenant_route: Some(self.route.to_wire()),
        })
    }

    pub(super) fn request<T>(
        &self,
        message: T,
        actor: &ActorAttribution,
    ) -> Result<tonic::Request<T>, UiError> {
        let mut request = tonic::Request::new(message);
        insert_header(&mut request, "x-erpx-access-token", &self.token)?;
        insert_header(&mut request, "x-erpx-actor", &actor.principal_ref)?;
        if let Some(session_id) = actor.session_id {
            insert_header(
                &mut request,
                "x-erpx-session",
                &session_id.as_uuid().to_string(),
            )?;
        }
        if let Some(capability_id) = actor.support_capability_id {
            insert_header(
                &mut request,
                "x-erpx-support-capability-id",
                &capability_id.to_string(),
            )?;
        }
        Ok(request)
    }
}

pub(super) fn command_result<R>(
    receipt: Option<wire::CommandReceipt>,
    response: R,
    expected_command: CommandId,
    expected_correlation: CorrelationId,
) -> Result<CommandReceipt<R>, UiError> {
    let receipt = receipt.ok_or_else(|| invalid_response("decode command receipt"))?;
    let command_id = command_id(&receipt.command_id)?;
    let correlation_id = correlation_id(&receipt.correlation_id)?;
    if command_id != expected_command || correlation_id != expected_correlation {
        return Err(invalid_response("decode command receipt"));
    }
    let aggregate = receipt
        .aggregate
        .ok_or_else(|| invalid_response("decode command receipt"))?;
    if aggregate.context.trim().is_empty()
        || aggregate.aggregate_type.trim().is_empty()
        || aggregate.version == 0
    {
        return Err(invalid_response("decode command receipt"));
    }
    let committed_events = receipt
        .committed_events
        .into_iter()
        .map(|position| {
            if position.global_sequence == 0 {
                return Err(invalid_response("decode command receipt"));
            }
            Ok(EventPosition {
                event_id: event_id(&position.event_id)?,
                global_sequence: position.global_sequence,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CommandReceipt {
        command_id,
        correlation_id,
        aggregate: AggregateVersion {
            context: aggregate.context,
            aggregate_type: aggregate.aggregate_type,
            aggregate_id: aggregate_id(&aggregate.aggregate_id)?,
            version: aggregate.version,
        },
        committed_events,
        committed_at: timestamp(&receipt.committed_at)?,
        replayed: receipt.replayed,
        response,
    })
}

pub(super) fn query_result<T>(
    metadata: Option<wire::QueryMetadata>,
    value: T,
    expected_correlation: CorrelationId,
    requested_consistency: QueryConsistency,
) -> Result<QueryResult<T>, UiError> {
    let metadata = metadata.ok_or_else(|| invalid_response("decode query metadata"))?;
    let correlation_id = correlation_id(&metadata.correlation_id)?;
    if correlation_id != expected_correlation {
        return Err(invalid_response("decode query metadata"));
    }
    let projection = metadata
        .projection
        .ok_or_else(|| invalid_response("decode query metadata"))?;
    let freshness = match wire::ProjectionFreshnessState::try_from(projection.freshness).ok() {
        Some(wire::ProjectionFreshnessState::Current) => FreshnessState::Current,
        Some(wire::ProjectionFreshnessState::Stale) => FreshnessState::Stale,
        Some(wire::ProjectionFreshnessState::Rebuilding) => FreshnessState::Rebuilding,
        Some(wire::ProjectionFreshnessState::Unavailable) => FreshnessState::Unavailable,
        Some(wire::ProjectionFreshnessState::Unspecified) | None => {
            return Err(invalid_response("decode query metadata"));
        }
    };
    let checkpoint = projection
        .checkpoint
        .map(|value| {
            if value.projection.trim().is_empty()
                || value.partition.trim().is_empty()
                || value.global_sequence == 0
            {
                return Err(invalid_response("decode query metadata"));
            }
            Ok(ProjectionCheckpoint {
                projection: value.projection,
                partition: value.partition,
                event_id: event_id(&value.event_id)?,
                global_sequence: value.global_sequence,
                updated_at: timestamp(&value.updated_at)?,
            })
        })
        .transpose()?;
    Ok(QueryResult {
        value,
        metadata: QueryMetadata {
            correlation_id,
            projection: ProjectionMetadata {
                checkpoint,
                freshness,
                lag_millis: projection.lag_millis,
            },
            // The request requirement is authoritative. The wire response is
            // deliberately not allowed to weaken ReadYourWrites semantics.
            consistency: requested_consistency,
        },
    })
}

pub(super) fn status_ui(operation: &'static str, status: tonic::Status) -> UiError {
    transport_ui(TransportError::from_status(operation, status))
}

pub(super) fn unsupported_ui(operation: &'static str) -> UiError {
    transport_ui(TransportError::new(
        TransportErrorKind::InvalidRequest,
        operation,
    ))
}

pub(super) fn invalid_response(operation: &'static str) -> UiError {
    transport_ui(TransportError::new(
        TransportErrorKind::InvalidResponse,
        operation,
    ))
}

pub(super) fn transport_ui(error: TransportError) -> UiError {
    let (kind, code, message, retryable) = match error.kind() {
        TransportErrorKind::InvalidRequest => (
            UiErrorKind::Invalid,
            "invalid_request",
            "The request context is invalid.",
            false,
        ),
        TransportErrorKind::InvalidResponse => (
            UiErrorKind::Failed,
            "invalid_response",
            "The service returned an invalid response.",
            true,
        ),
        TransportErrorKind::AuthenticationRequired => (
            UiErrorKind::Denied,
            "authentication_required",
            "Sign in again to continue.",
            false,
        ),
        TransportErrorKind::AuthorizationDenied => (
            UiErrorKind::Denied,
            "authorization_denied",
            "You do not have permission to perform this action.",
            false,
        ),
        TransportErrorKind::NotFound => (
            UiErrorKind::NotFound,
            "not_found",
            "The requested record was not found.",
            false,
        ),
        TransportErrorKind::NotReady => (
            UiErrorKind::NotReady,
            "projection_not_ready",
            "Your change was saved, but this view has not caught up yet.",
            true,
        ),
        TransportErrorKind::Conflict => (
            UiErrorKind::Conflict,
            "version_conflict",
            "The record changed. Refresh and try again.",
            false,
        ),
        TransportErrorKind::Rejected => (
            UiErrorKind::Invalid,
            "request_rejected",
            "The request was rejected.",
            false,
        ),
        TransportErrorKind::Throttled => (
            UiErrorKind::Offline,
            "service_throttled",
            "The service is busy. Please retry.",
            true,
        ),
        TransportErrorKind::Offline => (
            UiErrorKind::Offline,
            "transport_unavailable",
            "The service is temporarily unavailable.",
            true,
        ),
        TransportErrorKind::Service => (
            UiErrorKind::Failed,
            "service_failure",
            "The request could not be completed.",
            true,
        ),
    };
    UiError::bounded(kind, code, message, retryable, None)
}

pub(super) fn encode_scope(scope: &TenantScope) -> wire::TenantScope {
    let scope = match scope {
        TenantScope::Platform => wire::tenant_scope::Scope::Platform(wire::PlatformScope {}),
        TenantScope::Holding { holding_id } => {
            wire::tenant_scope::Scope::Holding(wire::HoldingScope {
                holding_id: holding_id.to_string(),
            })
        }
        TenantScope::Company {
            holding_id,
            company_id,
        } => wire::tenant_scope::Scope::Company(wire::CompanyScope {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
        }),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => wire::tenant_scope::Scope::Branch(wire::BranchScope {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
            branch_id: branch_id.to_string(),
        }),
        TenantScope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        } => wire::tenant_scope::Scope::Subtree(wire::SubtreeScope {
            holding_id: holding_id.to_string(),
            company_ids: company_ids.iter().map(ToString::to_string).collect(),
            branch_ids: branch_ids.iter().map(ToString::to_string).collect(),
        }),
    };
    wire::TenantScope { scope: Some(scope) }
}

pub(super) fn decode_scope(scope: Option<wire::TenantScope>) -> Result<TenantScope, UiError> {
    let scope = scope
        .and_then(|scope| scope.scope)
        .ok_or_else(|| invalid_response("decode tenant scope"))?;
    let decoded = match scope {
        wire::tenant_scope::Scope::Platform(_) => TenantScope::Platform,
        wire::tenant_scope::Scope::Holding(value) => TenantScope::Holding {
            holding_id: parse_response_uuid(&value.holding_id, "decode tenant scope")?,
        },
        wire::tenant_scope::Scope::Company(value) => TenantScope::Company {
            holding_id: parse_response_uuid(&value.holding_id, "decode tenant scope")?,
            company_id: parse_response_uuid(&value.company_id, "decode tenant scope")?,
        },
        wire::tenant_scope::Scope::Branch(value) => TenantScope::Branch {
            holding_id: parse_response_uuid(&value.holding_id, "decode tenant scope")?,
            company_id: parse_response_uuid(&value.company_id, "decode tenant scope")?,
            branch_id: parse_response_uuid(&value.branch_id, "decode tenant scope")?,
        },
        wire::tenant_scope::Scope::Subtree(value) => TenantScope::Subtree {
            holding_id: parse_response_uuid(&value.holding_id, "decode tenant scope")?,
            company_ids: value
                .company_ids
                .iter()
                .map(|id| parse_response_uuid(id, "decode tenant scope"))
                .collect::<Result<_, _>>()?,
            branch_ids: value
                .branch_ids
                .iter()
                .map(|id| parse_response_uuid(id, "decode tenant scope"))
                .collect::<Result<_, _>>()?,
        },
    };
    decoded
        .validate()
        .map_err(|_| invalid_response("decode tenant scope"))?;
    Ok(decoded)
}

pub(super) fn uuid(value: &str, operation: &'static str) -> Result<Uuid, UiError> {
    parse_response_uuid(value, operation)
}

pub(super) fn timestamp(value: &str) -> Result<DateTime<Utc>, UiError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| invalid_response("decode timestamp"))
}

fn encode_actor(actor: &ActorAttribution) -> wire::ActorAttribution {
    let kind = match actor.kind {
        ActorKind::PlatformUser => wire::ActorKind::PlatformUser,
        ActorKind::TenantUser => wire::ActorKind::TenantUser,
        ActorKind::Service => wire::ActorKind::Service,
        ActorKind::SystemWorker => wire::ActorKind::SystemWorker,
        // The legacy wire contract has no Agent discriminator. Encoding it as
        // unspecified fails closed instead of granting service authority.
        ActorKind::Agent => wire::ActorKind::Unspecified,
    };
    wire::ActorAttribution {
        principal_ref: actor.principal_ref.clone(),
        kind: kind as i32,
        session_id: actor
            .session_id
            .map(|value| value.as_uuid().to_string())
            .unwrap_or_default(),
        represented_principal_ref: actor.represented_principal_ref.clone().unwrap_or_default(),
        support_capability_id: actor
            .support_capability_id
            .map(|value| value.to_string())
            .unwrap_or_default(),
    }
}

fn encode_consistency(consistency: &QueryConsistency) -> wire::QueryConsistency {
    let requirement = match consistency {
        QueryConsistency::Eventual => {
            wire::query_consistency::Requirement::Eventual(wire::EventualConsistency {})
        }
        QueryConsistency::ReadYourWrites(value) => {
            wire::query_consistency::Requirement::ReadYourWrites(wire::ReadYourWrites {
                projection: value.projection.clone(),
                partition: value.partition.clone(),
                event_id: value.target.event_id.as_uuid().to_string(),
                global_sequence: value.target.global_sequence,
                timeout_millis: value.timeout_millis,
                ..Default::default()
            })
        }
    };
    wire::QueryConsistency {
        requirement: Some(requirement),
    }
}

fn insert_header<T>(
    request: &mut tonic::Request<T>,
    key: &'static str,
    value: &str,
) -> Result<(), UiError> {
    let value = MetadataValue::try_from(value).map_err(|_| invalid_ui("build gRPC request"))?;
    request.metadata_mut().insert(key, value);
    Ok(())
}

fn parse_uuid(value: &str, operation: &'static str) -> Result<Uuid, UiError> {
    Uuid::parse_str(value)
        .ok()
        .filter(|value| !value.is_nil())
        .ok_or_else(|| invalid_ui(operation))
}

fn parse_response_uuid(value: &str, operation: &'static str) -> Result<Uuid, UiError> {
    Uuid::parse_str(value)
        .ok()
        .filter(|value| !value.is_nil())
        .ok_or_else(|| invalid_response(operation))
}

fn command_id(value: &str) -> Result<CommandId, UiError> {
    CommandId::try_from_uuid(parse_response_uuid(value, "decode command receipt")?)
        .map_err(|_| invalid_response("decode command receipt"))
}

fn correlation_id(value: &str) -> Result<CorrelationId, UiError> {
    CorrelationId::try_from_uuid(parse_response_uuid(value, "decode correlation identity")?)
        .map_err(|_| invalid_response("decode correlation identity"))
}

fn aggregate_id(value: &str) -> Result<AggregateId, UiError> {
    AggregateId::try_from_uuid(parse_response_uuid(value, "decode aggregate identity")?)
        .map_err(|_| invalid_response("decode aggregate identity"))
}

fn event_id(value: &str) -> Result<EventId, UiError> {
    EventId::try_from_uuid(parse_response_uuid(value, "decode event identity")?)
        .map_err(|_| invalid_response("decode event identity"))
}

fn invalid_ui(operation: &'static str) -> UiError {
    transport_ui(TransportError::new(
        TransportErrorKind::InvalidRequest,
        operation,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_rejects_nil_and_control_characters() {
        assert!(Wave8Route::try_new(Uuid::nil().to_string(), "holding").is_err());
        assert!(Wave8Route::try_new(Uuid::new_v4().to_string(), "holding\nsecret").is_err());
    }

    #[test]
    fn bounded_transport_error_never_exposes_status_message() {
        let error = status_ui(
            "finance query",
            tonic::Status::internal("postgres://admin:secret@example.test"),
        );
        assert_eq!(error.kind(), UiErrorKind::Failed);
        assert!(!error.message().contains("secret"));
    }

    #[test]
    fn legacy_wire_actor_fails_closed_for_agent_identity() {
        let encoded = encode_actor(&ActorAttribution {
            principal_ref: "agent.safe-ref".into(),
            kind: ActorKind::Agent,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        });
        assert_eq!(encoded.kind, wire::ActorKind::Unspecified as i32);
    }
}
