use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use erpx_integration_contracts::{crm, customer};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_LINES: usize = 10_000;
pub const MAX_CODE_BYTES: usize = 128;
pub const MAX_TEXT_BYTES: usize = 255;

macro_rules! sales_id {
    ($name:ident, $field:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, SalesDomainError> {
                if value.is_nil() {
                    return Err(SalesDomainError::NilIdentifier { field: $field });
                }
                Ok(Self(value))
            }

            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl TryFrom<Uuid> for $name {
            type Error = SalesDomainError;

            fn try_from(value: Uuid) -> Result<Self, Self::Error> {
                Self::try_from_uuid(value)
            }
        }

        impl From<$name> for Uuid {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

sales_id!(QuoteId, "quote_id");
sales_id!(SalesOrderId, "sales_order_id");
sales_id!(SalesOrderLineId, "sales_order_line_id");

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SalesDomainError {
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("Sales records require an exact company or branch scope")]
    InvalidScope,
    #[error("{field} is required or exceeds its safe length")]
    InvalidText { field: &'static str },
    #[error("currency must be a three-letter uppercase ASCII code")]
    InvalidCurrency,
    #[error("Sales documents require between one and 10,000 lines")]
    InvalidLineCount,
    #[error("Sales line identifiers must be unique")]
    DuplicateLine,
    #[error("Sales quantities must be positive and prices cannot be negative")]
    InvalidAmount,
    #[error("all Sales lines must use the document currency")]
    CurrencyMismatch,
    #[error("Sales amount arithmetic overflowed")]
    ArithmeticOverflow,
    #[error("a quote can be created only from a won CRM opportunity")]
    OpportunityNotWon,
    #[error("the CRM opportunity currency and quote currency differ")]
    OpportunityCurrencyMismatch,
    #[error("quote validity must end after its creation time")]
    InvalidValidityWindow,
    #[error("quote lifecycle transition is not allowed")]
    InvalidQuoteTransition,
    #[error("quote validity has expired")]
    QuoteExpired,
    #[error("only an accepted quote can become a sales order")]
    QuoteNotAccepted,
    #[error("sales order lifecycle transition is not allowed")]
    InvalidOrderTransition,
    #[error("Sales timestamp cannot move backwards")]
    InvalidTimestamp,
    #[error("Sales aggregate version overflowed")]
    VersionOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesOrderLine {
    pub line_id: SalesOrderLineId,
    pub item_code: String,
    pub description: String,
    pub quantity: i64,
    pub unit_price_minor: i64,
    pub tax_minor: i64,
    pub currency: String,
}

impl SalesOrderLine {
    pub fn validate(&self) -> Result<(), SalesDomainError> {
        validate_code(&self.item_code, "item_code")?;
        validate_text(&self.description, "description")?;
        validate_currency(&self.currency)?;
        if self.quantity <= 0 || self.unit_price_minor < 0 || self.tax_minor < 0 {
            return Err(SalesDomainError::InvalidAmount);
        }
        self.line_value_minor()?;
        Ok(())
    }

    pub fn line_value_minor(&self) -> Result<i64, SalesDomainError> {
        self.quantity
            .checked_mul(self.unit_price_minor)
            .and_then(|subtotal| subtotal.checked_add(self.tax_minor))
            .ok_or(SalesDomainError::ArithmeticOverflow)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DomainEvent {
    QuoteCreated,
    QuoteChanged,
    OrderChanged { state: SalesOrderFactState },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SalesOrderFactState {
    pub sales_order_id: SalesOrderId,
    pub order_code: String,
    pub customer: customer::CustomerRef,
    pub currency: String,
    pub lines: Vec<SalesOrderLine>,
    pub total_minor: i64,
    pub status: SalesOrderStatus,
}

/// Sales-owned commercial offer. It retains only CRM's published identifiers
/// and values; no CRM aggregate is reachable through this type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quote {
    id: QuoteId,
    version: u64,
    scope: TenantScope,
    opportunity: crm::OpportunityRef,
    customer: customer::CustomerRef,
    quote_code: String,
    currency: String,
    lines: Vec<SalesOrderLine>,
    total_minor: i64,
    valid_until: DateTime<Utc>,
    status: QuoteStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl Quote {
    #[allow(clippy::too_many_arguments)]
    pub fn create_from_won_opportunity(
        id: QuoteId,
        scope: TenantScope,
        opportunity: crm::OpportunityFact,
        quote_code: String,
        currency: String,
        lines: Vec<SalesOrderLine>,
        valid_until: DateTime<Utc>,
        at: DateTime<Utc>,
    ) -> Result<Self, SalesDomainError> {
        if opportunity.stage != crm::OpportunityStage::Won {
            return Err(SalesDomainError::OpportunityNotWon);
        }
        if opportunity.expected_value.currency().as_str() != currency {
            return Err(SalesDomainError::OpportunityCurrencyMismatch);
        }
        let total_minor = validate_lines(&lines, &currency)?;
        let value = Self {
            id,
            version: 1,
            scope,
            opportunity: opportunity.opportunity,
            customer: opportunity.customer,
            quote_code,
            currency,
            lines,
            total_minor,
            valid_until,
            status: QuoteStatus::Draft,
            created_at: at,
            updated_at: at,
            uncommitted_events: vec![DomainEvent::QuoteCreated],
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), SalesDomainError> {
        validate_scope(&self.scope)?;
        validate_code(&self.quote_code, "quote_code")?;
        validate_currency(&self.currency)?;
        let total = validate_lines(&self.lines, &self.currency)?;
        if total != self.total_minor {
            return Err(SalesDomainError::ArithmeticOverflow);
        }
        if self.valid_until <= self.created_at {
            return Err(SalesDomainError::InvalidValidityWindow);
        }
        if self.updated_at < self.created_at || self.version == 0 {
            return Err(SalesDomainError::InvalidTimestamp);
        }
        Ok(())
    }

    pub fn send(&mut self, at: DateTime<Utc>) -> Result<(), SalesDomainError> {
        self.transition(QuoteStatus::Draft, QuoteStatus::Sent, at)
    }

    pub fn accept(&mut self, at: DateTime<Utc>) -> Result<(), SalesDomainError> {
        if at > self.valid_until {
            return Err(SalesDomainError::QuoteExpired);
        }
        self.transition(QuoteStatus::Sent, QuoteStatus::Accepted, at)
    }

    pub fn reject(&mut self, at: DateTime<Utc>) -> Result<(), SalesDomainError> {
        self.transition(QuoteStatus::Sent, QuoteStatus::Rejected, at)
    }

    pub fn expire(&mut self, at: DateTime<Utc>) -> Result<(), SalesDomainError> {
        if at <= self.valid_until {
            return Err(SalesDomainError::InvalidValidityWindow);
        }
        self.transition(QuoteStatus::Sent, QuoteStatus::Expired, at)
    }

    fn transition(
        &mut self,
        expected: QuoteStatus,
        next: QuoteStatus,
        at: DateTime<Utc>,
    ) -> Result<(), SalesDomainError> {
        if self.status != expected {
            return Err(SalesDomainError::InvalidQuoteTransition);
        }
        self.advance(at)?;
        self.status = next;
        self.uncommitted_events.push(DomainEvent::QuoteChanged);
        Ok(())
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), SalesDomainError> {
        if at < self.updated_at {
            return Err(SalesDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(SalesDomainError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub const fn id(&self) -> QuoteId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn opportunity(&self) -> crm::OpportunityRef {
        self.opportunity
    }
    pub const fn customer(&self) -> customer::CustomerRef {
        self.customer
    }
    pub fn quote_code(&self) -> &str {
        &self.quote_code
    }
    pub fn currency(&self) -> &str {
        &self.currency
    }
    pub fn lines(&self) -> &[SalesOrderLine] {
        &self.lines
    }
    pub const fn total_minor(&self) -> i64 {
        self.total_minor
    }
    pub const fn valid_until(&self) -> DateTime<Utc> {
        self.valid_until
    }
    pub const fn status(&self) -> QuoteStatus {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SalesOrderStatus {
    Draft,
    Confirmed,
    PartiallyFulfilled,
    Fulfilled,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesOrder {
    id: SalesOrderId,
    version: u64,
    scope: TenantScope,
    quote_id: QuoteId,
    opportunity: crm::OpportunityRef,
    customer: customer::CustomerRef,
    order_code: String,
    currency: String,
    lines: Vec<SalesOrderLine>,
    total_minor: i64,
    status: SalesOrderStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl SalesOrder {
    pub fn create_from_quote(
        id: SalesOrderId,
        quote: &Quote,
        order_code: String,
        at: DateTime<Utc>,
    ) -> Result<Self, SalesDomainError> {
        quote.validate()?;
        if quote.status != QuoteStatus::Accepted {
            return Err(SalesDomainError::QuoteNotAccepted);
        }
        let value = Self {
            id,
            version: 1,
            scope: quote.scope.clone(),
            quote_id: quote.id,
            opportunity: quote.opportunity,
            customer: quote.customer,
            order_code,
            currency: quote.currency.clone(),
            lines: quote.lines.clone(),
            total_minor: quote.total_minor,
            status: SalesOrderStatus::Draft,
            created_at: at,
            updated_at: at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), SalesDomainError> {
        validate_scope(&self.scope)?;
        validate_code(&self.order_code, "order_code")?;
        validate_currency(&self.currency)?;
        let total = validate_lines(&self.lines, &self.currency)?;
        if total != self.total_minor {
            return Err(SalesDomainError::ArithmeticOverflow);
        }
        if self.updated_at < self.created_at || self.version == 0 {
            return Err(SalesDomainError::InvalidTimestamp);
        }
        Ok(())
    }

    pub fn confirm(&mut self, at: DateTime<Utc>) -> Result<(), SalesDomainError> {
        self.transition(&[SalesOrderStatus::Draft], SalesOrderStatus::Confirmed, at)
    }

    pub fn mark_partially_fulfilled(&mut self, at: DateTime<Utc>) -> Result<(), SalesDomainError> {
        self.transition(
            &[SalesOrderStatus::Confirmed],
            SalesOrderStatus::PartiallyFulfilled,
            at,
        )
    }

    pub fn mark_fulfilled(&mut self, at: DateTime<Utc>) -> Result<(), SalesDomainError> {
        self.transition(
            &[
                SalesOrderStatus::Confirmed,
                SalesOrderStatus::PartiallyFulfilled,
            ],
            SalesOrderStatus::Fulfilled,
            at,
        )
    }

    pub fn cancel(&mut self, at: DateTime<Utc>) -> Result<(), SalesDomainError> {
        self.transition(
            &[
                SalesOrderStatus::Draft,
                SalesOrderStatus::Confirmed,
                SalesOrderStatus::PartiallyFulfilled,
            ],
            SalesOrderStatus::Cancelled,
            at,
        )
    }

    fn transition(
        &mut self,
        allowed: &[SalesOrderStatus],
        next: SalesOrderStatus,
        at: DateTime<Utc>,
    ) -> Result<(), SalesDomainError> {
        if !allowed.contains(&self.status) {
            return Err(SalesDomainError::InvalidOrderTransition);
        }
        if at < self.updated_at {
            return Err(SalesDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(SalesDomainError::VersionOverflow)?;
        self.status = next;
        self.updated_at = at;
        self.publish_changed();
        Ok(())
    }

    fn publish_changed(&mut self) {
        self.uncommitted_events.push(DomainEvent::OrderChanged {
            state: SalesOrderFactState {
                sales_order_id: self.id,
                order_code: self.order_code.clone(),
                customer: self.customer,
                currency: self.currency.clone(),
                lines: self.lines.clone(),
                total_minor: self.total_minor,
                status: self.status,
            },
        });
    }

    pub const fn id(&self) -> SalesOrderId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn quote_id(&self) -> QuoteId {
        self.quote_id
    }
    pub const fn opportunity(&self) -> crm::OpportunityRef {
        self.opportunity
    }
    pub const fn customer(&self) -> customer::CustomerRef {
        self.customer
    }
    pub fn order_code(&self) -> &str {
        &self.order_code
    }
    pub fn currency(&self) -> &str {
        &self.currency
    }
    pub fn lines(&self) -> &[SalesOrderLine] {
        &self.lines
    }
    pub const fn total_minor(&self) -> i64 {
        self.total_minor
    }
    pub const fn status(&self) -> SalesOrderStatus {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "state", rename_all = "snake_case")]
pub enum SalesAggregate {
    Quote(Quote),
    SalesOrder(SalesOrder),
}

impl SalesAggregate {
    pub const fn version(&self) -> u64 {
        match self {
            Self::Quote(value) => value.version(),
            Self::SalesOrder(value) => value.version(),
        }
    }

    pub const fn scope(&self) -> &TenantScope {
        match self {
            Self::Quote(value) => value.scope(),
            Self::SalesOrder(value) => value.scope(),
        }
    }

    pub const fn id_uuid(&self) -> Uuid {
        match self {
            Self::Quote(value) => value.id().as_uuid(),
            Self::SalesOrder(value) => value.id().as_uuid(),
        }
    }

    pub const fn aggregate_type(&self) -> &'static str {
        match self {
            Self::Quote(_) => "quote",
            Self::SalesOrder(_) => "sales_order",
        }
    }

    pub fn validate(&self) -> Result<(), SalesDomainError> {
        match self {
            Self::Quote(value) => value.validate(),
            Self::SalesOrder(value) => value.validate(),
        }
    }

    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        match self {
            Self::Quote(value) => value.take_uncommitted_events(),
            Self::SalesOrder(value) => value.take_uncommitted_events(),
        }
    }
}

fn validate_scope(scope: &TenantScope) -> Result<(), SalesDomainError> {
    scope
        .validate()
        .map_err(|_| SalesDomainError::InvalidScope)?;
    if matches!(
        scope,
        TenantScope::Company { .. } | TenantScope::Branch { .. }
    ) {
        Ok(())
    } else {
        Err(SalesDomainError::InvalidScope)
    }
}

fn validate_lines(lines: &[SalesOrderLine], currency: &str) -> Result<i64, SalesDomainError> {
    if lines.is_empty() || lines.len() > MAX_LINES {
        return Err(SalesDomainError::InvalidLineCount);
    }
    let mut ids = BTreeSet::new();
    lines.iter().try_fold(0_i64, |total, line| {
        line.validate()?;
        if !ids.insert(line.line_id) {
            return Err(SalesDomainError::DuplicateLine);
        }
        if line.currency != currency {
            return Err(SalesDomainError::CurrencyMismatch);
        }
        total
            .checked_add(line.line_value_minor()?)
            .ok_or(SalesDomainError::ArithmeticOverflow)
    })
}

fn validate_code(value: &str, field: &'static str) -> Result<(), SalesDomainError> {
    if value.trim().is_empty()
        || value.len() > MAX_CODE_BYTES
        || value.chars().any(char::is_control)
    {
        Err(SalesDomainError::InvalidText { field })
    } else {
        Ok(())
    }
}

fn validate_text(value: &str, field: &'static str) -> Result<(), SalesDomainError> {
    if value.trim().is_empty()
        || value.len() > MAX_TEXT_BYTES
        || value.chars().any(char::is_control)
    {
        Err(SalesDomainError::InvalidText { field })
    } else {
        Ok(())
    }
}

fn validate_currency(currency: &str) -> Result<(), SalesDomainError> {
    if currency.len() == 3 && currency.bytes().all(|byte| byte.is_ascii_uppercase()) {
        Ok(())
    } else {
        Err(SalesDomainError::InvalidCurrency)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};
    use erpx_integration_contracts::{CurrencyCode, MoneyAmount};

    use super::*;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 5, hour, 0, 0)
            .single()
            .unwrap()
    }

    fn scope() -> TenantScope {
        TenantScope::Company {
            holding_id: id(1),
            company_id: id(2),
        }
    }

    fn opportunity(stage: crm::OpportunityStage) -> crm::OpportunityFact {
        crm::OpportunityFact {
            opportunity: crm::OpportunityRef::try_from_uuid(id(3)).unwrap(),
            customer: customer::CustomerRef::try_from_uuid(id(4)).unwrap(),
            stage,
            expected_value: MoneyAmount::new(2_000, CurrencyCode::try_new("THB").unwrap()),
        }
    }

    fn line(value: u128) -> SalesOrderLine {
        SalesOrderLine {
            line_id: SalesOrderLineId::try_from_uuid(id(value)).unwrap(),
            item_code: format!("ITEM-{value}"),
            description: "Item".into(),
            quantity: 2,
            unit_price_minor: 900,
            tax_minor: 200,
            currency: "THB".into(),
        }
    }

    fn quote() -> Quote {
        Quote::create_from_won_opportunity(
            QuoteId::try_from_uuid(id(10)).unwrap(),
            scope(),
            opportunity(crm::OpportunityStage::Won),
            "QT-10".into(),
            "THB".into(),
            vec![line(11)],
            at(12),
            at(9),
        )
        .unwrap()
    }

    #[test]
    fn quote_requires_won_crm_fact_and_exact_record_scope() {
        let result = Quote::create_from_won_opportunity(
            QuoteId::try_from_uuid(id(10)).unwrap(),
            TenantScope::Holding { holding_id: id(1) },
            opportunity(crm::OpportunityStage::Proposal),
            "QT-10".into(),
            "THB".into(),
            vec![line(11)],
            at(12),
            at(9),
        );
        assert_eq!(result, Err(SalesDomainError::OpportunityNotWon));

        let result = Quote::create_from_won_opportunity(
            QuoteId::try_from_uuid(id(10)).unwrap(),
            TenantScope::Holding { holding_id: id(1) },
            opportunity(crm::OpportunityStage::Won),
            "QT-10".into(),
            "THB".into(),
            vec![line(11)],
            at(12),
            at(9),
        );
        assert_eq!(result, Err(SalesDomainError::InvalidScope));
    }

    #[test]
    fn quote_line_identity_currency_and_amount_invariants_are_enforced() {
        let duplicate = vec![line(11), line(11)];
        assert!(matches!(
            Quote::create_from_won_opportunity(
                QuoteId::try_from_uuid(id(10)).unwrap(),
                scope(),
                opportunity(crm::OpportunityStage::Won),
                "QT-10".into(),
                "THB".into(),
                duplicate,
                at(12),
                at(9),
            ),
            Err(SalesDomainError::DuplicateLine)
        ));
        let mut mismatched = line(12);
        mismatched.currency = "USD".into();
        assert!(matches!(
            Quote::create_from_won_opportunity(
                QuoteId::try_from_uuid(id(10)).unwrap(),
                scope(),
                opportunity(crm::OpportunityStage::Won),
                "QT-10".into(),
                "THB".into(),
                vec![mismatched],
                at(12),
                at(9),
            ),
            Err(SalesDomainError::CurrencyMismatch)
        ));
    }

    #[test]
    fn quote_acceptance_is_ordered_and_expiry_is_atomic() {
        let mut quote = quote();
        assert_eq!(
            quote.accept(at(10)),
            Err(SalesDomainError::InvalidQuoteTransition)
        );
        quote.send(at(10)).unwrap();
        let before = quote.clone();
        assert_eq!(
            quote.accept(quote.valid_until() + Duration::seconds(1)),
            Err(SalesDomainError::QuoteExpired)
        );
        assert_eq!(quote, before);
        quote.accept(at(11)).unwrap();
        assert_eq!(quote.status(), QuoteStatus::Accepted);
        assert_eq!(quote.version(), 3);
    }

    #[test]
    fn order_is_a_snapshot_of_an_accepted_quote_and_moves_forward_only() {
        let mut quote = quote();
        quote.send(at(10)).unwrap();
        assert_eq!(
            SalesOrder::create_from_quote(
                SalesOrderId::try_from_uuid(id(20)).unwrap(),
                &quote,
                "SO-20".into(),
                at(10),
            ),
            Err(SalesDomainError::QuoteNotAccepted)
        );
        quote.accept(at(11)).unwrap();
        let mut order = SalesOrder::create_from_quote(
            SalesOrderId::try_from_uuid(id(20)).unwrap(),
            &quote,
            "SO-20".into(),
            at(11),
        )
        .unwrap();
        assert_eq!(order.total_minor(), quote.total_minor());
        order.confirm(at(11)).unwrap();
        order.mark_partially_fulfilled(at(12)).unwrap();
        order.mark_fulfilled(at(12)).unwrap();
        assert_eq!(
            order.cancel(at(12)),
            Err(SalesDomainError::InvalidOrderTransition)
        );
    }

    #[test]
    fn timestamp_regression_does_not_mutate_the_quote() {
        let mut quote = quote();
        let before = quote.clone();
        assert_eq!(quote.send(at(8)), Err(SalesDomainError::InvalidTimestamp));
        assert_eq!(quote, before);
    }
}
