use chrono::{DateTime, NaiveDate, Utc};
use erpx_cqrs::{
    AggregateVersion, CommandEnvelope, CommandReceipt, EventPosition, FreshnessState,
    ProjectionCheckpoint, ProjectionMetadata, QueryConsistency, QueryEnvelope, QueryMetadata,
    QueryResult,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AggregateId, AuthorityScope, CommandId, CorrelationId, EventId,
};
use erpx_proto::common as wire;
use erpx_ui_foundation::{UiError, UiErrorKind};
use tonic::metadata::MetadataValue;
use uuid::Uuid;

use crate::transport::typed_cqrs::{TransportError, TransportErrorKind};

const MAX_ACCESS_TOKEN_LENGTH: usize = 16_384;
const MAX_ROUTE_REFERENCE_LENGTH: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Wave67Route {
    holding_id: Uuid,
    data_profile_key: String,
}

impl Wave67Route {
    pub(crate) fn try_new(
        holding_id: impl AsRef<str>,
        data_profile_key: impl Into<String>,
    ) -> Result<Self, UiError> {
        let holding_id = parse_request_uuid(holding_id.as_ref(), "build holding route")?;
        let data_profile_key = data_profile_key.into();
        if data_profile_key.trim().is_empty()
            || data_profile_key.len() > MAX_ROUTE_REFERENCE_LENGTH
            || data_profile_key.chars().any(char::is_control)
        {
            return Err(invalid_request("build holding route"));
        }
        Ok(Self {
            holding_id,
            data_profile_key,
        })
    }

    fn wire(&self) -> wire::HoldingRoute {
        wire::HoldingRoute {
            holding_id: self.holding_id.to_string(),
            data_profile_key: self.data_profile_key.clone(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct AuthenticatedRoute {
    token: String,
    route: Option<Wave67Route>,
}

impl AuthenticatedRoute {
    pub(crate) fn try_new(token: impl Into<String>, route: Wave67Route) -> Result<Self, UiError> {
        let token = token.into();
        if token.trim().is_empty()
            || token.len() > MAX_ACCESS_TOKEN_LENGTH
            || token.chars().any(char::is_control)
        {
            return Err(invalid_request("build authenticated transport"));
        }
        Ok(Self {
            token,
            route: Some(route),
        })
    }

    pub(crate) fn try_active_scope(token: impl Into<String>) -> Result<Self, UiError> {
        let token = token.into();
        if token.trim().is_empty()
            || token.len() > MAX_ACCESS_TOKEN_LENGTH
            || token.chars().any(char::is_control)
        {
            return Err(invalid_request("build authenticated transport"));
        }
        Ok(Self { token, route: None })
    }

    pub(crate) fn command<C>(
        &self,
        value: &CommandEnvelope<C>,
    ) -> Result<wire::CommandRequestMetadata, UiError> {
        value
            .validate()
            .map_err(|_| invalid_request("build command metadata"))?;
        Ok(wire::CommandRequestMetadata {
            contract_version: u32::from(value.contract_version),
            command_id: value.command_id.to_string(),
            idempotency_key: value.idempotency_key.as_str().into(),
            correlation_id: value.correlation_id.to_string(),
            causation_id: value.causation_id.map(|id| id.to_string()),
            actor: Some(encode_actor(&value.actor)),
            active_scope: Some(encode_scope(&value.active_scope)),
            audit: Some(wire::AuditContext {
                purpose: value.audit.purpose.clone(),
                customer_visible: value.audit.customer_visible,
                module: value.audit.module.clone().unwrap_or_default(),
                correlation_id: value.correlation_id.to_string(),
            }),
            expected_aggregate_version: value.expected_aggregate_version,
            subscription_id: None,
            mfa_code: None,
            holding_route: self.route.as_ref().map(Wave67Route::wire),
        })
    }

    pub(crate) fn query<Q>(
        &self,
        value: &QueryEnvelope<Q>,
    ) -> Result<wire::QueryRequestMetadata, UiError> {
        value
            .validate()
            .map_err(|_| invalid_request("build query metadata"))?;
        Ok(wire::QueryRequestMetadata {
            contract_version: u32::from(value.contract_version),
            query_id: value.query_id.to_string(),
            correlation_id: value.correlation_id.to_string(),
            actor: Some(encode_actor(&value.actor)),
            active_scope: Some(encode_scope(&value.active_scope)),
            audit: Some(wire::AuditContext {
                purpose: value.audit.purpose.clone(),
                customer_visible: value.audit.customer_visible,
                module: value.audit.module.clone().unwrap_or_default(),
                correlation_id: value.correlation_id.to_string(),
            }),
            consistency: Some(encode_consistency(&value.consistency)),
            holding_route: self.route.as_ref().map(Wave67Route::wire),
        })
    }

    pub(crate) fn request<T>(
        &self,
        message: T,
        actor: &ActorAttribution,
    ) -> Result<tonic::Request<T>, UiError> {
        let mut request = tonic::Request::new(message);
        insert(&mut request, "x-erpx-access-token", &self.token)?;
        insert(&mut request, "x-erpx-actor", &actor.principal_ref)?;
        if let Some(value) = actor.session_id {
            insert(&mut request, "x-erpx-session", &value.to_string())?;
        }
        if let Some(value) = actor.support_capability_id {
            insert(
                &mut request,
                "x-erpx-support-capability-id",
                &value.to_string(),
            )?;
        }
        Ok(request)
    }
}

pub(crate) fn command_result<R>(
    receipt: Option<wire::CommandReceipt>,
    response: R,
    expected_command: CommandId,
    expected_correlation: CorrelationId,
) -> Result<CommandReceipt<R>, UiError> {
    let receipt = receipt.ok_or_else(|| invalid_response("decode command receipt"))?;
    let command_id = CommandId::try_from_uuid(response_uuid(
        &receipt.command_id,
        "decode command receipt",
    )?)
    .map_err(|_| invalid_response("decode command receipt"))?;
    let correlation_id = CorrelationId::try_from_uuid(response_uuid(
        &receipt.correlation_id,
        "decode command receipt",
    )?)
    .map_err(|_| invalid_response("decode command receipt"))?;
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
        .map(|value| {
            if value.global_sequence == 0 {
                return Err(invalid_response("decode command receipt"));
            }
            Ok(EventPosition {
                event_id: EventId::try_from_uuid(response_uuid(
                    &value.event_id,
                    "decode command receipt",
                )?)
                .map_err(|_| invalid_response("decode command receipt"))?,
                global_sequence: value.global_sequence,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CommandReceipt {
        command_id,
        correlation_id,
        aggregate: AggregateVersion {
            context: aggregate.context,
            aggregate_type: aggregate.aggregate_type,
            aggregate_id: AggregateId::try_from_uuid(response_uuid(
                &aggregate.aggregate_id,
                "decode command receipt",
            )?)
            .map_err(|_| invalid_response("decode command receipt"))?,
            version: aggregate.version,
        },
        committed_events,
        committed_at: timestamp(&receipt.committed_at)?,
        replayed: receipt.replayed,
        response,
    })
}

pub(crate) fn query_result<T>(
    metadata: Option<wire::QueryMetadata>,
    value: T,
    expected_correlation: CorrelationId,
    requested_consistency: QueryConsistency,
) -> Result<QueryResult<T>, UiError> {
    let metadata = metadata.ok_or_else(|| invalid_response("decode query metadata"))?;
    let correlation_id = CorrelationId::try_from_uuid(response_uuid(
        &metadata.correlation_id,
        "decode query metadata",
    )?)
    .map_err(|_| invalid_response("decode query metadata"))?;
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
        _ => return Err(invalid_response("decode query metadata")),
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
                event_id: EventId::try_from_uuid(response_uuid(
                    &value.event_id,
                    "decode query metadata",
                )?)
                .map_err(|_| invalid_response("decode query metadata"))?,
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
            consistency: requested_consistency,
        },
    })
}

pub(crate) fn encode_scope(value: &AuthorityScope) -> wire::AuthorityScope {
    let scope = match value {
        AuthorityScope::Platform => wire::authority_scope::Scope::Platform(wire::PlatformScope {}),
        AuthorityScope::Holding { holding_id } => {
            wire::authority_scope::Scope::Holding(wire::HoldingScope {
                holding_id: holding_id.to_string(),
            })
        }
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => wire::authority_scope::Scope::Company(wire::CompanyScope {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
        }),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => wire::authority_scope::Scope::Branch(wire::BranchScope {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
            branch_id: branch_id.to_string(),
        }),
        AuthorityScope::Subtree {
            holding_id,
            organization_node_id,
        } => wire::authority_scope::Scope::Subtree(wire::SubtreeScope {
            holding_id: holding_id.to_string(),
            organization_node_id: organization_node_id.to_string(),
        }),
    };
    wire::AuthorityScope { scope: Some(scope) }
}

pub(crate) fn decode_scope(value: Option<wire::AuthorityScope>) -> Result<AuthorityScope, UiError> {
    use wire::authority_scope::Scope;
    let value = match value
        .and_then(|value| value.scope)
        .ok_or_else(|| invalid_response("decode holding scope"))?
    {
        Scope::Platform(_) => AuthorityScope::Platform,
        Scope::Holding(value) => AuthorityScope::Holding {
            holding_id: response_uuid(&value.holding_id, "decode holding scope")?,
        },
        Scope::Company(value) => AuthorityScope::Company {
            holding_id: response_uuid(&value.holding_id, "decode holding scope")?,
            company_id: response_uuid(&value.company_id, "decode holding scope")?,
        },
        Scope::Branch(value) => AuthorityScope::Branch {
            holding_id: response_uuid(&value.holding_id, "decode holding scope")?,
            company_id: response_uuid(&value.company_id, "decode holding scope")?,
            branch_id: response_uuid(&value.branch_id, "decode holding scope")?,
        },
        Scope::Subtree(value) => AuthorityScope::Subtree {
            holding_id: response_uuid(&value.holding_id, "decode holding scope")?,
            organization_node_id: uuid(&value.organization_node_id)?,
        },
    };
    value
        .validate()
        .map_err(|_| invalid_response("decode holding scope"))?;
    Ok(value)
}

pub(crate) fn uuid(value: &str, operation: &'static str) -> Result<Uuid, UiError> {
    response_uuid(value, operation)
}

pub(crate) fn timestamp(value: &str) -> Result<DateTime<Utc>, UiError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| invalid_response("decode timestamp"))
}

pub(super) fn date(value: &str) -> Result<NaiveDate, UiError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| invalid_response("decode date"))
}

pub(crate) fn status_ui(operation: &'static str, status: tonic::Status) -> UiError {
    transport_ui(TransportError::from_status(operation, status))
}

pub(super) fn unsupported_ui(operation: &'static str) -> UiError {
    transport_ui(TransportError::new(
        TransportErrorKind::InvalidRequest,
        operation,
    ))
}

pub(crate) fn invalid_response(operation: &'static str) -> UiError {
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

fn encode_actor(value: &ActorAttribution) -> wire::ActorAttribution {
    let kind = match value.kind {
        ActorKind::PlatformUser => wire::ActorKind::PlatformUser,
        ActorKind::HoldingUser => wire::ActorKind::HoldingUser,
        ActorKind::Service => wire::ActorKind::Service,
        ActorKind::SystemWorker => wire::ActorKind::SystemWorker,
        // The legacy wire contract has no Agent discriminator. Encoding it as
        // unspecified fails closed instead of granting service authority.
        ActorKind::Agent => wire::ActorKind::Unspecified,
    };
    wire::ActorAttribution {
        principal_ref: value.principal_ref.clone(),
        kind: kind as i32,
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
    let requirement = match value {
        QueryConsistency::Eventual => {
            wire::query_consistency::Requirement::Eventual(wire::EventualConsistency {})
        }
        QueryConsistency::ReadYourWrites(value) => {
            wire::query_consistency::Requirement::ReadYourWrites(wire::ReadYourWrites {
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

fn response_uuid(value: &str, operation: &'static str) -> Result<Uuid, UiError> {
    Uuid::parse_str(value)
        .ok()
        .filter(|value| !value.is_nil())
        .ok_or_else(|| invalid_response(operation))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_and_status_are_bounded() {
        assert!(Wave67Route::try_new(Uuid::nil().to_string(), "holding").is_err());
        assert!(Wave67Route::try_new(Uuid::new_v4().to_string(), "holding\nsecret").is_err());
        let error = status_ui(
            "wave 6/7 query",
            tonic::Status::internal("postgres://secret"),
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
