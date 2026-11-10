//! Native Billing job retry facts.
//!
//! The command requeues one Holding-owned billing job or a pending Stripe
//! inbox event. It never reconstructs an ambiguous Stripe operation and never
//! issues an invoice.

use crate::domain::command::{js_trim, request_invalid};
use erpx_server_shared::public::{BusinessError, CommandKey, RecordId};

const REASON_LIMIT: usize = 1_000;

#[derive(Clone, Debug)]
pub struct RetryJob {
    pub key: CommandKey,
    pub reason: String,
    pub job_id: RecordId,
}

impl RetryJob {
    pub fn validate(&self) -> Result<ValidatedRetryJob, BusinessError> {
        let reason = js_trim(&self.reason);
        if !(3..=REASON_LIMIT).contains(&reason.chars().count())
            || reason
                .chars()
                .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
            || self.job_id.as_str().is_empty()
            || self.job_id.as_str().len() > 200
        {
            return Err(request_invalid());
        }
        Ok(ValidatedRetryJob {
            key: self.key.clone(),
            reason: reason.to_owned(),
            job_id: self.job_id.as_str().to_owned(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedRetryJob {
    pub key: CommandKey,
    pub reason: String,
    pub job_id: String,
}
