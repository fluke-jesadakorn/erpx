//! Native Billing target evidence command facts.
//!
//! `record-operating-cost` and `review-target-capacity` are immutable evidence
//! commands used by the target rollout readiness read. They do not approve a
//! rollout, rate a period, issue an invoice or call Stripe.
use super::actor::BillingActor;
use super::command::{js_trim, request_invalid, result_json};
use crate::domain::target_pricing::month_bounds;
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, HoldingId, RecordId};
use serde::Serialize;
use serde_json::Value;

const REASON_LIMIT: usize = 1_000;
const AMOUNT_LIMIT: i64 = 100_000_000;
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

#[derive(Clone, Debug)]
pub struct RecordOperatingCost {
    pub key: CommandKey,
    pub expected_version: i64,
    pub reason: String,
    pub target_month: String,
    pub category: String,
    pub amount_satang: i64,
    pub evidence_id: RecordId,
}

#[derive(Clone, Debug)]
pub struct ReviewTargetCapacity {
    pub key: CommandKey,
    pub expected_version: i64,
    pub reason: String,
    pub target_month: String,
    pub projected_original_bytes: i64,
    pub primary_available_bytes: i64,
    pub backup_available_bytes: i64,
    pub reserved_bytes: i64,
    pub company_limit_bytes: i64,
    pub evidence_id: RecordId,
}

#[derive(Clone, Debug)]
pub enum BillingTargetEvidenceCommand {
    OperatingCost(RecordOperatingCost),
    Capacity(ReviewTargetCapacity),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedOperatingCost {
    pub target_month: String,
    pub category: String,
    pub amount_satang: i64,
    pub evidence_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedCapacity {
    pub target_month: String,
    pub projected_original_bytes: i64,
    pub primary_available_bytes: i64,
    pub backup_available_bytes: i64,
    pub reserved_bytes: i64,
    pub company_limit_bytes: i64,
    pub evidence_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValidatedTargetEvidence {
    OperatingCost(ValidatedOperatingCost),
    Capacity(ValidatedCapacity),
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

fn valid_amount(value: i64) -> bool {
    (0..=AMOUNT_LIMIT).contains(&value)
}

fn valid_bytes(value: i64) -> bool {
    (0..=MAX_SAFE_INTEGER).contains(&value)
}

impl RecordOperatingCost {
    pub fn validate(&self) -> Result<ValidatedOperatingCost, BusinessError> {
        if self.expected_version < 0
            || !valid_reason(&self.reason)
            || !valid_month(&self.target_month)
            || !matches!(
                self.category.as_str(),
                "infrastructure" | "backup" | "stripe" | "support"
            )
            || !valid_amount(self.amount_satang)
            || self.evidence_id.as_str().len() > 100
        {
            return Err(request_invalid());
        }
        Ok(ValidatedOperatingCost {
            target_month: self.target_month.clone(),
            category: self.category.clone(),
            amount_satang: self.amount_satang,
            evidence_id: self.evidence_id.as_str().to_owned(),
        })
    }
}

impl ReviewTargetCapacity {
    pub fn validate(&self) -> Result<ValidatedCapacity, BusinessError> {
        if self.expected_version < 0
            || !valid_reason(&self.reason)
            || !valid_month(&self.target_month)
            || !valid_bytes(self.projected_original_bytes)
            || !valid_bytes(self.primary_available_bytes)
            || !valid_bytes(self.backup_available_bytes)
            || !valid_bytes(self.reserved_bytes)
            || !valid_bytes(self.company_limit_bytes)
            || self.evidence_id.as_str().len() > 100
        {
            return Err(request_invalid());
        }
        Ok(ValidatedCapacity {
            target_month: self.target_month.clone(),
            projected_original_bytes: self.projected_original_bytes,
            primary_available_bytes: self.primary_available_bytes,
            backup_available_bytes: self.backup_available_bytes,
            reserved_bytes: self.reserved_bytes,
            company_limit_bytes: self.company_limit_bytes,
            evidence_id: self.evidence_id.as_str().to_owned(),
        })
    }
}

impl BillingTargetEvidenceCommand {
    pub fn expected_version(&self) -> i64 {
        match self {
            Self::OperatingCost(command) => command.expected_version,
            Self::Capacity(command) => command.expected_version,
        }
    }

    pub fn key(&self) -> &CommandKey {
        match self {
            Self::OperatingCost(command) => &command.key,
            Self::Capacity(command) => &command.key,
        }
    }

    pub fn reason(&self) -> &str {
        match self {
            Self::OperatingCost(command) => &command.reason,
            Self::Capacity(command) => &command.reason,
        }
    }

    pub fn action(&self) -> &'static str {
        match self {
            Self::OperatingCost(_) => "record-operating-cost",
            Self::Capacity(_) => "review-target-capacity",
        }
    }

    pub fn validate(&self) -> Result<ValidatedTargetEvidence, BusinessError> {
        match self {
            Self::OperatingCost(command) => command
                .validate()
                .map(ValidatedTargetEvidence::OperatingCost),
            Self::Capacity(command) => command.validate().map(ValidatedTargetEvidence::Capacity),
        }
    }

    pub fn identity(&self, actor: &BillingActor, target: &HoldingId) -> String {
        serde_json::to_string(&Identity {
            actor: actor.user_id.as_str(),
            holding: target.as_str(),
            action: self.action(),
            input: input_json(self),
        })
        .expect("the Billing target evidence identity always serializes")
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

fn input_json(command: &BillingTargetEvidenceCommand) -> Value {
    match command {
        BillingTargetEvidenceCommand::OperatingCost(command) => serde_json::json!({
            "version": command.expected_version,
            "reason": js_trim(&command.reason),
            "targetMonth": command.target_month,
            "operatingCategory": command.category,
            "operatingCostSatang": command.amount_satang,
            "evidenceId": command.evidence_id.as_str(),
        }),
        BillingTargetEvidenceCommand::Capacity(command) => serde_json::json!({
            "version": command.expected_version,
            "reason": js_trim(&command.reason),
            "targetMonth": command.target_month,
            "projectedOriginalBytes": command.projected_original_bytes,
            "primaryAvailableBytes": command.primary_available_bytes,
            "backupAvailableBytes": command.backup_available_bytes,
            "reservedBytes": command.reserved_bytes,
            "companyLimitBytes": command.company_limit_bytes,
            "evidenceId": command.evidence_id.as_str(),
        }),
    }
}

pub fn month_not_closed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "MONTH_NOT_CLOSED",
        "Record actual operating costs only after the month ends.",
    )
}

pub fn evidence_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "EVIDENCE_REQUIRED",
        "Attach available private evidence.",
    )
}

pub fn operating_cost_already_verified() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "OPERATING_COST_ALREADY_VERIFIED",
        "The actual cost is immutable; add a future adjustment with new evidence.",
    )
}

pub fn future_month_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "FUTURE_MONTH_REQUIRED",
        "Capacity approval must precede a future effective month.",
    )
}

pub fn capacity_already_reviewed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CAPACITY_ALREADY_REVIEWED",
        "Capacity evidence for this effective month is immutable.",
    )
}
