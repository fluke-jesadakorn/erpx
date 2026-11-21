//! Native Billing target price-assignment command facts.
//!
//! This is the `assign-target` command: an immutable Holding target offering
//! assignment for one future or current month. It does not rate a period,
//! approve a rollout, issue an invoice or call Stripe.
use super::actor::BillingActor;
use super::command::{js_trim, request_invalid, result_json};
use crate::domain::target_pricing::month_bounds;
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, HoldingId, RecordId};
use serde::Serialize;
use serde_json::Value;

const REASON_LIMIT: usize = 1_000;

#[derive(Clone, Debug)]
pub struct AssignTarget {
    pub key: CommandKey,
    pub expected_version: i64,
    pub reason: String,
    pub target_month: String,
    pub target_mode: String,
    pub price_id: Option<RecordId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedAssignment {
    pub target_month: String,
    pub target_mode: String,
    pub price_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetCommandPrice {
    pub id: String,
    pub effective_month: String,
}

fn valid_reason(value: &str) -> bool {
    let trimmed = js_trim(value);
    (3..=REASON_LIMIT).contains(&trimmed.chars().count())
        && !trimmed
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
}

fn valid_month(value: &str) -> bool {
    month_bounds(value).is_ok()
}

impl AssignTarget {
    pub fn validate(&self) -> Result<ValidatedAssignment, BusinessError> {
        if self.expected_version < 0
            || !valid_reason(&self.reason)
            || !matches!(self.target_mode.as_str(), "shadow" | "sandbox_invoice")
            || !valid_month(&self.target_month)
            || self
                .price_id
                .as_ref()
                .is_some_and(|id| id.as_str().len() > 100)
        {
            return Err(request_invalid());
        }
        Ok(ValidatedAssignment {
            target_month: self.target_month.clone(),
            target_mode: self.target_mode.clone(),
            price_id: self.price_id.as_ref().map(|id| id.as_str().to_owned()),
        })
    }

    pub fn identity(&self, actor: &BillingActor, target: &HoldingId) -> String {
        serde_json::to_string(&Identity {
            actor: actor.user_id.as_str(),
            holding: target.as_str(),
            action: "assign-target",
            input: input_json(self),
        })
        .expect("the Billing target command identity always serializes")
    }

    pub fn details(&self) -> String {
        input_json(self).to_string()
    }

    pub fn result_json(&self) -> &'static str {
        result_json()
    }
}

#[derive(Serialize)]
struct Identity<'a> {
    actor: &'a str,
    holding: &'a str,
    action: &'a str,
    input: Value,
}

fn input_json(command: &AssignTarget) -> Value {
    serde_json::json!({
        "version": command.expected_version,
        "reason": js_trim(&command.reason),
        "targetMonth": command.target_month,
        "targetMode": command.target_mode,
        "priceId": command.price_id.as_ref().map(|id| id.as_str()),
    })
}

pub fn target_price_missing() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "TARGET_PRICE_MISSING",
        "No approved Sandbox price version applies to this month.",
    )
}

pub fn backdate_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "TARGET_BACKDATE_FORBIDDEN",
        "Sandbox invoice assignments cannot be backdated.",
    )
}

pub fn rollout_not_ready() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "TARGET_ROLLOUT_NOT_READY",
        "This Holding needs explicit Sandbox test designation or approved future-month rollout evidence.",
    )
}

pub fn assignment_exists() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "TARGET_ASSIGNMENT_EXISTS",
        "This month's assignment is immutable. Choose a later month.",
    )
}
