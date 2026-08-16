use async_trait::async_trait;
use erpx_cqrs::{CqrsError, QueryEnvelope, QueryHandler, QueryMetadata, QueryResult};
use erpx_integration_contracts::customer::CustomerRef;
use erpx_kernel::{CorrelationId, ErrorCategory, ErrorContext, KernelError, TenantScope};
use serde::{Deserialize, Serialize};

use crate::{
    domain::{Customer360ProjectionInput, Customer360View, ProjectionApplyOutcome},
    ports::{
        AuthorizationPort, AuthorizationRequest, Customer360Action, PortError,
        ProjectionRepository, ProjectionWriteRequest, ReadRepository, SourceProjectionMetadata,
    },
};

mod owned_records;
pub use owned_records::Customer360RecordCommandHandler;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetCustomer360 {
    pub customer: CustomerRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetCustomer360Result {
    pub customer: Option<Customer360View>,
    pub source_projections: SourceProjectionMetadata,
}

pub struct Customer360QueryHandler<R, A> {
    read: R,
    authorization: A,
}

impl<R, A> Customer360QueryHandler<R, A> {
    pub const fn new(read: R, authorization: A) -> Self {
        Self {
            read,
            authorization,
        }
    }
}

#[async_trait]
impl<R, A> QueryHandler<GetCustomer360> for Customer360QueryHandler<R, A>
where
    R: ReadRepository,
    A: AuthorizationPort,
{
    type Response = GetCustomer360Result;

    async fn handle(
        &self,
        envelope: QueryEnvelope<GetCustomer360>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        self.authorization
            .authorize(AuthorizationRequest {
                actor: &envelope.actor,
                authority_scope: &envelope.tenant_scope,
                customer: envelope.query.customer,
                action: Customer360Action::View,
            })
            .await
            .map_err(|error| port_to_cqrs(error, envelope.correlation_id))?;

        let projection = self
            .read
            .get(
                &envelope.tenant_scope,
                envelope.query.customer,
                &envelope.consistency,
            )
            .await
            .map_err(|error| port_to_cqrs(error, envelope.correlation_id))?;
        if projection
            .value
            .as_ref()
            .is_some_and(|view| view.customer != envelope.query.customer)
        {
            return Err(operation_error(
                "customer_360.scope_mismatch",
                ErrorCategory::Denied,
                "Customer 360 projection is outside the requested customer scope",
                false,
                envelope.correlation_id,
            ));
        }

        Ok(QueryResult {
            value: GetCustomer360Result {
                customer: projection.value,
                source_projections: projection.sources,
            },
            metadata: QueryMetadata {
                correlation_id: envelope.correlation_id,
                projection: projection.query_metadata,
                consistency: envelope.consistency,
            },
        })
    }
}

/// Application-level process manager. It can write only the composed read
/// model through `ProjectionRepository`; no source-context command port exists.
pub struct Customer360ProjectionHandler<P> {
    projections: P,
}

impl<P> Customer360ProjectionHandler<P> {
    pub const fn new(projections: P) -> Self {
        Self { projections }
    }
}

impl<P: ProjectionRepository> Customer360ProjectionHandler<P> {
    pub async fn apply(
        &self,
        scope: TenantScope,
        input: Customer360ProjectionInput,
    ) -> Result<ProjectionApplyOutcome, PortError> {
        self.projections
            .apply(ProjectionWriteRequest { scope, input })
            .await
    }
}

fn port_to_cqrs(error: PortError, correlation_id: CorrelationId) -> CqrsError {
    match error {
        PortError::NotReady(value) => CqrsError::NotReady(value),
        PortError::Denied => operation_error(
            "customer_360.denied",
            ErrorCategory::Denied,
            "Customer 360 read is denied",
            false,
            correlation_id,
        ),
        PortError::Unavailable => operation_error(
            "customer_360.unavailable",
            ErrorCategory::Unavailable,
            "Customer 360 is temporarily unavailable",
            true,
            correlation_id,
        ),
        PortError::Internal => operation_error(
            "customer_360.internal",
            ErrorCategory::Internal,
            "Customer 360 read failed",
            false,
            correlation_id,
        ),
    }
}

fn operation_error(
    code: &'static str,
    category: ErrorCategory,
    message: &'static str,
    retryable: bool,
    correlation_id: CorrelationId,
) -> CqrsError {
    CqrsError::Operation(KernelError {
        code: code.into(),
        category,
        safe_message: message.into(),
        retryable,
        correlation_id,
        context: ErrorContext::empty(),
    })
}
