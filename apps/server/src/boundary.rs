use erpx_domain::contracts::CONTRACT_VERSION;
use erpx_proto::common::v2::{
    ActorAttribution, ActorKind, AuditContext, CommandContext, QueryContext,
};
use tonic::{Request, Status};
use uuid::Uuid;

use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundaryContext {
    pub actor: String,
    pub correlation_id: Uuid,
    pub request_id: Uuid,
}

/// Transport-level guard applied before a typed RPC handler is selected.
/// Typed query/command validation remains mandatory after deserialization.
pub fn transport<T>(request: Request<T>) -> Result<Request<T>, Status> {
    let actor = request
        .metadata()
        .get("x-erpx-actor")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Status::unauthenticated("x-erpx-actor is required"))?;
    let session = request
        .metadata()
        .get("x-erpx-session")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Status::unauthenticated("x-erpx-session is required"))?;
    if actor.len() > 128 || session.len() > 256 {
        return Err(Status::unauthenticated("transport identity is invalid"));
    }
    if actor.chars().any(char::is_control) || session.chars().any(char::is_control) {
        return Err(Status::unauthenticated("transport identity is invalid"));
    }
    Ok(request)
}

/// Returns the request correlation used when a handler rejects malformed or
/// unauthorized input before a typed query/command context can be decoded.
/// A fresh identifier is preferable to a nil correlation in those failures.
pub fn request_correlation<T>(request: &Request<T>) -> Uuid {
    correlation_values([
        request
            .metadata()
            .get("x-erpx-correlation-id")
            .map(|value| value.as_bytes()),
        request
            .metadata()
            .get("x-erpx-correlation")
            .map(|value| value.as_bytes()),
        request
            .metadata()
            .get("x-correlation-id")
            .map(|value| value.as_bytes()),
    ])
    .ok()
    .flatten()
    .unwrap_or_else(Uuid::new_v4)
}

/// Resolve correlation-header aliases without allowing conflicting audit
/// identifiers. The canonical header is `x-erpx-correlation-id`; the other
/// two names remain accepted only for compatibility.
pub fn correlation_values<'a>(
    values: impl IntoIterator<Item = Option<&'a [u8]>>,
) -> Result<Option<Uuid>, Status> {
    let mut resolved = None;
    for value in values.into_iter().flatten() {
        let value = std::str::from_utf8(value)
            .map_err(|_| Status::invalid_argument("correlation metadata is invalid"))?;
        if value.is_empty() {
            return Err(Status::invalid_argument("correlation metadata is invalid"));
        }
        let parsed = parse_uuid(value, "correlation metadata")?;
        if resolved.is_some_and(|current| current != parsed) {
            return Err(Status::invalid_argument(
                "correlation metadata aliases conflict",
            ));
        }
        resolved = Some(parsed);
    }
    Ok(resolved)
}

/// Hashes an unparsed request body for inbox/signature verification without
/// accepting a lossy re-serialization as the signed representation.
pub fn raw_body_digest(body: &[u8]) -> [u8; 32] {
    Sha256::digest(body).into()
}

pub fn query<T>(
    request: &Request<T>,
    context: Option<&QueryContext>,
) -> Result<BoundaryContext, Status> {
    let context =
        context.ok_or_else(|| Status::failed_precondition("query context is required"))?;
    validate_common(
        request,
        context.contract_version,
        &context.actor,
        &context.audit,
        &context.correlation_id,
    )?;
    if context.request_id.trim().is_empty() {
        return Err(Status::invalid_argument("query request_id is required"));
    }
    if context.tenant_scope.is_none() {
        return Err(Status::permission_denied("query tenant scope is required"));
    }
    if context.consistency.is_none() {
        return Err(Status::failed_precondition("query consistency is required"));
    }
    Ok(BoundaryContext {
        actor: context
            .actor
            .as_ref()
            .map(|actor| actor.principal_ref.clone())
            .unwrap_or_default(),
        correlation_id: parse_uuid(&context.correlation_id, "query correlation_id")?,
        request_id: parse_uuid(&context.request_id, "query request_id")?,
    })
}

pub fn command<T>(
    request: &Request<T>,
    context: Option<&CommandContext>,
) -> Result<BoundaryContext, Status> {
    let context =
        context.ok_or_else(|| Status::failed_precondition("command context is required"))?;
    validate_common(
        request,
        context.contract_version,
        &context.actor,
        &context.audit,
        &context.correlation_id,
    )?;
    if context.command_id.trim().is_empty() {
        return Err(Status::invalid_argument("command_id is required"));
    }
    if context.idempotency_key.trim().is_empty() {
        return Err(Status::invalid_argument("idempotency_key is required"));
    }
    if context.tenant_scope.is_none() {
        return Err(Status::permission_denied(
            "command tenant scope is required",
        ));
    }
    Ok(BoundaryContext {
        actor: context
            .actor
            .as_ref()
            .map(|actor| actor.principal_ref.clone())
            .unwrap_or_default(),
        correlation_id: parse_uuid(&context.correlation_id, "command correlation_id")?,
        request_id: parse_uuid(&context.command_id, "command_id")?,
    })
}

/// Convert a validated protobuf command context into the transport-neutral
/// domain contract used by application command services.
pub fn command_domain<T>(
    request: &Request<T>,
    context: Option<&CommandContext>,
) -> Result<erpx_domain::contracts::CommandContext, Status> {
    let _boundary = command(request, context)?;
    let context = context.expect("command validated above");
    let actor = context
        .actor
        .as_ref()
        .ok_or_else(|| Status::unauthenticated("actor attribution is required"))?;
    let audit = context
        .audit
        .as_ref()
        .ok_or_else(|| Status::failed_precondition("audit context is required"))?;
    let tenant_scope = context
        .tenant_scope
        .as_ref()
        .and_then(|scope| scope.scope.as_ref())
        .ok_or_else(|| Status::permission_denied("command tenant scope is required"))?;
    let tenant_scope = match tenant_scope {
        erpx_proto::common::v2::tenant_scope::Scope::Platform(_) => {
            erpx_domain::contracts::TenantScope::Platform
        }
        erpx_proto::common::v2::tenant_scope::Scope::Holding(scope) => {
            erpx_domain::contracts::TenantScope::Holding {
                holding_id: parse_uuid(&scope.holding_id, "holding_id")?,
            }
        }
        erpx_proto::common::v2::tenant_scope::Scope::Company(scope) => {
            erpx_domain::contracts::TenantScope::Company {
                holding_id: parse_uuid(&scope.holding_id, "holding_id")?,
                company_id: parse_uuid(&scope.company_id, "company_id")?,
            }
        }
        erpx_proto::common::v2::tenant_scope::Scope::Branch(scope) => {
            erpx_domain::contracts::TenantScope::Branch {
                holding_id: parse_uuid(&scope.holding_id, "holding_id")?,
                company_id: parse_uuid(&scope.company_id, "company_id")?,
                branch_id: parse_uuid(&scope.branch_id, "branch_id")?,
            }
        }
        erpx_proto::common::v2::tenant_scope::Scope::Subtree(scope) => {
            erpx_domain::contracts::TenantScope::Subtree {
                holding_id: parse_uuid(&scope.holding_id, "holding_id")?,
                company_ids: scope
                    .company_ids
                    .iter()
                    .map(|id| parse_uuid(id, "company_id"))
                    .collect::<Result<Vec<_>, _>>()?,
                branch_ids: scope
                    .branch_ids
                    .iter()
                    .map(|id| parse_uuid(id, "branch_id"))
                    .collect::<Result<Vec<_>, _>>()?,
            }
        }
    };
    let kind = match actor.kind {
        value if value == ActorKind::PlatformUser as i32 => {
            erpx_domain::contracts::ActorKind::PlatformUser
        }
        value if value == ActorKind::TenantUser as i32 => {
            erpx_domain::contracts::ActorKind::TenantUser
        }
        value if value == ActorKind::Service as i32 => erpx_domain::contracts::ActorKind::Service,
        value if value == ActorKind::SystemWorker as i32 => {
            erpx_domain::contracts::ActorKind::SystemWorker
        }
        _ => return Err(Status::unauthenticated("actor kind is required")),
    };
    Ok(erpx_domain::contracts::CommandContext {
        contract_version: u16::try_from(context.contract_version)
            .map_err(|_| Status::failed_precondition("unsupported contract version"))?,
        command_id: parse_uuid(&context.command_id, "command_id")?,
        idempotency_key: context.idempotency_key.clone(),
        correlation_id: parse_uuid(&context.correlation_id, "correlation_id")?,
        causation_id: (!context.causation_id.is_empty())
            .then(|| parse_uuid(&context.causation_id, "causation_id"))
            .transpose()?,
        actor: erpx_domain::contracts::ActorAttribution {
            principal_ref: actor.principal_ref.clone(),
            kind,
            session_id: (!actor.session_id.is_empty())
                .then(|| parse_uuid(&actor.session_id, "session_id"))
                .transpose()?,
            represented_principal_ref: (!actor.represented_principal_ref.is_empty())
                .then_some(actor.represented_principal_ref.clone()),
            support_capability_id: (!actor.support_capability_id.is_empty())
                .then(|| parse_uuid(&actor.support_capability_id, "support_capability_id"))
                .transpose()?,
        },
        tenant_scope,
        audit: erpx_domain::contracts::AuditContext {
            purpose: audit.purpose.clone(),
            customer_visible: audit.customer_visible,
            module: (!audit.module.is_empty()).then_some(audit.module.clone()),
            correlation_id: parse_uuid(&audit.correlation_id, "audit correlation_id")?,
        },
        expected_aggregate_version: (context.expected_aggregate_version > 0)
            .then_some(context.expected_aggregate_version),
        subscription_id: (!context.subscription_id.is_empty())
            .then(|| parse_uuid(&context.subscription_id, "subscription_id"))
            .transpose()?,
    })
}

fn validate_common<T>(
    request: &Request<T>,
    contract_version: u32,
    actor: &Option<ActorAttribution>,
    audit: &Option<AuditContext>,
    correlation_id: &str,
) -> Result<(), Status> {
    if contract_version != u32::from(CONTRACT_VERSION) {
        return Err(Status::failed_precondition("unsupported contract version"));
    }
    let actor_header = request
        .metadata()
        .get("x-erpx-actor")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Status::unauthenticated("x-erpx-actor is required"))?;
    if actor_header.len() > 128 || actor_header.chars().any(char::is_control) {
        return Err(Status::unauthenticated("actor identity is invalid"));
    }
    let session_header = request
        .metadata()
        .get("x-erpx-session")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Status::unauthenticated("x-erpx-session is required"))?;
    if session_header.len() > 256 || session_header.chars().any(char::is_control) {
        return Err(Status::unauthenticated("session identity is invalid"));
    }
    let actor = actor
        .as_ref()
        .ok_or_else(|| Status::unauthenticated("actor attribution is required"))?;
    if actor.principal_ref != actor_header {
        return Err(Status::permission_denied(
            "actor attribution does not match session",
        ));
    }
    if actor.session_id != session_header {
        return Err(Status::permission_denied(
            "session attribution does not match request",
        ));
    }
    if actor.kind == ActorKind::Unspecified as i32 {
        return Err(Status::unauthenticated("actor kind is required"));
    }
    let audit = audit
        .as_ref()
        .ok_or_else(|| Status::failed_precondition("audit context is required"))?;
    validate_bounded_text(&audit.purpose, "audit purpose", 128)?;
    if !audit.module.trim().is_empty() {
        validate_bounded_text(&audit.module, "audit module", 64)?;
    }
    if audit.correlation_id != correlation_id {
        return Err(Status::invalid_argument(
            "audit correlation does not match request",
        ));
    }
    let header_correlation = correlation_values([
        request
            .metadata()
            .get("x-erpx-correlation-id")
            .map(|value| value.as_bytes()),
        request
            .metadata()
            .get("x-erpx-correlation")
            .map(|value| value.as_bytes()),
        request
            .metadata()
            .get("x-correlation-id")
            .map(|value| value.as_bytes()),
    ])?;
    if header_correlation.is_some_and(|header| header.to_string() != correlation_id) {
        return Err(Status::invalid_argument(
            "correlation metadata does not match context",
        ));
    }
    let correlation = parse_uuid(correlation_id, "correlation_id")?;
    tracing::debug!(actor = %actor_header, %correlation, "validated ERPX request boundary");
    Ok(())
}

fn parse_uuid(value: &str, field: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(value).map_err(|_| Status::invalid_argument(format!("{field} must be a UUID")))
}

fn validate_bounded_text(
    value: &str,
    field: &'static str,
    maximum_bytes: usize,
) -> Result<(), Status> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Status::invalid_argument(format!("{field} is required")));
    }
    if value.len() > maximum_bytes || value.chars().any(char::is_control) {
        return Err(Status::invalid_argument(format!("{field} is invalid")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_proto::common::v2::{
        ActorKind, EventualConsistency, PlatformScope, QueryConsistency, TenantScope,
        query_consistency,
    };
    use tonic::metadata::MetadataValue;

    fn query_context(actor: &str, session: &str, correlation: &str) -> QueryContext {
        QueryContext {
            contract_version: u32::from(CONTRACT_VERSION),
            request_id: Uuid::new_v4().to_string(),
            correlation_id: correlation.into(),
            actor: Some(ActorAttribution {
                principal_ref: actor.into(),
                kind: ActorKind::PlatformUser as i32,
                session_id: session.into(),
                represented_principal_ref: String::new(),
                support_capability_id: String::new(),
            }),
            tenant_scope: Some(TenantScope {
                scope: Some(erpx_proto::common::v2::tenant_scope::Scope::Platform(
                    PlatformScope {},
                )),
            }),
            consistency: Some(QueryConsistency {
                requirement: Some(query_consistency::Requirement::Eventual(
                    EventualConsistency {},
                )),
            }),
            audit: Some(AuditContext {
                purpose: "test".into(),
                customer_visible: true,
                module: "platform".into(),
                correlation_id: correlation.into(),
            }),
        }
    }

    #[test]
    fn query_requires_attributed_session_and_correlated_audit() {
        let correlation = Uuid::new_v4().to_string();
        let context = query_context("agent", "session", &correlation);
        let mut request = Request::new(());
        request.metadata_mut().insert(
            "x-erpx-actor",
            MetadataValue::try_from("agent").expect("actor metadata"),
        );
        request.metadata_mut().insert(
            "x-erpx-session",
            MetadataValue::try_from("session").expect("session metadata"),
        );
        assert!(query(&request, Some(&context)).is_ok());
        let mut mismatch = context.clone();
        mismatch.audit.as_mut().expect("audit").correlation_id = Uuid::new_v4().to_string();
        assert!(query(&request, Some(&mismatch)).is_err());
    }

    #[test]
    fn missing_context_is_rejected_before_handler_logic() {
        let request = Request::new(());
        assert_eq!(
            query(&request, None).expect_err("missing context").code(),
            tonic::Code::FailedPrecondition
        );
    }

    #[test]
    fn transport_rejects_requests_without_a_session() {
        let request = Request::new(());
        assert_eq!(
            transport(request).expect_err("missing session").code(),
            tonic::Code::Unauthenticated
        );
    }

    #[test]
    fn raw_body_digest_is_stable_without_parsing() {
        assert_eq!(raw_body_digest(b"{}"), raw_body_digest(b"{}"));
        assert_ne!(raw_body_digest(b"{}"), raw_body_digest(b" { } "));
    }

    #[test]
    fn correlation_aliases_must_agree() {
        let id = Uuid::from_u128(17);
        let value = id.to_string();
        assert_eq!(
            correlation_values([Some(value.as_bytes()), Some(value.as_bytes())]).unwrap(),
            Some(id)
        );
        let other = Uuid::from_u128(18).to_string();
        assert!(correlation_values([Some(value.as_bytes()), Some(other.as_bytes())]).is_err());
    }

    #[test]
    fn request_correlation_accepts_all_supported_aliases() {
        let id = Uuid::from_u128(19);
        for header in [
            "x-erpx-correlation-id",
            "x-erpx-correlation",
            "x-correlation-id",
        ] {
            let mut request = Request::new(());
            request
                .metadata_mut()
                .insert(header, MetadataValue::try_from(id.to_string()).unwrap());
            assert_eq!(request_correlation(&request), id);
        }
    }
}
