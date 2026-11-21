//! Native Billing `rate-target` command facts.
//!
//! Rating is append-only: every attempt records a run, while the current rated
//! period is replaced only while it is not approved. It never approves a
//! period, issues an invoice or calls Stripe.
use super::actor::BillingActor;
use super::command::{js_trim, request_invalid, result_json};
use crate::domain::target_pricing::month_bounds;
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, HoldingId};
use serde::Serialize;
use serde_json::Value;

const REASON_LIMIT: usize = 1_000;

#[derive(Clone, Debug)]
pub struct RateTarget {
    pub key: CommandKey,
    pub expected_version: i64,
    pub reason: String,
    pub target_month: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedRating {
    pub target_month: String,
}

impl RateTarget {
    pub fn validate(&self) -> Result<ValidatedRating, BusinessError> {
        let reason = js_trim(&self.reason);
        if self.expected_version < 0
            || !(3..=REASON_LIMIT).contains(&reason.chars().count())
            || reason
                .chars()
                .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
            || month_bounds(&self.target_month).is_err()
        {
            return Err(request_invalid());
        }
        Ok(ValidatedRating {
            target_month: self.target_month.clone(),
        })
    }

    pub fn identity(&self, actor: &BillingActor, target: &HoldingId) -> String {
        serde_json::to_string(&Identity {
            actor: actor.user_id.as_str(),
            holding: target.as_str(),
            action: "rate-target",
            input: serde_json::json!({
                "version": self.expected_version,
                "reason": js_trim(&self.reason),
                "targetMonth": self.target_month,
            }),
        })
        .expect("the Billing rate-target identity always serializes")
    }

    pub fn details(&self) -> String {
        serde_json::json!({
            "version": self.expected_version,
            "reason": js_trim(&self.reason),
            "targetMonth": self.target_month,
        })
        .to_string()
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

pub fn assignment_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "TARGET_ASSIGNMENT_REQUIRED",
        "Assign a target price before rating this month.",
    )
}

pub fn period_frozen() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "TARGET_PERIOD_FROZEN",
        "Approved target charges cannot be recalculated.",
    )
}
