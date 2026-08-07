use erpx_cqrs::{
    CONTRACT_VERSION, CommandEnvelope, EnvelopeError, IdempotencyKey, QueryConsistency,
    QueryEnvelope,
};
use erpx_kernel::{
    ActorAttribution, AuditContext, CommandId, CorrelationId, EventId, QueryId, ScopeError,
    TenantScope, ValidationError,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionContext {
    pub actor: ActorAttribution,
    pub tenant_scope: TenantScope,
}

impl SessionContext {
    pub fn validate(&self) -> Result<(), RequestContextError> {
        self.actor.validate()?;
        self.tenant_scope.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestContextBuilder {
    session: SessionContext,
    audit: AuditContext,
}

impl RequestContextBuilder {
    pub fn try_new(
        session: SessionContext,
        purpose: impl Into<String>,
        customer_visible: bool,
        module: Option<String>,
    ) -> Result<Self, RequestContextError> {
        session.validate()?;
        let audit = AuditContext {
            purpose: purpose.into(),
            customer_visible,
            module,
        };
        audit.validate()?;
        Ok(Self { session, audit })
    }

    pub fn command<C>(
        &self,
        command_id: CommandId,
        idempotency_key: IdempotencyKey,
        correlation_id: CorrelationId,
        causation_id: Option<EventId>,
        expected_aggregate_version: Option<u64>,
        command: C,
    ) -> Result<CommandEnvelope<C>, RequestContextError> {
        let envelope = CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            command_id,
            idempotency_key,
            correlation_id,
            causation_id,
            actor: self.session.actor.clone(),
            tenant_scope: self.session.tenant_scope.clone(),
            audit: self.audit.clone(),
            expected_aggregate_version,
            command,
        };
        envelope.validate()?;
        Ok(envelope)
    }

    pub fn query<Q>(
        &self,
        query_id: QueryId,
        correlation_id: CorrelationId,
        consistency: QueryConsistency,
        query: Q,
    ) -> Result<QueryEnvelope<Q>, RequestContextError> {
        let envelope = QueryEnvelope {
            contract_version: CONTRACT_VERSION,
            query_id,
            correlation_id,
            actor: self.session.actor.clone(),
            tenant_scope: self.session.tenant_scope.clone(),
            audit: self.audit.clone(),
            consistency,
            query,
        };
        envelope.validate()?;
        Ok(envelope)
    }

    pub fn session(&self) -> &SessionContext {
        &self.session
    }

    pub fn audit(&self) -> &AuditContext {
        &self.audit
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RequestContextError {
    #[error("session or audit context is invalid")]
    Validation(#[from] ValidationError),
    #[error("tenant scope is invalid")]
    Scope(#[from] ScopeError),
    #[error("command or query envelope is invalid")]
    Envelope(#[from] EnvelopeError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_cqrs::IdempotencyKey;
    use erpx_kernel::{ActorKind, CommandId, CorrelationId, QueryId};

    fn command_id(value: u128) -> CommandId {
        CommandId::try_from_uuid(
            format!("{value:032x}")
                .parse()
                .expect("test UUID must parse"),
        )
        .unwrap()
    }

    fn correlation_id(value: u128) -> CorrelationId {
        CorrelationId::try_from_uuid(
            format!("{value:032x}")
                .parse()
                .expect("test UUID must parse"),
        )
        .unwrap()
    }

    fn query_id(value: u128) -> QueryId {
        QueryId::try_from_uuid(
            format!("{value:032x}")
                .parse()
                .expect("test UUID must parse"),
        )
        .unwrap()
    }

    fn builder() -> RequestContextBuilder {
        RequestContextBuilder::try_new(
            SessionContext {
                actor: ActorAttribution {
                    principal_ref: "user:1".into(),
                    kind: ActorKind::TenantUser,
                    session_id: None,
                    represented_principal_ref: None,
                    support_capability_id: None,
                },
                tenant_scope: TenantScope::Platform,
            },
            "quality inspection",
            true,
            Some("quality".into()),
        )
        .unwrap()
    }

    #[test]
    fn command_and_query_envelopes_share_validated_session_context() {
        let builder = builder();
        let command = builder
            .command(
                command_id(1),
                IdempotencyKey::try_new("quality:inspection:1").unwrap(),
                correlation_id(2),
                None,
                Some(3),
                "approve",
            )
            .unwrap();
        let query = builder
            .query(
                query_id(3),
                correlation_id(2),
                QueryConsistency::Eventual,
                "get",
            )
            .unwrap();
        assert_eq!(command.actor, query.actor);
        assert_eq!(command.tenant_scope, query.tenant_scope);
        assert_eq!(command.audit, query.audit);
    }

    #[test]
    fn invalid_sessions_fail_before_an_envelope_is_built() {
        let result = RequestContextBuilder::try_new(
            SessionContext {
                actor: ActorAttribution {
                    principal_ref: "".into(),
                    kind: ActorKind::TenantUser,
                    session_id: None,
                    represented_principal_ref: None,
                    support_capability_id: None,
                },
                tenant_scope: TenantScope::Platform,
            },
            "quality inspection",
            true,
            None,
        );
        assert!(matches!(result, Err(RequestContextError::Validation(_))));
    }
}
