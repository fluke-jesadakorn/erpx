use chrono::{DateTime, Utc};
use erpx_cqrs::{
    AggregateVersion, CommandEnvelope, CommandReceipt, EventPosition, FreshnessState,
    ProjectionCheckpoint, ProjectionMetadata, ProjectionTarget, QueryConsistency, QueryEnvelope,
    QueryMetadata, QueryResult, ReadYourWrites,
};
use erpx_kernel::{ActorKind, AggregateId, CommandId, CorrelationId, EventId, TenantScope};
use erpx_proto::common::v1 as wire;
use erpx_ui_foundation::{UiError, UiErrorKind};
use tonic::metadata::MetadataValue;
use uuid::Uuid;

use crate::transport::typed_cqrs::{TransportError, TransportErrorKind};

const MAX_TOKEN_BYTES: usize = 16_384;
const MAX_ROUTE_BYTES: usize = 256;

#[derive(Clone)]
pub(super) struct RpcAuthority {
    access_token: MetadataValue<tonic::metadata::Ascii>,
    route: Option<wire::TenantRoute>,
}

impl RpcAuthority {
    pub(super) fn platform(access_token: impl AsRef<str>) -> Result<Self, UiError> {
        Self::build(access_token.as_ref(), None)
    }

    pub(super) fn tenant(
        access_token: impl AsRef<str>,
        tenant_id: impl AsRef<str>,
        holding_ref: impl AsRef<str>,
    ) -> Result<Self, UiError> {
        let tenant_id = tenant_id.as_ref().trim();
        let holding_ref = holding_ref.as_ref().trim();
        if Uuid::parse_str(tenant_id).is_err()
            || holding_ref.is_empty()
            || holding_ref.len() > MAX_ROUTE_BYTES
        {
            return Err(invalid_request());
        }
        Self::build(
            access_token.as_ref(),
            Some(wire::TenantRoute {
                tenant_id: tenant_id.into(),
                holding_ref: holding_ref.into(),
            }),
        )
    }

    pub(super) fn active_scope(access_token: impl AsRef<str>) -> Result<Self, UiError> {
        Self::build(access_token.as_ref(), None)
    }

    fn build(access_token: &str, route: Option<wire::TenantRoute>) -> Result<Self, UiError> {
        if access_token.is_empty() || access_token.len() > MAX_TOKEN_BYTES {
            return Err(invalid_request());
        }
        let access_token = MetadataValue::try_from(access_token).map_err(|_| invalid_request())?;
        Ok(Self {
            access_token,
            route,
        })
    }

    pub(super) fn command<C, T>(
        &self,
        envelope: CommandEnvelope<C>,
        build: impl FnOnce(C, wire::CommandRequestMetadata) -> T,
    ) -> Result<tonic::Request<T>, UiError> {
        envelope.validate().map_err(|_| invalid_request())?;
        let principal_ref = envelope.actor.principal_ref.clone();
        let session_id = envelope.actor.session_id.map(|value| value.to_string());
        let metadata = wire::CommandRequestMetadata {
            contract_version: u32::from(envelope.contract_version),
            command_id: envelope.command_id.to_string(),
            idempotency_key: envelope.idempotency_key.as_str().into(),
            correlation_id: envelope.correlation_id.to_string(),
            causation_id: envelope.causation_id.map(|value| value.to_string()),
            actor: Some(encode_actor(&envelope.actor)),
            tenant_scope: Some(encode_scope(&envelope.tenant_scope)),
            audit: Some(encode_audit(&envelope.audit, envelope.correlation_id)),
            expected_aggregate_version: envelope.expected_aggregate_version,
            subscription_id: None,
            mfa_code: None,
            tenant_route: self.route.clone(),
        };
        self.authenticated(
            build(envelope.command, metadata),
            &principal_ref,
            session_id,
        )
    }

    pub(super) fn query<Q, T>(
        &self,
        envelope: QueryEnvelope<Q>,
        build: impl FnOnce(Q, wire::QueryRequestMetadata) -> T,
    ) -> Result<tonic::Request<T>, UiError> {
        envelope.validate().map_err(|_| invalid_request())?;
        let principal_ref = envelope.actor.principal_ref.clone();
        let session_id = envelope.actor.session_id.map(|value| value.to_string());
        let metadata = wire::QueryRequestMetadata {
            contract_version: u32::from(envelope.contract_version),
            query_id: envelope.query_id.to_string(),
            correlation_id: envelope.correlation_id.to_string(),
            actor: Some(encode_actor(&envelope.actor)),
            tenant_scope: Some(encode_scope(&envelope.tenant_scope)),
            audit: Some(encode_audit(&envelope.audit, envelope.correlation_id)),
            consistency: Some(encode_consistency(&envelope.consistency)),
            tenant_route: self.route.clone(),
        };
        self.authenticated(build(envelope.query, metadata), &principal_ref, session_id)
    }

    fn authenticated<T>(
        &self,
        message: T,
        principal_ref: &str,
        session_id: Option<String>,
    ) -> Result<tonic::Request<T>, UiError> {
        let mut request = tonic::Request::new(message);
        request
            .metadata_mut()
            .insert("x-erpx-access-token", self.access_token.clone());
        insert(&mut request, "x-erpx-actor", principal_ref)?;
        if let Some(session_id) = session_id {
            insert(&mut request, "x-erpx-session", &session_id)?;
        }
        Ok(request)
    }
}

fn insert<T>(
    request: &mut tonic::Request<T>,
    key: &'static str,
    value: &str,
) -> Result<(), UiError> {
    let value = MetadataValue::try_from(value).map_err(|_| invalid_request())?;
    request.metadata_mut().insert(key, value);
    Ok(())
}

fn encode_actor(value: &erpx_kernel::ActorAttribution) -> wire::ActorAttribution {
    wire::ActorAttribution {
        principal_ref: value.principal_ref.clone(),
        kind: match value.kind {
            ActorKind::PlatformUser => wire::ActorKind::PlatformUser as i32,
            ActorKind::TenantUser => wire::ActorKind::TenantUser as i32,
            ActorKind::Service => wire::ActorKind::Service as i32,
            ActorKind::SystemWorker => wire::ActorKind::SystemWorker as i32,
            // The legacy wire contract has no Agent discriminator. Encoding it
            // as unspecified fails closed instead of granting service authority.
            ActorKind::Agent => wire::ActorKind::Unspecified as i32,
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

fn encode_audit(
    value: &erpx_kernel::AuditContext,
    correlation: CorrelationId,
) -> wire::AuditContext {
    wire::AuditContext {
        purpose: value.purpose.clone(),
        customer_visible: value.customer_visible,
        module: value.module.clone().unwrap_or_default(),
        correlation_id: correlation.to_string(),
    }
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

pub(super) fn decode_scope(value: wire::TenantScope) -> Result<TenantScope, UiError> {
    use wire::tenant_scope::Scope;
    let scope = match value.scope.ok_or_else(invalid_response)? {
        Scope::Platform(_) => TenantScope::Platform,
        Scope::Holding(value) => TenantScope::Holding {
            holding_id: uuid(&value.holding_id)?,
        },
        Scope::Company(value) => TenantScope::Company {
            holding_id: uuid(&value.holding_id)?,
            company_id: uuid(&value.company_id)?,
        },
        Scope::Branch(value) => TenantScope::Branch {
            holding_id: uuid(&value.holding_id)?,
            company_id: uuid(&value.company_id)?,
            branch_id: uuid(&value.branch_id)?,
        },
        Scope::Subtree(value) => TenantScope::Subtree {
            holding_id: uuid(&value.holding_id)?,
            company_ids: value
                .company_ids
                .iter()
                .map(|v| uuid(v))
                .collect::<Result<_, _>>()?,
            branch_ids: value
                .branch_ids
                .iter()
                .map(|v| uuid(v))
                .collect::<Result<_, _>>()?,
        },
    };
    scope.validate().map_err(|_| invalid_response())?;
    Ok(scope)
}

fn encode_consistency(value: &QueryConsistency) -> wire::QueryConsistency {
    use wire::query_consistency::Requirement;
    let requirement = match value {
        QueryConsistency::Eventual => Requirement::Eventual(wire::EventualConsistency {}),
        QueryConsistency::ReadYourWrites(value) => {
            #[allow(deprecated)]
            let value = wire::ReadYourWrites {
                command_id: String::new(),
                projection: value.projection.clone(),
                partition: value.partition.clone(),
                event_id: value.target.event_id.to_string(),
                global_sequence: value.target.global_sequence,
                timeout_millis: value.timeout_millis,
            };
            Requirement::ReadYourWrites(value)
        }
    };
    wire::QueryConsistency {
        requirement: Some(requirement),
    }
}

fn decode_consistency(value: Option<wire::QueryConsistency>) -> Result<QueryConsistency, UiError> {
    use wire::query_consistency::Requirement;
    match value.and_then(|value| value.requirement) {
        None | Some(Requirement::Eventual(_)) => Ok(QueryConsistency::Eventual),
        Some(Requirement::ReadYourWrites(value)) => {
            Ok(QueryConsistency::ReadYourWrites(ReadYourWrites {
                projection: value.projection,
                partition: value.partition,
                target: ProjectionTarget {
                    event_id: event_id(&value.event_id)?,
                    global_sequence: value.global_sequence,
                },
                timeout_millis: value.timeout_millis,
            }))
        }
        Some(Requirement::AggregateVersion(_)) => Err(invalid_response()),
    }
}

pub(super) fn command_receipt<R>(
    value: Option<wire::CommandReceipt>,
    response: R,
) -> Result<CommandReceipt<R>, UiError> {
    let value = value.ok_or_else(invalid_response)?;
    let aggregate = value.aggregate.ok_or_else(invalid_response)?;
    let receipt = CommandReceipt {
        command_id: CommandId::try_from_uuid(uuid(&value.command_id)?)
            .map_err(|_| invalid_response())?,
        correlation_id: CorrelationId::try_from_uuid(uuid(&value.correlation_id)?)
            .map_err(|_| invalid_response())?,
        aggregate: AggregateVersion {
            context: aggregate.context,
            aggregate_type: aggregate.aggregate_type,
            aggregate_id: AggregateId::try_from_uuid(uuid(&aggregate.aggregate_id)?)
                .map_err(|_| invalid_response())?,
            version: aggregate.version,
        },
        committed_events: value
            .committed_events
            .into_iter()
            .map(|event| {
                Ok(EventPosition {
                    event_id: event_id(&event.event_id)?,
                    global_sequence: event.global_sequence,
                })
            })
            .collect::<Result<_, UiError>>()?,
        committed_at: time(&value.committed_at)?,
        replayed: value.replayed,
        response,
    };
    if receipt.aggregate.version == 0
        || receipt
            .committed_events
            .iter()
            .any(|event| event.global_sequence == 0)
    {
        return Err(invalid_response());
    }
    Ok(receipt)
}

pub(super) fn query_result<T>(
    value: Option<wire::QueryMetadata>,
    result: T,
) -> Result<QueryResult<T>, UiError> {
    let value = value.ok_or_else(invalid_response)?;
    let projection = value.projection.ok_or_else(invalid_response)?;
    let freshness = match wire::ProjectionFreshnessState::try_from(projection.freshness).ok() {
        Some(wire::ProjectionFreshnessState::Current) => FreshnessState::Current,
        Some(wire::ProjectionFreshnessState::Stale) => FreshnessState::Stale,
        Some(wire::ProjectionFreshnessState::Rebuilding) => FreshnessState::Rebuilding,
        Some(wire::ProjectionFreshnessState::Unavailable) => FreshnessState::Unavailable,
        _ => return Err(invalid_response()),
    };
    let checkpoint = projection
        .checkpoint
        .map(|value| {
            Ok(ProjectionCheckpoint {
                projection: value.projection,
                partition: value.partition,
                event_id: event_id(&value.event_id)?,
                global_sequence: value.global_sequence,
                updated_at: time(&value.updated_at)?,
            })
        })
        .transpose()?;
    let metadata = QueryMetadata {
        correlation_id: CorrelationId::try_from_uuid(uuid(&value.correlation_id)?)
            .map_err(|_| invalid_response())?,
        projection: ProjectionMetadata {
            checkpoint,
            freshness,
            lag_millis: projection.lag_millis,
        },
        consistency: decode_consistency(value.consistency)?,
    };
    Ok(QueryResult {
        value: result,
        metadata,
    })
}

pub(super) fn rpc(error: tonic::Status, operation: &'static str) -> UiError {
    transport(TransportError::from_status(operation, error))
}

fn transport(error: TransportError) -> UiError {
    match error.kind() {
        TransportErrorKind::InvalidRequest | TransportErrorKind::Rejected => invalid_request(),
        TransportErrorKind::InvalidResponse | TransportErrorKind::Service => UiError::bounded(
            UiErrorKind::Failed,
            "invalid_service_response",
            "The service returned an invalid response.",
            false,
            None,
        ),
        TransportErrorKind::AuthenticationRequired => UiError::bounded(
            UiErrorKind::Denied,
            "authentication_required",
            "Please sign in again.",
            false,
            None,
        ),
        TransportErrorKind::AuthorizationDenied => UiError::bounded(
            UiErrorKind::Denied,
            "authorization_denied",
            "You are not allowed to perform this operation.",
            false,
            None,
        ),
        TransportErrorKind::NotFound => UiError::bounded(
            UiErrorKind::NotFound,
            "record_not_found",
            "The requested record was not found.",
            false,
            None,
        ),
        TransportErrorKind::NotReady => UiError::not_ready(
            "Your change was saved, but this view has not caught up yet. Please retry.",
        ),
        TransportErrorKind::Conflict => UiError::bounded(
            UiErrorKind::Conflict,
            "write_conflict",
            "The record changed before this request completed.",
            true,
            None,
        ),
        TransportErrorKind::Throttled | TransportErrorKind::Offline => UiError::offline(),
    }
}

pub(super) fn invalid_request() -> UiError {
    UiError::invalid("The request context is invalid.")
}

pub(super) fn invalid_response() -> UiError {
    UiError::bounded(
        UiErrorKind::Failed,
        "invalid_service_response",
        "The service returned an invalid response.",
        false,
        None,
    )
}

pub(super) fn uuid(value: &str) -> Result<Uuid, UiError> {
    let value = Uuid::parse_str(value).map_err(|_| invalid_response())?;
    if value.is_nil() {
        Err(invalid_response())
    } else {
        Ok(value)
    }
}

fn event_id(value: &str) -> Result<EventId, UiError> {
    EventId::try_from_uuid(uuid(value)?).map_err(|_| invalid_response())
}

pub(super) fn time(value: &str) -> Result<DateTime<Utc>, UiError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| invalid_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_reject_secret_or_route_metadata_that_cannot_be_encoded() {
        assert!(RpcAuthority::platform("\n").is_err());
        assert!(RpcAuthority::tenant("token", "not-a-uuid", "holding").is_err());
    }

    #[test]
    fn rpc_errors_are_bounded_and_never_echo_server_diagnostics() {
        let error = rpc(
            tonic::Status::internal("postgres://root:secret@example.invalid"),
            "test",
        );
        assert_eq!(error.code(), "invalid_service_response");
        assert!(!error.message().contains("secret"));
    }

    #[test]
    fn legacy_wire_actor_fails_closed_for_agent_identity() {
        let encoded = encode_actor(&erpx_kernel::ActorAttribution {
            principal_ref: "agent.safe-ref".into(),
            kind: ActorKind::Agent,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        });
        assert_eq!(encoded.kind, wire::ActorKind::Unspecified as i32);
    }
}
