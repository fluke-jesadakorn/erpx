//! Tenant-bound Sales contracts.
//!
//! Sales owns the commercial progression from a customer master reference to
//! a collection state. It does not own customer master data, inventory,
//! delivery execution, invoicing, or cash application; the handoff records
//! below are immutable, scoped references for those bounded contexts.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::contracts::{CommandContext, TenantScope};

pub const SALES_CONTRACT_VERSION: u16 = 1;
pub const MAX_SALES_TEXT_BYTES: usize = 255;
pub const MAX_SALES_REFERENCE_BYTES: usize = 128;
pub const MAX_SALES_IDEMPOTENCY_BYTES: usize = 512;
pub const MAX_SALES_LINES: usize = 10_000;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SalesError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds the maximum length of {max} bytes")]
    TooLong { field: &'static str, max: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("sales records require an explicit company or branch scope")]
    InvalidScope,
    #[error("command scope {command_scope:?} does not contain record scope {record_scope:?}")]
    CommandScopeOutside {
        command_scope: Box<TenantScope>,
        record_scope: Box<TenantScope>,
    },
    #[error("customer reference scope does not contain the sales record scope")]
    CustomerScopeOutside,
    #[error("customer reference does not match the sales holding or company")]
    CustomerBindingMismatch,
    #[error("{aggregate_type} lifecycle transition is not allowed: {from} -> {to}")]
    InvalidTransition {
        aggregate_type: &'static str,
        from: String,
        to: String,
    },
    #[error("{field} is not the deterministic value expected for this aggregate")]
    InvalidDeterministicField { field: &'static str },
    #[error("command context is invalid: {0}")]
    InvalidCommandContext(String),
    #[error("aggregate command identity does not match the command context")]
    CommandIdentityMismatch {
        aggregate_command_id: Uuid,
        context_command_id: Uuid,
    },
    #[error("aggregate idempotency key does not match the command context")]
    CommandIdempotencyMismatch { expected: String, actual: String },
    #[error("currency must be a three-letter uppercase ISO code")]
    InvalidCurrency,
    #[error("{field} must be positive")]
    NonPositive { field: &'static str },
    #[error("{field} cannot be negative")]
    Negative { field: &'static str },
    #[error("sales line ID {0} is duplicated")]
    DuplicateLineId(Uuid),
    #[error("sales line total arithmetic overflowed")]
    AmountOverflow,
    #[error("sales document must contain at least one line")]
    EmptyLines,
    #[error("date window is invalid")]
    InvalidWindow,
    #[error("quote must be accepted before it can become a sales order")]
    QuoteNotAccepted,
    #[error("quote cannot be accepted after its validity window")]
    QuoteExpired,
    #[error("sales order must be confirmed before a handoff is created")]
    OrderNotConfirmed,
    #[error("delivery handoff must be handed off before invoicing begins")]
    DeliveryNotHandedOff,
    #[error("invoice handoff must be handed off before collection tracking begins")]
    InvoiceNotHandedOff,
    #[error("payment exceeds the outstanding balance")]
    Overpayment,
    #[error("payment amount must be positive")]
    InvalidPayment,
    #[error("{aggregate_type} is not linked to the authoritative {related_type} aggregate")]
    WorkflowLinkageMismatch {
        aggregate_type: &'static str,
        related_type: &'static str,
    },
}

/// Tenant-scoped source/read reconciliation facts for the disposable Sales
/// projection. The write aggregate and outbox remain authoritative; callers
/// must treat any non-zero mismatch as a rebuild/incident signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SalesProjectionReconciliation {
    pub source_event_count: u64,
    pub source_aggregate_count: u64,
    pub projected_aggregate_count: u64,
    pub missing_aggregate_count: u64,
    pub stale_aggregate_count: u64,
    pub orphan_aggregate_count: u64,
    pub invalid_projection_count: u64,
    pub workflow_linkage_error_count: u64,
}

impl SalesProjectionReconciliation {
    pub const fn is_consistent(self) -> bool {
        self.missing_aggregate_count == 0
            && self.stale_aggregate_count == 0
            && self.orphan_aggregate_count == 0
            && self.invalid_projection_count == 0
            && self.workflow_linkage_error_count == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SalesAggregateKind {
    Lead,
    Opportunity,
    Quote,
    SalesOrder,
    DeliveryHandoff,
    InvoiceHandoff,
    CollectionState,
}

impl SalesAggregateKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lead => "lead",
            Self::Opportunity => "opportunity",
            Self::Quote => "quote",
            Self::SalesOrder => "sales_order",
            Self::DeliveryHandoff => "delivery_handoff",
            Self::InvoiceHandoff => "invoice_handoff",
            Self::CollectionState => "collection_state",
        }
    }

    const fn reference_prefix(self) -> &'static str {
        match self {
            Self::Lead => "LEAD",
            Self::Opportunity => "OPP",
            Self::Quote => "QUOTE",
            Self::SalesOrder => "SO",
            Self::DeliveryHandoff => "DELIVERY",
            Self::InvoiceHandoff => "INVOICE",
            Self::CollectionState => "COLLECTION",
        }
    }
}

/// A stable human-readable reference derived only from aggregate kind and ID.
/// It is safe to regenerate after a read-model rebuild.
pub fn deterministic_reference(kind: SalesAggregateKind, id: Uuid) -> String {
    format!(
        "{}-{}",
        kind.reference_prefix(),
        id.simple().to_string().to_ascii_uppercase()
    )
}

/// A namespaced idempotency key derived from the caller's command key. The
/// namespace prevents a key collision between Sales aggregate kinds.
pub fn deterministic_idempotency_key(
    kind: SalesAggregateKind,
    context: &CommandContext,
) -> Result<String, SalesError> {
    validate_context(context)?;
    let value = format!("sales/{}/{}", kind.as_str(), context.idempotency_key.trim());
    bounded(&value, "idempotency_key", MAX_SALES_IDEMPOTENCY_BYTES)?;
    Ok(value)
}

/// Validate the command boundary used by generic Sales save transports.
///
/// A lifecycle command may first validate an existing aggregate and then
/// rotate its command identity. A generic save receives the already-materialized
/// aggregate, so it must instead prove that the aggregate carries this exact
/// command ID and the deterministic idempotency namespace for this command.
/// The caller's scope may be broader than the record scope, but may never be a
/// sibling or child scope that does not contain the record.
pub fn validate_save_command_binding(
    kind: SalesAggregateKind,
    aggregate_command_id: Uuid,
    aggregate_idempotency_key: &str,
    aggregate_scope: &TenantScope,
    context: &CommandContext,
) -> Result<(), SalesError> {
    validate_command_scope(context, aggregate_scope)?;
    if aggregate_command_id != context.command_id {
        return Err(SalesError::CommandIdentityMismatch {
            aggregate_command_id,
            context_command_id: context.command_id,
        });
    }
    let expected = deterministic_idempotency_key(kind, context)?;
    if aggregate_idempotency_key != expected {
        return Err(SalesError::CommandIdempotencyMismatch {
            expected,
            actual: aggregate_idempotency_key.to_owned(),
        });
    }
    Ok(())
}

/// A reference into the Customer bounded context. Sales never copies or
/// mutates customer master data; it records this scoped identity and verifies
/// that the customer scope contains the sales record scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerMasterReference {
    pub customer_id: Uuid,
    pub customer_reference: String,
    pub scope: TenantScope,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
}

impl CustomerMasterReference {
    pub fn new(
        customer_id: Uuid,
        scope: TenantScope,
        holding_id: Uuid,
        company_id: Uuid,
        branch_id: Option<Uuid>,
    ) -> Result<Self, SalesError> {
        let value = Self {
            customer_id,
            customer_reference: format!("CUST-{}", customer_id.simple()),
            scope,
            holding_id,
            company_id,
            branch_id,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), SalesError> {
        require_id(self.customer_id, "customer_id")?;
        validate_scope_binding(
            &self.scope,
            self.holding_id,
            self.company_id,
            self.branch_id,
        )?;
        if self.customer_reference != format!("CUST-{}", self.customer_id.simple()) {
            return Err(SalesError::InvalidDeterministicField {
                field: "customer_reference",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesLine {
    pub line_id: Uuid,
    pub item_reference: String,
    pub description: String,
    pub quantity: u64,
    pub unit_price_minor: i64,
    pub tax_minor: i64,
}

impl SalesLine {
    pub fn validate(&self) -> Result<(), SalesError> {
        require_id(self.line_id, "line_id")?;
        bounded(&self.item_reference, "item_reference", MAX_SALES_TEXT_BYTES)?;
        bounded(&self.description, "description", MAX_SALES_TEXT_BYTES)?;
        if self.quantity == 0 {
            return Err(SalesError::NonPositive { field: "quantity" });
        }
        if self.unit_price_minor < 0 {
            return Err(SalesError::Negative {
                field: "unit_price_minor",
            });
        }
        if self.tax_minor < 0 {
            return Err(SalesError::Negative { field: "tax_minor" });
        }
        self.total_minor()?;
        Ok(())
    }

    pub fn total_minor(&self) -> Result<i64, SalesError> {
        let quantity = i64::try_from(self.quantity).map_err(|_| SalesError::AmountOverflow)?;
        quantity
            .checked_mul(self.unit_price_minor)
            .and_then(|subtotal| subtotal.checked_add(self.tax_minor))
            .ok_or(SalesError::AmountOverflow)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeadStatus {
    New,
    Qualified,
    Disqualified,
    Converted,
}

impl LeadStatus {
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::New, Self::Qualified | Self::Disqualified)
                | (Self::Qualified, Self::Converted | Self::Disqualified)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lead {
    pub id: Uuid,
    pub reference: String,
    pub idempotency_key: String,
    pub command_id: Uuid,
    pub scope: TenantScope,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub customer: Option<CustomerMasterReference>,
    pub name: String,
    pub source: String,
    pub status: LeadStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Lead {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        scope: TenantScope,
        holding_id: Uuid,
        company_id: Uuid,
        branch_id: Option<Uuid>,
        customer: Option<CustomerMasterReference>,
        name: impl Into<String>,
        source: impl Into<String>,
        at: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<Self, SalesError> {
        let (reference, idempotency_key) = identity_fields(SalesAggregateKind::Lead, id, context)?;
        let value = Self {
            id,
            reference,
            idempotency_key,
            command_id: context.command_id,
            scope,
            holding_id,
            company_id,
            branch_id,
            customer,
            name: name.into(),
            source: source.into(),
            status: LeadStatus::New,
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        validate_command_scope(context, &value.scope)?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), SalesError> {
        validate_identity(
            SalesAggregateKind::Lead,
            self.id,
            &self.reference,
            &self.idempotency_key,
            self.command_id,
        )?;
        validate_scope_binding(
            &self.scope,
            self.holding_id,
            self.company_id,
            self.branch_id,
        )?;
        validate_customer(
            self.customer.as_ref(),
            &self.scope,
            self.holding_id,
            self.company_id,
        )?;
        bounded(&self.name, "lead_name", MAX_SALES_TEXT_BYTES)?;
        bounded(&self.source, "lead_source", MAX_SALES_TEXT_BYTES)?;
        if self.updated_at < self.created_at {
            return Err(SalesError::InvalidWindow);
        }
        Ok(())
    }

    pub fn transition(
        &mut self,
        next: LeadStatus,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<(), SalesError> {
        self.validate_for_command(context)?;
        if !self.status.can_transition_to(next) {
            return Err(invalid_transition("lead", self.status, next));
        }
        if at < self.updated_at {
            return Err(SalesError::InvalidWindow);
        }
        let idempotency_key = deterministic_idempotency_key(SalesAggregateKind::Lead, context)?;
        let mut candidate = self.clone();
        candidate.command_id = context.command_id;
        candidate.idempotency_key = idempotency_key;
        candidate.status = next;
        candidate.updated_at = at;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn validate_for_command(&self, context: &CommandContext) -> Result<(), SalesError> {
        self.validate()?;
        validate_command_scope(context, &self.scope)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpportunityStatus {
    Open,
    Won,
    Lost,
    Abandoned,
}

impl OpportunityStatus {
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Open, Self::Won | Self::Lost | Self::Abandoned)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Opportunity {
    pub id: Uuid,
    pub reference: String,
    pub idempotency_key: String,
    pub command_id: Uuid,
    pub scope: TenantScope,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub lead_id: Option<Uuid>,
    pub customer: CustomerMasterReference,
    pub name: String,
    pub currency: String,
    pub value_minor: i64,
    pub status: OpportunityStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Opportunity {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        scope: TenantScope,
        holding_id: Uuid,
        company_id: Uuid,
        branch_id: Option<Uuid>,
        lead_id: Option<Uuid>,
        customer: CustomerMasterReference,
        name: impl Into<String>,
        currency: impl Into<String>,
        value_minor: i64,
        at: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<Self, SalesError> {
        let (reference, idempotency_key) =
            identity_fields(SalesAggregateKind::Opportunity, id, context)?;
        let value = Self {
            id,
            reference,
            idempotency_key,
            command_id: context.command_id,
            scope,
            holding_id,
            company_id,
            branch_id,
            lead_id,
            customer,
            name: name.into(),
            currency: currency.into(),
            value_minor,
            status: OpportunityStatus::Open,
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        validate_command_scope(context, &value.scope)?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), SalesError> {
        validate_identity(
            SalesAggregateKind::Opportunity,
            self.id,
            &self.reference,
            &self.idempotency_key,
            self.command_id,
        )?;
        validate_scope_binding(
            &self.scope,
            self.holding_id,
            self.company_id,
            self.branch_id,
        )?;
        validate_customer(
            Some(&self.customer),
            &self.scope,
            self.holding_id,
            self.company_id,
        )?;
        if let Some(lead_id) = self.lead_id {
            require_id(lead_id, "lead_id")?;
        }
        bounded(&self.name, "opportunity_name", MAX_SALES_TEXT_BYTES)?;
        validate_currency(&self.currency)?;
        if self.value_minor < 0 {
            return Err(SalesError::Negative {
                field: "value_minor",
            });
        }
        if self.updated_at < self.created_at {
            return Err(SalesError::InvalidWindow);
        }
        Ok(())
    }

    pub fn transition(
        &mut self,
        next: OpportunityStatus,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<(), SalesError> {
        self.validate_for_command(context)?;
        if !self.status.can_transition_to(next) {
            return Err(invalid_transition("opportunity", self.status, next));
        }
        if at < self.updated_at {
            return Err(SalesError::InvalidWindow);
        }
        let idempotency_key =
            deterministic_idempotency_key(SalesAggregateKind::Opportunity, context)?;
        let mut candidate = self.clone();
        candidate.command_id = context.command_id;
        candidate.idempotency_key = idempotency_key;
        candidate.status = next;
        candidate.updated_at = at;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn validate_for_command(&self, context: &CommandContext) -> Result<(), SalesError> {
        self.validate()?;
        validate_command_scope(context, &self.scope)
    }

    pub fn validate_against_lead(&self, lead: &Lead) -> Result<(), SalesError> {
        self.validate()?;
        lead.validate()?;
        if self.lead_id != Some(lead.id)
            || !compatible_scopes(&self.scope, &lead.scope)
            || self.customer.holding_id != lead.holding_id
            || self.customer.company_id != lead.company_id
            || lead.customer.as_ref().is_some_and(|customer| {
                customer.customer_id != self.customer.customer_id
                    || customer.customer_reference != self.customer.customer_reference
            })
        {
            return Err(workflow_linkage_mismatch("opportunity", "lead"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuoteStatus {
    Draft,
    Sent,
    Accepted,
    Rejected,
    Expired,
}

impl QuoteStatus {
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::Sent)
                | (Self::Sent, Self::Accepted | Self::Rejected | Self::Expired)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quote {
    pub id: Uuid,
    pub reference: String,
    pub idempotency_key: String,
    pub command_id: Uuid,
    pub scope: TenantScope,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub opportunity_id: Option<Uuid>,
    pub customer: CustomerMasterReference,
    pub currency: String,
    pub lines: Vec<SalesLine>,
    pub total_minor: i64,
    pub valid_until: DateTime<Utc>,
    pub status: QuoteStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Quote {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        scope: TenantScope,
        holding_id: Uuid,
        company_id: Uuid,
        branch_id: Option<Uuid>,
        opportunity_id: Option<Uuid>,
        customer: CustomerMasterReference,
        currency: impl Into<String>,
        lines: Vec<SalesLine>,
        valid_until: DateTime<Utc>,
        at: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<Self, SalesError> {
        let total_minor = total_lines(&lines)?;
        let (reference, idempotency_key) = identity_fields(SalesAggregateKind::Quote, id, context)?;
        let value = Self {
            id,
            reference,
            idempotency_key,
            command_id: context.command_id,
            scope,
            holding_id,
            company_id,
            branch_id,
            opportunity_id,
            customer,
            currency: currency.into(),
            lines,
            total_minor,
            valid_until,
            status: QuoteStatus::Draft,
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        validate_command_scope(context, &value.scope)?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), SalesError> {
        validate_identity(
            SalesAggregateKind::Quote,
            self.id,
            &self.reference,
            &self.idempotency_key,
            self.command_id,
        )?;
        validate_scope_binding(
            &self.scope,
            self.holding_id,
            self.company_id,
            self.branch_id,
        )?;
        validate_customer(
            Some(&self.customer),
            &self.scope,
            self.holding_id,
            self.company_id,
        )?;
        if let Some(opportunity_id) = self.opportunity_id {
            require_id(opportunity_id, "opportunity_id")?;
        }
        validate_currency(&self.currency)?;
        let expected_total = total_lines(&self.lines)?;
        if expected_total != self.total_minor {
            return Err(SalesError::InvalidDeterministicField {
                field: "total_minor",
            });
        }
        if self.valid_until <= self.created_at || self.updated_at < self.created_at {
            return Err(SalesError::InvalidWindow);
        }
        Ok(())
    }

    pub fn transition(
        &mut self,
        next: QuoteStatus,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<(), SalesError> {
        self.validate_for_command(context)?;
        if !self.status.can_transition_to(next) {
            return Err(invalid_transition("quote", self.status, next));
        }
        if at < self.updated_at {
            return Err(SalesError::InvalidWindow);
        }
        if next == QuoteStatus::Accepted && at > self.valid_until {
            return Err(SalesError::QuoteExpired);
        }
        let idempotency_key = deterministic_idempotency_key(SalesAggregateKind::Quote, context)?;
        let mut candidate = self.clone();
        candidate.command_id = context.command_id;
        candidate.idempotency_key = idempotency_key;
        candidate.status = next;
        candidate.updated_at = at;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn validate_for_command(&self, context: &CommandContext) -> Result<(), SalesError> {
        self.validate()?;
        validate_command_scope(context, &self.scope)
    }

    pub fn validate_against_opportunity(
        &self,
        opportunity: &Opportunity,
    ) -> Result<(), SalesError> {
        self.validate()?;
        opportunity.validate()?;
        if self.opportunity_id != Some(opportunity.id)
            || !compatible_scopes(&self.scope, &opportunity.scope)
            || self.customer != opportunity.customer
        {
            return Err(workflow_linkage_mismatch("quote", "opportunity"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SalesOrderStatus {
    Draft,
    Confirmed,
    Fulfillment,
    Completed,
    Cancelled,
}

impl SalesOrderStatus {
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::Confirmed | Self::Cancelled)
                | (Self::Confirmed, Self::Fulfillment | Self::Cancelled)
                | (Self::Fulfillment, Self::Completed | Self::Cancelled)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesOrder {
    pub id: Uuid,
    pub reference: String,
    pub idempotency_key: String,
    pub command_id: Uuid,
    pub scope: TenantScope,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub quote_id: Uuid,
    pub quote_reference: String,
    pub customer: CustomerMasterReference,
    pub currency: String,
    pub lines: Vec<SalesLine>,
    pub total_minor: i64,
    pub status: SalesOrderStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SalesOrder {
    pub fn new_from_quote(
        id: Uuid,
        quote: &Quote,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<Self, SalesError> {
        quote.validate_for_command(context)?;
        if quote.status != QuoteStatus::Accepted {
            return Err(SalesError::QuoteNotAccepted);
        }
        let (reference, idempotency_key) =
            identity_fields(SalesAggregateKind::SalesOrder, id, context)?;
        let value = Self {
            id,
            reference,
            idempotency_key,
            command_id: context.command_id,
            scope: quote.scope.clone(),
            holding_id: quote.holding_id,
            company_id: quote.company_id,
            branch_id: quote.branch_id,
            quote_id: quote.id,
            quote_reference: quote.reference.clone(),
            customer: quote.customer.clone(),
            currency: quote.currency.clone(),
            lines: quote.lines.clone(),
            total_minor: quote.total_minor,
            status: SalesOrderStatus::Draft,
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), SalesError> {
        validate_identity(
            SalesAggregateKind::SalesOrder,
            self.id,
            &self.reference,
            &self.idempotency_key,
            self.command_id,
        )?;
        validate_scope_binding(
            &self.scope,
            self.holding_id,
            self.company_id,
            self.branch_id,
        )?;
        validate_customer(
            Some(&self.customer),
            &self.scope,
            self.holding_id,
            self.company_id,
        )?;
        require_id(self.quote_id, "quote_id")?;
        bounded(
            &self.quote_reference,
            "quote_reference",
            MAX_SALES_REFERENCE_BYTES,
        )?;
        validate_currency(&self.currency)?;
        let expected_total = total_lines(&self.lines)?;
        if expected_total != self.total_minor {
            return Err(SalesError::InvalidDeterministicField {
                field: "total_minor",
            });
        }
        if self.updated_at < self.created_at {
            return Err(SalesError::InvalidWindow);
        }
        Ok(())
    }

    pub fn transition(
        &mut self,
        next: SalesOrderStatus,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<(), SalesError> {
        self.validate_for_command(context)?;
        if !self.status.can_transition_to(next) {
            return Err(invalid_transition("sales_order", self.status, next));
        }
        if at < self.updated_at {
            return Err(SalesError::InvalidWindow);
        }
        let idempotency_key =
            deterministic_idempotency_key(SalesAggregateKind::SalesOrder, context)?;
        let mut candidate = self.clone();
        candidate.command_id = context.command_id;
        candidate.idempotency_key = idempotency_key;
        candidate.status = next;
        candidate.updated_at = at;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn validate_for_command(&self, context: &CommandContext) -> Result<(), SalesError> {
        self.validate()?;
        validate_command_scope(context, &self.scope)
    }

    pub fn validate_against_quote(&self, quote: &Quote) -> Result<(), SalesError> {
        self.validate()?;
        quote.validate()?;
        if quote.status != QuoteStatus::Accepted
            || self.quote_id != quote.id
            || self.quote_reference != quote.reference
            || self.scope != quote.scope
            || self.holding_id != quote.holding_id
            || self.company_id != quote.company_id
            || self.branch_id != quote.branch_id
            || self.customer != quote.customer
            || self.currency != quote.currency
            || self.lines != quote.lines
            || self.total_minor != quote.total_minor
        {
            return Err(workflow_linkage_mismatch("sales_order", "quote"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffStatus {
    Pending,
    Requested,
    HandedOff,
    Failed,
    Cancelled,
}

impl HandoffStatus {
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Pending, Self::Requested | Self::Cancelled)
                | (
                    Self::Requested,
                    Self::HandedOff | Self::Failed | Self::Cancelled
                )
                | (Self::Failed, Self::Requested)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryHandoff {
    pub id: Uuid,
    pub reference: String,
    pub idempotency_key: String,
    pub command_id: Uuid,
    pub scope: TenantScope,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub sales_order_id: Uuid,
    pub sales_order_reference: String,
    pub status: HandoffStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl DeliveryHandoff {
    pub fn new_from_order(
        id: Uuid,
        order: &SalesOrder,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<Self, SalesError> {
        order.validate_for_command(context)?;
        if order.status != SalesOrderStatus::Confirmed
            && order.status != SalesOrderStatus::Fulfillment
        {
            return Err(SalesError::OrderNotConfirmed);
        }
        let (reference, idempotency_key) =
            identity_fields(SalesAggregateKind::DeliveryHandoff, id, context)?;
        let value = Self {
            id,
            reference,
            idempotency_key,
            command_id: context.command_id,
            scope: order.scope.clone(),
            holding_id: order.holding_id,
            company_id: order.company_id,
            branch_id: order.branch_id,
            sales_order_id: order.id,
            sales_order_reference: order.reference.clone(),
            status: HandoffStatus::Pending,
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), SalesError> {
        validate_identity(
            SalesAggregateKind::DeliveryHandoff,
            self.id,
            &self.reference,
            &self.idempotency_key,
            self.command_id,
        )?;
        validate_scope_binding(
            &self.scope,
            self.holding_id,
            self.company_id,
            self.branch_id,
        )?;
        require_id(self.sales_order_id, "sales_order_id")?;
        bounded(
            &self.sales_order_reference,
            "sales_order_reference",
            MAX_SALES_REFERENCE_BYTES,
        )?;
        if self.updated_at < self.created_at {
            return Err(SalesError::InvalidWindow);
        }
        Ok(())
    }

    pub fn transition(
        &mut self,
        next: HandoffStatus,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<(), SalesError> {
        self.validate_for_command(context)?;
        if !self.status.can_transition_to(next) {
            return Err(invalid_transition("delivery_handoff", self.status, next));
        }
        if at < self.updated_at {
            return Err(SalesError::InvalidWindow);
        }
        let idempotency_key =
            deterministic_idempotency_key(SalesAggregateKind::DeliveryHandoff, context)?;
        let mut candidate = self.clone();
        candidate.command_id = context.command_id;
        candidate.idempotency_key = idempotency_key;
        candidate.status = next;
        candidate.updated_at = at;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn validate_for_command(&self, context: &CommandContext) -> Result<(), SalesError> {
        self.validate()?;
        validate_command_scope(context, &self.scope)
    }

    pub fn validate_against_order(&self, order: &SalesOrder) -> Result<(), SalesError> {
        self.validate()?;
        order.validate()?;
        if matches!(
            order.status,
            SalesOrderStatus::Draft | SalesOrderStatus::Cancelled
        ) || self.scope != order.scope
            || self.holding_id != order.holding_id
            || self.company_id != order.company_id
            || self.branch_id != order.branch_id
            || self.sales_order_id != order.id
            || self.sales_order_reference != order.reference
        {
            return Err(workflow_linkage_mismatch("delivery_handoff", "sales_order"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceHandoff {
    pub id: Uuid,
    pub reference: String,
    pub idempotency_key: String,
    pub command_id: Uuid,
    pub scope: TenantScope,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub sales_order_id: Uuid,
    pub sales_order_reference: String,
    pub delivery_handoff_id: Option<Uuid>,
    pub currency: String,
    pub amount_due_minor: i64,
    pub status: HandoffStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl InvoiceHandoff {
    pub fn new_from_order(
        id: Uuid,
        order: &SalesOrder,
        delivery_handoff: Option<&DeliveryHandoff>,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<Self, SalesError> {
        order.validate_for_command(context)?;
        if order.status != SalesOrderStatus::Confirmed
            && order.status != SalesOrderStatus::Fulfillment
            && order.status != SalesOrderStatus::Completed
        {
            return Err(SalesError::OrderNotConfirmed);
        }
        if let Some(delivery) = delivery_handoff {
            delivery.validate_for_command(context)?;
            if delivery.scope != order.scope || delivery.sales_order_id != order.id {
                return Err(SalesError::CustomerBindingMismatch);
            }
            if delivery.status != HandoffStatus::HandedOff {
                return Err(SalesError::DeliveryNotHandedOff);
            }
        }
        let (reference, idempotency_key) =
            identity_fields(SalesAggregateKind::InvoiceHandoff, id, context)?;
        let value = Self {
            id,
            reference,
            idempotency_key,
            command_id: context.command_id,
            scope: order.scope.clone(),
            holding_id: order.holding_id,
            company_id: order.company_id,
            branch_id: order.branch_id,
            sales_order_id: order.id,
            sales_order_reference: order.reference.clone(),
            delivery_handoff_id: delivery_handoff.map(|handoff| handoff.id),
            currency: order.currency.clone(),
            amount_due_minor: order.total_minor,
            status: HandoffStatus::Pending,
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), SalesError> {
        validate_identity(
            SalesAggregateKind::InvoiceHandoff,
            self.id,
            &self.reference,
            &self.idempotency_key,
            self.command_id,
        )?;
        validate_scope_binding(
            &self.scope,
            self.holding_id,
            self.company_id,
            self.branch_id,
        )?;
        require_id(self.sales_order_id, "sales_order_id")?;
        if let Some(delivery_id) = self.delivery_handoff_id {
            require_id(delivery_id, "delivery_handoff_id")?;
        }
        bounded(
            &self.sales_order_reference,
            "sales_order_reference",
            MAX_SALES_REFERENCE_BYTES,
        )?;
        validate_currency(&self.currency)?;
        if self.amount_due_minor <= 0 {
            return Err(SalesError::NonPositive {
                field: "amount_due_minor",
            });
        }
        if self.updated_at < self.created_at {
            return Err(SalesError::InvalidWindow);
        }
        Ok(())
    }

    pub fn transition(
        &mut self,
        next: HandoffStatus,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<(), SalesError> {
        self.validate_for_command(context)?;
        if !self.status.can_transition_to(next) {
            return Err(invalid_transition("invoice_handoff", self.status, next));
        }
        if at < self.updated_at {
            return Err(SalesError::InvalidWindow);
        }
        let idempotency_key =
            deterministic_idempotency_key(SalesAggregateKind::InvoiceHandoff, context)?;
        let mut candidate = self.clone();
        candidate.command_id = context.command_id;
        candidate.idempotency_key = idempotency_key;
        candidate.status = next;
        candidate.updated_at = at;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn validate_for_command(&self, context: &CommandContext) -> Result<(), SalesError> {
        self.validate()?;
        validate_command_scope(context, &self.scope)
    }
    pub fn validate_against_order_and_delivery(
        &self,
        order: &SalesOrder,
        delivery: Option<&DeliveryHandoff>,
    ) -> Result<(), SalesError> {
        self.validate()?;
        order.validate()?;
        if matches!(
            order.status,
            SalesOrderStatus::Draft | SalesOrderStatus::Cancelled
        ) || self.scope != order.scope
            || self.holding_id != order.holding_id
            || self.company_id != order.company_id
            || self.branch_id != order.branch_id
            || self.sales_order_id != order.id
            || self.sales_order_reference != order.reference
            || self.currency != order.currency
            || self.amount_due_minor != order.total_minor
        {
            return Err(workflow_linkage_mismatch("invoice_handoff", "sales_order"));
        }
        match (self.delivery_handoff_id, delivery) {
            (None, None) => Ok(()),
            (Some(delivery_id), Some(delivery)) if delivery_id == delivery.id => {
                if delivery.status != HandoffStatus::HandedOff {
                    return Err(SalesError::DeliveryNotHandedOff);
                }
                delivery.validate_against_order(order)
            }
            _ => Err(workflow_linkage_mismatch(
                "invoice_handoff",
                "delivery_handoff",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectionStatus {
    Unpaid,
    PartiallyPaid,
    Paid,
    WrittenOff,
}

impl CollectionStatus {
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::Unpaid,
                Self::PartiallyPaid | Self::Paid | Self::WrittenOff
            ) | (Self::PartiallyPaid, Self::Paid | Self::WrittenOff)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionState {
    pub id: Uuid,
    pub reference: String,
    pub idempotency_key: String,
    pub command_id: Uuid,
    pub scope: TenantScope,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub invoice_handoff_id: Uuid,
    pub invoice_reference: String,
    pub currency: String,
    pub amount_due_minor: i64,
    pub amount_collected_minor: i64,
    pub status: CollectionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl CollectionState {
    pub fn new_from_invoice(
        id: Uuid,
        invoice: &InvoiceHandoff,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<Self, SalesError> {
        invoice.validate_for_command(context)?;
        if invoice.status != HandoffStatus::HandedOff {
            return Err(SalesError::InvoiceNotHandedOff);
        }
        let (reference, idempotency_key) =
            identity_fields(SalesAggregateKind::CollectionState, id, context)?;
        let value = Self {
            id,
            reference,
            idempotency_key,
            command_id: context.command_id,
            scope: invoice.scope.clone(),
            holding_id: invoice.holding_id,
            company_id: invoice.company_id,
            branch_id: invoice.branch_id,
            invoice_handoff_id: invoice.id,
            invoice_reference: invoice.reference.clone(),
            currency: invoice.currency.clone(),
            amount_due_minor: invoice.amount_due_minor,
            amount_collected_minor: 0,
            status: CollectionStatus::Unpaid,
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), SalesError> {
        validate_identity(
            SalesAggregateKind::CollectionState,
            self.id,
            &self.reference,
            &self.idempotency_key,
            self.command_id,
        )?;
        validate_scope_binding(
            &self.scope,
            self.holding_id,
            self.company_id,
            self.branch_id,
        )?;
        require_id(self.invoice_handoff_id, "invoice_handoff_id")?;
        bounded(
            &self.invoice_reference,
            "invoice_reference",
            MAX_SALES_REFERENCE_BYTES,
        )?;
        validate_currency(&self.currency)?;
        if self.amount_due_minor <= 0 {
            return Err(SalesError::NonPositive {
                field: "amount_due_minor",
            });
        }
        if self.amount_collected_minor < 0 {
            return Err(SalesError::Negative {
                field: "amount_collected_minor",
            });
        }
        if self.amount_collected_minor > self.amount_due_minor {
            return Err(SalesError::Overpayment);
        }
        let expected_status = if self.amount_collected_minor == 0 {
            CollectionStatus::Unpaid
        } else if self.amount_collected_minor == self.amount_due_minor {
            CollectionStatus::Paid
        } else {
            CollectionStatus::PartiallyPaid
        };
        if self.status != expected_status && self.status != CollectionStatus::WrittenOff {
            return Err(SalesError::InvalidDeterministicField {
                field: "collection_status",
            });
        }
        if self.updated_at < self.created_at {
            return Err(SalesError::InvalidWindow);
        }
        Ok(())
    }

    pub fn record_payment(
        &mut self,
        amount_minor: i64,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<(), SalesError> {
        self.validate_for_command(context)?;
        if amount_minor <= 0 {
            return Err(SalesError::InvalidPayment);
        }
        if matches!(
            self.status,
            CollectionStatus::Paid | CollectionStatus::WrittenOff
        ) {
            return Err(invalid_transition(
                "collection_state",
                self.status,
                CollectionStatus::Paid,
            ));
        }
        let collected = self
            .amount_collected_minor
            .checked_add(amount_minor)
            .ok_or(SalesError::AmountOverflow)?;
        if collected > self.amount_due_minor {
            return Err(SalesError::Overpayment);
        }
        if at < self.updated_at {
            return Err(SalesError::InvalidWindow);
        }
        let idempotency_key =
            deterministic_idempotency_key(SalesAggregateKind::CollectionState, context)?;
        let mut candidate = self.clone();
        candidate.command_id = context.command_id;
        candidate.idempotency_key = idempotency_key;
        candidate.amount_collected_minor = collected;
        candidate.status = if collected == self.amount_due_minor {
            CollectionStatus::Paid
        } else {
            CollectionStatus::PartiallyPaid
        };
        candidate.updated_at = at;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn write_off(
        &mut self,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<(), SalesError> {
        self.transition(CollectionStatus::WrittenOff, context, at)
    }

    pub fn transition(
        &mut self,
        next: CollectionStatus,
        context: &CommandContext,
        at: DateTime<Utc>,
    ) -> Result<(), SalesError> {
        self.validate_for_command(context)?;
        if !self.status.can_transition_to(next) {
            return Err(invalid_transition("collection_state", self.status, next));
        }
        if next == CollectionStatus::Paid && self.amount_collected_minor != self.amount_due_minor {
            return Err(SalesError::InvalidDeterministicField {
                field: "amount_collected_minor",
            });
        }
        if at < self.updated_at {
            return Err(SalesError::InvalidWindow);
        }
        let idempotency_key =
            deterministic_idempotency_key(SalesAggregateKind::CollectionState, context)?;
        let mut candidate = self.clone();
        candidate.command_id = context.command_id;
        candidate.idempotency_key = idempotency_key;
        candidate.status = next;
        candidate.updated_at = at;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn validate_for_command(&self, context: &CommandContext) -> Result<(), SalesError> {
        self.validate()?;
        validate_command_scope(context, &self.scope)
    }
    pub fn validate_against_invoice(&self, invoice: &InvoiceHandoff) -> Result<(), SalesError> {
        self.validate()?;
        invoice.validate()?;
        if invoice.status != HandoffStatus::HandedOff {
            return Err(SalesError::InvoiceNotHandedOff);
        }
        if self.scope != invoice.scope
            || self.holding_id != invoice.holding_id
            || self.company_id != invoice.company_id
            || self.branch_id != invoice.branch_id
            || self.invoice_handoff_id != invoice.id
            || self.invoice_reference != invoice.reference
            || self.currency != invoice.currency
            || self.amount_due_minor != invoice.amount_due_minor
        {
            return Err(workflow_linkage_mismatch(
                "collection_state",
                "invoice_handoff",
            ));
        }
        Ok(())
    }
}

macro_rules! impl_sales_save_validation {
    ($aggregate:ty, $kind:expr) => {
        impl $aggregate {
            /// Validate this aggregate at the generic save transport boundary.
            ///
            /// Unlike `validate_for_command`, this method requires the
            /// materialized aggregate to carry the exact command identity and
            /// idempotency context supplied by the caller.
            pub fn validate_for_save(&self, context: &CommandContext) -> Result<(), SalesError> {
                self.validate()?;
                validate_save_command_binding(
                    $kind,
                    self.command_id,
                    &self.idempotency_key,
                    &self.scope,
                    context,
                )
            }
        }
    };
}

impl_sales_save_validation!(Lead, SalesAggregateKind::Lead);
impl_sales_save_validation!(Opportunity, SalesAggregateKind::Opportunity);
impl_sales_save_validation!(Quote, SalesAggregateKind::Quote);
impl_sales_save_validation!(SalesOrder, SalesAggregateKind::SalesOrder);
impl_sales_save_validation!(DeliveryHandoff, SalesAggregateKind::DeliveryHandoff);
impl_sales_save_validation!(InvoiceHandoff, SalesAggregateKind::InvoiceHandoff);
impl_sales_save_validation!(CollectionState, SalesAggregateKind::CollectionState);

fn validate_customer(
    customer: Option<&CustomerMasterReference>,
    scope: &TenantScope,
    holding_id: Uuid,
    company_id: Uuid,
) -> Result<(), SalesError> {
    let Some(customer) = customer else {
        return Ok(());
    };
    customer.validate()?;
    if customer.holding_id != holding_id || customer.company_id != company_id {
        return Err(SalesError::CustomerBindingMismatch);
    }
    if !customer.scope.contains(scope) {
        return Err(SalesError::CustomerScopeOutside);
    }
    Ok(())
}

fn identity_fields(
    kind: SalesAggregateKind,
    id: Uuid,
    context: &CommandContext,
) -> Result<(String, String), SalesError> {
    require_id(id, "aggregate_id")?;
    let idempotency_key = deterministic_idempotency_key(kind, context)?;
    Ok((deterministic_reference(kind, id), idempotency_key))
}

fn validate_identity(
    kind: SalesAggregateKind,
    id: Uuid,
    reference: &str,
    idempotency_key: &str,
    command_id: Uuid,
) -> Result<(), SalesError> {
    require_id(id, "aggregate_id")?;
    require_id(command_id, "command_id")?;
    bounded(reference, "reference", MAX_SALES_REFERENCE_BYTES)?;
    if reference != deterministic_reference(kind, id) {
        return Err(SalesError::InvalidDeterministicField { field: "reference" });
    }
    bounded(
        idempotency_key,
        "idempotency_key",
        MAX_SALES_IDEMPOTENCY_BYTES,
    )?;
    if !idempotency_key.starts_with(&format!("sales/{}/", kind.as_str())) {
        return Err(SalesError::InvalidDeterministicField {
            field: "idempotency_key",
        });
    }
    Ok(())
}

fn compatible_scopes(left: &TenantScope, right: &TenantScope) -> bool {
    left.contains(right) || right.contains(left)
}

fn workflow_linkage_mismatch(
    aggregate_type: &'static str,
    related_type: &'static str,
) -> SalesError {
    SalesError::WorkflowLinkageMismatch {
        aggregate_type,
        related_type,
    }
}

fn validate_context(context: &CommandContext) -> Result<(), SalesError> {
    context
        .validate()
        .map_err(|error| SalesError::InvalidCommandContext(error.to_string()))?;
    require_id(context.command_id, "command_id")?;
    require_id(context.correlation_id, "correlation_id")?;
    bounded(
        &context.idempotency_key,
        "command_idempotency_key",
        MAX_SALES_IDEMPOTENCY_BYTES,
    )
}

fn validate_command_scope(
    context: &CommandContext,
    record_scope: &TenantScope,
) -> Result<(), SalesError> {
    validate_context(context)?;
    if context.tenant_scope.contains(record_scope) {
        Ok(())
    } else {
        Err(SalesError::CommandScopeOutside {
            command_scope: Box::new(context.tenant_scope.clone()),
            record_scope: Box::new(record_scope.clone()),
        })
    }
}

fn validate_scope_binding(
    scope: &TenantScope,
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Option<Uuid>,
) -> Result<(), SalesError> {
    require_id(holding_id, "holding_id")?;
    require_id(company_id, "company_id")?;
    if let Some(branch_id) = branch_id {
        require_id(branch_id, "branch_id")?;
    }
    match (scope, branch_id) {
        (
            TenantScope::Company {
                holding_id: scoped_holding,
                company_id: scoped_company,
            },
            None,
        ) if *scoped_holding == holding_id && *scoped_company == company_id => Ok(()),
        (
            TenantScope::Branch {
                holding_id: scoped_holding,
                company_id: scoped_company,
                branch_id: scoped_branch,
            },
            Some(branch),
        ) if *scoped_holding == holding_id
            && *scoped_company == company_id
            && *scoped_branch == branch =>
        {
            Ok(())
        }
        _ => Err(SalesError::InvalidScope),
    }
}

fn total_lines(lines: &[SalesLine]) -> Result<i64, SalesError> {
    if lines.is_empty() {
        return Err(SalesError::EmptyLines);
    }
    if lines.len() > MAX_SALES_LINES {
        return Err(SalesError::TooLong {
            field: "lines",
            max: MAX_SALES_LINES,
        });
    }
    let mut seen = BTreeSet::new();
    lines.iter().try_fold(0_i64, |total, line| {
        line.validate()?;
        if !seen.insert(line.line_id) {
            return Err(SalesError::DuplicateLineId(line.line_id));
        }
        total
            .checked_add(line.total_minor()?)
            .ok_or(SalesError::AmountOverflow)
    })
}

fn validate_currency(currency: &str) -> Result<(), SalesError> {
    if currency.len() != 3
        || !currency
            .chars()
            .all(|character| character.is_ascii_uppercase())
    {
        return Err(SalesError::InvalidCurrency);
    }
    Ok(())
}

fn invalid_transition<T: std::fmt::Debug>(
    aggregate_type: &'static str,
    from: T,
    to: T,
) -> SalesError {
    SalesError::InvalidTransition {
        aggregate_type,
        from: format!("{from:?}"),
        to: format!("{to:?}"),
    }
}

fn require_id(value: Uuid, field: &'static str) -> Result<(), SalesError> {
    if value.is_nil() {
        Err(SalesError::NilIdentifier { field })
    } else {
        Ok(())
    }
}

fn bounded(value: &str, field: &'static str, max: usize) -> Result<(), SalesError> {
    if value.trim().is_empty() {
        return Err(SalesError::Required { field });
    }
    if value.len() > max {
        return Err(SalesError::TooLong { field, max });
    }
    if value.chars().any(char::is_control) {
        return Err(SalesError::ControlCharacter { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{ActorAttribution, ActorKind};
    use chrono::Duration;

    fn company_scope() -> TenantScope {
        TenantScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        }
    }

    fn branch_scope(branch_id: u128) -> TenantScope {
        TenantScope::Branch {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
            branch_id: Uuid::from_u128(branch_id),
        }
    }

    fn context(scope: TenantScope, key: &str) -> CommandContext {
        CommandContext::try_new(
            key,
            ActorAttribution {
                principal_ref: "sales-test-user".into(),
                kind: ActorKind::TenantUser,
                session_id: Some(Uuid::from_u128(90)),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope,
            "sales unit test",
        )
        .expect("valid command context")
    }

    fn customer(scope: TenantScope, branch_id: Option<Uuid>) -> CustomerMasterReference {
        CustomerMasterReference::new(
            Uuid::from_u128(10),
            scope,
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            branch_id,
        )
        .expect("valid customer reference")
    }

    fn line(line_id: u128, price: i64) -> SalesLine {
        SalesLine {
            line_id: Uuid::from_u128(line_id),
            item_reference: format!("SKU-{line_id}"),
            description: "Item".into(),
            quantity: 2,
            unit_price_minor: price,
            tax_minor: 0,
        }
    }

    #[test]
    fn rejects_platform_scope_and_unbound_branch_scope() {
        let invalid_customer = CustomerMasterReference::new(
            Uuid::from_u128(10),
            TenantScope::Platform,
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            None,
        );
        assert_eq!(invalid_customer, Err(SalesError::InvalidScope));

        let invalid_branch = CustomerMasterReference::new(
            Uuid::from_u128(10),
            branch_scope(3),
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Some(Uuid::from_u128(4)),
        );
        assert_eq!(invalid_branch, Err(SalesError::InvalidScope));
    }

    #[test]
    fn branch_scope_is_contained_by_company_command_but_not_sibling_branch() {
        let company_context = context(company_scope(), "lead-branch-1");
        let lead = Lead::new(
            Uuid::from_u128(11),
            branch_scope(3),
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Some(Uuid::from_u128(3)),
            Some(customer(company_scope(), None)),
            "Branch lead",
            "web",
            Utc::now(),
            &company_context,
        )
        .expect("company scope contains branch");
        assert!(lead.validate_for_command(&company_context).is_ok());

        let sibling_context = context(branch_scope(4), "lead-branch-2");
        assert!(matches!(
            lead.validate_for_command(&sibling_context),
            Err(SalesError::CommandScopeOutside { .. })
        ));
    }

    #[test]
    fn deterministic_reference_and_idempotency_are_bound_to_kind_and_command() {
        let context = context(company_scope(), "lead-create-1");
        let lead = Lead::new(
            Uuid::from_u128(11),
            company_scope(),
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            None,
            None,
            "Qualified later",
            "referral",
            Utc::now(),
            &context,
        )
        .expect("lead");
        assert_eq!(
            lead.reference,
            deterministic_reference(SalesAggregateKind::Lead, lead.id)
        );
        assert_eq!(lead.idempotency_key, "sales/lead/lead-create-1".to_owned());
        assert!(lead.validate().is_ok());

        let mut tampered = lead.clone();
        tampered.reference = "LEAD-MANUAL".into();
        assert!(matches!(
            tampered.validate(),
            Err(SalesError::InvalidDeterministicField { field: "reference" })
        ));
    }

    #[test]
    fn generic_save_binding_requires_exact_command_identity_and_idempotency() {
        let command_context = context(company_scope(), "lead-save-boundary");
        let lead = Lead::new(
            Uuid::from_u128(13),
            branch_scope(3),
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Some(Uuid::from_u128(3)),
            Some(customer(company_scope(), None)),
            "Save boundary lead",
            "import",
            Utc::now(),
            &command_context,
        )
        .expect("lead");

        assert!(lead.validate_for_save(&command_context).is_ok());

        let mut different_command = command_context.clone();
        different_command.command_id = Uuid::from_u128(14);
        assert!(matches!(
            lead.validate_for_save(&different_command),
            Err(SalesError::CommandIdentityMismatch { .. })
        ));

        let mut different_key = command_context.clone();
        different_key.idempotency_key = "lead-save-replay-with-different-key".into();
        assert!(matches!(
            lead.validate_for_save(&different_key),
            Err(SalesError::CommandIdempotencyMismatch { .. })
        ));

        let sibling_context = context(branch_scope(4), "lead-save-sibling");
        assert!(matches!(
            lead.validate_for_save(&sibling_context),
            Err(SalesError::CommandScopeOutside { .. })
        ));
    }

    #[test]
    fn generic_save_binding_rechecks_nested_customer_scope() {
        let command_context = context(company_scope(), "opportunity-save-relation");
        let mut opportunity = Opportunity::new(
            Uuid::from_u128(15),
            branch_scope(3),
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Some(Uuid::from_u128(3)),
            None,
            customer(company_scope(), None),
            "Scoped opportunity",
            "THB",
            500,
            Utc::now(),
            &command_context,
        )
        .expect("opportunity");
        opportunity.customer.scope = branch_scope(4);
        opportunity.customer.branch_id = Some(Uuid::from_u128(4));

        assert_eq!(
            opportunity.validate_for_save(&command_context),
            Err(SalesError::CustomerScopeOutside)
        );
    }

    #[test]
    fn lifecycle_mutation_rotates_command_identity_for_the_next_write_version() {
        let create_context = context(company_scope(), "lead-create-identity");
        let mut lead = Lead::new(
            Uuid::from_u128(12),
            company_scope(),
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            None,
            None,
            "Identity rotation",
            "referral",
            Utc::now(),
            &create_context,
        )
        .expect("lead");
        let creation_command_id = lead.command_id;
        let creation_key = lead.idempotency_key.clone();
        let update_context = context(company_scope(), "lead-qualify-identity");

        lead.transition(
            LeadStatus::Qualified,
            &update_context,
            lead.updated_at + Duration::minutes(1),
        )
        .expect("qualify lead");

        assert_ne!(lead.command_id, creation_command_id);
        assert_ne!(lead.idempotency_key, creation_key);
        assert_eq!(
            lead.idempotency_key,
            deterministic_idempotency_key(SalesAggregateKind::Lead, &update_context)
                .expect("update idempotency key")
        );
        assert!(lead.validate().is_ok());
    }

    #[test]
    fn lifecycle_boundaries_require_ordered_handoffs_and_payment_completion() {
        let create_context = context(company_scope(), "quote-create");
        let at = Utc::now();
        let mut quote = Quote::new(
            Uuid::from_u128(20),
            company_scope(),
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            None,
            None,
            customer(company_scope(), None),
            "THB",
            vec![line(21, 500)],
            at + Duration::days(7),
            at,
            &create_context,
        )
        .expect("quote");
        assert!(matches!(
            quote.transition(QuoteStatus::Accepted, &create_context, at),
            Err(SalesError::InvalidTransition { .. })
        ));
        quote
            .transition(QuoteStatus::Sent, &create_context, at)
            .expect("send quote");
        quote
            .transition(QuoteStatus::Accepted, &create_context, at)
            .expect("accept quote");

        let order_context = context(company_scope(), "order-create");
        let mut order = SalesOrder::new_from_quote(
            Uuid::from_u128(30),
            &quote,
            &order_context,
            at + Duration::minutes(1),
        )
        .expect("order from accepted quote");
        assert!(matches!(
            DeliveryHandoff::new_from_order(Uuid::from_u128(31), &order, &order_context, at),
            Err(SalesError::OrderNotConfirmed)
        ));
        order
            .transition(
                SalesOrderStatus::Confirmed,
                &order_context,
                at + Duration::minutes(1),
            )
            .expect("confirm order");
        let mut delivery = DeliveryHandoff::new_from_order(
            Uuid::from_u128(31),
            &order,
            &order_context,
            at + Duration::minutes(2),
        )
        .expect("delivery handoff");
        assert_eq!(
            InvoiceHandoff::new_from_order(
                Uuid::from_u128(32),
                &order,
                Some(&delivery),
                &order_context,
                at + Duration::minutes(2),
            ),
            Err(SalesError::DeliveryNotHandedOff)
        );
        delivery
            .transition(
                HandoffStatus::Requested,
                &order_context,
                at + Duration::minutes(2),
            )
            .expect("request delivery");
        delivery
            .transition(
                HandoffStatus::HandedOff,
                &order_context,
                at + Duration::minutes(3),
            )
            .expect("hand off delivery");

        let mut invoice = InvoiceHandoff::new_from_order(
            Uuid::from_u128(32),
            &order,
            Some(&delivery),
            &order_context,
            at + Duration::minutes(3),
        )
        .expect("invoice handoff");
        invoice
            .transition(
                HandoffStatus::Requested,
                &order_context,
                at + Duration::minutes(4),
            )
            .expect("request invoice");
        invoice
            .transition(
                HandoffStatus::HandedOff,
                &order_context,
                at + Duration::minutes(5),
            )
            .expect("hand off invoice");

        let mut collection = CollectionState::new_from_invoice(
            Uuid::from_u128(33),
            &invoice,
            &order_context,
            at + Duration::minutes(4),
        )
        .expect("collection state");
        collection
            .record_payment(500, &order_context, at + Duration::minutes(6))
            .expect("partial payment");
        assert_eq!(collection.status, CollectionStatus::PartiallyPaid);
        assert!(matches!(
            collection.record_payment(501, &order_context, at + Duration::minutes(6)),
            Err(SalesError::Overpayment)
        ));
        collection
            .record_payment(500, &order_context, at + Duration::minutes(7))
            .expect("final payment");
        assert_eq!(collection.status, CollectionStatus::Paid);
        assert!(matches!(
            collection.transition(
                CollectionStatus::Unpaid,
                &order_context,
                at + Duration::minutes(7),
            ),
            Err(SalesError::InvalidTransition { .. })
        ));
    }

    #[test]
    fn workflow_linkage_rejects_tampered_quote_snapshot() {
        let at = Utc::now();
        let scope = company_scope();
        let quote_context = context(scope.clone(), "workflow-quote");
        let mut quote = Quote::new(
            Uuid::from_u128(200),
            scope.clone(),
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            None,
            None,
            customer(scope.clone(), None),
            "THB",
            vec![line(201, 125)],
            at + Duration::days(1),
            at,
            &quote_context,
        )
        .expect("quote");
        quote
            .transition(
                QuoteStatus::Sent,
                &context(scope.clone(), "workflow-quote-send"),
                at,
            )
            .expect("send quote");
        quote
            .transition(
                QuoteStatus::Accepted,
                &context(scope.clone(), "workflow-quote-accept"),
                at,
            )
            .expect("accept quote");
        let order = SalesOrder::new_from_quote(
            Uuid::from_u128(202),
            &quote,
            &context(scope, "workflow-order"),
            at,
        )
        .expect("order");
        assert!(order.validate_against_quote(&quote).is_ok());

        let mut tampered = order.clone();
        tampered.quote_reference = "QUOTE-TAMPERED".into();
        assert!(matches!(
            tampered.validate_against_quote(&quote),
            Err(SalesError::WorkflowLinkageMismatch {
                aggregate_type: "sales_order",
                related_type: "quote"
            })
        ));
    }

    #[test]
    fn quote_acceptance_after_validity_window_is_rejected_without_mutation() {
        let at = Utc::now();
        let validity_end = at + Duration::hours(1);
        let create_context = context(company_scope(), "quote-expiry-create");
        let mut quote = Quote::new(
            Uuid::from_u128(50),
            company_scope(),
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            None,
            None,
            customer(company_scope(), None),
            "THB",
            vec![line(51, 750)],
            validity_end,
            at,
            &create_context,
        )
        .expect("quote");
        quote
            .transition(
                QuoteStatus::Sent,
                &context(company_scope(), "quote-expiry-send"),
                at + Duration::minutes(1),
            )
            .expect("send quote");

        let before = quote.clone();
        let result = quote.transition(
            QuoteStatus::Accepted,
            &context(company_scope(), "quote-expiry-accept"),
            validity_end + Duration::seconds(1),
        );

        assert_eq!(result, Err(SalesError::QuoteExpired));
        assert_eq!(quote, before);
    }

    #[test]
    fn customer_branch_reference_cannot_cross_to_a_sibling_branch() {
        let context = context(branch_scope(3), "opp-branch");
        let result = Opportunity::new(
            Uuid::from_u128(40),
            branch_scope(3),
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Some(Uuid::from_u128(3)),
            None,
            customer(branch_scope(4), Some(Uuid::from_u128(4))),
            "Wrong branch",
            "THB",
            100,
            Utc::now(),
            &context,
        );
        assert_eq!(result, Err(SalesError::CustomerScopeOutside));
    }

    #[test]
    fn projection_reconciliation_is_green_only_without_mismatches() {
        assert!(SalesProjectionReconciliation::default().is_consistent());
        assert!(
            !SalesProjectionReconciliation {
                stale_aggregate_count: 1,
                ..SalesProjectionReconciliation::default()
            }
            .is_consistent()
        );
        assert!(
            !SalesProjectionReconciliation {
                invalid_projection_count: 1,
                ..SalesProjectionReconciliation::default()
            }
            .is_consistent()
        );
        assert!(
            !SalesProjectionReconciliation {
                workflow_linkage_error_count: 1,
                ..SalesProjectionReconciliation::default()
            }
            .is_consistent()
        );
    }

    #[test]
    fn lifecycle_rejects_regressing_time_without_mutating_the_aggregate() {
        let created_at = Utc::now();
        let scope = company_scope();
        let create_context = context(scope.clone(), "lead-window-create");
        let mut lead = Lead::new(
            Uuid::from_u128(80),
            scope.clone(),
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            None,
            None,
            "Window test",
            "web",
            created_at,
            &create_context,
        )
        .expect("lead");
        let before = lead.clone();
        let result = lead.transition(
            LeadStatus::Qualified,
            &context(scope, "lead-window-update"),
            created_at - Duration::seconds(1),
        );
        assert_eq!(result, Err(SalesError::InvalidWindow));
        assert_eq!(lead, before);
    }
}
