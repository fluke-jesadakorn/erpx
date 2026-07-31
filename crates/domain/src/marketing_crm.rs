//! Pure Marketing-to-CRM handoff, opportunity, and attribution contracts.
//!
//! Marketing owns acquisition and qualification. CRM owns only an accepted
//! handoff and the resulting account/contact/opportunity references. This
//! module deliberately contains no provider, persistence, or delivery logic.
//! Constructors canonicalize attribution input before it enters a CRM
//! aggregate, so read-model rebuilds can reproduce the same allocations.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::contracts::{CONTRACT_VERSION, CommandContext, TenantScope};

pub const MARKETING_CRM_CONTRACT_VERSION: u16 = 1;
pub const MAX_OPAQUE_ID_BYTES: usize = 128;
pub const MAX_IDEMPOTENCY_KEY_BYTES: usize = 512;
pub const MAX_REFERENCE_BYTES: usize = 160;
pub const MAX_NAME_BYTES: usize = 255;
pub const MAX_ATTRIBUTION_TOUCHES: usize = 10_000;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MarketingCrmError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds {max} bytes")]
    TooLong { field: &'static str, max: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("identifier cannot be nil: {field}")]
    NilIdentifier { field: &'static str },
    #[error("CRM records require a non-Platform company or branch scope")]
    InvalidScope,
    #[error("command scope does not contain the CRM record scope")]
    CommandScopeOutside,
    #[error("{field} reference scope does not contain the CRM record scope")]
    ReferenceOutsideScope { field: &'static str },
    #[error("related references do not belong to the same holding and company")]
    CrossTenantReference,
    #[error("command context is invalid: {0}")]
    InvalidCommandContext(String),
    #[error("{aggregate_type} lifecycle transition is not allowed: {from} -> {to}")]
    InvalidTransition {
        aggregate_type: &'static str,
        from: String,
        to: String,
    },
    #[error("{field} cannot be negative")]
    NegativeAmount { field: &'static str },
    #[error("amount arithmetic overflowed")]
    AmountOverflow,
    #[error("currency must be a three-letter uppercase ISO code")]
    InvalidCurrency,
    #[error("{field} is not the deterministic value expected for this aggregate")]
    InvalidDeterministicField { field: &'static str },
    #[error("attribution contains duplicate touch {0}")]
    DuplicateTouch(String),
    #[error("attribution inputs exceed the bounded maximum")]
    TooManyTouches,
    #[error("weighted attribution requires every touch to have a weight")]
    MissingWeight,
    #[error("weighted attribution weights must total exactly 10,000 basis points")]
    InvalidWeights,
    #[error("attribution revenue cannot be negative")]
    NegativeAttributionRevenue,
    #[error("idempotency key is already bound to another reference")]
    IdempotencyConflict,
    #[error("idempotency key and reference are already registered")]
    DuplicateIdempotency,
    #[error("reference is already bound to another idempotency key")]
    ReferenceConflict,
}

/// A bounded identifier whose contents have no business meaning to this
/// bounded context. It is suitable for opaque IDs from another context or
/// provider, but is never accepted as an unvalidated arbitrary string.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct OpaqueId(String);

impl OpaqueId {
    pub fn new(value: impl Into<String>) -> Result<Self, MarketingCrmError> {
        let value = value.into();
        bounded(&value, "opaque_id", MAX_OPAQUE_ID_BYTES)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Every reference carries its tenant scope. References are intentionally
/// typed below so a CRM account cannot be passed where a Marketing source is
/// expected.
macro_rules! scoped_reference {
    ($name:ident, $field:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        pub struct $name {
            pub id: OpaqueId,
            pub scope: TenantScope,
        }

        impl $name {
            pub fn new(
                id: impl Into<String>,
                scope: TenantScope,
            ) -> Result<Self, MarketingCrmError> {
                let value = Self {
                    id: OpaqueId::new(id)?,
                    scope,
                };
                value.validate()?;
                Ok(value)
            }

            pub fn validate(&self) -> Result<(), MarketingCrmError> {
                validate_non_platform_scope(&self.scope)?;
                bounded(self.id.as_str(), $field, MAX_OPAQUE_ID_BYTES)
            }

            pub fn id(&self) -> &OpaqueId {
                &self.id
            }

            pub fn scope(&self) -> &TenantScope {
                &self.scope
            }
        }
    };
}

scoped_reference!(MarketingLeadReference, "marketing_lead_id");
scoped_reference!(MarketingAcquisitionReference, "acquisition_id");
scoped_reference!(MarketingQualificationReference, "qualification_id");
scoped_reference!(AccountReference, "account_id");
scoped_reference!(ContactReference, "contact_id");
scoped_reference!(AcceptedLeadReference, "accepted_lead_id");
scoped_reference!(OpportunityReference, "opportunity_id");

/// The source-side ownership boundary. Marketing creates and qualifies this
/// information; CRM can only accept its immutable references through a
/// handoff and cannot mutate acquisition or qualification state here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingQualifiedSource {
    pub lead: MarketingLeadReference,
    pub acquisition: MarketingAcquisitionReference,
    pub qualification: MarketingQualificationReference,
}

impl MarketingQualifiedSource {
    pub fn validate_for_scope(&self, record_scope: &TenantScope) -> Result<(), MarketingCrmError> {
        self.lead.validate()?;
        self.acquisition.validate()?;
        self.qualification.validate()?;
        validate_reference_scope(&self.lead.scope, record_scope, "marketing_lead")?;
        validate_reference_scope(&self.acquisition.scope, record_scope, "acquisition")?;
        validate_reference_scope(&self.qualification.scope, record_scope, "qualification")?;
        ensure_same_tenant(&[
            &self.lead.scope,
            &self.acquisition.scope,
            &self.qualification.scope,
        ])?;
        Ok(())
    }
}

/// An immutable acceptance boundary between Marketing and CRM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptedLeadHandoff {
    pub id: Uuid,
    pub reference: String,
    pub idempotency_key: String,
    pub command_id: Uuid,
    pub scope: TenantScope,
    pub source: MarketingQualifiedSource,
    pub account: AccountReference,
    pub contact: ContactReference,
    pub accepted_at: DateTime<Utc>,
}

impl AcceptedLeadHandoff {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        scope: TenantScope,
        source: MarketingQualifiedSource,
        account: AccountReference,
        contact: ContactReference,
        accepted_at: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<Self, MarketingCrmError> {
        let value = Self {
            id,
            reference: deterministic_reference(CrmAggregateKind::AcceptedLead, id),
            idempotency_key: deterministic_idempotency_key(
                CrmAggregateKind::AcceptedLead,
                context,
            )?,
            command_id: context.command_id,
            scope,
            source,
            account,
            contact,
            accepted_at,
        };
        value.validate()?;
        value.validate_for_command(context)?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), MarketingCrmError> {
        require_id(self.id, "accepted_lead_id")?;
        require_id(self.command_id, "command_id")?;
        validate_non_platform_scope(&self.scope)?;
        validate_deterministic_reference(CrmAggregateKind::AcceptedLead, self.id, &self.reference)?;
        validate_namespaced_idempotency(CrmAggregateKind::AcceptedLead, &self.idempotency_key)?;
        self.source.validate_for_scope(&self.scope)?;
        self.account.validate()?;
        self.contact.validate()?;
        validate_reference_scope(&self.account.scope, &self.scope, "account")?;
        validate_reference_scope(&self.contact.scope, &self.scope, "contact")?;
        ensure_same_tenant(&[&self.scope, &self.account.scope, &self.contact.scope])?;
        Ok(())
    }

    pub fn validate_for_command(&self, context: &CommandContext) -> Result<(), MarketingCrmError> {
        validate_context(context)?;
        if matches!(context.tenant_scope, TenantScope::Platform)
            || !context.tenant_scope.contains(&self.scope)
        {
            return Err(MarketingCrmError::CommandScopeOutside);
        }
        Ok(())
    }

    pub fn as_reference(&self) -> Result<AcceptedLeadReference, MarketingCrmError> {
        AcceptedLeadReference::new(self.reference.clone(), self.scope.clone())
    }
}

/// Validate the command metadata required by every CRM write, including
/// writes that do not carry a CRM aggregate payload such as attribution.
/// Keeping this at the domain boundary prevents an infrastructure adapter
/// from emitting an event with incomplete command provenance.
pub fn validate_command_context(context: &CommandContext) -> Result<(), MarketingCrmError> {
    validate_context(context)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpportunityStatus {
    Draft,
    Qualified,
    Won,
    Lost,
}

impl OpportunityStatus {
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::Qualified) | (Self::Qualified, Self::Won | Self::Lost)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrmAggregateKind {
    AcceptedLead,
    Opportunity,
}

impl CrmAggregateKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::AcceptedLead => "accepted_lead",
            Self::Opportunity => "opportunity",
        }
    }

    const fn reference_prefix(self) -> &'static str {
        match self {
            Self::AcceptedLead => "CRM-LEAD",
            Self::Opportunity => "CRM-OPP",
        }
    }
}

/// Acquisition touch inputs are immutable once canonicalized by
/// AttributionInputs::new. Cost is recorded at the touch and is not invented
/// by an attribution model; revenue allocation is derived from the
/// opportunity's immutable expected revenue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributionTouch {
    pub touch_id: OpaqueId,
    pub source_id: OpaqueId,
    pub campaign_id: Option<OpaqueId>,
    pub scope: TenantScope,
    pub occurred_at: DateTime<Utc>,
    pub cost_minor: i64,
    pub weight_bps: Option<u16>,
}

impl AttributionTouch {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        touch_id: impl Into<String>,
        source_id: impl Into<String>,
        campaign_id: Option<String>,
        scope: TenantScope,
        occurred_at: DateTime<Utc>,
        cost_minor: i64,
        weight_bps: Option<u16>,
    ) -> Result<Self, MarketingCrmError> {
        let value = Self {
            touch_id: OpaqueId::new(touch_id)?,
            source_id: OpaqueId::new(source_id)?,
            campaign_id: campaign_id.map(OpaqueId::new).transpose()?,
            scope,
            occurred_at,
            cost_minor,
            weight_bps,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), MarketingCrmError> {
        bounded(
            self.touch_id.as_str(),
            "attribution_touch_id",
            MAX_OPAQUE_ID_BYTES,
        )?;
        bounded(
            self.source_id.as_str(),
            "attribution_source_id",
            MAX_OPAQUE_ID_BYTES,
        )?;
        if let Some(campaign_id) = &self.campaign_id {
            bounded(
                campaign_id.as_str(),
                "attribution_campaign_id",
                MAX_OPAQUE_ID_BYTES,
            )?;
        }
        validate_non_platform_scope(&self.scope)?;
        if self.cost_minor < 0 {
            return Err(MarketingCrmError::NegativeAmount {
                field: "touch_cost_minor",
            });
        }
        if self.weight_bps.is_some_and(|weight| weight > 10_000) {
            return Err(MarketingCrmError::InvalidWeights);
        }
        Ok(())
    }
}

/// Canonicalized, immutable attribution inputs. The private vector prevents
/// callers from changing order or facts after validation; callers receive
/// only a slice and must create a new value for a new attribution revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributionInputs {
    scope: TenantScope,
    touches: Vec<AttributionTouch>,
    total_cost_minor: i64,
}

impl AttributionInputs {
    pub fn new(
        scope: TenantScope,
        mut touches: Vec<AttributionTouch>,
    ) -> Result<Self, MarketingCrmError> {
        validate_non_platform_scope(&scope)?;
        if touches.is_empty() {
            return Err(MarketingCrmError::Required {
                field: "attribution_touches",
            });
        }
        if touches.len() > MAX_ATTRIBUTION_TOUCHES {
            return Err(MarketingCrmError::TooManyTouches);
        }
        for touch in &touches {
            touch.validate()?;
            validate_reference_scope(&touch.scope, &scope, "attribution_touch")?;
        }
        let mut seen = BTreeSet::new();
        for touch in &touches {
            if !seen.insert(touch.touch_id.clone()) {
                return Err(MarketingCrmError::DuplicateTouch(
                    touch.touch_id.as_str().to_owned(),
                ));
            }
        }
        touches.sort_by(|left, right| {
            left.occurred_at
                .cmp(&right.occurred_at)
                .then_with(|| left.touch_id.cmp(&right.touch_id))
        });
        let total_cost_minor = touches.iter().try_fold(0_i64, |total, touch| {
            total
                .checked_add(touch.cost_minor)
                .ok_or(MarketingCrmError::AmountOverflow)
        })?;
        Ok(Self {
            scope,
            touches,
            total_cost_minor,
        })
    }

    pub fn validate(&self) -> Result<(), MarketingCrmError> {
        validate_non_platform_scope(&self.scope)?;
        let rebuilt = Self::new(self.scope.clone(), self.touches.clone())?;
        if rebuilt.touches != self.touches || rebuilt.total_cost_minor != self.total_cost_minor {
            return Err(MarketingCrmError::InvalidDeterministicField {
                field: "attribution_inputs",
            });
        }
        Ok(())
    }

    pub fn scope(&self) -> &TenantScope {
        &self.scope
    }

    pub fn touches(&self) -> &[AttributionTouch] {
        &self.touches
    }

    pub const fn total_cost_minor(&self) -> i64 {
        self.total_cost_minor
    }

    pub fn validate_for_scope(&self, record_scope: &TenantScope) -> Result<(), MarketingCrmError> {
        self.validate()?;
        validate_reference_scope(&self.scope, record_scope, "attribution")?;
        for touch in &self.touches {
            validate_reference_scope(&touch.scope, record_scope, "attribution_touch")?;
        }
        Ok(())
    }

    /// Allocate revenue with exact integer totals. Remainders go to the
    /// earliest canonical touch after sorting by largest fractional remainder,
    /// then canonical index, making every rebuild deterministic.
    pub fn attribute(
        &self,
        model: AttributionModel,
        revenue_minor: i64,
    ) -> Result<AttributionResult, MarketingCrmError> {
        self.attribute_with_model(model, revenue_minor)
    }

    pub fn attribute_with_model(
        &self,
        model: AttributionModel,
        revenue_minor: i64,
    ) -> Result<AttributionResult, MarketingCrmError> {
        self.validate()?;
        if revenue_minor < 0 {
            return Err(MarketingCrmError::NegativeAttributionRevenue);
        }
        let numerators = model.numerators(&self.touches)?;
        let revenue = allocate_exact(revenue_minor, &numerators)?;
        let entries = self
            .touches
            .iter()
            .zip(revenue)
            .map(|(touch, revenue_minor)| AttributedTouch {
                touch_id: touch.touch_id.clone(),
                source_id: touch.source_id.clone(),
                campaign_id: touch.campaign_id.clone(),
                revenue_minor,
                cost_minor: touch.cost_minor,
            })
            .collect::<Vec<_>>();
        let total_revenue_minor = entries.iter().try_fold(0_i64, |total, entry| {
            total
                .checked_add(entry.revenue_minor)
                .ok_or(MarketingCrmError::AmountOverflow)
        })?;
        Ok(AttributionResult {
            model,
            entries,
            total_revenue_minor,
            total_cost_minor: self.total_cost_minor,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttributionModel {
    FirstTouch,
    LastTouch,
    Linear,
    UShaped,
    Weighted,
}

impl AttributionModel {
    fn numerators(self, touches: &[AttributionTouch]) -> Result<Vec<u64>, MarketingCrmError> {
        let touch_count = touches.len();
        if touch_count == 0 {
            return Err(MarketingCrmError::Required {
                field: "attribution_touches",
            });
        }
        match self {
            Self::FirstTouch => {
                let mut values = vec![0; touch_count];
                values[0] = 1;
                Ok(values)
            }
            Self::LastTouch => {
                let mut values = vec![0; touch_count];
                values[touch_count - 1] = 1;
                Ok(values)
            }
            Self::Linear => Ok(vec![1; touch_count]),
            Self::UShaped if touch_count == 1 => Ok(vec![1]),
            Self::UShaped if touch_count == 2 => Ok(vec![1, 1]),
            Self::UShaped => {
                let middle_count = touch_count - 2;
                let mut values = vec![20; touch_count];
                values[0] = 40 * middle_count as u64;
                values[touch_count - 1] = 40 * middle_count as u64;
                Ok(values)
            }
            Self::Weighted => {
                let values = touches
                    .iter()
                    .map(|touch| {
                        touch
                            .weight_bps
                            .map(u64::from)
                            .ok_or(MarketingCrmError::MissingWeight)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if values.iter().sum::<u64>() != 10_000 {
                    return Err(MarketingCrmError::InvalidWeights);
                }
                Ok(values)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributedTouch {
    pub touch_id: OpaqueId,
    pub source_id: OpaqueId,
    pub campaign_id: Option<OpaqueId>,
    pub revenue_minor: i64,
    pub cost_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributionResult {
    pub model: AttributionModel,
    pub entries: Vec<AttributedTouch>,
    pub total_revenue_minor: i64,
    pub total_cost_minor: i64,
}

/// CRM owns the accepted lead and opportunity lifecycle. It never exposes a
/// transition for Marketing acquisition or qualification state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrmOpportunity {
    pub id: Uuid,
    pub reference: String,
    pub idempotency_key: String,
    pub command_id: Uuid,
    pub scope: TenantScope,
    pub accepted_lead: AcceptedLeadReference,
    pub account: AccountReference,
    pub contact: ContactReference,
    pub name: String,
    pub currency: String,
    pub expected_revenue_minor: i64,
    pub status: OpportunityStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    attribution: AttributionInputs,
}

impl CrmOpportunity {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        handoff: &AcceptedLeadHandoff,
        account: AccountReference,
        contact: ContactReference,
        name: impl Into<String>,
        currency: impl Into<String>,
        expected_revenue_minor: i64,
        attribution: AttributionInputs,
        at: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<Self, MarketingCrmError> {
        handoff.validate()?;
        handoff.validate_for_command(context)?;
        let value = Self {
            id,
            reference: deterministic_reference(CrmAggregateKind::Opportunity, id),
            idempotency_key: deterministic_idempotency_key(CrmAggregateKind::Opportunity, context)?,
            command_id: context.command_id,
            scope: handoff.scope.clone(),
            accepted_lead: handoff.as_reference()?,
            account,
            contact,
            name: name.into(),
            currency: currency.into(),
            expected_revenue_minor,
            status: OpportunityStatus::Draft,
            created_at: at,
            updated_at: at,
            attribution,
        };
        value.validate()?;
        value.validate_for_command(context)?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), MarketingCrmError> {
        require_id(self.id, "opportunity_id")?;
        require_id(self.command_id, "command_id")?;
        validate_non_platform_scope(&self.scope)?;
        validate_deterministic_reference(CrmAggregateKind::Opportunity, self.id, &self.reference)?;
        validate_namespaced_idempotency(CrmAggregateKind::Opportunity, &self.idempotency_key)?;
        self.accepted_lead.validate()?;
        self.account.validate()?;
        self.contact.validate()?;
        validate_reference_scope(&self.accepted_lead.scope, &self.scope, "accepted_lead")?;
        validate_reference_scope(&self.account.scope, &self.scope, "account")?;
        validate_reference_scope(&self.contact.scope, &self.scope, "contact")?;
        ensure_same_tenant(&[
            &self.scope,
            &self.accepted_lead.scope,
            &self.account.scope,
            &self.contact.scope,
        ])?;
        bounded(&self.name, "opportunity_name", MAX_NAME_BYTES)?;
        validate_currency(&self.currency)?;
        if self.expected_revenue_minor < 0 {
            return Err(MarketingCrmError::NegativeAmount {
                field: "expected_revenue_minor",
            });
        }
        if self.updated_at < self.created_at {
            return Err(MarketingCrmError::InvalidDeterministicField {
                field: "updated_at",
            });
        }
        self.attribution.validate_for_scope(&self.scope)?;
        Ok(())
    }

    pub fn transition(
        &mut self,
        next: OpportunityStatus,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<(), MarketingCrmError> {
        self.validate_for_command(context)?;
        if !self.status.can_transition_to(next) {
            return Err(MarketingCrmError::InvalidTransition {
                aggregate_type: "crm_opportunity",
                from: format!("{:?}", self.status),
                to: format!("{next:?}"),
            });
        }
        if at < self.created_at {
            return Err(MarketingCrmError::InvalidDeterministicField {
                field: "updated_at",
            });
        }
        self.status = next;
        self.updated_at = at;
        self.command_id = context.command_id;
        self.idempotency_key =
            deterministic_idempotency_key(CrmAggregateKind::Opportunity, context)?;
        Ok(())
    }

    pub fn validate_for_command(&self, context: &CommandContext) -> Result<(), MarketingCrmError> {
        self.validate()?;
        validate_context(context)?;
        if matches!(context.tenant_scope, TenantScope::Platform)
            || !context.tenant_scope.contains(&self.scope)
        {
            return Err(MarketingCrmError::CommandScopeOutside);
        }
        Ok(())
    }

    pub fn attribution(&self) -> &AttributionInputs {
        &self.attribution
    }

    pub fn as_reference(&self) -> Result<OpportunityReference, MarketingCrmError> {
        OpportunityReference::new(self.reference.clone(), self.scope.clone())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdempotencyRegistration {
    Registered,
    Duplicate,
}

/// Pure write-side duplicate/conflict detection. Persistence should enforce
/// the same two-way binding with a unique idempotency key and unique aggregate
/// reference in the tenant write database.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IdempotencyBindings {
    by_key: BTreeMap<String, String>,
    by_reference: BTreeMap<String, String>,
}

impl IdempotencyBindings {
    pub fn register(
        &mut self,
        idempotency_key: impl Into<String>,
        reference: impl Into<String>,
    ) -> Result<IdempotencyRegistration, MarketingCrmError> {
        let idempotency_key = idempotency_key.into();
        let reference = reference.into();
        bounded(
            &idempotency_key,
            "idempotency_key",
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        bounded(&reference, "reference", MAX_REFERENCE_BYTES)?;
        if let Some(existing_reference) = self.by_key.get(&idempotency_key) {
            if existing_reference == &reference {
                return Ok(IdempotencyRegistration::Duplicate);
            }
            return Err(MarketingCrmError::IdempotencyConflict);
        }
        if let Some(existing_key) = self.by_reference.get(&reference)
            && existing_key != &idempotency_key
        {
            return Err(MarketingCrmError::ReferenceConflict);
        }
        self.by_key
            .insert(idempotency_key.clone(), reference.clone());
        self.by_reference.insert(reference, idempotency_key);
        Ok(IdempotencyRegistration::Registered)
    }
}

pub fn deterministic_reference(kind: CrmAggregateKind, id: Uuid) -> String {
    format!(
        "{}-{}",
        kind.reference_prefix(),
        id.simple().to_string().to_ascii_uppercase()
    )
}

pub fn deterministic_idempotency_key(
    kind: CrmAggregateKind,
    context: &CommandContext,
) -> Result<String, MarketingCrmError> {
    validate_context(context)?;
    let value = format!("crm/{}/{}", kind.as_str(), context.idempotency_key.trim());
    bounded(&value, "idempotency_key", MAX_IDEMPOTENCY_KEY_BYTES)?;
    Ok(value)
}

fn validate_deterministic_reference(
    kind: CrmAggregateKind,
    id: Uuid,
    reference: &str,
) -> Result<(), MarketingCrmError> {
    bounded(reference, "reference", MAX_REFERENCE_BYTES)?;
    if reference != deterministic_reference(kind, id) {
        return Err(MarketingCrmError::InvalidDeterministicField { field: "reference" });
    }
    Ok(())
}

fn validate_namespaced_idempotency(
    kind: CrmAggregateKind,
    idempotency_key: &str,
) -> Result<(), MarketingCrmError> {
    bounded(
        idempotency_key,
        "idempotency_key",
        MAX_IDEMPOTENCY_KEY_BYTES,
    )?;
    let prefix = format!("crm/{}/", kind.as_str());
    if !idempotency_key.starts_with(&prefix) || idempotency_key[prefix.len()..].trim().is_empty() {
        return Err(MarketingCrmError::InvalidDeterministicField {
            field: "idempotency_key",
        });
    }
    Ok(())
}

fn validate_context(context: &CommandContext) -> Result<(), MarketingCrmError> {
    if context.contract_version != CONTRACT_VERSION {
        return Err(MarketingCrmError::InvalidCommandContext(
            "unsupported contract version".into(),
        ));
    }
    context
        .validate()
        .map_err(|error| MarketingCrmError::InvalidCommandContext(error.to_string()))?;
    require_id(context.command_id, "command_id")?;
    require_id(context.correlation_id, "correlation_id")?;
    bounded(
        context.idempotency_key.trim(),
        "idempotency_key",
        MAX_IDEMPOTENCY_KEY_BYTES,
    )
}

fn validate_non_platform_scope(scope: &TenantScope) -> Result<(), MarketingCrmError> {
    match scope {
        TenantScope::Company {
            holding_id,
            company_id,
        } if !holding_id.is_nil() && !company_id.is_nil() => Ok(()),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } if !holding_id.is_nil() && !company_id.is_nil() && !branch_id.is_nil() => Ok(()),
        _ => Err(MarketingCrmError::InvalidScope),
    }
}

fn validate_reference_scope(
    reference_scope: &TenantScope,
    record_scope: &TenantScope,
    field: &'static str,
) -> Result<(), MarketingCrmError> {
    validate_non_platform_scope(reference_scope)?;
    validate_non_platform_scope(record_scope)?;
    if !reference_scope.contains(record_scope) {
        return Err(MarketingCrmError::ReferenceOutsideScope { field });
    }
    Ok(())
}

fn ensure_same_tenant(scopes: &[&TenantScope]) -> Result<(), MarketingCrmError> {
    let Some(first) = scopes.first() else {
        return Ok(());
    };
    let (holding_id, company_id) = scope_company(first)?;
    for scope in scopes.iter().skip(1) {
        let (actual_holding, actual_company) = scope_company(scope)?;
        if actual_holding != holding_id || actual_company != company_id {
            return Err(MarketingCrmError::CrossTenantReference);
        }
    }
    Ok(())
}

fn scope_company(scope: &TenantScope) -> Result<(Uuid, Uuid), MarketingCrmError> {
    match scope {
        TenantScope::Company {
            holding_id,
            company_id,
        }
        | TenantScope::Branch {
            holding_id,
            company_id,
            ..
        } => Ok((*holding_id, *company_id)),
        _ => Err(MarketingCrmError::InvalidScope),
    }
}

fn validate_currency(currency: &str) -> Result<(), MarketingCrmError> {
    if currency.len() != 3 || !currency.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return Err(MarketingCrmError::InvalidCurrency);
    }
    Ok(())
}

fn allocate_exact(total: i64, numerators: &[u64]) -> Result<Vec<i64>, MarketingCrmError> {
    if numerators.is_empty() {
        return Err(MarketingCrmError::Required {
            field: "attribution_touches",
        });
    }
    let denominator = numerators
        .iter()
        .try_fold(0_u64, |sum, value| sum.checked_add(*value))
        .ok_or(MarketingCrmError::AmountOverflow)?;
    if denominator == 0 {
        return Err(MarketingCrmError::InvalidWeights);
    }
    let total = u128::try_from(total).map_err(|_| MarketingCrmError::NegativeAttributionRevenue)?;
    let denominator = u128::from(denominator);
    let mut result = Vec::with_capacity(numerators.len());
    let mut remainders = Vec::with_capacity(numerators.len());
    let mut allocated = 0_u128;
    for (index, numerator) in numerators.iter().enumerate() {
        let product = total
            .checked_mul(u128::from(*numerator))
            .ok_or(MarketingCrmError::AmountOverflow)?;
        let whole = product / denominator;
        allocated = allocated
            .checked_add(whole)
            .ok_or(MarketingCrmError::AmountOverflow)?;
        result.push(i64::try_from(whole).map_err(|_| MarketingCrmError::AmountOverflow)?);
        remainders.push((product % denominator, index));
    }
    let mut remainder = total
        .checked_sub(allocated)
        .ok_or(MarketingCrmError::AmountOverflow)?;
    remainders.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    for (_, index) in remainders {
        if remainder == 0 {
            break;
        }
        result[index] = result[index]
            .checked_add(1)
            .ok_or(MarketingCrmError::AmountOverflow)?;
        remainder -= 1;
    }
    if remainder != 0 {
        return Err(MarketingCrmError::AmountOverflow);
    }
    Ok(result)
}

fn bounded(value: &str, field: &'static str, max: usize) -> Result<(), MarketingCrmError> {
    if value.trim().is_empty() {
        return Err(MarketingCrmError::Required { field });
    }
    if value.len() > max {
        return Err(MarketingCrmError::TooLong { field, max });
    }
    if value.chars().any(char::is_control) {
        return Err(MarketingCrmError::ControlCharacter { field });
    }
    Ok(())
}

fn require_id(id: Uuid, field: &'static str) -> Result<(), MarketingCrmError> {
    if id.is_nil() {
        Err(MarketingCrmError::NilIdentifier { field })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{ActorAttribution, ActorKind, AuditContext};

    fn ids() -> (Uuid, Uuid, Uuid, Uuid) {
        (
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
    }

    fn scope(holding_id: Uuid, company_id: Uuid, branch_id: Option<Uuid>) -> TenantScope {
        match branch_id {
            Some(branch_id) => TenantScope::Branch {
                holding_id,
                company_id,
                branch_id,
            },
            None => TenantScope::Company {
                holding_id,
                company_id,
            },
        }
    }

    fn context(record_scope: TenantScope, key: &str) -> CommandContext {
        let command_id = Uuid::new_v4();
        let correlation_id = Uuid::new_v4();
        CommandContext {
            contract_version: 1,
            command_id,
            idempotency_key: key.to_owned(),
            correlation_id,
            causation_id: None,
            actor: ActorAttribution {
                principal_ref: "crm-agent".to_owned(),
                kind: ActorKind::TenantUser,
                session_id: Some(Uuid::new_v4()),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            tenant_scope: record_scope,
            audit: AuditContext {
                purpose: "crm-test".to_owned(),
                customer_visible: false,
                module: Some("marketing_crm".to_owned()),
                correlation_id,
            },
            expected_aggregate_version: None,
            subscription_id: None,
        }
    }

    fn source(record_scope: &TenantScope) -> MarketingQualifiedSource {
        MarketingQualifiedSource {
            lead: MarketingLeadReference::new("mkt-lead-1", record_scope.clone()).unwrap(),
            acquisition: MarketingAcquisitionReference::new(
                "mkt-acquisition-1",
                record_scope.clone(),
            )
            .unwrap(),
            qualification: MarketingQualificationReference::new(
                "mkt-qualification-1",
                record_scope.clone(),
            )
            .unwrap(),
        }
    }

    fn handoff(record_scope: TenantScope, key: &str) -> AcceptedLeadHandoff {
        AcceptedLeadHandoff::new(
            Uuid::new_v4(),
            record_scope.clone(),
            source(&record_scope),
            AccountReference::new("crm-account-1", record_scope.clone()).unwrap(),
            ContactReference::new("crm-contact-1", record_scope.clone()).unwrap(),
            Utc::now(),
            &context(record_scope, key),
        )
        .unwrap()
    }

    #[test]
    fn scope_isolation_rejects_platform_and_cross_tenant_references() {
        let (holding, company, branch, other_holding) = ids();
        assert!(matches!(
            AccountReference::new("account", TenantScope::Platform),
            Err(MarketingCrmError::InvalidScope)
        ));

        let record_scope = scope(holding, company, Some(branch));
        let other_scope = scope(other_holding, company, Some(branch));
        let result = AcceptedLeadHandoff::new(
            Uuid::new_v4(),
            record_scope.clone(),
            MarketingQualifiedSource {
                lead: MarketingLeadReference::new("lead", record_scope.clone()).unwrap(),
                acquisition: MarketingAcquisitionReference::new(
                    "acquisition",
                    record_scope.clone(),
                )
                .unwrap(),
                qualification: MarketingQualificationReference::new(
                    "qualification",
                    record_scope.clone(),
                )
                .unwrap(),
            },
            AccountReference::new("account", other_scope).unwrap(),
            ContactReference::new("contact", record_scope.clone()).unwrap(),
            Utc::now(),
            &context(record_scope, "scope-isolation"),
        );
        assert!(matches!(
            result,
            Err(MarketingCrmError::ReferenceOutsideScope { field: "account" })
                | Err(MarketingCrmError::CrossTenantReference)
        ));
    }

    #[test]
    fn opportunity_lifecycle_requires_ordered_crm_transitions() {
        let (holding, company, branch, _) = ids();
        let record_scope = scope(holding, company, Some(branch));
        let handoff = handoff(record_scope.clone(), "lifecycle-handoff");
        let touches = AttributionInputs::new(
            record_scope.clone(),
            vec![
                AttributionTouch::new(
                    "touch-1",
                    "source-1",
                    None,
                    record_scope.clone(),
                    Utc::now(),
                    10,
                    None,
                )
                .unwrap(),
            ],
        )
        .unwrap();
        let command = context(record_scope.clone(), "lifecycle-opportunity");
        let mut opportunity = CrmOpportunity::new(
            Uuid::new_v4(),
            &handoff,
            AccountReference::new("account", record_scope.clone()).unwrap(),
            ContactReference::new("contact", record_scope.clone()).unwrap(),
            "Opportunity",
            "THB",
            1_000,
            touches,
            Utc::now(),
            &command,
        )
        .unwrap();
        assert_eq!(opportunity.status, OpportunityStatus::Draft);
        assert!(
            opportunity
                .transition(OpportunityStatus::Lost, &command, Utc::now())
                .is_err()
        );
        opportunity
            .transition(OpportunityStatus::Qualified, &command, Utc::now())
            .unwrap();
        opportunity
            .transition(OpportunityStatus::Won, &command, Utc::now())
            .unwrap();
        assert!(
            opportunity
                .transition(OpportunityStatus::Qualified, &command, Utc::now())
                .is_err()
        );
    }

    #[test]
    fn attribution_order_and_totals_are_deterministic() {
        let (holding, company, branch, _) = ids();
        let record_scope = scope(holding, company, Some(branch));
        let late = AttributionTouch::new(
            "late",
            "source-late",
            Some("campaign".to_owned()),
            record_scope.clone(),
            Utc::now(),
            30,
            None,
        )
        .unwrap();
        let early = AttributionTouch::new(
            "early",
            "source-early",
            None,
            record_scope.clone(),
            Utc::now() - chrono::Duration::days(1),
            20,
            None,
        )
        .unwrap();
        let inputs_a =
            AttributionInputs::new(record_scope.clone(), vec![late.clone(), early.clone()])
                .unwrap();
        let inputs_b = AttributionInputs::new(record_scope, vec![early, late]).unwrap();
        assert_eq!(inputs_a, inputs_b);
        let first = inputs_a
            .attribute_with_model(AttributionModel::Linear, 101)
            .unwrap();
        let second = inputs_b
            .attribute_with_model(AttributionModel::Linear, 101)
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.entries[0].revenue_minor, 51);
        assert_eq!(first.total_revenue_minor, 101);
        assert_eq!(first.total_cost_minor, 50);
    }

    #[test]
    fn invalid_weights_and_negative_revenue_or_cost_fail_closed() {
        let (holding, company, branch, _) = ids();
        let record_scope = scope(holding, company, Some(branch));
        assert!(matches!(
            AttributionTouch::new(
                "touch",
                "source",
                None,
                record_scope.clone(),
                Utc::now(),
                -1,
                Some(5_000),
            ),
            Err(MarketingCrmError::NegativeAmount {
                field: "touch_cost_minor"
            })
        ));
        let inputs = AttributionInputs::new(
            record_scope,
            vec![
                AttributionTouch::new(
                    "touch",
                    "source",
                    None,
                    scope(holding, company, Some(branch)),
                    Utc::now(),
                    1,
                    Some(5_000),
                )
                .unwrap(),
            ],
        )
        .unwrap();
        assert!(matches!(
            inputs.attribute_with_model(AttributionModel::Weighted, -1),
            Err(MarketingCrmError::NegativeAttributionRevenue)
        ));
        assert!(matches!(
            inputs.attribute_with_model(AttributionModel::Weighted, 100),
            Err(MarketingCrmError::InvalidWeights)
        ));
    }

    #[test]
    fn idempotency_and_reference_bindings_reject_conflicts() {
        let mut bindings = IdempotencyBindings::default();
        assert_eq!(
            bindings
                .register("crm/opportunity/key", "CRM-OPP-1")
                .unwrap(),
            IdempotencyRegistration::Registered
        );
        assert_eq!(
            bindings
                .register("crm/opportunity/key", "CRM-OPP-1")
                .unwrap(),
            IdempotencyRegistration::Duplicate
        );
        assert!(matches!(
            bindings.register("crm/opportunity/key", "CRM-OPP-2"),
            Err(MarketingCrmError::IdempotencyConflict)
        ));
        assert!(matches!(
            bindings.register("crm/opportunity/other", "CRM-OPP-1"),
            Err(MarketingCrmError::ReferenceConflict)
        ));
    }

    #[test]
    fn command_context_validation_rejects_unsupported_versions_and_empty_namespaces() {
        let (holding, company, branch, _) = ids();
        let record_scope = scope(holding, company, Some(branch));
        let mut invalid_context = context(record_scope, "context-validation");
        invalid_context.contract_version = CONTRACT_VERSION + 1;
        assert!(matches!(
            validate_command_context(&invalid_context),
            Err(MarketingCrmError::InvalidCommandContext(message))
                if message == "unsupported contract version"
        ));

        let mut invalid_handoff = handoff(
            scope(holding, company, Some(branch)),
            "empty-idempotency-suffix",
        );
        invalid_handoff.idempotency_key = "crm/accepted_lead/".into();
        assert!(matches!(
            invalid_handoff.validate(),
            Err(MarketingCrmError::InvalidDeterministicField {
                field: "idempotency_key"
            })
        ));
    }
}
