//! Native service-completion fulfillment facts.
//!
//! This bounded slice records completion of service order lines only. Goods
//! delivery, lot/serial allocation, Inventory movement and goods journal
//! posting remain separate retained operations.
use crate::domain::order::{
    CommerceOrderActor, OrderKind, OrderState, SalesOrderRow, parse_decimal_units, request_invalid,
    source_invalid, valid_date,
};
use erpx_server_shared::public::{BusinessError, CommandKey, RecordId, TimestampMillis};
use serde_json::{Value, json};

pub const COMPLETION_LINES_LIMIT: usize = 20;
const NOTE_MIN: usize = 3;
const NOTE_MAX: usize = 1_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceCompletionLineInput {
    pub line_id: String,
    pub quantity: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceCompletionCommand {
    pub key: CommandKey,
    pub order_id: RecordId,
    pub date: String,
    pub evidence_note: String,
    pub expected_version: i64,
    pub lines: Vec<ServiceCompletionLineInput>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceLineFact {
    pub id: String,
    pub description: String,
    pub item_type: String,
    pub quantity_milli: i64,
    pub fulfilled_milli: i64,
    pub price_minor: i64,
    pub tax_bps: i64,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub prior_subtotal_minor: i64,
    pub prior_tax_minor: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceCompletionLinePlan {
    pub id: String,
    pub order_line_id: String,
    pub description: String,
    pub quantity_milli: i64,
    pub price_minor: i64,
    pub tax_bps: i64,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub ordinal: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceCompletionPlan {
    pub id: String,
    pub order_id: String,
    pub company_id: String,
    pub number: String,
    pub date: String,
    pub note: String,
    pub expected_version: i64,
    pub currency: String,
    pub lines: Vec<ServiceCompletionLinePlan>,
    pub actor_id: String,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceCompletionReceipt {
    pub id: String,
    pub fulfillment_id: String,
    pub version: i64,
    pub state: String,
}

impl ServiceCompletionReceipt {
    pub fn to_wire(&self) -> Value {
        json!({
            "id": self.id,
            "fulfillmentId": self.fulfillment_id,
            "version": self.version,
            "state": self.state,
        })
    }
}

pub fn service_completion_identity(
    actor: &CommerceOrderActor,
    command: &ServiceCompletionCommand,
) -> String {
    json!({
        "user": actor.user_id.as_str(),
        "assignment": actor.assignment_id.as_str(),
        "company": actor.company_id.as_str(),
        "branch": actor.branch_id.as_ref().map(|value| value.as_str()),
        "department": actor.department_id.as_ref().map(|value| value.as_str()),
        "operation": "commerce.order.fulfill",
        "recordId": command.order_id.as_str(),
        "input": {
            "completionMode": "service",
            "expectedVersion": command.expected_version,
            "date": command.date,
            "evidenceNote": command.evidence_note.trim(),
            "lines": command.lines.iter().map(|line| json!({
                "lineId": line.line_id,
                "quantity": line.quantity,
            })).collect::<Vec<_>>(),
        },
    })
    .to_string()
}

pub fn service_completion_number(key: &CommandKey) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(key.as_str().as_bytes());
    let mut suffix = String::with_capacity(24);
    for byte in &digest[..12] {
        use std::fmt::Write;
        let _ = write!(suffix, "{byte:02X}");
    }
    format!("DN-{suffix}")
}

pub fn service_completion_required() -> BusinessError {
    BusinessError::invalid(
        "SERVICE_COMPLETION_REQUIRED",
        "Complete service lines through the service action.",
    )
}

pub fn over_fulfillment() -> BusinessError {
    BusinessError::invalid(
        "OVER_FULFILLMENT",
        "Quantity exceeds the remaining order quantity.",
    )
}

pub fn line_invalid() -> BusinessError {
    BusinessError::invalid("LINE_INVALID", "Order line not found.")
}

pub fn duplicate_line() -> BusinessError {
    BusinessError::invalid("DUPLICATE_LINE", "Each order line may appear once.")
}

pub fn invalid_completion_date() -> BusinessError {
    BusinessError::invalid(
        "INVALID_COMPLETION_DATE",
        "Completion cannot precede the order date.",
    )
}

pub fn service_note_required() -> BusinessError {
    BusinessError::invalid(
        "SERVICE_COMPLETION_NOTE_REQUIRED",
        "Describe the service completed and who confirmed it.",
    )
}

pub fn order_required() -> BusinessError {
    BusinessError::invalid(
        "ORDER_REQUIRED",
        "Convert the quotation to a sales order first.",
    )
}

fn scaled_amount(amount: i64, numerator: i64, denominator: i64) -> Result<i64, BusinessError> {
    let value = (i128::from(amount) * i128::from(numerator) + i128::from(denominator / 2))
        / i128::from(denominator);
    if value > 9_007_199_254_740_991 {
        return Err(BusinessError::invalid(
            "ORDER_AMOUNT_INVALID",
            "Order total must be positive and within supported limits.",
        ));
    }
    i64::try_from(value).map_err(|_| request_invalid())
}

fn line_amounts(
    quantity_milli: i64,
    price_minor: i64,
    tax_bps: i64,
) -> Result<(i64, i64), BusinessError> {
    let subtotal = scaled_amount(price_minor, quantity_milli, 1_000)?;
    let tax = scaled_amount(subtotal, tax_bps, 10_000)?;
    Ok((subtotal, tax))
}

impl ServiceCompletionCommand {
    pub fn validate(&self) -> Result<(), BusinessError> {
        let note = self.evidence_note.trim();
        if !valid_date(&self.date)
            || self.expected_version < 1
            || note.len() < NOTE_MIN
            || note.chars().count() > NOTE_MAX
            || self.lines.is_empty()
            || self.lines.len() > COMPLETION_LINES_LIMIT
        {
            return Err(request_invalid());
        }
        let mut ids = std::collections::BTreeSet::new();
        for line in &self.lines {
            if !crate::domain::order::valid_identifier(&line.line_id)
                || parse_decimal_units(&line.quantity, 3)?.is_none_or(|value| value <= 0)
            {
                return Err(request_invalid());
            }
            if !ids.insert(line.line_id.as_str()) {
                return Err(duplicate_line());
            }
        }
        if note.chars().count() < NOTE_MIN {
            return Err(service_note_required());
        }
        Ok(())
    }
}

pub fn build_service_completion(
    actor: &CommerceOrderActor,
    command: &ServiceCompletionCommand,
    order: &SalesOrderRow,
    facts: &[Option<ServiceLineFact>],
    now: TimestampMillis,
) -> Result<ServiceCompletionPlan, BusinessError> {
    actor.require_write(now)?;
    command.validate()?;
    order.validate(actor)?;
    if order.kind() != Some(OrderKind::Sales) {
        return Err(order_required());
    }
    if !matches!(
        order.state(),
        Some(OrderState::Confirmed) | Some(OrderState::PartiallyFulfilled)
    ) || order.version != command.expected_version
    {
        return Err(crate::domain::order::version_conflict());
    }
    if command.date < order.order_date {
        return Err(invalid_completion_date());
    }
    if facts.len() != command.lines.len() {
        return Err(request_invalid());
    }
    let mut lines = Vec::with_capacity(facts.len());
    for (index, (input, fact)) in command.lines.iter().zip(facts).enumerate() {
        let fact = fact.as_ref().ok_or_else(line_invalid)?;
        if fact.item_type != "service" {
            return Err(service_completion_required());
        }
        let quantity = parse_decimal_units(&input.quantity, 3)?
            .filter(|value| *value > 0)
            .ok_or_else(request_invalid)?;
        let remaining = fact.quantity_milli.saturating_sub(fact.fulfilled_milli);
        if quantity > remaining {
            return Err(over_fulfillment());
        }
        let (subtotal, tax) = if quantity == remaining {
            (
                fact.subtotal_minor - fact.prior_subtotal_minor,
                fact.tax_minor - fact.prior_tax_minor,
            )
        } else {
            line_amounts(quantity, fact.price_minor, fact.tax_bps)?
        };
        if subtotal < 0 || tax < 0 {
            return Err(source_invalid());
        }
        lines.push(ServiceCompletionLinePlan {
            id: format!("delivery-{}-{index}", command.key.as_str()),
            order_line_id: fact.id.clone(),
            description: fact.description.clone(),
            quantity_milli: quantity,
            price_minor: fact.price_minor,
            tax_bps: fact.tax_bps,
            subtotal_minor: subtotal,
            tax_minor: tax,
            ordinal: index as i64,
        });
    }
    let note = command.evidence_note.trim().to_owned();
    Ok(ServiceCompletionPlan {
        id: format!("delivery-{}", command.key.as_str()),
        order_id: order.id.clone(),
        company_id: order.company_id.clone(),
        number: service_completion_number(&command.key),
        date: command.date.clone(),
        note,
        expected_version: command.expected_version,
        currency: order.currency.clone(),
        lines,
        actor_id: actor.user_id.as_str().to_owned(),
        now: now.get(),
    })
}
