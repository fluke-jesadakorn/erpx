use chrono::{DateTime, Utc};
use erpx_cqrs::{
    AggregateVersion, CommandEnvelope, CommandReceipt, EventPosition, FreshnessState,
    ProjectionCheckpoint, ProjectionMetadata, QueryConsistency, QueryEnvelope, QueryMetadata,
    QueryResult,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AggregateId, CommandId, CorrelationId, EventId, TenantScope,
};
use erpx_proto::common::v1 as wire;
use erpx_ui_foundation::{UiError, UiErrorKind};
use tonic::metadata::MetadataValue;
use uuid::Uuid;

use crate::transport::typed_cqrs::{TransportError, TransportErrorKind};

const MAX_ACCESS_TOKEN_LENGTH: usize = 16_384;
const MAX_ROUTE_REFERENCE_LENGTH: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Wave9Route {
    tenant_id: Uuid,
    holding_ref: String,
}

impl Wave9Route {
    pub(crate) fn try_new(
        tenant_id: impl AsRef<str>,
        holding_ref: impl Into<String>,
    ) -> Result<Self, UiError> {
        let tenant_id = parse_request_uuid(tenant_id.as_ref(), "build Wave 9 route")?;
        let holding_ref = holding_ref.into();
        if holding_ref.trim().is_empty()
            || holding_ref.len() > MAX_ROUTE_REFERENCE_LENGTH
            || holding_ref.chars().any(char::is_control)
        {
            return Err(invalid_request("build Wave 9 route"));
        }
        Ok(Self {
            tenant_id,
            holding_ref,
        })
    }

    pub(super) const fn tenant_id(&self) -> Uuid {
        self.tenant_id
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
    route: Wave9Route,
}

impl AuthenticatedRoute {
    pub(super) fn try_new(token: impl Into<String>, route: Wave9Route) -> Result<Self, UiError> {
        let token = token.into();
        if token.trim().is_empty()
            || token.len() > MAX_ACCESS_TOKEN_LENGTH
            || token.chars().any(char::is_control)
        {
            return Err(invalid_request("build Wave 9 transport"));
        }
        Ok(Self { token, route })
    }

    pub(super) const fn route(&self) -> &Wave9Route {
        &self.route
    }

    pub(super) fn command<C>(
        &self,
        envelope: &CommandEnvelope<C>,
    ) -> Result<wire::CommandRequestMetadata, UiError> {
        envelope
            .validate()
            .map_err(|_| invalid_request("build Wave 9 command metadata"))?;
        Ok(wire::CommandRequestMetadata {
            contract_version: u32::from(envelope.contract_version),
            command_id: envelope.command_id.to_string(),
            idempotency_key: envelope.idempotency_key.as_str().into(),
            correlation_id: envelope.correlation_id.to_string(),
            causation_id: envelope.causation_id.map(|value| value.to_string()),
            actor: Some(encode_actor(&envelope.actor)),
            tenant_scope: Some(encode_scope(&envelope.tenant_scope)),
            audit: Some(wire::AuditContext {
                purpose: envelope.audit.purpose.clone(),
                customer_visible: envelope.audit.customer_visible,
                module: envelope.audit.module.clone().unwrap_or_default(),
                correlation_id: envelope.correlation_id.to_string(),
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
            .map_err(|_| invalid_request("build Wave 9 query metadata"))?;
        Ok(wire::QueryRequestMetadata {
            contract_version: u32::from(envelope.contract_version),
            query_id: envelope.query_id.to_string(),
            correlation_id: envelope.correlation_id.to_string(),
            actor: Some(encode_actor(&envelope.actor)),
            tenant_scope: Some(encode_scope(&envelope.tenant_scope)),
            audit: Some(wire::AuditContext {
                purpose: envelope.audit.purpose.clone(),
                customer_visible: envelope.audit.customer_visible,
                module: envelope.audit.module.clone().unwrap_or_default(),
                correlation_id: envelope.correlation_id.to_string(),
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
        insert(&mut request, "x-erpx-access-token", &self.token)?;
        insert(&mut request, "x-erpx-actor", &actor.principal_ref)?;
        if let Some(session_id) = actor.session_id {
            insert(&mut request, "x-erpx-session", &session_id.to_string())?;
        }
        if let Some(capability_id) = actor.support_capability_id {
            insert(
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
    let receipt = receipt.ok_or_else(|| invalid_response("decode Wave 9 command receipt"))?;
    let command_id = CommandId::try_from_uuid(response_uuid(&receipt.command_id)?)
        .map_err(|_| invalid_response("decode Wave 9 command receipt"))?;
    let correlation_id = CorrelationId::try_from_uuid(response_uuid(&receipt.correlation_id)?)
        .map_err(|_| invalid_response("decode Wave 9 command receipt"))?;
    if command_id != expected_command || correlation_id != expected_correlation {
        return Err(invalid_response("decode Wave 9 command receipt"));
    }
    let aggregate = receipt
        .aggregate
        .ok_or_else(|| invalid_response("decode Wave 9 command receipt"))?;
    if aggregate.context.trim().is_empty()
        || aggregate.aggregate_type.trim().is_empty()
        || aggregate.version == 0
    {
        return Err(invalid_response("decode Wave 9 command receipt"));
    }
    let committed_events = receipt
        .committed_events
        .into_iter()
        .map(|value| {
            if value.global_sequence == 0 {
                return Err(invalid_response("decode Wave 9 command receipt"));
            }
            Ok(EventPosition {
                event_id: event_id(&value.event_id)?,
                global_sequence: value.global_sequence,
            })
        })
        .collect::<Result<_, UiError>>()?;
    Ok(CommandReceipt {
        command_id,
        correlation_id,
        aggregate: AggregateVersion {
            context: aggregate.context,
            aggregate_type: aggregate.aggregate_type,
            aggregate_id: AggregateId::try_from_uuid(response_uuid(&aggregate.aggregate_id)?)
                .map_err(|_| invalid_response("decode Wave 9 command receipt"))?,
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
    let metadata = metadata.ok_or_else(|| invalid_response("decode Wave 9 query metadata"))?;
    let correlation_id = CorrelationId::try_from_uuid(response_uuid(&metadata.correlation_id)?)
        .map_err(|_| invalid_response("decode Wave 9 query metadata"))?;
    if correlation_id != expected_correlation {
        return Err(invalid_response("decode Wave 9 query metadata"));
    }
    Ok(QueryResult {
        value,
        metadata: QueryMetadata {
            correlation_id,
            projection: projection_metadata(metadata.projection)?,
            // The request is authoritative; a response cannot weaken RYW.
            consistency: requested_consistency,
        },
    })
}

pub(super) fn projection_metadata(
    value: Option<wire::ProjectionMetadata>,
) -> Result<ProjectionMetadata, UiError> {
    let value = value.ok_or_else(|| invalid_response("decode projection metadata"))?;
    let freshness = match wire::ProjectionFreshnessState::try_from(value.freshness).ok() {
        Some(wire::ProjectionFreshnessState::Current) => FreshnessState::Current,
        Some(wire::ProjectionFreshnessState::Stale) => FreshnessState::Stale,
        Some(wire::ProjectionFreshnessState::Rebuilding) => FreshnessState::Rebuilding,
        Some(wire::ProjectionFreshnessState::Unavailable) => FreshnessState::Unavailable,
        _ => return Err(invalid_response("decode projection metadata")),
    };
    let checkpoint = value
        .checkpoint
        .map(|checkpoint| {
            if checkpoint.projection.trim().is_empty()
                || checkpoint.partition.trim().is_empty()
                || checkpoint.global_sequence == 0
            {
                return Err(invalid_response("decode projection checkpoint"));
            }
            Ok(ProjectionCheckpoint {
                projection: checkpoint.projection,
                partition: checkpoint.partition,
                event_id: event_id(&checkpoint.event_id)?,
                global_sequence: checkpoint.global_sequence,
                updated_at: timestamp(&checkpoint.updated_at)?,
            })
        })
        .transpose()?;
    Ok(ProjectionMetadata {
        checkpoint,
        freshness,
        lag_millis: value.lag_millis,
    })
}

pub(super) fn encode_scope(value: &TenantScope) -> wire::TenantScope {
    use wire::tenant_scope::Scope;
    let scope = match value {
        TenantScope::Platform => Scope::Platform(wire::PlatformScope {}),
        TenantScope::Holding { holding_id } => Scope::Holding(wire::HoldingScope {
            holding_id: holding_id.to_string(),
        }),
        TenantScope::Company {
            holding_id,
            company_id,
        } => Scope::Company(wire::CompanyScope {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
        }),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Scope::Branch(wire::BranchScope {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
            branch_id: branch_id.to_string(),
        }),
        TenantScope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        } => Scope::Subtree(wire::SubtreeScope {
            holding_id: holding_id.to_string(),
            company_ids: company_ids.iter().map(ToString::to_string).collect(),
            branch_ids: branch_ids.iter().map(ToString::to_string).collect(),
        }),
    };
    wire::TenantScope { scope: Some(scope) }
}

pub(super) fn decode_scope(value: Option<wire::TenantScope>) -> Result<TenantScope, UiError> {
    use wire::tenant_scope::Scope;
    let value = value
        .and_then(|value| value.scope)
        .ok_or_else(|| invalid_response("decode tenant scope"))?;
    let scope = match value {
        Scope::Platform(_) => TenantScope::Platform,
        Scope::Holding(value) => TenantScope::Holding {
            holding_id: response_uuid(&value.holding_id)?,
        },
        Scope::Company(value) => TenantScope::Company {
            holding_id: response_uuid(&value.holding_id)?,
            company_id: response_uuid(&value.company_id)?,
        },
        Scope::Branch(value) => TenantScope::Branch {
            holding_id: response_uuid(&value.holding_id)?,
            company_id: response_uuid(&value.company_id)?,
            branch_id: response_uuid(&value.branch_id)?,
        },
        Scope::Subtree(value) => TenantScope::Subtree {
            holding_id: response_uuid(&value.holding_id)?,
            company_ids: value
                .company_ids
                .iter()
                .map(|id| response_uuid(id))
                .collect::<Result<_, _>>()?,
            branch_ids: value
                .branch_ids
                .iter()
                .map(|id| response_uuid(id))
                .collect::<Result<_, _>>()?,
        },
    };
    scope
        .validate()
        .map_err(|_| invalid_response("decode tenant scope"))?;
    Ok(scope)
}

pub(super) fn uuid(value: &str) -> Result<Uuid, UiError> {
    response_uuid(value)
}

pub(super) fn timestamp(value: &str) -> Result<DateTime<Utc>, UiError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| invalid_response("decode timestamp"))
}

pub(super) fn status_ui(operation: &'static str, status: tonic::Status) -> UiError {
    transport_ui(TransportError::from_status(operation, status))
}

pub(super) fn invalid_response(operation: &'static str) -> UiError {
    transport_ui(TransportError::new(
        TransportErrorKind::InvalidResponse,
        operation,
    ))
}

fn invalid_request(operation: &'static str) -> UiError {
    transport_ui(TransportError::new(
        TransportErrorKind::InvalidRequest,
        operation,
    ))
}

fn transport_ui(error: TransportError) -> UiError {
    let (kind, code, message, retryable) = match error.kind() {
        TransportErrorKind::InvalidRequest | TransportErrorKind::Rejected => (
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
        TransportErrorKind::Throttled | TransportErrorKind::Offline => (
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

fn encode_actor(value: &ActorAttribution) -> wire::ActorAttribution {
    wire::ActorAttribution {
        principal_ref: value.principal_ref.clone(),
        kind: match value.kind {
            ActorKind::PlatformUser => wire::ActorKind::PlatformUser as i32,
            ActorKind::TenantUser => wire::ActorKind::TenantUser as i32,
            ActorKind::Service => wire::ActorKind::Service as i32,
            ActorKind::SystemWorker => wire::ActorKind::SystemWorker as i32,
        },
        session_id: value
            .session_id
            .map(|value| value.to_string())
            .unwrap_or_default(),
        represented_principal_ref: value.represented_principal_ref.clone().unwrap_or_default(),
        support_capability_id: value
            .support_capability_id
            .map(|value| value.to_string())
            .unwrap_or_default(),
    }
}

fn encode_consistency(value: &QueryConsistency) -> wire::QueryConsistency {
    use wire::query_consistency::Requirement;
    let requirement = match value {
        QueryConsistency::Eventual => Requirement::Eventual(wire::EventualConsistency {}),
        QueryConsistency::ReadYourWrites(value) => {
            Requirement::ReadYourWrites(wire::ReadYourWrites {
                projection: value.projection.clone(),
                partition: value.partition.clone(),
                event_id: value.target.event_id.to_string(),
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

fn insert<T>(
    request: &mut tonic::Request<T>,
    key: &'static str,
    value: &str,
) -> Result<(), UiError> {
    let value =
        MetadataValue::try_from(value).map_err(|_| invalid_request("build gRPC request"))?;
    request.metadata_mut().insert(key, value);
    Ok(())
}

fn parse_request_uuid(value: &str, operation: &'static str) -> Result<Uuid, UiError> {
    Uuid::parse_str(value)
        .ok()
        .filter(|value| !value.is_nil())
        .ok_or_else(|| invalid_request(operation))
}

fn response_uuid(value: &str) -> Result<Uuid, UiError> {
    Uuid::parse_str(value)
        .ok()
        .filter(|value| !value.is_nil())
        .ok_or_else(|| invalid_response("decode identifier"))
}

fn event_id(value: &str) -> Result<EventId, UiError> {
    EventId::try_from_uuid(response_uuid(value)?)
        .map_err(|_| invalid_response("decode event identity"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_and_transport_errors_fail_closed() {
        assert!(Wave9Route::try_new(Uuid::nil().to_string(), "holding").is_err());
        assert!(Wave9Route::try_new(Uuid::new_v4().to_string(), "holding\nsecret").is_err());
        let error = status_ui(
            "wave9",
            tonic::Status::internal("secret database diagnostics"),
        );
        assert_eq!(error.kind(), UiErrorKind::Failed);
        assert!(!error.message().contains("secret"));
    }
}
