//! Native Billing target provider evidence command facts.
//!
//! `verify-provider-cost` and `verify-statement` record the provider cost and
//! statement evidence used by target rating. They do not rate a period, approve
//! a rollout, issue an invoice or call Stripe.
use super::actor::BillingActor;
use super::command::{js_trim, request_invalid, result_json};
use crate::domain::target_pricing::month_bounds;
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, HoldingId, RecordId};
use serde::Serialize;
use serde_json::Value;

const REASON_LIMIT: usize = 1_000;
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
const AMOUNT_LIMIT: i64 = 100_000_000;

#[derive(Clone, Debug)]
pub struct VerifyProviderCost {
    pub key: CommandKey,
    pub expected_version: i64,
    pub reason: String,
    pub target_month: String,
    pub source_key: String,
    pub provider: String,
    pub statement_id: String,
    pub original_currency: String,
    pub original_minor: i64,
    pub fx_numerator: i64,
    pub fx_denominator: i64,
    pub provider_cost_satang: i64,
    pub evidence_id: RecordId,
}

#[derive(Clone, Debug)]
pub struct VerifyStatement {
    pub key: CommandKey,
    pub expected_version: i64,
    pub reason: String,
    pub target_month: String,
    pub provider: String,
    pub statement_id: String,
    pub original_currency: String,
    pub provider_total_minor: i64,
    pub evidence_id: RecordId,
}

#[derive(Clone, Debug)]
pub enum BillingTargetProviderCommand {
    ProviderCost(VerifyProviderCost),
    Statement(VerifyStatement),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedProviderCost {
    pub target_month: String,
    pub source_key: String,
    pub provider: String,
    pub statement_id: String,
    pub original_currency: String,
    pub original_minor: i64,
    pub fx_numerator: i64,
    pub fx_denominator: i64,
    pub provider_cost_satang: i64,
    pub evidence_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedStatement {
    pub target_month: String,
    pub provider: String,
    pub statement_id: String,
    pub original_currency: String,
    pub provider_total_minor: i64,
    pub evidence_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValidatedProviderEvidence {
    ProviderCost(ValidatedProviderCost),
    Statement(ValidatedStatement),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderMeter {
    pub provider: String,
    pub cost_status: Option<String>,
    pub cost_reconciled_at: Option<String>,
    pub verified_cost_satang: Option<String>,
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

fn valid_source_key(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn valid_currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase())
}

fn valid_text(value: &str, max: usize) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && trimmed.chars().count() <= max && !trimmed.chars().any(char::is_control)
}

fn valid_safe(value: i64) -> bool {
    (0..=MAX_SAFE_INTEGER).contains(&value)
}

impl VerifyProviderCost {
    pub fn validate(&self) -> Result<ValidatedProviderCost, BusinessError> {
        if self.expected_version < 0
            || !valid_reason(&self.reason)
            || !valid_month(&self.target_month)
            || !valid_source_key(&self.source_key)
            || !valid_text(&self.provider, 100)
            || !valid_text(&self.statement_id, 200)
            || !valid_currency(&self.original_currency)
            || !valid_safe(self.original_minor)
            || !(1..=MAX_SAFE_INTEGER).contains(&self.fx_numerator)
            || !(1..=MAX_SAFE_INTEGER).contains(&self.fx_denominator)
            || !(0..=AMOUNT_LIMIT).contains(&self.provider_cost_satang)
            || self.evidence_id.as_str().len() > 100
        {
            return Err(request_invalid());
        }
        Ok(ValidatedProviderCost {
            target_month: self.target_month.clone(),
            source_key: self.source_key.clone(),
            provider: js_trim(&self.provider).to_owned(),
            statement_id: js_trim(&self.statement_id).to_owned(),
            original_currency: self.original_currency.clone(),
            original_minor: self.original_minor,
            fx_numerator: self.fx_numerator,
            fx_denominator: self.fx_denominator,
            provider_cost_satang: self.provider_cost_satang,
            evidence_id: self.evidence_id.as_str().to_owned(),
        })
    }
}

impl VerifyStatement {
    pub fn validate(&self) -> Result<ValidatedStatement, BusinessError> {
        if self.expected_version < 0
            || !valid_reason(&self.reason)
            || !valid_month(&self.target_month)
            || !valid_text(&self.provider, 100)
            || !valid_text(&self.statement_id, 200)
            || !valid_currency(&self.original_currency)
            || !valid_safe(self.provider_total_minor)
            || self.evidence_id.as_str().len() > 100
        {
            return Err(request_invalid());
        }
        Ok(ValidatedStatement {
            target_month: self.target_month.clone(),
            provider: js_trim(&self.provider).to_owned(),
            statement_id: js_trim(&self.statement_id).to_owned(),
            original_currency: self.original_currency.clone(),
            provider_total_minor: self.provider_total_minor,
            evidence_id: self.evidence_id.as_str().to_owned(),
        })
    }
}

impl BillingTargetProviderCommand {
    pub fn expected_version(&self) -> i64 {
        match self {
            Self::ProviderCost(command) => command.expected_version,
            Self::Statement(command) => command.expected_version,
        }
    }

    pub fn key(&self) -> &CommandKey {
        match self {
            Self::ProviderCost(command) => &command.key,
            Self::Statement(command) => &command.key,
        }
    }

    pub fn reason(&self) -> &str {
        match self {
            Self::ProviderCost(command) => &command.reason,
            Self::Statement(command) => &command.reason,
        }
    }

    pub fn action(&self) -> &'static str {
        match self {
            Self::ProviderCost(_) => "verify-provider-cost",
            Self::Statement(_) => "verify-statement",
        }
    }

    pub fn validate(&self) -> Result<ValidatedProviderEvidence, BusinessError> {
        match self {
            Self::ProviderCost(command) => command
                .validate()
                .map(ValidatedProviderEvidence::ProviderCost),
            Self::Statement(command) => {
                command.validate().map(ValidatedProviderEvidence::Statement)
            }
        }
    }

    pub fn identity(&self, actor: &BillingActor, target: &HoldingId) -> String {
        serde_json::to_string(&Identity {
            actor: actor.user_id.as_str(),
            holding: target.as_str(),
            action: self.action(),
            input: input_json(self),
        })
        .expect("the Billing target provider identity always serializes")
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

fn input_json(command: &BillingTargetProviderCommand) -> Value {
    match command {
        BillingTargetProviderCommand::ProviderCost(command) => serde_json::json!({
            "version": command.expected_version,
            "reason": js_trim(&command.reason),
            "targetMonth": command.target_month,
            "sourceKey": command.source_key,
            "provider": js_trim(&command.provider),
            "statementId": js_trim(&command.statement_id),
            "originalCurrency": command.original_currency,
            "originalMinor": command.original_minor,
            "fxNumerator": command.fx_numerator,
            "fxDenominator": command.fx_denominator,
            "providerCostSatang": command.provider_cost_satang,
            "evidenceId": command.evidence_id.as_str(),
        }),
        BillingTargetProviderCommand::Statement(command) => serde_json::json!({
            "version": command.expected_version,
            "reason": js_trim(&command.reason),
            "targetMonth": command.target_month,
            "provider": js_trim(&command.provider),
            "statementId": js_trim(&command.statement_id),
            "originalCurrency": command.original_currency,
            "providerTotalMinor": command.provider_total_minor,
            "evidenceId": command.evidence_id.as_str(),
        }),
    }
}

pub fn provider_cost_unverified() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROVIDER_COST_UNVERIFIED",
        "Match a successful Platform-key meter to verified provider cost first.",
    )
}

pub fn fx_mismatch() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "FX_MISMATCH",
        "The reviewed THB amount does not match original units and the FX ratio.",
    )
}

pub fn evidence_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "EVIDENCE_REQUIRED",
        "Attach available private evidence.",
    )
}

pub fn period_frozen() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "TARGET_PERIOD_FROZEN",
        "Approved target charges cannot be changed.",
    )
}

pub fn provider_cost_already_verified() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROVIDER_COST_ALREADY_VERIFIED",
        "The source already has verified cost; preserve its audit trail.",
    )
}

pub fn statement_mismatch() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "STATEMENT_MISMATCH",
        "Statement total must equal the matched provider source costs.",
    )
}

pub fn statement_already_verified() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "STATEMENT_ALREADY_VERIFIED",
        "The provider statement was already verified.",
    )
}
