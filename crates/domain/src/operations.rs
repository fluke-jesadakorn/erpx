//! Organization hierarchy and Operations orchestration contracts.
//!
//! Operations coordinates work emitted by source contexts. It stores only
//! immutable references to source transactions; sales, procurement, stock,
//! invoices, and journals remain owned by their source contexts.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::access::Scope;
use crate::contracts::{ActorAttribution, EventEnvelope, TenantScope};

pub mod completion;

pub const OPERATIONS_CONTRACT_VERSION: u16 = 1;

/// Operations references are opaque identifiers, not free-form user content.
/// Keeping their limits in the domain contract prevents transport adapters and
/// repositories from silently accepting unbounded routing or assignment data.
pub const MAX_EXCEPTION_REFERENCE_BYTES: usize = 255;
pub const MAX_EXCEPTION_TEXT_BYTES: usize = 128;
pub const MAX_EXCEPTION_SLA_POLICY_BYTES: usize = 128;
pub const MAX_EXCEPTION_IDEMPOTENCY_KEY_BYTES: usize = 255;
pub const MAX_EXCEPTION_SLA_MINUTES: u32 = 7 * 24 * 60;
pub const MAX_OPERATIONS_REFERENCE_BYTES: usize = 255;
pub const MAX_OPERATIONS_NAME_BYTES: usize = 255;
pub const MAX_OPERATIONS_DOMAIN_BYTES: usize = 128;
pub const MAX_OPERATIONS_STEP_BYTES: usize = 1_024;
pub const MAX_OPERATIONS_SLA_MINUTES: u32 = 7 * 24 * 60;
pub const MAX_OPERATIONS_POLICY_DOMAINS: usize = 16;
pub const MAX_OPERATIONS_SOURCE_EVENT_TYPE_BYTES: usize = 255;
pub const MAX_OPERATIONS_SOURCE_EVENT_PAYLOAD_BYTES: usize = 256 * 1024;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum OperationsError {
    #[error("{field} cannot be empty")]
    EmptyField { field: &'static str },
    #[error("aggregate ID cannot be nil")]
    NilAggregateId,
    #[error("aggregate scope must be an explicit holding, company, or branch scope")]
    InvalidAggregateScope,
    #[error("scope {child:?} is outside parent scope {parent:?}")]
    ScopeOutsideParent {
        parent: Box<Scope>,
        child: Box<Scope>,
    },
    #[error("{field} must end after it starts")]
    InvalidWindow { field: &'static str },
    #[error("invalid {aggregate_type} lifecycle transition: {from} -> {to}")]
    InvalidTransition {
        aggregate_type: &'static str,
        from: String,
        to: String,
    },
    #[error("event version must be positive")]
    InvalidEventVersion,
    #[error("idempotency key cannot be empty")]
    EmptyIdempotencyKey,
    #[error("aggregate version overflow")]
    VersionOverflow,
    #[error("source reference type cannot be owned by Operations")]
    InvalidSourceReference,
    #[error("work queue policy source domain list is outside its bounded range")]
    InvalidWorkQueuePolicyDomains,
    #[error("work queue policy source domain is duplicated: {domain}")]
    DuplicateSourceDomain { domain: String },
    #[error("exception command ID cannot be nil")]
    InvalidExceptionCommandId,
    #[error("exception command expected version {expected}, actual {actual}")]
    ExceptionVersionMismatch { expected: u64, actual: u64 },
    #[error("exception action {action} is not allowed while status is {status}")]
    InvalidExceptionAction {
        action: &'static str,
        status: String,
    },
    #[error("{field} principal reference {principal_ref} is not active at command time")]
    InactiveExceptionPrincipal {
        field: &'static str,
        principal_ref: String,
    },
    #[error("exception {field} is required")]
    MissingExceptionField { field: &'static str },
    #[error("exception state {status} has inconsistent assignment or SLA fields")]
    InvalidExceptionState { status: String },
    #[error("{field} exceeds the maximum length of {max} bytes")]
    ExceptionFieldTooLong { field: &'static str, max: usize },
    #[error("exception SLA metadata is outside its bounded window")]
    InvalidExceptionSla,
    #[error("unsupported Operations source domain: {domain}")]
    UnsupportedSourceDomain { domain: String },
    #[error("source event payload exceeds the bounded size")]
    SourceEventPayloadTooLarge,
    #[error("source event metadata is invalid")]
    InvalidSourceEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregateMetadata {
    pub id: Uuid,
    pub holding_ref: String,
    pub scope: Scope,
    /// Version zero is the unpersisted form. The write repository assigns the
    /// first version atomically with insertion.
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AggregateMetadata {
    pub fn new(
        id: Uuid,
        holding_ref: impl Into<String>,
        scope: Scope,
        at: DateTime<Utc>,
    ) -> Result<Self, OperationsError> {
        let metadata = Self {
            id,
            holding_ref: holding_ref.into(),
            scope,
            version: 0,
            created_at: at,
            updated_at: at,
        };
        metadata.validate()?;
        Ok(metadata)
    }

    pub fn validate(&self) -> Result<(), OperationsError> {
        if self.id.is_nil() {
            return Err(OperationsError::NilAggregateId);
        }
        require_text(&self.holding_ref, "holding_ref")?;
        validate_persisted_scope(&self.scope)?;
        if self.updated_at < self.created_at {
            return Err(OperationsError::InvalidWindow {
                field: "aggregate timestamps",
            });
        }
        Ok(())
    }

    pub fn next_version(&self) -> Result<u64, OperationsError> {
        self.version
            .checked_add(1)
            .ok_or(OperationsError::VersionOverflow)
    }
}

pub fn ensure_scope_containment(parent: &Scope, child: &Scope) -> Result<(), OperationsError> {
    if parent.contains(child) {
        Ok(())
    } else {
        Err(OperationsError::ScopeOutsideParent {
            parent: Box::new(parent.clone()),
            child: Box::new(child.clone()),
        })
    }
}

fn require_scope_holding(scope: &Scope, expected: Uuid) -> Result<(), OperationsError> {
    let actual = match scope {
        Scope::Holding { holding_id }
        | Scope::Company { holding_id, .. }
        | Scope::Branch { holding_id, .. } => *holding_id,
        Scope::Platform | Scope::Subtree { .. } => {
            return Err(OperationsError::InvalidAggregateScope);
        }
    };
    if actual == expected {
        Ok(())
    } else {
        Err(OperationsError::ScopeOutsideParent {
            parent: Box::new(Scope::Holding {
                holding_id: expected,
            }),
            child: Box::new(scope.clone()),
        })
    }
}

fn require_text(value: &str, field: &'static str) -> Result<(), OperationsError> {
    if value.trim().is_empty() {
        Err(OperationsError::EmptyField { field })
    } else {
        Ok(())
    }
}

fn require_bounded_text(
    value: &str,
    field: &'static str,
    max: usize,
) -> Result<(), OperationsError> {
    require_text(value, field)?;
    if value.len() > max {
        return Err(OperationsError::ExceptionFieldTooLong { field, max });
    }
    Ok(())
}

fn require_window(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    field: &'static str,
) -> Result<(), OperationsError> {
    if end > start {
        Ok(())
    } else {
        Err(OperationsError::InvalidWindow { field })
    }
}

/// Operations records are always bound to one concrete tenant hierarchy node.
/// Platform and access-query subtree scopes are authorization scopes, not
/// source/aggregate record scopes; nil hierarchy IDs would also make RLS and
/// branch-isolation checks meaningless.
fn validate_persisted_scope(scope: &Scope) -> Result<(), OperationsError> {
    let valid = match scope {
        Scope::Holding { holding_id } => !holding_id.is_nil(),
        Scope::Company {
            holding_id,
            company_id,
        } => !holding_id.is_nil() && !company_id.is_nil(),
        Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => !holding_id.is_nil() && !company_id.is_nil() && !branch_id.is_nil(),
        Scope::Platform | Scope::Subtree { .. } => false,
    };
    if valid {
        Ok(())
    } else {
        Err(OperationsError::InvalidAggregateScope)
    }
}

fn ensure_exact_scope(actual: &Scope, expected: &Scope) -> Result<(), OperationsError> {
    if actual == expected {
        Ok(())
    } else {
        Err(OperationsError::ScopeOutsideParent {
            parent: Box::new(actual.clone()),
            child: Box::new(expected.clone()),
        })
    }
}

/// Source contexts that may publish facts into the Operations orchestration
/// boundary. The enum is intentionally closed: accepting an arbitrary string
/// here would make a new source context look supported before its ownership,
/// scope, and reconciliation contract exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationsSourceDomain {
    Sales,
    Procurement,
    Inventory,
    Finance,
    Maintenance,
    Support,
    Integrations,
}

impl OperationsSourceDomain {
    pub const ALL: [Self; 7] = [
        Self::Sales,
        Self::Procurement,
        Self::Inventory,
        Self::Finance,
        Self::Maintenance,
        Self::Support,
        Self::Integrations,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sales => "sales",
            Self::Procurement => "procurement",
            Self::Inventory => "inventory",
            Self::Finance => "finance",
            Self::Maintenance => "maintenance",
            Self::Support => "support",
            Self::Integrations => "integrations",
        }
    }

    pub fn parse(value: &str) -> Result<Self, OperationsError> {
        match value {
            "sales" => Ok(Self::Sales),
            "procurement" => Ok(Self::Procurement),
            "inventory" => Ok(Self::Inventory),
            "finance" => Ok(Self::Finance),
            "maintenance" => Ok(Self::Maintenance),
            "support" => Ok(Self::Support),
            "integrations" => Ok(Self::Integrations),
            domain => Err(OperationsError::UnsupportedSourceDomain {
                domain: domain.into(),
            }),
        }
    }
}

/// A source-owned fact presented to Operations. The source context remains the
/// owner of the transaction and its payload. Operations persists only the
/// source identity/provenance attached to an orchestration aggregate and the
/// resulting Operations event state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationsSourceEvent {
    pub source_domain: OperationsSourceDomain,
    pub source_type: String,
    pub source_id: Uuid,
    pub source_event_id: Uuid,
    pub source_version: u64,
    pub event_type: String,
    pub event_version: u16,
    pub holding_ref: String,
    pub scope: Scope,
    pub actor: ActorAttribution,
    pub correlation_id: Uuid,
    pub causation_id: Option<Uuid>,
    pub occurred_at: DateTime<Utc>,
    pub payload: Value,
}

impl OperationsSourceEvent {
    pub fn validate(&self) -> Result<(), OperationsError> {
        require_bounded_text(
            &self.source_type,
            "source_type",
            MAX_OPERATIONS_REFERENCE_BYTES,
        )?;
        require_bounded_text(
            &self.event_type,
            "event_type",
            MAX_OPERATIONS_SOURCE_EVENT_TYPE_BYTES,
        )?;
        require_bounded_text(
            &self.holding_ref,
            "holding_ref",
            MAX_OPERATIONS_REFERENCE_BYTES,
        )?;
        if self.source_id.is_nil()
            || self.source_event_id.is_nil()
            || self.correlation_id.is_nil()
            || self.source_version == 0
            || self.event_version == 0
        {
            return Err(OperationsError::InvalidSourceEvent);
        }
        if validate_persisted_scope(&self.scope).is_err() {
            return Err(OperationsError::InvalidSourceEvent);
        }
        if let Some(causation_id) = self.causation_id
            && causation_id.is_nil()
        {
            return Err(OperationsError::InvalidSourceEvent);
        }
        let payload_bytes =
            serde_json::to_vec(&self.payload).map_err(|_| OperationsError::InvalidSourceEvent)?;
        if payload_bytes.len() > MAX_OPERATIONS_SOURCE_EVENT_PAYLOAD_BYTES {
            return Err(OperationsError::SourceEventPayloadTooLarge);
        }
        if self
            .event_type
            .chars()
            .any(|character| character.is_control())
        {
            return Err(OperationsError::InvalidSourceEvent);
        }
        Ok(())
    }

    pub fn source_reference(&self) -> Result<SourceTransactionRef, OperationsError> {
        self.validate()?;
        SourceTransactionRef::new(
            self.source_domain.as_str(),
            self.source_type.clone(),
            self.source_id,
            self.source_event_id,
        )
    }

    pub fn idempotency_key(&self) -> Result<String, OperationsError> {
        self.validate()?;
        Ok(format!("operations-source-event:{}", self.source_event_id))
    }

    pub fn operations_event_type(&self) -> Result<String, OperationsError> {
        self.validate()?;
        let event_type = format!(
            "operations.source.{}.{}",
            self.source_domain.as_str(),
            self.event_type
        );
        if event_type.len() > MAX_OPERATIONS_SOURCE_EVENT_TYPE_BYTES {
            return Err(OperationsError::ExceptionFieldTooLong {
                field: "operations_event_type",
                max: MAX_OPERATIONS_SOURCE_EVENT_TYPE_BYTES,
            });
        }
        Ok(event_type)
    }

    /// A source fact may feed an aggregate at the same or a narrower scope,
    /// never a parent or sibling scope. This keeps branch events from creating
    /// holding-wide orchestration state accidentally.
    pub fn validate_for_aggregate_scope(
        &self,
        aggregate_scope: &Scope,
    ) -> Result<(), OperationsError> {
        self.validate()?;
        ensure_scope_containment(&self.scope, aggregate_scope)
    }
}

fn require_unique_ids(values: &[Uuid], field: &'static str) -> Result<(), OperationsError> {
    if values.is_empty() || values.iter().any(Uuid::is_nil) {
        return Err(OperationsError::EmptyField { field });
    }
    let mut ids = BTreeSet::new();
    if values.iter().any(|value| !ids.insert(*value)) {
        return Err(OperationsError::InvalidSourceReference);
    }
    Ok(())
}

fn tenant_scope(scope: &Scope) -> TenantScope {
    match scope {
        Scope::Platform => TenantScope::Platform,
        Scope::Holding { holding_id } => TenantScope::Holding {
            holding_id: *holding_id,
        },
        Scope::Company {
            holding_id,
            company_id,
        } => TenantScope::Company {
            holding_id: *holding_id,
            company_id: *company_id,
        },
        Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => TenantScope::Branch {
            holding_id: *holding_id,
            company_id: *company_id,
            branch_id: *branch_id,
        },
        Scope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        } => TenantScope::Subtree {
            holding_id: *holding_id,
            company_ids: company_ids.clone(),
            branch_ids: branch_ids.clone(),
        },
    }
}

/// An immutable pointer to an event owned by another bounded context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceTransactionRef {
    pub source_domain: String,
    pub source_type: String,
    pub source_id: Uuid,
    pub source_event_id: Uuid,
}

impl SourceTransactionRef {
    pub fn new(
        source_domain: impl Into<String>,
        source_type: impl Into<String>,
        source_id: Uuid,
        source_event_id: Uuid,
    ) -> Result<Self, OperationsError> {
        let value = Self {
            source_domain: source_domain.into(),
            source_type: source_type.into(),
            source_id,
            source_event_id,
        };
        require_bounded_text(
            &value.source_domain,
            "source_domain",
            MAX_OPERATIONS_DOMAIN_BYTES,
        )?;
        require_bounded_text(
            &value.source_type,
            "source_type",
            MAX_OPERATIONS_REFERENCE_BYTES,
        )?;
        if value.source_id.is_nil() || value.source_event_id.is_nil() {
            return Err(OperationsError::InvalidSourceReference);
        }
        if value.source_domain.eq_ignore_ascii_case("operations") {
            return Err(OperationsError::InvalidSourceReference);
        }
        Ok(value)
    }

    fn validate(&self) -> Result<(), OperationsError> {
        Self::new(
            self.source_domain.clone(),
            self.source_type.clone(),
            self.source_id,
            self.source_event_id,
        )
        .map(|_| ())
    }
}

/// Validate a complete immutable provenance set. A source event can only be
/// referenced once by an aggregate; duplicate rows would make replay and
/// reconciliation ambiguous without adding ownership to Operations.
pub fn validate_source_references(
    source_refs: &[SourceTransactionRef],
) -> Result<(), OperationsError> {
    let mut event_ids = BTreeSet::new();
    for source in source_refs {
        source.validate()?;
        if !event_ids.insert(source.source_event_id) {
            return Err(OperationsError::InvalidSourceReference);
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionEventMetadata {
    pub projection_name: String,
    pub event_id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub projected_at: DateTime<Utc>,
}

impl ProjectionEventMetadata {
    pub fn validate(&self) -> Result<(), OperationsError> {
        require_text(&self.projection_name, "projection_name")?;
        require_text(&self.aggregate_type, "aggregate_type")?;
        if self.event_id.is_nil() || self.aggregate_id.is_nil() || self.aggregate_version == 0 {
            return Err(OperationsError::InvalidEventVersion);
        }
        Ok(())
    }
}

pub trait OperationsAggregate: Clone + Serialize + DeserializeOwned {
    const AGGREGATE_TYPE: &'static str;

    fn metadata(&self) -> &AggregateMetadata;
    /// Immutable provenance owned by source contexts. Operations may reference
    /// these events but never creates or mutates their transaction records.
    fn source_references(&self) -> &[SourceTransactionRef] {
        &[]
    }
    fn lifecycle_status(&self) -> &'static str;
    fn validate(&self) -> Result<(), OperationsError>;
    fn canonicalized(&self) -> Self {
        self.clone()
    }

    fn canonical_json(&self) -> Result<Value, OperationsError> {
        serde_json::to_value(self.canonicalized()).map_err(|_| OperationsError::EmptyField {
            field: "aggregate payload",
        })
    }

    fn canonical_bytes(&self) -> Result<Vec<u8>, OperationsError> {
        serde_json::to_vec(&self.canonicalized()).map_err(|_| OperationsError::EmptyField {
            field: "aggregate payload",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationsEvent {
    pub envelope: EventEnvelope,
    pub projection: ProjectionEventMetadata,
}

#[allow(clippy::too_many_arguments)]
pub fn event_for<A: OperationsAggregate>(
    aggregate: &A,
    event_type: impl Into<String>,
    event_version: u16,
    actor: ActorAttribution,
    idempotency_key: Option<String>,
    correlation_id: Uuid,
    causation_id: Option<Uuid>,
    occurred_at: DateTime<Utc>,
) -> Result<OperationsEvent, OperationsError> {
    if event_version == 0 {
        return Err(OperationsError::InvalidEventVersion);
    }
    if let Some(key) = &idempotency_key
        && key.trim().is_empty()
    {
        return Err(OperationsError::EmptyIdempotencyKey);
    }
    aggregate.validate()?;
    let event_id = Uuid::new_v4();
    let envelope = EventEnvelope {
        contract_version: OPERATIONS_CONTRACT_VERSION,
        event_id,
        event_type: event_type.into(),
        event_version,
        occurred_at,
        aggregate_type: A::AGGREGATE_TYPE.into(),
        aggregate_id: aggregate.metadata().id,
        aggregate_version: aggregate.metadata().version,
        tenant_scope: tenant_scope(&aggregate.metadata().scope),
        actor,
        correlation_id,
        causation_id,
        idempotency_key,
        payload: aggregate.canonical_json()?,
    };
    let projection = ProjectionEventMetadata {
        projection_name: "operations_read_model".into(),
        event_id,
        aggregate_type: A::AGGREGATE_TYPE.into(),
        aggregate_id: aggregate.metadata().id,
        aggregate_version: aggregate.metadata().version,
        projected_at: occurred_at,
    };
    Ok(OperationsEvent {
        envelope,
        projection,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationStatus {
    Active,
    Suspended,
    Archived,
}

impl OrganizationStatus {
    fn transition(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Active, Self::Suspended | Self::Archived)
                | (Self::Suspended, Self::Active | Self::Archived)
        )
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Suspended => "suspended",
            Self::Archived => "archived",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Holding {
    pub meta: AggregateMetadata,
    pub code: String,
    pub name: String,
    pub status: OrganizationStatus,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Company {
    pub meta: AggregateMetadata,
    pub holding_id: Uuid,
    pub registration_number: String,
    pub legal_name: String,
    pub status: OrganizationStatus,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Headquarters {
    pub meta: AggregateMetadata,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub name: String,
    pub status: OrganizationStatus,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Branch {
    pub meta: AggregateMetadata,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub code: String,
    pub name: String,
    pub status: OrganizationStatus,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Department {
    pub meta: AggregateMetadata,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub name: String,
    pub status: OrganizationStatus,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Team {
    pub meta: AggregateMetadata,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub department_id: Option<Uuid>,
    pub name: String,
    pub status: OrganizationStatus,
}

macro_rules! organization_aggregate {
    ($type:ty, $kind:expr, $expected:expr, $holding:expr, $name:expr) => {
        impl OperationsAggregate for $type {
            const AGGREGATE_TYPE: &'static str = $kind;
            fn metadata(&self) -> &AggregateMetadata {
                &self.meta
            }
            fn lifecycle_status(&self) -> &'static str {
                self.status.as_str()
            }
            fn validate(&self) -> Result<(), OperationsError> {
                self.meta.validate()?;
                let name = ($name)(self);
                require_text(&name, "name")?;
                require_scope_holding(&self.meta.scope, ($holding)(self))?;
                ensure_exact_scope(&self.meta.scope, &($expected)(self))
            }
        }
    };
}

organization_aggregate!(
    Holding,
    "organization.holding",
    |value: &Holding| Scope::Holding {
        holding_id: self_holding_id(value)
    },
    |value: &Holding| self_holding_id(value),
    |value: &Holding| value.name.clone()
);
organization_aggregate!(
    Company,
    "organization.company",
    |value: &Company| Scope::Company {
        holding_id: value.holding_id,
        company_id: value.meta.id
    },
    |value: &Company| value.holding_id,
    |value: &Company| value.legal_name.clone()
);
organization_aggregate!(
    Headquarters,
    "organization.headquarters",
    |value: &Headquarters| Scope::Company {
        holding_id: value.holding_id,
        company_id: value.company_id
    },
    |value: &Headquarters| value.holding_id,
    |value: &Headquarters| value.name.clone()
);
organization_aggregate!(
    Branch,
    "organization.branch",
    |value: &Branch| Scope::Branch {
        holding_id: value.holding_id,
        company_id: value.company_id,
        branch_id: value.meta.id
    },
    |value: &Branch| value.holding_id,
    |value: &Branch| value.name.clone()
);

fn self_holding_id(value: &Holding) -> Uuid {
    match value.meta.scope {
        Scope::Holding { holding_id } => holding_id,
        _ => Uuid::nil(),
    }
}

impl OperationsAggregate for Department {
    const AGGREGATE_TYPE: &'static str = "organization.department";
    fn metadata(&self) -> &AggregateMetadata {
        &self.meta
    }
    fn lifecycle_status(&self) -> &'static str {
        self.status.as_str()
    }
    fn validate(&self) -> Result<(), OperationsError> {
        self.meta.validate()?;
        require_text(&self.name, "name")?;
        require_scope_holding(&self.meta.scope, self.holding_id)?;
        let expected = self.branch_id.map_or(
            Scope::Company {
                holding_id: self.holding_id,
                company_id: self.company_id,
            },
            |branch_id| Scope::Branch {
                holding_id: self.holding_id,
                company_id: self.company_id,
                branch_id,
            },
        );
        ensure_exact_scope(&self.meta.scope, &expected)
    }
}

impl OperationsAggregate for Team {
    const AGGREGATE_TYPE: &'static str = "organization.team";
    fn metadata(&self) -> &AggregateMetadata {
        &self.meta
    }
    fn lifecycle_status(&self) -> &'static str {
        self.status.as_str()
    }
    fn validate(&self) -> Result<(), OperationsError> {
        self.meta.validate()?;
        require_text(&self.name, "name")?;
        require_scope_holding(&self.meta.scope, self.holding_id)?;
        let expected = self.branch_id.map_or(
            Scope::Company {
                holding_id: self.holding_id,
                company_id: self.company_id,
            },
            |branch_id| Scope::Branch {
                holding_id: self.holding_id,
                company_id: self.company_id,
                branch_id,
            },
        );
        ensure_exact_scope(&self.meta.scope, &expected)
    }
}

macro_rules! organization_transition {
    ($type:ty) => {
        impl $type {
            pub fn transition_to(
                &mut self,
                next: OrganizationStatus,
                at: DateTime<Utc>,
            ) -> Result<(), OperationsError> {
                if at < self.meta.updated_at {
                    return Err(OperationsError::InvalidWindow {
                        field: "organization transition time",
                    });
                }
                if !self.status.transition(next) {
                    return Err(OperationsError::InvalidTransition {
                        aggregate_type: <$type as OperationsAggregate>::AGGREGATE_TYPE,
                        from: self.status.as_str().into(),
                        to: next.as_str().into(),
                    });
                }
                let mut candidate = self.clone();
                candidate.status = next;
                candidate.meta.updated_at = at;
                candidate.validate()?;
                *self = candidate;
                Ok(())
            }
        }
    };
}

organization_transition!(Holding);
organization_transition!(Company);
organization_transition!(Headquarters);
organization_transition!(Branch);
organization_transition!(Department);
organization_transition!(Team);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationalPlanStatus {
    Draft,
    Approved,
    Active,
    Completed,
    Cancelled,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitmentStatus {
    Proposed,
    Committed,
    Fulfilled,
    Cancelled,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FulfillmentPlanStatus {
    Draft,
    Planned,
    Dispatched,
    Completed,
    Cancelled,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapacityPlanStatus {
    Draft,
    Published,
    Retired,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExceptionStatus {
    Detected,
    Assigned,
    Acknowledged,
    Escalated,
    Resolved,
    Closed,
}

/// An authoritative, time-bounded principal reference supplied by the
/// authorization evaluator to an Operations command. Operations never
/// resolves identity or role state itself; missing or expired validity proof
/// fails closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExceptionPrincipalReference {
    pub principal_ref: String,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
}

impl ExceptionPrincipalReference {
    pub fn validate(&self, at: DateTime<Utc>, field: &'static str) -> Result<(), OperationsError> {
        require_bounded_text(&self.principal_ref, field, MAX_EXCEPTION_REFERENCE_BYTES)?;
        if self.valid_until <= self.valid_from || at < self.valid_from || at >= self.valid_until {
            return Err(OperationsError::InactiveExceptionPrincipal {
                field,
                principal_ref: self.principal_ref.clone(),
            });
        }
        Ok(())
    }
}

/// Bounded SLA metadata stored with an exception snapshot. The due time is
/// the resolution deadline; escalation_after_minutes is the maximum age at
/// which an unhandled exception may remain before a manager escalation is
/// required by a future worker/policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExceptionSlaMetadata {
    pub policy_ref: String,
    pub due_at: DateTime<Utc>,
    pub escalation_after_minutes: u32,
}

impl ExceptionSlaMetadata {
    pub fn validate_shape(&self, created_at: DateTime<Utc>) -> Result<(), OperationsError> {
        require_bounded_text(
            &self.policy_ref,
            "sla_policy_ref",
            MAX_EXCEPTION_SLA_POLICY_BYTES,
        )?;
        if self.due_at <= created_at
            || self.escalation_after_minutes == 0
            || self.escalation_after_minutes > MAX_EXCEPTION_SLA_MINUTES
        {
            return Err(OperationsError::InvalidExceptionSla);
        }
        Ok(())
    }

    pub fn validate_for_command(&self, at: DateTime<Utc>) -> Result<(), OperationsError> {
        self.validate_shape(at)?;
        if self.due_at > at + chrono::Duration::minutes(i64::from(MAX_EXCEPTION_SLA_MINUTES)) {
            return Err(OperationsError::InvalidExceptionSla);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExceptionLifecycleAction {
    Assign {
        assignee: ExceptionPrincipalReference,
        manager: ExceptionPrincipalReference,
        sla: ExceptionSlaMetadata,
    },
    Reassign {
        assignee: ExceptionPrincipalReference,
        manager: ExceptionPrincipalReference,
    },
    Acknowledge,
    Escalate {
        manager: ExceptionPrincipalReference,
    },
    Resolve,
    /// Permanently close a resolved exception after its corrective work has
    /// been accepted. Closing is deliberately separate from resolution so the
    /// queue can retain a resolved case until an authorized operator performs
    /// the final handoff.
    Close,
}

impl ExceptionLifecycleAction {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Assign { .. } => "assign",
            Self::Reassign { .. } => "reassign",
            Self::Acknowledge => "acknowledge",
            Self::Escalate { .. } => "escalate",
            Self::Resolve => "resolve",
            Self::Close => "close",
        }
    }

    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Assign { .. } => "operations.exception.assigned",
            Self::Reassign { .. } => "operations.exception.reassigned",
            Self::Acknowledge => "operations.exception.acknowledged",
            Self::Escalate { .. } => "operations.exception.escalated",
            Self::Resolve => "operations.exception.resolved",
            Self::Close => "operations.exception.closed",
        }
    }
}

/// Transport-neutral Operations lifecycle command. `actor` and every
/// assignment/manager reference include their active validity window. The
/// authenticated boundary must obtain these proofs from the authoritative
/// identity/RBAC evaluator before constructing the command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExceptionLifecycleCommand {
    pub exception_id: Uuid,
    pub expected_version: u64,
    pub at: DateTime<Utc>,
    pub actor: ExceptionPrincipalReference,
    pub actor_attribution: ActorAttribution,
    pub idempotency_key: String,
    pub correlation_id: Uuid,
    pub causation_id: Option<Uuid>,
    pub action: ExceptionLifecycleAction,
}

impl ExceptionLifecycleCommand {
    pub fn validate(&self) -> Result<(), OperationsError> {
        if self.exception_id.is_nil() || self.correlation_id.is_nil() {
            return Err(OperationsError::InvalidExceptionCommandId);
        }
        require_bounded_text(
            &self.idempotency_key,
            "idempotency_key",
            MAX_EXCEPTION_IDEMPOTENCY_KEY_BYTES,
        )?;
        self.actor.validate(self.at, "actor")?;
        if self.actor_attribution.principal_ref != self.actor.principal_ref {
            return Err(OperationsError::InvalidSourceReference);
        }
        match &self.action {
            ExceptionLifecycleAction::Assign {
                assignee,
                manager,
                sla,
            } => {
                assignee.validate(self.at, "assignee")?;
                manager.validate(self.at, "manager")?;
                sla.validate_for_command(self.at)?;
            }
            ExceptionLifecycleAction::Reassign { assignee, manager } => {
                assignee.validate(self.at, "assignee")?;
                manager.validate(self.at, "manager")?;
            }
            ExceptionLifecycleAction::Acknowledge
            | ExceptionLifecycleAction::Resolve
            | ExceptionLifecycleAction::Close => {}
            ExceptionLifecycleAction::Escalate { manager } => {
                manager.validate(self.at, "manager")?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionLifecycleTransition {
    pub from: ExceptionStatus,
    pub to: ExceptionStatus,
    pub aggregate_version: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkQueuePolicyStatus {
    Draft,
    Active,
    Paused,
    Retired,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatingCalendarStatus {
    Draft,
    Published,
    Closed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShiftStatus {
    Planned,
    Open,
    HandoverPending,
    Closed,
    Cancelled,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoverStatus {
    Draft,
    Submitted,
    Accepted,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StandardOperatingProcedureStatus {
    Draft,
    Approved,
    Published,
    Retired,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchBatchStatus {
    Draft,
    Scheduled,
    Dispatched,
    Completed,
    Cancelled,
}

macro_rules! status_impl {
    ($( $status:ident { $( $from:ident => [$( $to:ident ),*] => $from_name:literal ),* ; terminal [$( $terminal:ident => $terminal_name:literal ),*] } ),* $(,)?) => {
        $(
            impl $status {
                fn can_transition(self, next: Self) -> bool {
                    match self {
                        $(Self::$from => matches!(next, $(Self::$to)|*),)*
                        $(Self::$terminal => false,)*
                    }
                }
                fn as_str(self) -> &'static str {
                    match self {
                        $(Self::$from => $from_name,)*
                        $(Self::$terminal => $terminal_name,)*
                    }
                }
            }
        )*
    };
}

status_impl! {
    OperationalPlanStatus { Draft => [Approved, Cancelled] => "draft", Approved => [Active, Cancelled] => "approved", Active => [Completed, Cancelled] => "active"; terminal [Completed => "completed", Cancelled => "cancelled"] },
    CommitmentStatus { Proposed => [Committed, Cancelled] => "proposed", Committed => [Fulfilled, Cancelled] => "committed"; terminal [Fulfilled => "fulfilled", Cancelled => "cancelled"] },
    FulfillmentPlanStatus { Draft => [Planned, Cancelled] => "draft", Planned => [Dispatched, Cancelled] => "planned", Dispatched => [Completed, Cancelled] => "dispatched"; terminal [Completed => "completed", Cancelled => "cancelled"] },
    CapacityPlanStatus { Draft => [Published] => "draft", Published => [Retired] => "published"; terminal [Retired => "retired"] },
    ExceptionStatus { Detected => [Assigned, Escalated, Resolved] => "detected", Assigned => [Acknowledged, Escalated, Resolved] => "assigned", Acknowledged => [Escalated, Resolved] => "acknowledged", Escalated => [Acknowledged, Resolved] => "escalated", Resolved => [Closed] => "resolved"; terminal [Closed => "closed"] },
    WorkQueuePolicyStatus { Draft => [Active, Retired] => "draft", Active => [Paused, Retired] => "active", Paused => [Active, Retired] => "paused"; terminal [Retired => "retired"] },
    OperatingCalendarStatus { Draft => [Published, Closed] => "draft", Published => [Closed] => "published"; terminal [Closed => "closed"] },
    ShiftStatus { Planned => [Open, Cancelled] => "planned", Open => [HandoverPending, Closed, Cancelled] => "open", HandoverPending => [Closed] => "handover_pending"; terminal [Closed => "closed", Cancelled => "cancelled"] },
    HandoverStatus { Draft => [Submitted] => "draft", Submitted => [Accepted] => "submitted"; terminal [Accepted => "accepted"] },
    StandardOperatingProcedureStatus { Draft => [Approved, Retired] => "draft", Approved => [Published, Retired] => "approved", Published => [Retired] => "published"; terminal [Retired => "retired"] },
    DispatchBatchStatus { Draft => [Scheduled, Cancelled] => "draft", Scheduled => [Dispatched, Cancelled] => "scheduled", Dispatched => [Completed, Cancelled] => "dispatched"; terminal [Completed => "completed", Cancelled => "cancelled"] },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalPlan {
    pub meta: AggregateMetadata,
    pub name: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub branch_scopes: Vec<Scope>,
    #[serde(default)]
    pub source_refs: Vec<SourceTransactionRef>,
    pub status: OperationalPlanStatus,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalCommitment {
    pub meta: AggregateMetadata,
    pub plan_id: Uuid,
    pub subject: String,
    pub quantity: i64,
    pub unit: String,
    pub due_at: DateTime<Utc>,
    pub source: SourceTransactionRef,
    pub status: CommitmentStatus,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FulfillmentPlan {
    pub meta: AggregateMetadata,
    pub operational_plan_id: Uuid,
    pub commitment_ids: Vec<Uuid>,
    #[serde(default)]
    pub source_refs: Vec<SourceTransactionRef>,
    pub status: FulfillmentPlanStatus,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapacityPlan {
    pub meta: AggregateMetadata,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub capacities: BTreeMap<String, i64>,
    #[serde(default)]
    pub source_refs: Vec<SourceTransactionRef>,
    pub status: CapacityPlanStatus,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExceptionCase {
    pub meta: AggregateMetadata,
    pub category: String,
    pub severity: String,
    pub source: SourceTransactionRef,
    pub assigned_to: Option<String>,
    pub due_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub manager_ref: Option<String>,
    #[serde(default)]
    pub sla: Option<ExceptionSlaMetadata>,
    pub status: ExceptionStatus,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkQueuePolicy {
    pub meta: AggregateMetadata,
    pub name: String,
    pub allowed_domains: Vec<String>,
    pub escalation_after_minutes: u32,
    #[serde(default)]
    pub source_refs: Vec<SourceTransactionRef>,
    pub status: WorkQueuePolicyStatus,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatingCalendar {
    pub meta: AggregateMetadata,
    pub name: String,
    pub timezone: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    #[serde(default)]
    pub source_refs: Vec<SourceTransactionRef>,
    pub status: OperatingCalendarStatus,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shift {
    pub meta: AggregateMetadata,
    pub calendar_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Uuid,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub assignee_refs: Vec<String>,
    #[serde(default)]
    pub source_refs: Vec<SourceTransactionRef>,
    pub status: ShiftStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShiftScheduleInput {
    pub calendar_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Uuid,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub assignee_refs: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShiftHandover {
    pub meta: AggregateMetadata,
    pub shift_id: Uuid,
    pub outgoing_ref: String,
    pub incoming_ref: String,
    pub summary: String,
    #[serde(default)]
    pub source_refs: Vec<SourceTransactionRef>,
    pub status: HandoverStatus,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StandardOperatingProcedure {
    pub meta: AggregateMetadata,
    pub name: String,
    pub revision: u32,
    pub steps: Vec<String>,
    #[serde(default)]
    pub source_refs: Vec<SourceTransactionRef>,
    pub status: StandardOperatingProcedureStatus,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispatchBatch {
    pub meta: AggregateMetadata,
    pub fulfillment_plan_id: Uuid,
    pub item_ids: Vec<Uuid>,
    #[serde(default)]
    pub source_refs: Vec<SourceTransactionRef>,
    pub status: DispatchBatchStatus,
}

macro_rules! operations_aggregate {
    ($type:ty, $kind:expr, $status:ident, |$value:ident| $checks:block, $refs:path) => {
        impl OperationsAggregate for $type {
            const AGGREGATE_TYPE: &'static str = $kind;
            fn metadata(&self) -> &AggregateMetadata {
                &self.meta
            }
            fn lifecycle_status(&self) -> &'static str {
                self.status.as_str()
            }
            fn source_references(&self) -> &[SourceTransactionRef] {
                $refs(self)
            }
            fn canonicalized(&self) -> Self {
                <$type>::canonicalized(self)
            }
            fn validate(&self) -> Result<(), OperationsError> {
                self.meta.validate()?;
                validate_source_references(self.source_references())?;
                let $value = self;
                $checks
                Ok(())
            }
        }
    };
}

fn operational_plan_source_refs(value: &OperationalPlan) -> &[SourceTransactionRef] {
    &value.source_refs
}

fn operational_commitment_source_refs(value: &OperationalCommitment) -> &[SourceTransactionRef] {
    std::slice::from_ref(&value.source)
}

fn fulfillment_plan_source_refs(value: &FulfillmentPlan) -> &[SourceTransactionRef] {
    &value.source_refs
}

fn capacity_plan_source_refs(value: &CapacityPlan) -> &[SourceTransactionRef] {
    &value.source_refs
}

fn exception_case_source_refs(value: &ExceptionCase) -> &[SourceTransactionRef] {
    std::slice::from_ref(&value.source)
}

fn work_queue_policy_source_refs(value: &WorkQueuePolicy) -> &[SourceTransactionRef] {
    &value.source_refs
}

fn operating_calendar_source_refs(value: &OperatingCalendar) -> &[SourceTransactionRef] {
    &value.source_refs
}

fn shift_source_refs(value: &Shift) -> &[SourceTransactionRef] {
    &value.source_refs
}

fn shift_handover_source_refs(value: &ShiftHandover) -> &[SourceTransactionRef] {
    &value.source_refs
}

fn standard_operating_procedure_source_refs(
    value: &StandardOperatingProcedure,
) -> &[SourceTransactionRef] {
    &value.source_refs
}

fn dispatch_batch_source_refs(value: &DispatchBatch) -> &[SourceTransactionRef] {
    &value.source_refs
}

operations_aggregate!(
    OperationalPlan,
    "operations.operational_plan",
    OperationalPlanStatus,
    |value| {
        require_bounded_text(&value.name, "name", MAX_OPERATIONS_NAME_BYTES)?;
        require_window(value.period_start, value.period_end, "period")?;
        let mut scopes = BTreeSet::new();
        for scope in &value.branch_scopes {
            if !matches!(scope, Scope::Branch { .. }) {
                return Err(OperationsError::InvalidAggregateScope);
            }
            ensure_scope_containment(&value.meta.scope, scope)?;
            if !scopes.insert(scope.canonical_key()) {
                return Err(OperationsError::InvalidAggregateScope);
            }
        }
    },
    operational_plan_source_refs
);
operations_aggregate!(
    OperationalCommitment,
    "operations.operational_commitment",
    CommitmentStatus,
    |value| {
        if value.plan_id.is_nil() || value.quantity < 0 || value.due_at <= value.meta.created_at {
            return Err(OperationsError::InvalidSourceReference);
        }
        require_bounded_text(&value.subject, "subject", MAX_OPERATIONS_NAME_BYTES)?;
        require_bounded_text(&value.unit, "unit", MAX_OPERATIONS_REFERENCE_BYTES)?;
        value.source.validate()?;
    },
    operational_commitment_source_refs
);
operations_aggregate!(
    FulfillmentPlan,
    "operations.fulfillment_plan",
    FulfillmentPlanStatus,
    |value| {
        if value.operational_plan_id.is_nil() {
            return Err(OperationsError::InvalidSourceReference);
        }
        require_unique_ids(&value.commitment_ids, "commitment_ids")?;
    },
    fulfillment_plan_source_refs
);
operations_aggregate!(
    CapacityPlan,
    "operations.capacity_plan",
    CapacityPlanStatus,
    |value| {
        require_window(value.period_start, value.period_end, "period")?;
        if value.capacities.values().any(|capacity| *capacity < 0) {
            return Err(OperationsError::InvalidSourceReference);
        }
        for key in value.capacities.keys() {
            require_bounded_text(key, "capacity_key", MAX_OPERATIONS_REFERENCE_BYTES)?;
        }
    },
    capacity_plan_source_refs
);
operations_aggregate!(
    ExceptionCase,
    "operations.exception_case",
    ExceptionStatus,
    |value| {
        require_bounded_text(&value.category, "category", MAX_EXCEPTION_TEXT_BYTES)?;
        require_bounded_text(&value.severity, "severity", MAX_EXCEPTION_TEXT_BYTES)?;
        value.source.validate()?;
        if let Some(assignee) = &value.assigned_to {
            require_bounded_text(assignee, "assigned_to", MAX_EXCEPTION_REFERENCE_BYTES)?;
        }
        if let Some(manager) = &value.manager_ref {
            require_bounded_text(manager, "manager_ref", MAX_EXCEPTION_REFERENCE_BYTES)?;
        }
        if let Some(sla) = &value.sla {
            sla.validate_shape(value.meta.created_at)?;
            if value.due_at != Some(sla.due_at) {
                return Err(OperationsError::InvalidExceptionSla);
            }
        }
        let has_assignment = value.assigned_to.is_some() && value.manager_ref.is_some();
        let has_sla = value.due_at.is_some() && value.sla.is_some();
        match value.status {
            ExceptionStatus::Detected
                if value.assigned_to.is_some()
                    || value.manager_ref.is_some()
                    || value.due_at.is_some()
                    || value.sla.is_some() =>
            {
                return Err(OperationsError::InvalidExceptionState {
                    status: value.status.as_str().into(),
                });
            }
            ExceptionStatus::Detected => {}
            _ if !has_assignment || !has_sla => {
                return Err(OperationsError::InvalidExceptionState {
                    status: value.status.as_str().into(),
                });
            }
            _ => {}
        }
    },
    exception_case_source_refs
);
operations_aggregate!(
    WorkQueuePolicy,
    "operations.work_queue_policy",
    WorkQueuePolicyStatus,
    |value| {
        require_bounded_text(&value.name, "name", MAX_OPERATIONS_NAME_BYTES)?;
        if value.allowed_domains.is_empty()
            || value.allowed_domains.len() > MAX_OPERATIONS_POLICY_DOMAINS
        {
            return Err(OperationsError::InvalidWorkQueuePolicyDomains);
        }
        let mut domains = BTreeSet::new();
        for domain in &value.allowed_domains {
            require_bounded_text(domain, "allowed_domains", MAX_OPERATIONS_DOMAIN_BYTES)?;
            let parsed = OperationsSourceDomain::parse(domain)?;
            if !domains.insert(parsed) {
                return Err(OperationsError::DuplicateSourceDomain {
                    domain: domain.clone(),
                });
            }
        }
        if value.escalation_after_minutes == 0
            || value.escalation_after_minutes > MAX_OPERATIONS_SLA_MINUTES
        {
            return Err(OperationsError::InvalidWindow {
                field: "escalation_after_minutes",
            });
        }
    },
    work_queue_policy_source_refs
);
operations_aggregate!(
    OperatingCalendar,
    "operations.operating_calendar",
    OperatingCalendarStatus,
    |value| {
        require_bounded_text(&value.name, "name", MAX_OPERATIONS_NAME_BYTES)?;
        require_bounded_text(&value.timezone, "timezone", MAX_OPERATIONS_REFERENCE_BYTES)?;
        require_window(value.period_start, value.period_end, "period")?;
    },
    operating_calendar_source_refs
);
operations_aggregate!(
    Shift,
    "operations.shift",
    ShiftStatus,
    |value| {
        if value.calendar_id.is_nil() || value.company_id.is_nil() || value.branch_id.is_nil() {
            return Err(OperationsError::InvalidSourceReference);
        }
        require_window(value.starts_at, value.ends_at, "shift")?;
        require_scope_holding(&value.meta.scope, value.meta.scope.holding_id())?;
        ensure_scope_containment(
            &value.meta.scope,
            &Scope::Branch {
                holding_id: value.meta.scope.holding_id(),
                company_id: value.company_id,
                branch_id: value.branch_id,
            },
        )?;
        for assignee in &value.assignee_refs {
            require_bounded_text(assignee, "assignee_refs", MAX_OPERATIONS_REFERENCE_BYTES)?;
        }
    },
    shift_source_refs
);
operations_aggregate!(
    ShiftHandover,
    "operations.shift_handover",
    HandoverStatus,
    |value| {
        if value.shift_id.is_nil() {
            return Err(OperationsError::InvalidSourceReference);
        }
        require_bounded_text(
            &value.outgoing_ref,
            "outgoing_ref",
            MAX_OPERATIONS_REFERENCE_BYTES,
        )?;
        require_bounded_text(
            &value.incoming_ref,
            "incoming_ref",
            MAX_OPERATIONS_REFERENCE_BYTES,
        )?;
        require_bounded_text(&value.summary, "summary", MAX_OPERATIONS_STEP_BYTES)?;
    },
    shift_handover_source_refs
);
operations_aggregate!(
    StandardOperatingProcedure,
    "operations.standard_operating_procedure",
    StandardOperatingProcedureStatus,
    |value| {
        require_bounded_text(&value.name, "name", MAX_OPERATIONS_NAME_BYTES)?;
        if value.revision == 0 || value.steps.is_empty() {
            return Err(OperationsError::InvalidSourceReference);
        }
        for step in &value.steps {
            require_bounded_text(step, "steps", MAX_OPERATIONS_STEP_BYTES)?;
        }
    },
    standard_operating_procedure_source_refs
);
operations_aggregate!(
    DispatchBatch,
    "operations.dispatch_batch",
    DispatchBatchStatus,
    |value| {
        if value.fulfillment_plan_id.is_nil() {
            return Err(OperationsError::InvalidSourceReference);
        }
        require_unique_ids(&value.item_ids, "item_ids")?;
    },
    dispatch_batch_source_refs
);

trait ScopeHoldingId {
    fn holding_id(&self) -> Uuid;
}

impl ScopeHoldingId for Scope {
    fn holding_id(&self) -> Uuid {
        match self {
            Scope::Holding { holding_id }
            | Scope::Company { holding_id, .. }
            | Scope::Branch { holding_id, .. }
            | Scope::Subtree { holding_id, .. } => *holding_id,
            Scope::Platform => Uuid::nil(),
        }
    }
}

macro_rules! operations_transition {
    ($type:ty, $status:ident) => {
        impl $type {
            pub fn transition_to(
                &mut self,
                next: $status,
                at: DateTime<Utc>,
            ) -> Result<(), OperationsError> {
                if at < self.meta.updated_at {
                    return Err(OperationsError::InvalidWindow {
                        field: "aggregate transition time",
                    });
                }
                if !self.status.can_transition(next) {
                    return Err(OperationsError::InvalidTransition {
                        aggregate_type: <$type as OperationsAggregate>::AGGREGATE_TYPE,
                        from: self.status.as_str().into(),
                        to: next.as_str().into(),
                    });
                }
                let mut candidate = self.clone();
                candidate.status = next;
                candidate.meta.updated_at = at;
                candidate.validate()?;
                *self = candidate;
                Ok(())
            }
        }
    };
}

operations_transition!(OperationalPlan, OperationalPlanStatus);
operations_transition!(OperationalCommitment, CommitmentStatus);
operations_transition!(FulfillmentPlan, FulfillmentPlanStatus);
operations_transition!(CapacityPlan, CapacityPlanStatus);
operations_transition!(ExceptionCase, ExceptionStatus);
operations_transition!(WorkQueuePolicy, WorkQueuePolicyStatus);
operations_transition!(OperatingCalendar, OperatingCalendarStatus);
operations_transition!(Shift, ShiftStatus);
operations_transition!(ShiftHandover, HandoverStatus);
operations_transition!(StandardOperatingProcedure, StandardOperatingProcedureStatus);
operations_transition!(DispatchBatch, DispatchBatchStatus);

impl OperationalPlan {
    pub fn new(
        meta: AggregateMetadata,
        name: impl Into<String>,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
        branch_scopes: Vec<Scope>,
        source_refs: Vec<SourceTransactionRef>,
    ) -> Result<Self, OperationsError> {
        let value = Self {
            meta,
            name: name.into(),
            period_start,
            period_end,
            branch_scopes,
            source_refs,
            status: OperationalPlanStatus::Draft,
        };
        value.validate()?;
        Ok(value)
    }
}

impl OperationalCommitment {
    pub fn canonicalized(&self) -> Self {
        self.clone()
    }
}

impl ExceptionCase {
    pub fn canonicalized(&self) -> Self {
        self.clone()
    }
}

impl OperationalCommitment {
    pub fn new(
        meta: AggregateMetadata,
        plan_id: Uuid,
        subject: impl Into<String>,
        quantity: i64,
        unit: impl Into<String>,
        due_at: DateTime<Utc>,
        source: SourceTransactionRef,
    ) -> Result<Self, OperationsError> {
        let value = Self {
            meta,
            plan_id,
            subject: subject.into(),
            quantity,
            unit: unit.into(),
            due_at,
            source,
            status: CommitmentStatus::Proposed,
        };
        value.validate()?;
        Ok(value)
    }
}

impl FulfillmentPlan {
    pub fn new(
        meta: AggregateMetadata,
        operational_plan_id: Uuid,
        commitment_ids: Vec<Uuid>,
        source_refs: Vec<SourceTransactionRef>,
    ) -> Result<Self, OperationsError> {
        let value = Self {
            meta,
            operational_plan_id,
            commitment_ids,
            source_refs,
            status: FulfillmentPlanStatus::Draft,
        };
        value.validate()?;
        Ok(value)
    }
}

impl CapacityPlan {
    pub fn new(
        meta: AggregateMetadata,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
        capacities: BTreeMap<String, i64>,
        source_refs: Vec<SourceTransactionRef>,
    ) -> Result<Self, OperationsError> {
        let value = Self {
            meta,
            period_start,
            period_end,
            capacities,
            source_refs,
            status: CapacityPlanStatus::Draft,
        };
        value.validate()?;
        Ok(value)
    }
}

impl WorkQueuePolicy {
    pub fn new(
        meta: AggregateMetadata,
        name: impl Into<String>,
        allowed_domains: Vec<String>,
        escalation_after_minutes: u32,
        source_refs: Vec<SourceTransactionRef>,
    ) -> Result<Self, OperationsError> {
        let value = Self {
            meta,
            name: name.into(),
            allowed_domains,
            escalation_after_minutes,
            source_refs,
            status: WorkQueuePolicyStatus::Draft,
        };
        value.validate()?;
        Ok(value)
    }
}

impl OperatingCalendar {
    pub fn new(
        meta: AggregateMetadata,
        name: impl Into<String>,
        timezone: impl Into<String>,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
        source_refs: Vec<SourceTransactionRef>,
    ) -> Result<Self, OperationsError> {
        let value = Self {
            meta,
            name: name.into(),
            timezone: timezone.into(),
            period_start,
            period_end,
            source_refs,
            status: OperatingCalendarStatus::Draft,
        };
        value.validate()?;
        Ok(value)
    }
}

impl Shift {
    pub fn new(
        meta: AggregateMetadata,
        schedule: ShiftScheduleInput,
        source_refs: Vec<SourceTransactionRef>,
    ) -> Result<Self, OperationsError> {
        let value = Self {
            meta,
            calendar_id: schedule.calendar_id,
            company_id: schedule.company_id,
            branch_id: schedule.branch_id,
            starts_at: schedule.starts_at,
            ends_at: schedule.ends_at,
            assignee_refs: schedule.assignee_refs,
            source_refs,
            status: ShiftStatus::Planned,
        };
        value.validate()?;
        Ok(value)
    }
}

impl ShiftHandover {
    pub fn new(
        meta: AggregateMetadata,
        shift_id: Uuid,
        outgoing_ref: impl Into<String>,
        incoming_ref: impl Into<String>,
        summary: impl Into<String>,
        source_refs: Vec<SourceTransactionRef>,
    ) -> Result<Self, OperationsError> {
        let value = Self {
            meta,
            shift_id,
            outgoing_ref: outgoing_ref.into(),
            incoming_ref: incoming_ref.into(),
            summary: summary.into(),
            source_refs,
            status: HandoverStatus::Draft,
        };
        value.validate()?;
        Ok(value)
    }
}

impl StandardOperatingProcedure {
    pub fn new(
        meta: AggregateMetadata,
        name: impl Into<String>,
        revision: u32,
        steps: Vec<String>,
        source_refs: Vec<SourceTransactionRef>,
    ) -> Result<Self, OperationsError> {
        let value = Self {
            meta,
            name: name.into(),
            revision,
            steps,
            source_refs,
            status: StandardOperatingProcedureStatus::Draft,
        };
        value.validate()?;
        Ok(value)
    }
}

impl DispatchBatch {
    pub fn new(
        meta: AggregateMetadata,
        fulfillment_plan_id: Uuid,
        item_ids: Vec<Uuid>,
        source_refs: Vec<SourceTransactionRef>,
    ) -> Result<Self, OperationsError> {
        let value = Self {
            meta,
            fulfillment_plan_id,
            item_ids,
            source_refs,
            status: DispatchBatchStatus::Draft,
        };
        value.validate()?;
        Ok(value)
    }
}

impl ExceptionCase {
    /// Construct a detected orchestration exception from a source fact. The
    /// aggregate starts at version one because this helper is intended for the
    /// atomic source-event application boundary, not an unpersisted draft.
    pub fn from_source_event(
        source_event: &OperationsSourceEvent,
        exception_id: Uuid,
        category: impl Into<String>,
        severity: impl Into<String>,
    ) -> Result<Self, OperationsError> {
        source_event.validate()?;
        if exception_id.is_nil() {
            return Err(OperationsError::InvalidExceptionCommandId);
        }
        let source = source_event.source_reference()?;
        let exception = Self {
            meta: AggregateMetadata {
                id: exception_id,
                holding_ref: source_event.holding_ref.clone(),
                scope: source_event.scope.clone(),
                version: 1,
                created_at: source_event.occurred_at,
                updated_at: source_event.occurred_at,
            },
            category: category.into(),
            severity: severity.into(),
            source,
            assigned_to: None,
            due_at: None,
            manager_ref: None,
            sla: None,
            status: ExceptionStatus::Detected,
        };
        exception.validate()?;
        Ok(exception)
    }

    /// Reconciliation is source-event driven but remains an Operations
    /// lifecycle mutation. The source identity must match the exception's
    /// immutable provenance and its scope must contain the exception scope.
    pub fn validate_source_event_link(
        &self,
        source_event: &OperationsSourceEvent,
    ) -> Result<(), OperationsError> {
        source_event.validate_for_aggregate_scope(&self.meta.scope)?;
        if self.source != source_event.source_reference()? {
            return Err(OperationsError::InvalidSourceReference);
        }
        Ok(())
    }
}

impl ExceptionLifecycleCommand {
    /// Bind a lifecycle reconciliation command to a source event. The source
    /// event supplies idempotency, causation, correlation, and event time;
    /// actor and expected aggregate version remain caller-provided authority
    /// proofs and are never inferred from a source payload.
    pub fn bound_to_source_event(
        &self,
        source_event: &OperationsSourceEvent,
    ) -> Result<Self, OperationsError> {
        source_event.validate()?;
        let mut command = self.clone();
        command.at = source_event.occurred_at;
        command.idempotency_key = source_event.idempotency_key()?;
        command.correlation_id = source_event.correlation_id;
        command.causation_id = Some(source_event.source_event_id);
        command.validate()?;
        Ok(command)
    }
}

impl ExceptionCase {
    /// Apply one lifecycle command after the caller has obtained authoritative
    /// active-window proofs for every referenced principal. Reassignment is
    /// deliberately a versioned mutation without a status change so an
    /// expired assignee can never remain the implicit owner of a live case.
    pub fn apply_lifecycle_command(
        &mut self,
        command: &ExceptionLifecycleCommand,
    ) -> Result<ExceptionLifecycleTransition, OperationsError> {
        command.validate()?;
        if self.meta.id != command.exception_id {
            return Err(OperationsError::InvalidExceptionCommandId);
        }
        if self.meta.version != command.expected_version {
            return Err(OperationsError::ExceptionVersionMismatch {
                expected: command.expected_version,
                actual: self.meta.version,
            });
        }
        if command.at < self.meta.updated_at {
            return Err(OperationsError::InvalidWindow {
                field: "exception command time",
            });
        }

        let from = self.status;
        let mut candidate = self.clone();
        match &command.action {
            ExceptionLifecycleAction::Assign {
                assignee,
                manager,
                sla,
            } => {
                ensure_exception_action(candidate.status, ExceptionStatus::Detected, "assign")?;
                candidate.assigned_to = Some(assignee.principal_ref.clone());
                candidate.manager_ref = Some(manager.principal_ref.clone());
                candidate.due_at = Some(sla.due_at);
                candidate.sla = Some(sla.clone());
                candidate.transition_to(ExceptionStatus::Assigned, command.at)?;
            }
            ExceptionLifecycleAction::Reassign { assignee, manager } => {
                if !matches!(
                    candidate.status,
                    ExceptionStatus::Assigned
                        | ExceptionStatus::Acknowledged
                        | ExceptionStatus::Escalated
                ) {
                    return Err(OperationsError::InvalidExceptionAction {
                        action: "reassign",
                        status: candidate.status.as_str().into(),
                    });
                }
                candidate.assigned_to = Some(assignee.principal_ref.clone());
                candidate.manager_ref = Some(manager.principal_ref.clone());
                candidate.meta.updated_at = command.at;
            }
            ExceptionLifecycleAction::Acknowledge => {
                ensure_exception_assignment(&candidate)?;
                candidate.transition_to(ExceptionStatus::Acknowledged, command.at)?;
            }
            ExceptionLifecycleAction::Escalate { manager } => {
                ensure_exception_assignment(&candidate)?;
                candidate.manager_ref = Some(manager.principal_ref.clone());
                candidate.transition_to(ExceptionStatus::Escalated, command.at)?;
            }
            ExceptionLifecycleAction::Resolve => {
                ensure_exception_assignment(&candidate)?;
                candidate.transition_to(ExceptionStatus::Resolved, command.at)?;
            }
            ExceptionLifecycleAction::Close => {
                ensure_exception_assignment(&candidate)?;
                candidate.transition_to(ExceptionStatus::Closed, command.at)?;
            }
        }

        let aggregate_version = candidate.meta.next_version()?;
        candidate.meta.version = aggregate_version;
        candidate.validate()?;
        *self = candidate;
        Ok(ExceptionLifecycleTransition {
            from,
            to: self.status,
            aggregate_version,
        })
    }
}

fn ensure_exception_action(
    status: ExceptionStatus,
    expected: ExceptionStatus,
    action: &'static str,
) -> Result<(), OperationsError> {
    if status == expected {
        Ok(())
    } else {
        Err(OperationsError::InvalidExceptionAction {
            action,
            status: status.as_str().into(),
        })
    }
}

fn ensure_exception_assignment(exception: &ExceptionCase) -> Result<(), OperationsError> {
    if exception
        .assigned_to
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        return Err(OperationsError::MissingExceptionField {
            field: "assigned_to",
        });
    }
    if exception
        .manager_ref
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        return Err(OperationsError::MissingExceptionField {
            field: "manager_ref",
        });
    }
    Ok(())
}

fn canonicalize_source_refs(source_refs: &mut [SourceTransactionRef]) {
    source_refs.sort_by_key(|source| {
        (
            source.source_domain.clone(),
            source.source_type.clone(),
            source.source_id,
            source.source_event_id,
        )
    });
}

impl OperationalPlan {
    pub fn canonicalized(&self) -> Self {
        let mut value = self.clone();
        value
            .branch_scopes
            .sort_by_key(|scope| serde_json::to_string(scope).unwrap_or_default());
        canonicalize_source_refs(&mut value.source_refs);
        value
    }
}

impl FulfillmentPlan {
    pub fn canonicalized(&self) -> Self {
        let mut value = self.clone();
        value.commitment_ids.sort_unstable();
        canonicalize_source_refs(&mut value.source_refs);
        value
    }
}

impl CapacityPlan {
    pub fn canonicalized(&self) -> Self {
        let mut value = self.clone();
        canonicalize_source_refs(&mut value.source_refs);
        value
    }
}

impl WorkQueuePolicy {
    pub fn canonicalized(&self) -> Self {
        let mut value = self.clone();
        value.allowed_domains.sort_unstable();
        canonicalize_source_refs(&mut value.source_refs);
        value
    }
}

impl OperatingCalendar {
    pub fn canonicalized(&self) -> Self {
        let mut value = self.clone();
        canonicalize_source_refs(&mut value.source_refs);
        value
    }
}

impl Shift {
    pub fn canonicalized(&self) -> Self {
        let mut value = self.clone();
        value.assignee_refs.sort_unstable();
        canonicalize_source_refs(&mut value.source_refs);
        value
    }
}

impl ShiftHandover {
    pub fn canonicalized(&self) -> Self {
        let mut value = self.clone();
        canonicalize_source_refs(&mut value.source_refs);
        value
    }
}

impl StandardOperatingProcedure {
    pub fn canonicalized(&self) -> Self {
        let mut value = self.clone();
        canonicalize_source_refs(&mut value.source_refs);
        value
    }
}

impl DispatchBatch {
    pub fn canonicalized(&self) -> Self {
        let mut value = self.clone();
        value.item_ids.sort_unstable();
        canonicalize_source_refs(&mut value.source_refs);
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> Scope {
        Scope::Holding {
            holding_id: Uuid::from_u128(1),
        }
    }

    fn metadata() -> AggregateMetadata {
        AggregateMetadata::new(Uuid::from_u128(2), "holding-ref", scope(), Utc::now()).unwrap()
    }

    fn source() -> SourceTransactionRef {
        SourceTransactionRef::new(
            "sales",
            "sales_order",
            Uuid::from_u128(3),
            Uuid::from_u128(4),
        )
        .unwrap()
    }

    #[test]
    fn lifecycle_status_keys_match_operations_read_model_contract() {
        assert_eq!(ExceptionStatus::Detected.as_str(), "detected");
        assert_eq!(ExceptionStatus::Assigned.as_str(), "assigned");
        assert_eq!(ExceptionStatus::Acknowledged.as_str(), "acknowledged");
        assert_eq!(ExceptionStatus::Escalated.as_str(), "escalated");
        assert_eq!(ExceptionStatus::Resolved.as_str(), "resolved");
        assert_eq!(ExceptionStatus::Closed.as_str(), "closed");
        assert_eq!(ShiftStatus::HandoverPending.as_str(), "handover_pending");
    }

    #[test]
    fn dependency_ready_operations_aggregates_are_validated_and_provenance_bound() {
        let at = Utc::now();
        let source_refs = vec![source()];
        let branch = Scope::Branch {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(5),
            branch_id: Uuid::from_u128(6),
        };
        let plan = OperationalPlan::new(
            metadata(),
            "weekly plan",
            at,
            at + chrono::Duration::hours(8),
            vec![branch.clone()],
            source_refs.clone(),
        )
        .unwrap();
        assert_eq!(plan.source_references(), source_refs.as_slice());

        let commitment = OperationalCommitment::new(
            metadata(),
            plan.meta.id,
            "deliver",
            2,
            "units",
            at + chrono::Duration::hours(2),
            source(),
        )
        .unwrap();
        assert_eq!(commitment.source_references().len(), 1);

        let fulfillment = FulfillmentPlan::new(
            metadata(),
            plan.meta.id,
            vec![commitment.meta.id],
            source_refs.clone(),
        )
        .unwrap();
        let mut capacities = BTreeMap::new();
        capacities.insert("dock".into(), 4);
        let capacity = CapacityPlan::new(
            metadata(),
            at,
            at + chrono::Duration::hours(8),
            capacities,
            source_refs.clone(),
        )
        .unwrap();
        let policy = WorkQueuePolicy::new(
            metadata(),
            "queue",
            vec!["sales".into(), "procurement".into()],
            60,
            source_refs.clone(),
        )
        .unwrap();
        let calendar = OperatingCalendar::new(
            metadata(),
            "business hours",
            "Asia/Bangkok",
            at,
            at + chrono::Duration::hours(12),
            source_refs.clone(),
        )
        .unwrap();
        let shift = Shift::new(
            metadata(),
            ShiftScheduleInput {
                calendar_id: calendar.meta.id,
                company_id: Uuid::from_u128(5),
                branch_id: Uuid::from_u128(6),
                starts_at: at,
                ends_at: at + chrono::Duration::hours(8),
                assignee_refs: vec!["operator".into()],
            },
            source_refs.clone(),
        )
        .unwrap();
        let handover = ShiftHandover::new(
            metadata(),
            shift.meta.id,
            "outgoing",
            "incoming",
            "all clear",
            source_refs.clone(),
        )
        .unwrap();
        let procedure = StandardOperatingProcedure::new(
            metadata(),
            "dispatch checklist",
            1,
            vec!["scan".into(), "seal".into()],
            source_refs.clone(),
        )
        .unwrap();
        let batch = DispatchBatch::new(
            metadata(),
            fulfillment.meta.id,
            vec![Uuid::from_u128(90)],
            source_refs.clone(),
        )
        .unwrap();

        assert!(fulfillment.validate().is_ok());
        assert!(capacity.validate().is_ok());
        assert!(policy.validate().is_ok());
        assert!(calendar.validate().is_ok());
        assert!(shift.validate().is_ok());
        assert!(handover.validate().is_ok());
        assert!(procedure.validate().is_ok());
        assert!(batch.validate().is_ok());
        assert!(
            OperationalPlan::new(
                metadata(),
                "leaking plan",
                at,
                at + chrono::Duration::hours(1),
                vec![Scope::Branch {
                    holding_id: Uuid::from_u128(999),
                    company_id: Uuid::from_u128(5),
                    branch_id: Uuid::from_u128(6),
                }],
                source_refs,
            )
            .is_err()
        );
    }

    #[test]
    fn work_queue_policies_accept_only_unique_closed_source_domains() {
        let valid = WorkQueuePolicy::new(
            metadata(),
            "queue",
            vec!["sales".into(), "procurement".into()],
            60,
            Vec::new(),
        )
        .expect("closed source domains are valid");
        assert!(valid.validate().is_ok());

        assert!(matches!(
            WorkQueuePolicy::new(
                metadata(),
                "unknown source",
                vec!["unknown".into()],
                60,
                Vec::new(),
            ),
            Err(OperationsError::UnsupportedSourceDomain { .. })
        ));
        assert!(matches!(
            WorkQueuePolicy::new(
                metadata(),
                "duplicate source",
                vec!["sales".into(), "sales".into()],
                60,
                Vec::new(),
            ),
            Err(OperationsError::DuplicateSourceDomain { .. })
        ));
        assert!(matches!(
            WorkQueuePolicy::new(metadata(), "empty source", Vec::new(), 60, Vec::new(),),
            Err(OperationsError::InvalidWorkQueuePolicyDomains)
        ));

        let too_many = (0..=MAX_OPERATIONS_POLICY_DOMAINS)
            .map(|index| format!("domain-{index}"))
            .collect();
        assert!(matches!(
            WorkQueuePolicy::new(metadata(), "too many sources", too_many, 60, Vec::new()),
            Err(OperationsError::InvalidWorkQueuePolicyDomains)
        ));
    }

    #[test]
    fn source_reference_rejection_and_shift_company_scope_are_fail_closed() {
        let at = Utc::now();
        let invalid_source = SourceTransactionRef {
            source_domain: "operations".into(),
            source_type: "internal_plan".into(),
            source_id: Uuid::from_u128(30),
            source_event_id: Uuid::from_u128(31),
        };
        assert!(
            OperationalPlan::new(
                metadata(),
                "invalid provenance",
                at,
                at + chrono::Duration::hours(1),
                Vec::new(),
                vec![invalid_source],
            )
            .is_err()
        );

        let company_scope_meta = AggregateMetadata {
            scope: Scope::Company {
                holding_id: Uuid::from_u128(1),
                company_id: Uuid::from_u128(5),
            },
            ..metadata()
        };
        assert!(
            Shift::new(
                company_scope_meta,
                ShiftScheduleInput {
                    calendar_id: Uuid::from_u128(40),
                    company_id: Uuid::from_u128(6),
                    branch_id: Uuid::from_u128(7),
                    starts_at: at,
                    ends_at: at + chrono::Duration::hours(1),
                    assignee_refs: vec!["operator".into()],
                },
                vec![source()],
            )
            .is_err()
        );
    }

    #[test]
    fn organization_and_operations_scopes_are_contained() {
        let holding = scope();
        let branch = Scope::Branch {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(5),
            branch_id: Uuid::from_u128(6),
        };
        assert!(ensure_scope_containment(&holding, &branch).is_ok());
        assert!(ensure_scope_containment(&branch, &holding).is_err());
    }

    #[test]
    fn organization_aggregates_require_their_exact_persisted_scope() {
        let at = Utc::now();
        let mut company = Company {
            meta: AggregateMetadata::new(
                Uuid::from_u128(201),
                "holding-ref",
                Scope::Holding {
                    holding_id: Uuid::from_u128(1),
                },
                at,
            )
            .unwrap(),
            holding_id: Uuid::from_u128(1),
            registration_number: "REG-201".into(),
            legal_name: "Exact Scope Co".into(),
            status: OrganizationStatus::Active,
        };
        assert!(matches!(
            company.validate(),
            Err(OperationsError::ScopeOutsideParent { .. })
        ));

        company.meta.scope = Scope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(201),
        };
        assert!(company.validate().is_ok());
        let before = company.clone();
        assert_eq!(
            company.transition_to(
                OrganizationStatus::Suspended,
                at - chrono::Duration::seconds(1),
            ),
            Err(OperationsError::InvalidWindow {
                field: "organization transition time"
            })
        );
        assert_eq!(company, before);
    }

    #[test]
    fn lifecycle_transitions_reject_skips_and_terminal_reuse() {
        let mut plan = OperationalPlan {
            meta: metadata(),
            name: "plan".into(),
            period_start: Utc::now(),
            period_end: Utc::now() + chrono::Duration::hours(1),
            branch_scopes: vec![],
            source_refs: vec![source()],
            status: OperationalPlanStatus::Draft,
        };
        let at = plan.meta.updated_at + chrono::Duration::seconds(1);
        assert!(
            plan.transition_to(OperationalPlanStatus::Active, at)
                .is_err()
        );
        plan.transition_to(OperationalPlanStatus::Approved, at)
            .unwrap();
        plan.transition_to(OperationalPlanStatus::Active, at)
            .unwrap();
        plan.transition_to(OperationalPlanStatus::Completed, at)
            .unwrap();
        assert!(
            plan.transition_to(OperationalPlanStatus::Active, at)
                .is_err()
        );
    }

    #[test]
    fn canonical_serialization_is_independent_of_unordered_references() {
        let left = FulfillmentPlan {
            meta: metadata(),
            operational_plan_id: Uuid::from_u128(7),
            commitment_ids: vec![Uuid::from_u128(9), Uuid::from_u128(8)],
            source_refs: vec![source()],
            status: FulfillmentPlanStatus::Draft,
        };
        let mut right = left.clone();
        right.commitment_ids.reverse();
        assert_eq!(
            left.canonical_bytes().unwrap(),
            right.canonical_bytes().unwrap()
        );
        let at = left.meta.updated_at + chrono::Duration::seconds(1);
        let mut transitioned = left;
        transitioned
            .transition_to(FulfillmentPlanStatus::Planned, at)
            .unwrap();
    }

    #[test]
    fn event_and_projection_metadata_are_idempotency_ready() {
        let at = Utc::now();
        let aggregate = ExceptionCase {
            meta: metadata(),
            category: "late".into(),
            severity: "high".into(),
            source: source(),
            assigned_to: None,
            due_at: None,
            manager_ref: None,
            sla: None,
            status: ExceptionStatus::Detected,
        };
        let event = event_for(
            &aggregate,
            "operations.exception.detected",
            1,
            ActorAttribution {
                principal_ref: "worker".into(),
                kind: crate::contracts::ActorKind::SystemWorker,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            Some("operations-command-1".into()),
            Uuid::from_u128(10),
            None,
            at,
        )
        .unwrap();
        assert_eq!(event.envelope.aggregate_type, "operations.exception_case");
        assert_eq!(event.projection.event_id, event.envelope.event_id);
    }

    fn source_event(domain: OperationsSourceDomain) -> OperationsSourceEvent {
        OperationsSourceEvent {
            source_domain: domain,
            source_type: "source_order".into(),
            source_id: Uuid::from_u128(30),
            source_event_id: Uuid::from_u128(31),
            source_version: 4,
            event_type: "source.updated".into(),
            event_version: 1,
            holding_ref: "holding-ref".into(),
            scope: scope(),
            actor: ActorAttribution {
                principal_ref: "source-worker".into(),
                kind: crate::contracts::ActorKind::SystemWorker,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            correlation_id: Uuid::from_u128(32),
            causation_id: None,
            occurred_at: Utc::now(),
            payload: serde_json::json!({"state": "updated"}),
        }
    }

    #[test]
    fn source_event_boundary_is_closed_scoped_and_idempotency_stable() {
        for domain in OperationsSourceDomain::ALL {
            let event = source_event(domain);
            event.validate().unwrap();
            assert_eq!(
                OperationsSourceDomain::parse(domain.as_str()).unwrap(),
                domain
            );
            assert_eq!(
                event.source_reference().unwrap().source_domain,
                domain.as_str()
            );
            assert_eq!(
                event.idempotency_key().unwrap(),
                "operations-source-event:00000000-0000-0000-0000-00000000001f"
            );
        }
        assert!(matches!(
            OperationsSourceDomain::parse("operations"),
            Err(OperationsError::UnsupportedSourceDomain { .. })
        ));
        let mut event = source_event(OperationsSourceDomain::Sales);
        event.scope = Scope::Branch {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(5),
            branch_id: Uuid::from_u128(6),
        };
        assert!(matches!(
            event.validate_for_aggregate_scope(&scope()),
            Err(OperationsError::ScopeOutsideParent { .. })
        ));
    }

    #[test]
    fn source_events_and_aggregates_require_concrete_tenant_scopes() {
        let mut event = source_event(OperationsSourceDomain::Sales);
        event.scope = Scope::Holding {
            holding_id: Uuid::nil(),
        };
        assert_eq!(event.validate(), Err(OperationsError::InvalidSourceEvent));

        let mut event = source_event(OperationsSourceDomain::Sales);
        event.scope = Scope::Subtree {
            holding_id: Uuid::from_u128(1),
            company_ids: Vec::new(),
            branch_ids: Vec::new(),
        };
        assert_eq!(event.validate(), Err(OperationsError::InvalidSourceEvent));

        let mut metadata = metadata();
        metadata.scope = Scope::Company {
            holding_id: metadata.scope.holding_id(),
            company_id: Uuid::nil(),
        };
        assert_eq!(
            metadata.validate(),
            Err(OperationsError::InvalidAggregateScope)
        );
    }

    #[test]
    fn source_event_exception_creation_and_reconciliation_are_explicit() {
        let event = source_event(OperationsSourceDomain::Support);
        let exception = ExceptionCase::from_source_event(
            &event,
            Uuid::from_u128(33),
            "support_failure",
            "high",
        )
        .unwrap();
        assert_eq!(exception.meta.version, 1);
        assert_eq!(exception.status, ExceptionStatus::Detected);
        exception.validate_source_event_link(&event).unwrap();

        let command = exception_command(
            exception.meta.id,
            1,
            event.occurred_at,
            ExceptionLifecycleAction::Resolve,
        );
        let bound = command.bound_to_source_event(&event).unwrap();
        assert_eq!(bound.idempotency_key, event.idempotency_key().unwrap());
        assert_eq!(bound.causation_id, Some(event.source_event_id));
        assert_eq!(bound.correlation_id, event.correlation_id);
    }

    fn exception_principal(
        name: &str,
        from: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> ExceptionPrincipalReference {
        ExceptionPrincipalReference {
            principal_ref: name.into(),
            valid_from: from,
            valid_until: until,
        }
    }

    fn exception_attribution(name: &str) -> ActorAttribution {
        ActorAttribution {
            principal_ref: name.into(),
            kind: crate::contracts::ActorKind::TenantUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        }
    }

    fn exception_command(
        exception_id: Uuid,
        expected_version: u64,
        at: DateTime<Utc>,
        action: ExceptionLifecycleAction,
    ) -> ExceptionLifecycleCommand {
        ExceptionLifecycleCommand {
            exception_id,
            expected_version,
            at,
            actor: exception_principal("operator", at, at + chrono::Duration::hours(1)),
            actor_attribution: exception_attribution("operator"),
            idempotency_key: format!("exception-command-{expected_version}"),
            correlation_id: Uuid::from_u128(100 + expected_version as u128),
            causation_id: None,
            action,
        }
    }

    #[test]
    fn exception_lifecycle_commands_follow_assignment_and_terminal_rules() {
        let created_at = Utc::now();
        let exception_id = Uuid::from_u128(20);
        let mut exception = ExceptionCase {
            meta: AggregateMetadata {
                id: exception_id,
                holding_ref: "holding-ref".into(),
                scope: scope(),
                version: 1,
                created_at,
                updated_at: created_at,
            },
            category: "late_delivery".into(),
            severity: "high".into(),
            source: source(),
            assigned_to: None,
            due_at: None,
            manager_ref: None,
            sla: None,
            status: ExceptionStatus::Detected,
        };
        let assignee = exception_principal(
            "assignee",
            created_at,
            created_at + chrono::Duration::hours(2),
        );
        let manager = exception_principal(
            "manager",
            created_at,
            created_at + chrono::Duration::hours(2),
        );
        let sla = ExceptionSlaMetadata {
            policy_ref: "standard-high".into(),
            due_at: created_at + chrono::Duration::hours(1),
            escalation_after_minutes: 60,
        };
        let mut command = exception_command(
            exception_id,
            1,
            created_at,
            ExceptionLifecycleAction::Assign {
                assignee: assignee.clone(),
                manager: manager.clone(),
                sla: sla.clone(),
            },
        );
        let assigned = exception
            .apply_lifecycle_command(&command)
            .expect("detected exception can be assigned");
        assert_eq!(assigned.from, ExceptionStatus::Detected);
        assert_eq!(assigned.to, ExceptionStatus::Assigned);
        assert_eq!(assigned.aggregate_version, 2);
        assert_eq!(exception.assigned_to.as_deref(), Some("assignee"));
        assert_eq!(exception.manager_ref.as_deref(), Some("manager"));
        assert_eq!(exception.due_at, Some(sla.due_at));

        command = exception_command(
            exception_id,
            2,
            created_at + chrono::Duration::minutes(1),
            ExceptionLifecycleAction::Acknowledge,
        );
        exception
            .apply_lifecycle_command(&command)
            .expect("assigned exception can be acknowledged");
        command = exception_command(
            exception_id,
            3,
            created_at + chrono::Duration::minutes(2),
            ExceptionLifecycleAction::Escalate {
                manager: manager.clone(),
            },
        );
        exception
            .apply_lifecycle_command(&command)
            .expect("acknowledged exception can be escalated");
        command = exception_command(
            exception_id,
            4,
            created_at + chrono::Duration::minutes(3),
            ExceptionLifecycleAction::Resolve,
        );
        exception
            .apply_lifecycle_command(&command)
            .expect("escalated exception can be resolved");
        assert_eq!(exception.status, ExceptionStatus::Resolved);

        command = exception_command(
            exception_id,
            5,
            created_at + chrono::Duration::minutes(4),
            ExceptionLifecycleAction::Close,
        );
        let closed = exception
            .apply_lifecycle_command(&command)
            .expect("resolved exception can be closed");
        assert_eq!(closed.from, ExceptionStatus::Resolved);
        assert_eq!(closed.to, ExceptionStatus::Closed);
        assert_eq!(closed.aggregate_version, 6);
        command = exception_command(
            exception_id,
            6,
            created_at + chrono::Duration::minutes(5),
            ExceptionLifecycleAction::Resolve,
        );
        assert!(matches!(
            exception.apply_lifecycle_command(&command),
            Err(OperationsError::InvalidExceptionAction { .. })
                | Err(OperationsError::InvalidTransition { .. })
        ));
    }

    #[test]
    fn exception_commands_fail_closed_at_principal_expiry_and_sla_boundaries() {
        let at = Utc::now();
        let expired = exception_principal("expired", at - chrono::Duration::hours(1), at);
        assert!(matches!(
            expired.validate(at, "assignee"),
            Err(OperationsError::InactiveExceptionPrincipal { .. })
        ));
        let future = exception_principal(
            "future",
            at + chrono::Duration::seconds(1),
            at + chrono::Duration::hours(1),
        );
        assert!(matches!(
            future.validate(at, "manager"),
            Err(OperationsError::InactiveExceptionPrincipal { .. })
        ));
        let sla = ExceptionSlaMetadata {
            policy_ref: "bounded".into(),
            due_at: at + chrono::Duration::days(8),
            escalation_after_minutes: MAX_EXCEPTION_SLA_MINUTES + 1,
        };
        assert!(matches!(
            sla.validate_for_command(at),
            Err(OperationsError::InvalidExceptionSla)
        ));
        let command = ExceptionLifecycleCommand {
            exception_id: Uuid::from_u128(21),
            expected_version: 1,
            at,
            actor: exception_principal("operator", at, at + chrono::Duration::hours(1)),
            actor_attribution: exception_attribution("operator"),
            idempotency_key: "expired-reassignment".into(),
            correlation_id: Uuid::from_u128(101),
            causation_id: None,
            action: ExceptionLifecycleAction::Reassign {
                assignee: expired.clone(),
                manager: exception_principal("manager", at, at + chrono::Duration::hours(1)),
            },
        };
        assert!(matches!(
            command.validate(),
            Err(OperationsError::InactiveExceptionPrincipal {
                field: "assignee",
                ..
            })
        ));
    }

    #[test]
    fn exception_state_cannot_claim_assignment_or_resolution_without_authority() {
        // Keep the transition timestamp after the independently-created test
        // metadata so this assertion exercises the authoritative assignment
        // invariant rather than racing the aggregate timestamp guard.
        let at = Utc::now() + chrono::Duration::seconds(1);
        let mut assigned_without_authority = ExceptionCase {
            meta: metadata(),
            category: "late_delivery".into(),
            severity: "high".into(),
            source: source(),
            assigned_to: None,
            due_at: None,
            manager_ref: None,
            sla: None,
            status: ExceptionStatus::Assigned,
        };
        assert!(matches!(
            assigned_without_authority.validate(),
            Err(OperationsError::InvalidExceptionState { .. })
        ));
        assert!(matches!(
            assigned_without_authority.transition_to(ExceptionStatus::Resolved, at),
            Err(OperationsError::InvalidExceptionState { .. })
                | Err(OperationsError::InvalidTransition { .. })
        ));

        let mut detected_with_stale_authority = assigned_without_authority.clone();
        detected_with_stale_authority.status = ExceptionStatus::Detected;
        detected_with_stale_authority.assigned_to = Some("stale-assignee".into());
        assert!(matches!(
            detected_with_stale_authority.validate(),
            Err(OperationsError::InvalidExceptionState { .. })
        ));
    }

    #[test]
    fn exception_version_overflow_does_not_commit_lifecycle_changes() {
        let created_at = Utc::now();
        let exception_id = Uuid::from_u128(220);
        let mut exception = ExceptionCase {
            meta: AggregateMetadata {
                id: exception_id,
                holding_ref: "holding-ref".into(),
                scope: scope(),
                version: u64::MAX,
                created_at,
                updated_at: created_at,
            },
            category: "late_delivery".into(),
            severity: "high".into(),
            source: source(),
            assigned_to: None,
            due_at: None,
            manager_ref: None,
            sla: None,
            status: ExceptionStatus::Detected,
        };
        let command_at = created_at + chrono::Duration::minutes(1);
        let command = ExceptionLifecycleCommand {
            exception_id,
            expected_version: u64::MAX,
            at: command_at,
            actor: exception_principal(
                "operator",
                command_at,
                command_at + chrono::Duration::hours(1),
            ),
            actor_attribution: exception_attribution("operator"),
            idempotency_key: "exception-overflow".into(),
            correlation_id: Uuid::from_u128(221),
            causation_id: None,
            action: ExceptionLifecycleAction::Assign {
                assignee: exception_principal(
                    "assignee",
                    command_at,
                    command_at + chrono::Duration::hours(1),
                ),
                manager: exception_principal(
                    "manager",
                    command_at,
                    command_at + chrono::Duration::hours(1),
                ),
                sla: ExceptionSlaMetadata {
                    policy_ref: "standard-high".into(),
                    due_at: command_at + chrono::Duration::hours(1),
                    escalation_after_minutes: 60,
                },
            },
        };
        let before = exception.clone();
        assert_eq!(
            exception.apply_lifecycle_command(&command),
            Err(OperationsError::VersionOverflow)
        );
        assert_eq!(exception, before);
    }
}
