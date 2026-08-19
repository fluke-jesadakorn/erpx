use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use erpx_integration_contracts::{
    BoundedContext,
    commercial::{SubscriptionFact, SubscriptionRef},
    crm::{OpportunityFact, OpportunityRef},
    customer::{Customer360SourceFact, CustomerProfileFact, CustomerRef},
    sales::{SalesOrderFact, SalesOrderRef},
    support::{SupportCaseFact, SupportCaseRef},
};
use erpx_kernel::EventId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod owned_record;
pub use owned_record::{Customer360OwnedRecord, Customer360RecordKind, Customer360RecordStatus};

/// Customer Authority facts are sourced from control-write and therefore own
/// a checkpoint that cannot be compared with a holding-write sequence.
pub const CONTROL_CUSTOMERS_PROJECTION_NAME: &str = "customer360.control_customers";
/// Commercial, CRM, Sales, and Support facts share the holding activity stream.
pub const HOLDING_ACTIVITY_PROJECTION_NAME: &str = "customer360.holding_activity";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Customer360ProjectionStream {
    ControlCustomers,
    HoldingActivity,
}

impl Customer360ProjectionStream {
    pub const fn projection_name(self) -> &'static str {
        match self {
            Self::ControlCustomers => CONTROL_CUSTOMERS_PROJECTION_NAME,
            Self::HoldingActivity => HOLDING_ACTIVITY_PROJECTION_NAME,
        }
    }
}

/// Source position carried by the process manager. Aggregate version prevents
/// an older owner snapshot from replacing a newer one, while global sequence
/// gives each independently sourced projection stream a deterministic order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionPosition {
    pub event_id: EventId,
    pub aggregate_version: u64,
    pub global_sequence: u64,
    pub occurred_at: DateTime<Utc>,
}

/// Typed, transport-neutral input accepted by the Customer 360 reducer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Customer360ProjectionInput {
    pub stream: Customer360ProjectionStream,
    pub position: ProjectionPosition,
    pub fact: Customer360SourceFact,
}

impl Customer360ProjectionInput {
    pub fn try_new(
        stream: Customer360ProjectionStream,
        position: ProjectionPosition,
        fact: Customer360SourceFact,
    ) -> Result<Self, Customer360DomainError> {
        if position.aggregate_version == 0 {
            return Err(Customer360DomainError::ZeroAggregateVersion);
        }
        if position.global_sequence == 0 {
            return Err(Customer360DomainError::ZeroGlobalSequence);
        }
        if position.occurred_at.timestamp() < 0 {
            return Err(Customer360DomainError::InvalidTimestamp);
        }
        let value = Self {
            stream,
            position,
            fact,
        };
        value.validate_stream()?;
        Ok(value)
    }

    pub const fn customer(&self) -> CustomerRef {
        self.fact.customer()
    }

    pub const fn source_context(&self) -> BoundedContext {
        self.fact.source_context()
    }

    fn validate_stream(&self) -> Result<(), Customer360DomainError> {
        let expected = match self.fact {
            Customer360SourceFact::Customer(_) => Customer360ProjectionStream::ControlCustomers,
            Customer360SourceFact::Commercial(_)
            | Customer360SourceFact::Crm(_)
            | Customer360SourceFact::Sales(_)
            | Customer360SourceFact::Support(_) => Customer360ProjectionStream::HoldingActivity,
        };
        if self.stream == expected {
            Ok(())
        } else {
            Err(Customer360DomainError::IncorrectProjectionStream)
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionPositions {
    pub control_customers: Option<ProjectionPosition>,
    pub holding_activity: Option<ProjectionPosition>,
}

impl ProjectionPositions {
    pub const fn get(self, stream: Customer360ProjectionStream) -> Option<ProjectionPosition> {
        match stream {
            Customer360ProjectionStream::ControlCustomers => self.control_customers,
            Customer360ProjectionStream::HoldingActivity => self.holding_activity,
        }
    }

    fn set(&mut self, stream: Customer360ProjectionStream, position: ProjectionPosition) {
        match stream {
            Customer360ProjectionStream::ControlCustomers => {
                self.control_customers = Some(position);
            }
            Customer360ProjectionStream::HoldingActivity => {
                self.holding_activity = Some(position);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionApplyOutcome {
    Applied,
    Duplicate,
    IgnoredOutOfOrder,
    IgnoredStaleSourceVersion,
}

/// Whether a read section is backed by an observed owner fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionAvailability {
    Ready,
    AwaitingSource,
    Stale,
    Unavailable,
    Denied,
    /// No current typed integration fact can authoritatively build this section.
    NotProjected,
}

/// Domain-owned classification of an owner projection's ability to answer a
/// Customer 360 section. CQRS checkpoint states are translated into this type
/// at the read-adapter boundary so the domain remains framework-free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFreshness {
    Current,
    Stale,
    Unavailable,
}

/// Tracks source observation independently from collection emptiness. An empty
/// collection can be a valid projected business state and must not look like a
/// missing source.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceCoverage {
    pub customer_profile: bool,
    pub commercial_subscriptions: bool,
    pub crm_opportunities: bool,
    pub sales_orders: bool,
    pub support_cases: bool,
}

impl SourceCoverage {
    fn record(&mut self, context: BoundedContext) {
        match context {
            BoundedContext::Customer => self.customer_profile = true,
            BoundedContext::Commercial => self.commercial_subscriptions = true,
            BoundedContext::Crm => self.crm_opportunities = true,
            BoundedContext::Sales => self.sales_orders = true,
            BoundedContext::Support => self.support_cases = true,
            _ => unreachable!("Customer360SourceFact only contains five owner contexts"),
        }
    }
}

/// Availability is explicit so consumers cannot mistake missing contracts for
/// empty business state. The five ready/awaiting sections are the only current
/// owner-qualified inputs. All other legacy Customer 360 sections fail closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Customer360SectionAvailability {
    pub overview: SectionAvailability,
    pub commercial: SectionAvailability,
    pub relationship_activity: SectionAvailability,
    pub sales_activity: SectionAvailability,
    pub support: SectionAvailability,
    pub organization: SectionAvailability,
    pub contacts: SectionAvailability,
    pub entitlements: SectionAvailability,
    pub onboarding: SectionAvailability,
    pub health: SectionAvailability,
    pub access_audit: SectionAvailability,
    pub notes: SectionAvailability,
}

impl From<SourceCoverage> for Customer360SectionAvailability {
    fn from(coverage: SourceCoverage) -> Self {
        let observed = |value| {
            if value {
                SectionAvailability::Ready
            } else {
                SectionAvailability::AwaitingSource
            }
        };
        Self {
            overview: observed(coverage.customer_profile),
            commercial: observed(coverage.commercial_subscriptions),
            relationship_activity: observed(coverage.crm_opportunities),
            sales_activity: observed(coverage.sales_orders),
            support: observed(coverage.support_cases),
            organization: SectionAvailability::NotProjected,
            contacts: SectionAvailability::NotProjected,
            entitlements: SectionAvailability::NotProjected,
            onboarding: SectionAvailability::NotProjected,
            health: SectionAvailability::NotProjected,
            access_audit: SectionAvailability::NotProjected,
            notes: SectionAvailability::NotProjected,
        }
    }
}

impl Customer360SectionAvailability {
    /// Derive section state from projection authority rather than collection
    /// row count. A current, fully consumed source can authoritatively report
    /// an empty collection.
    pub fn from_source_freshness(
        coverage: SourceCoverage,
        control: SourceFreshness,
        holding: SourceFreshness,
    ) -> Self {
        let section = |observed: bool, freshness: SourceFreshness| match freshness {
            SourceFreshness::Current if observed => SectionAvailability::Ready,
            SourceFreshness::Current => SectionAvailability::AwaitingSource,
            SourceFreshness::Stale => SectionAvailability::Stale,
            SourceFreshness::Unavailable => SectionAvailability::Unavailable,
        };
        Self {
            overview: section(coverage.customer_profile, control),
            commercial: section(coverage.commercial_subscriptions, holding),
            relationship_activity: section(coverage.crm_opportunities, holding),
            sales_activity: section(coverage.sales_orders, holding),
            support: section(coverage.support_cases, holding),
            organization: SectionAvailability::NotProjected,
            contacts: SectionAvailability::NotProjected,
            entitlements: SectionAvailability::NotProjected,
            onboarding: SectionAvailability::NotProjected,
            health: SectionAvailability::NotProjected,
            access_audit: SectionAvailability::NotProjected,
            notes: SectionAvailability::NotProjected,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct VersionedFact<T> {
    position: ProjectionPosition,
    fact: T,
}

/// Disposable, rebuildable composed read model. It owns no source aggregate
/// and exposes no mutation command surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Customer360Projection {
    customer: CustomerRef,
    profile: Option<VersionedFact<CustomerProfileFact>>,
    subscriptions: BTreeMap<SubscriptionRef, VersionedFact<SubscriptionFact>>,
    opportunities: BTreeMap<OpportunityRef, VersionedFact<OpportunityFact>>,
    sales_orders: BTreeMap<SalesOrderRef, VersionedFact<SalesOrderFact>>,
    support_cases: BTreeMap<SupportCaseRef, VersionedFact<SupportCaseFact>>,
    coverage: SourceCoverage,
    positions: ProjectionPositions,
}

impl Customer360Projection {
    pub const fn new(customer: CustomerRef) -> Self {
        Self {
            customer,
            profile: None,
            subscriptions: BTreeMap::new(),
            opportunities: BTreeMap::new(),
            sales_orders: BTreeMap::new(),
            support_cases: BTreeMap::new(),
            coverage: SourceCoverage {
                customer_profile: false,
                commercial_subscriptions: false,
                crm_opportunities: false,
                sales_orders: false,
                support_cases: false,
            },
            positions: ProjectionPositions {
                control_customers: None,
                holding_activity: None,
            },
        }
    }

    pub const fn customer(&self) -> CustomerRef {
        self.customer
    }

    pub const fn coverage(&self) -> SourceCoverage {
        self.coverage
    }

    pub const fn positions(&self) -> ProjectionPositions {
        self.positions
    }

    pub const fn last_position(
        &self,
        stream: Customer360ProjectionStream,
    ) -> Option<ProjectionPosition> {
        self.positions.get(stream)
    }

    pub fn apply(
        &mut self,
        input: Customer360ProjectionInput,
    ) -> Result<ProjectionApplyOutcome, Customer360DomainError> {
        input.validate()?;
        if input.customer() != self.customer {
            return Err(Customer360DomainError::CustomerMismatch);
        }

        if let Some(last) = self.positions.get(input.stream) {
            if input.position.global_sequence < last.global_sequence {
                return Ok(ProjectionApplyOutcome::IgnoredOutOfOrder);
            }
            if input.position.global_sequence == last.global_sequence {
                return if input.position.event_id == last.event_id {
                    Ok(ProjectionApplyOutcome::Duplicate)
                } else {
                    Err(Customer360DomainError::ConflictingGlobalSequence)
                };
            }
        }

        self.coverage.record(input.source_context());
        let stream = input.stream;
        let position = input.position;
        let outcome = match input.fact {
            Customer360SourceFact::Customer(fact) => {
                replace_if_newer(&mut self.profile, position, fact)
            }
            Customer360SourceFact::Commercial(fact) => {
                let key = fact.subscription;
                replace_map_if_newer(&mut self.subscriptions, key, position, fact)
            }
            Customer360SourceFact::Crm(fact) => {
                let key = fact.opportunity;
                replace_map_if_newer(&mut self.opportunities, key, position, fact)
            }
            Customer360SourceFact::Sales(fact) => {
                let key = fact.sales_order;
                replace_map_if_newer(&mut self.sales_orders, key, position, fact)
            }
            Customer360SourceFact::Support(fact) => {
                let key = fact.support_case;
                replace_map_if_newer(&mut self.support_cases, key, position, fact)
            }
        };
        // Even a stale owner version has been deterministically consumed and
        // advances its source checkpoint. This prevents an endless retry loop.
        self.positions.set(stream, position);
        Ok(outcome)
    }

    /// Rebuilds deterministically from source facts. Inputs may be supplied in
    /// any order; global positions are compared only within their source stream.
    pub fn rebuild(
        customer: CustomerRef,
        inputs: impl IntoIterator<Item = Customer360ProjectionInput>,
    ) -> Result<Self, Customer360DomainError> {
        let mut inputs = inputs.into_iter().collect::<Vec<_>>();
        inputs.sort_by_key(|input| (input.stream, input.position.global_sequence));
        let mut projection = Self::new(customer);
        for input in inputs {
            projection.apply(input)?;
        }
        projection.validate()?;
        Ok(projection)
    }

    /// Rebuild one independently switched source generation. The returned
    /// projection contains only facts from that stream so an adapter cannot
    /// accidentally copy facts across the control/holding database boundary.
    pub fn rebuild_stream(
        customer: CustomerRef,
        stream: Customer360ProjectionStream,
        inputs: impl IntoIterator<Item = Customer360ProjectionInput>,
    ) -> Result<Self, Customer360DomainError> {
        let inputs = inputs.into_iter().collect::<Vec<_>>();
        if inputs.iter().any(|input| input.stream != stream) {
            return Err(Customer360DomainError::IncorrectProjectionStream);
        }
        Self::rebuild(customer, inputs)
    }

    pub fn view(&self) -> Customer360View {
        Customer360View {
            customer: self.customer,
            profile: self.profile.as_ref().map(|value| value.fact.clone()),
            subscriptions: self
                .subscriptions
                .values()
                .map(|value| value.fact.clone())
                .collect(),
            opportunities: self
                .opportunities
                .values()
                .map(|value| value.fact.clone())
                .collect(),
            sales_orders: self
                .sales_orders
                .values()
                .map(|value| value.fact.clone())
                .collect(),
            support_cases: self
                .support_cases
                .values()
                .map(|value| value.fact.clone())
                .collect(),
            coverage: self.coverage,
            availability: self.coverage.into(),
        }
    }

    pub fn validate(&self) -> Result<(), Customer360DomainError> {
        let all_match = self
            .profile
            .as_ref()
            .is_none_or(|value| value.fact.customer == self.customer)
            && self
                .subscriptions
                .values()
                .all(|value| value.fact.customer == self.customer)
            && self
                .opportunities
                .values()
                .all(|value| value.fact.customer == self.customer)
            && self
                .sales_orders
                .values()
                .all(|value| value.fact.customer == self.customer)
            && self
                .support_cases
                .values()
                .all(|value| value.fact.customer == self.customer);
        if !all_match {
            return Err(Customer360DomainError::CustomerMismatch);
        }
        if [
            self.positions.control_customers,
            self.positions.holding_activity,
        ]
        .into_iter()
        .flatten()
        .any(|position| position.aggregate_version == 0 || position.global_sequence == 0)
        {
            return Err(Customer360DomainError::InvalidPersistedPosition);
        }
        Ok(())
    }
}

fn replace_if_newer<T>(
    target: &mut Option<VersionedFact<T>>,
    position: ProjectionPosition,
    fact: T,
) -> ProjectionApplyOutcome {
    if target
        .as_ref()
        .is_some_and(|current| current.position.aggregate_version >= position.aggregate_version)
    {
        ProjectionApplyOutcome::IgnoredStaleSourceVersion
    } else {
        *target = Some(VersionedFact { position, fact });
        ProjectionApplyOutcome::Applied
    }
}

fn replace_map_if_newer<K: Ord, T>(
    target: &mut BTreeMap<K, VersionedFact<T>>,
    key: K,
    position: ProjectionPosition,
    fact: T,
) -> ProjectionApplyOutcome {
    if target
        .get(&key)
        .is_some_and(|current| current.position.aggregate_version >= position.aggregate_version)
    {
        ProjectionApplyOutcome::IgnoredStaleSourceVersion
    } else {
        target.insert(key, VersionedFact { position, fact });
        ProjectionApplyOutcome::Applied
    }
}

impl Customer360ProjectionInput {
    fn validate(&self) -> Result<(), Customer360DomainError> {
        if self.position.aggregate_version == 0 {
            return Err(Customer360DomainError::ZeroAggregateVersion);
        }
        if self.position.global_sequence == 0 {
            return Err(Customer360DomainError::ZeroGlobalSequence);
        }
        if self.position.occurred_at.timestamp() < 0 {
            return Err(Customer360DomainError::InvalidTimestamp);
        }
        self.validate_stream()
    }
}

/// Query DTO made only from public integration facts. It deliberately has no
/// organization, contact, health, audit, or note payload fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Customer360View {
    pub customer: CustomerRef,
    pub profile: Option<CustomerProfileFact>,
    pub subscriptions: Vec<SubscriptionFact>,
    pub opportunities: Vec<OpportunityFact>,
    pub sales_orders: Vec<SalesOrderFact>,
    pub support_cases: Vec<SupportCaseFact>,
    pub coverage: SourceCoverage,
    pub availability: Customer360SectionAvailability,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Customer360DomainError {
    #[error("Customer 360 owned record is invalid")]
    InvalidOwnedRecord,
    #[error("source aggregate version must be nonzero")]
    ZeroAggregateVersion,
    #[error("source global sequence must be nonzero")]
    ZeroGlobalSequence,
    #[error("source occurrence timestamp predates the Unix epoch")]
    InvalidTimestamp,
    #[error("source fact belongs to another customer")]
    CustomerMismatch,
    #[error("different events reuse the same global sequence")]
    ConflictingGlobalSequence,
    #[error("source fact was routed to the wrong control/holding projection stream")]
    IncorrectProjectionStream,
    #[error("persisted projection position is invalid")]
    InvalidPersistedPosition,
}

#[cfg(test)]
mod tests;
