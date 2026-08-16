use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::{
    domain::{
        ManufacturingDomainError, ManufacturingSpecification, ProductionOrder, ProductionOrderId,
        SpecificationId,
    },
    ports::{
        ActorContext, AuthorizationPort, CommitReceipt, ManufacturingAction,
        ManufacturingAggregate, ManufacturingCommit, ManufacturingReadRepository,
        ManufacturingWriteRepository, MutationIdentity, PortError,
    },
    record::ManufacturingRecord,
};

pub struct ManufacturingCommandService<A, W> {
    authorization: A,
    write: W,
}

impl<A, W> ManufacturingCommandService<A, W>
where
    A: AuthorizationPort,
    W: ManufacturingWriteRepository,
{
    pub const fn new(authorization: A, write: W) -> Self {
        Self {
            authorization,
            write,
        }
    }

    pub async fn publish_specification(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        correlation_id: erpx_kernel::CorrelationId,
        mut specification: ManufacturingSpecification,
        at: DateTime<Utc>,
    ) -> Result<CommitReceipt, ManufacturingError> {
        identity.validate()?;
        let authority = self
            .authorization
            .authorize(
                actor,
                specification.scope(),
                ManufacturingAction::PublishSpecification,
            )
            .await?;
        let existing = self
            .write
            .load_specification(&authority, specification.reference().specification_id)
            .await?;
        if existing.is_some() {
            return Err(ManufacturingError::Conflict);
        }
        specification.release(at)?;
        let version = specification.reference().version;
        self.write
            .commit(ManufacturingCommit {
                identity,
                correlation_id,
                authority,
                expected_version: None,
                event_type: "manufacturing.specification_released".into(),
                event_payload: serde_json::json!({"version": version}),
                aggregate: ManufacturingAggregate::Specification(specification),
            })
            .await
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_order(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        order_id: ProductionOrderId,
        target_scope: &erpx_kernel::TenantScope,
        specification_id: SpecificationId,
        specification_version: u64,
        planned_output: erpx_integration_contracts::Quantity,
        correlation_id: erpx_kernel::CorrelationId,
        at: DateTime<Utc>,
    ) -> Result<CommitReceipt, ManufacturingError> {
        identity.validate()?;
        let authority = self
            .authorization
            .authorize(actor, target_scope, ManufacturingAction::CreateOrder)
            .await?;
        if self.write.load_order(&authority, order_id).await?.is_some() {
            return Err(ManufacturingError::Conflict);
        }
        let specification = self
            .write
            .load_specification(&authority, specification_id)
            .await?
            .ok_or(ManufacturingError::Neutral)?;
        if specification.scope() != target_scope
            || specification.reference().version != specification_version
        {
            return Err(ManufacturingError::Neutral);
        }
        let order = ProductionOrder::draft(
            order_id,
            target_scope.clone(),
            &specification,
            planned_output,
            correlation_id,
            at,
        )?;
        self.write
            .commit(ManufacturingCommit {
                identity,
                correlation_id,
                authority,
                expected_version: None,
                event_type: "manufacturing.order_created".into(),
                event_payload: serde_json::json!({
                    "specification_id": specification_id.as_uuid(),
                    "specification_version": specification_version,
                }),
                aggregate: ManufacturingAggregate::Order(order),
            })
            .await
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn mutate_order<F>(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        order_id: ProductionOrderId,
        target_scope: &erpx_kernel::TenantScope,
        requested_version: Option<u64>,
        action: ManufacturingAction,
        event_type: &str,
        change: F,
    ) -> Result<CommitReceipt, ManufacturingError>
    where
        F: FnOnce(&mut ProductionOrder) -> Result<(), ManufacturingDomainError>,
    {
        identity.validate()?;
        let authority = self
            .authorization
            .authorize(actor, target_scope, action)
            .await?;
        let mut order = self
            .write
            .load_order(&authority, order_id)
            .await?
            .ok_or(ManufacturingError::Neutral)?;
        if order.scope() != target_scope {
            return Err(ManufacturingError::Neutral);
        }
        let expected = order.version();
        if requested_version != Some(expected) {
            return Err(ManufacturingError::Conflict);
        }
        change(&mut order)?;
        let correlation_id = order.correlation_id();
        self.write
            .commit(ManufacturingCommit {
                identity,
                correlation_id,
                authority,
                expected_version: Some(expected),
                event_type: event_type.into(),
                event_payload: serde_json::json!({"version": order.version()}),
                aggregate: ManufacturingAggregate::Order(order),
            })
            .await
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn govern_record<F>(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        correlation_id: erpx_kernel::CorrelationId,
        record_id: uuid::Uuid,
        target_scope: &erpx_kernel::TenantScope,
        event_type: &str,
        change: F,
    ) -> Result<CommitReceipt, ManufacturingError>
    where
        F: FnOnce(&mut ManufacturingRecord) -> Result<(), ManufacturingDomainError>,
    {
        identity.validate()?;
        let authority = self
            .authorization
            .authorize(actor, target_scope, ManufacturingAction::GovernRecord)
            .await?;
        let mut record = self
            .write
            .load_record(&authority, record_id)
            .await?
            .ok_or(ManufacturingError::Neutral)?;
        if record.scope() != target_scope {
            return Err(ManufacturingError::Neutral);
        }
        let expected = record.version();
        change(&mut record)?;
        self.write
            .commit(ManufacturingCommit {
                identity,
                correlation_id,
                authority,
                expected_version: Some(expected),
                event_type: event_type.into(),
                event_payload: serde_json::json!({"version": record.version()}),
                aggregate: ManufacturingAggregate::Record(record),
            })
            .await
            .map_err(Into::into)
    }

    pub async fn create_record(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        correlation_id: erpx_kernel::CorrelationId,
        record: ManufacturingRecord,
        event_type: &str,
    ) -> Result<CommitReceipt, ManufacturingError> {
        identity.validate()?;
        record.validate()?;
        if record.version() != 1 {
            return Err(ManufacturingError::Conflict);
        }
        let authority = self
            .authorization
            .authorize(actor, record.scope(), ManufacturingAction::GovernRecord)
            .await?;
        if self
            .write
            .load_record(&authority, record.id())
            .await?
            .is_some()
        {
            return Err(ManufacturingError::Conflict);
        }
        self.write
            .commit(ManufacturingCommit {
                identity,
                correlation_id,
                authority,
                expected_version: None,
                event_type: event_type.into(),
                event_payload: serde_json::json!({"version": record.version()}),
                aggregate: ManufacturingAggregate::Record(record),
            })
            .await
            .map_err(Into::into)
    }
}

pub struct ManufacturingQueryService<A, R> {
    authorization: A,
    read: R,
}
impl<A, R> ManufacturingQueryService<A, R>
where
    A: AuthorizationPort,
    R: ManufacturingReadRepository,
{
    pub const fn new(authorization: A, read: R) -> Self {
        Self {
            authorization,
            read,
        }
    }
    pub async fn get_order(
        &self,
        actor: &ActorContext,
        scope: &erpx_kernel::TenantScope,
        id: ProductionOrderId,
    ) -> Result<crate::ports::ManufacturingOrderProjection, ManufacturingError> {
        let authority = self
            .authorization
            .authorize(actor, scope, ManufacturingAction::View)
            .await?;
        self.read
            .get_order(&authority, id)
            .await?
            .ok_or(ManufacturingError::Neutral)
    }

    pub async fn get_specification(
        &self,
        actor: &ActorContext,
        scope: &erpx_kernel::TenantScope,
        id: SpecificationId,
    ) -> Result<crate::ports::ManufacturingSpecificationProjection, ManufacturingError> {
        let authority = self
            .authorization
            .authorize(actor, scope, ManufacturingAction::View)
            .await?;
        self.read
            .get_specification(&authority, id)
            .await?
            .ok_or(ManufacturingError::Neutral)
    }

    pub async fn get_record(
        &self,
        actor: &ActorContext,
        scope: &erpx_kernel::TenantScope,
        id: uuid::Uuid,
    ) -> Result<crate::ports::ManufacturingRecordProjection, ManufacturingError> {
        let authority = self
            .authorization
            .authorize(actor, scope, ManufacturingAction::View)
            .await?;
        self.read
            .get_record(&authority, id)
            .await?
            .ok_or(ManufacturingError::Neutral)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ManufacturingError {
    #[error("manufacturing item is unavailable")]
    Neutral,
    #[error("manufacturing request conflicts with current state")]
    Conflict,
    #[error("invalid manufacturing request: {0}")]
    Invalid(String),
    #[error("manufacturing dependency is unavailable")]
    Unavailable,
    #[error("manufacturing operation failed")]
    Internal,
}
impl From<ManufacturingDomainError> for ManufacturingError {
    fn from(value: ManufacturingDomainError) -> Self {
        Self::Invalid(value.to_string())
    }
}
impl From<PortError> for ManufacturingError {
    fn from(value: PortError) -> Self {
        match value {
            PortError::Denied | PortError::NotFound => Self::Neutral,
            PortError::Conflict => Self::Conflict,
            PortError::Stale | PortError::Unavailable => Self::Unavailable,
            PortError::InvalidState => Self::Invalid("adapter rejected invalid state".into()),
            PortError::Internal => Self::Internal,
        }
    }
}

#[allow(dead_code)]
fn _ids(_: SpecificationId, _: ProductionOrderId) {}
