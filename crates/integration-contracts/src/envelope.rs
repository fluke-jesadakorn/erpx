use chrono::{DateTime, Utc};
use erpx_kernel::{AggregateId, AuthorityScope, CorrelationId, EventId};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{BoundedText, ContractValidationError};

pub type AggregateKind = BoundedText<{ crate::common::MAX_AGGREGATE_KIND_BYTES }>;
pub type EventType = BoundedText<{ crate::common::MAX_EVENT_TYPE_BYTES }>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundedContext {
    IdentityAccess,
    Organization,
    Commercial,
    PlatformAdministration,
    Brand,
    Customer,
    Marketing,
    Crm,
    Procurement,
    Inventory,
    Assets,
    Maintenance,
    Quality,
    Sales,
    PeoplePayroll,
    Pos,
    Finance,
    Legal,
    Support,
    Operations,
}

pub trait IntegrationEvent:
    std::fmt::Debug + Clone + PartialEq + Serialize + DeserializeOwned + Send + Sync + 'static
{
    fn owner(&self) -> BoundedContext;
    fn event_type(&self) -> &'static str;
    fn schema_version(&self) -> u16;

    fn validate(&self) -> Result<(), ContractValidationError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntegrationEventEnvelope<E> {
    owner: BoundedContext,
    event_id: EventId,
    event_type: EventType,
    event_version: u16,
    active_scope: AuthorityScope,
    aggregate_kind: AggregateKind,
    aggregate_id: AggregateId,
    aggregate_version: u64,
    correlation_id: CorrelationId,
    causation_id: Option<EventId>,
    occurred_at: DateTime<Utc>,
    payload: E,
}

impl<E: IntegrationEvent> IntegrationEventEnvelope<E> {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        event_id: EventId,
        active_scope: AuthorityScope,
        aggregate_kind: AggregateKind,
        aggregate_id: AggregateId,
        aggregate_version: u64,
        correlation_id: CorrelationId,
        causation_id: Option<EventId>,
        occurred_at: DateTime<Utc>,
        payload: E,
    ) -> Result<Self, ContractValidationError> {
        let event_type = EventType::try_new(payload.event_type())?;
        let value = Self {
            owner: payload.owner(),
            event_id,
            event_type,
            event_version: payload.schema_version(),
            active_scope,
            aggregate_kind,
            aggregate_id,
            aggregate_version,
            correlation_id,
            causation_id,
            occurred_at,
            payload,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ContractValidationError> {
        if self.owner != self.payload.owner() {
            return Err(ContractValidationError::OwnerMismatch);
        }
        if self.event_type.as_str() != self.payload.event_type() {
            return Err(ContractValidationError::EventTypeMismatch);
        }
        if self.event_version == 0 {
            return Err(ContractValidationError::ZeroVersion("event_version"));
        }
        if self.event_version != self.payload.schema_version() {
            return Err(ContractValidationError::EventVersionMismatch);
        }
        if self.aggregate_version == 0 {
            return Err(ContractValidationError::ZeroVersion("aggregate_version"));
        }
        if self.event_id.as_uuid().is_nil()
            || self.aggregate_id.as_uuid().is_nil()
            || self.correlation_id.as_uuid().is_nil()
            || self.causation_id.is_some_and(|id| id.as_uuid().is_nil())
        {
            return Err(ContractValidationError::NilIdentifier);
        }
        self.active_scope
            .validate()
            .map_err(|_| ContractValidationError::InvalidHoldingScope)?;
        if self.occurred_at.timestamp() < 0 {
            return Err(ContractValidationError::InvalidTimestamp);
        }
        self.payload.validate()
    }

    pub const fn owner(&self) -> BoundedContext {
        self.owner
    }

    pub const fn event_id(&self) -> EventId {
        self.event_id
    }

    pub fn event_type(&self) -> &str {
        self.event_type.as_str()
    }

    pub const fn event_version(&self) -> u16 {
        self.event_version
    }

    pub const fn active_scope(&self) -> &AuthorityScope {
        &self.active_scope
    }

    pub fn aggregate_kind(&self) -> &str {
        self.aggregate_kind.as_str()
    }

    pub const fn aggregate_id(&self) -> AggregateId {
        self.aggregate_id
    }

    pub const fn aggregate_version(&self) -> u64 {
        self.aggregate_version
    }

    pub const fn correlation_id(&self) -> CorrelationId {
        self.correlation_id
    }

    pub const fn causation_id(&self) -> Option<EventId> {
        self.causation_id
    }

    pub const fn occurred_at(&self) -> &DateTime<Utc> {
        &self.occurred_at
    }

    pub const fn payload(&self) -> &E {
        &self.payload
    }

    pub fn into_payload(self) -> E {
        self.payload
    }
}
