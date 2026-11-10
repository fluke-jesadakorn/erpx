//! Per-Company AI request quota facts. The owner never stores prompts,
//! completions or credentials in the quota ledger.
use erpx_server_shared::public::BusinessError;
use std::collections::BTreeMap;

fn validation(message: &'static str) -> BusinessError {
    BusinessError::invalid("AI_QUOTA_INPUT_INVALID", message)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FundingSource {
    Erpx,
    Byok,
}

impl FundingSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Erpx => "erpx",
            Self::Byok => "byok",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuotaReservation {
    pub request_id: String,
    pub usage_id: String,
    pub holding_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub connection_id: Option<String>,
    pub funding_source: FundingSource,
    pub provider: String,
    pub model: String,
    pub feature: String,
    pub reservation_satang: i64,
    pub monthly_request_limit: i64,
    pub window_start: i64,
    pub window_end: i64,
    pub now: i64,
}

impl QuotaReservation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        request_id: impl Into<String>,
        usage_id: impl Into<String>,
        holding_id: impl Into<String>,
        company_id: impl Into<String>,
        provider: impl Into<String>,
        model: impl Into<String>,
        feature: impl Into<String>,
        monthly_request_limit: i64,
        window_start: i64,
        window_end: i64,
        now: i64,
    ) -> Result<Self, BusinessError> {
        let reservation = Self {
            request_id: request_id.into(),
            usage_id: usage_id.into(),
            holding_id: holding_id.into(),
            company_id: company_id.into(),
            branch_id: None,
            connection_id: None,
            funding_source: FundingSource::Erpx,
            provider: provider.into(),
            model: model.into(),
            feature: feature.into(),
            reservation_satang: 0,
            monthly_request_limit,
            window_start,
            window_end,
            now,
        };
        reservation.validate()?;
        Ok(reservation)
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        let bounded = |value: &str, max: usize| {
            !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
        };
        if !bounded(&self.request_id, 180)
            || !bounded(&self.usage_id, 180)
            || !bounded(&self.holding_id, 180)
            || !bounded(&self.company_id, 180)
            || !bounded(&self.provider, 120)
            || !bounded(&self.model, 200)
            || !bounded(&self.feature, 120)
            || self
                .branch_id
                .as_deref()
                .is_some_and(|value| !bounded(value, 180))
            || self
                .connection_id
                .as_deref()
                .is_some_and(|value| !bounded(value, 180))
        {
            return Err(validation("Provide bounded AI quota scope facts."));
        }
        if self.monthly_request_limit < 1
            || self.reservation_satang < 0
            || self.window_start < 0
            || self.window_end <= self.window_start
        {
            return Err(validation("Provide a valid AI quota window and limit."));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuotaOutcome {
    Succeeded,
    Failed,
}

impl QuotaOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuotaFinish {
    pub request_id: String,
    pub company_id: String,
    pub outcome: QuotaOutcome,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub units: BTreeMap<String, i64>,
    pub now: i64,
}

impl QuotaFinish {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.request_id.trim().is_empty()
            || self.request_id.len() > 180
            || self.company_id.trim().is_empty()
            || self.company_id.len() > 180
            || self.input_tokens < 0
            || self.output_tokens < 0
            || self.now < 0
            || self
                .units
                .iter()
                .any(|(key, value)| key.is_empty() || key.len() > 80 || *value < 0)
        {
            return Err(validation("Provide valid AI quota completion facts."));
        }
        Ok(())
    }
}

pub fn quota_exhausted() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Conflict,
        "AI_QUOTA_EXHAUSTED",
        "This Company's AI request allowance is exhausted for the current month.",
    )
}

pub fn reservation_unavailable() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Conflict,
        "AI_QUOTA_RESERVATION_UNAVAILABLE",
        "This AI request reservation is no longer active.",
    )
}
