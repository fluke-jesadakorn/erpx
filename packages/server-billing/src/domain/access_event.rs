//! Native Billing evidence-access audit command facts.
//!
//! These facts record one Platform evidence access decision. The pure planner
//! validates the bounded command at the owner boundary; it never touches a
//! database, issues a session or derives an actor. Authority is checked by the
//! application use case and the SQL lives only in the owner adapter.

use crate::domain::command::{js_trim, request_invalid};
use erpx_server_shared::public::{BusinessError, CommandKey, RecordId};

const REASON_LIMIT: usize = 200;
const ID_LIMIT: usize = 200;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AccessOutcome {
    Denied,
    Allowed,
}

impl AccessOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccessOutcome::Denied => "denied",
            AccessOutcome::Allowed => "allowed",
        }
    }
}

#[derive(Clone, Debug)]
pub struct AccessEvent {
    pub key: CommandKey,
    pub event_id: RecordId,
    pub evidence_id: RecordId,
    pub outcome: AccessOutcome,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedAccessEvent {
    pub key: String,
    pub event_id: String,
    pub evidence_id: String,
    pub outcome: &'static str,
    pub reason: String,
}

impl AccessEvent {
    /// Validate one bounded evidence-access command. The command key, ids,
    /// outcome and reason are carried exactly once to the port; no projection
    /// or convention is inferred here.
    pub fn validate(&self) -> Result<ValidatedAccessEvent, BusinessError> {
        let reason = js_trim(&self.reason);
        if reason.chars().count() > REASON_LIMIT
            || reason
                .chars()
                .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
            || self.event_id.as_str().is_empty()
            || self.event_id.as_str().len() > ID_LIMIT
            || self.evidence_id.as_str().is_empty()
            || self.evidence_id.as_str().len() > ID_LIMIT
        {
            return Err(request_invalid());
        }
        Ok(ValidatedAccessEvent {
            key: self.key.as_str().to_owned(),
            event_id: self.event_id.as_str().to_owned(),
            evidence_id: self.evidence_id.as_str().to_owned(),
            outcome: self.outcome.as_str(),
            reason: reason.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(reason: &str, outcome: AccessOutcome) -> AccessEvent {
        AccessEvent {
            key: CommandKey::new("billing:access:1").unwrap(),
            event_id: RecordId::new("billing-access-1").unwrap(),
            evidence_id: RecordId::new("evidence-1").unwrap(),
            outcome,
            reason: reason.to_owned(),
        }
    }

    #[test]
    fn validates_exact_access_events() {
        let denied = event(" EVIDENCE_NOT_AVAILABLE ", AccessOutcome::Denied)
            .validate()
            .unwrap();
        assert_eq!(denied.outcome, "denied");
        assert_eq!(denied.reason, "EVIDENCE_NOT_AVAILABLE");
        let allowed = event("", AccessOutcome::Allowed).validate().unwrap();
        assert_eq!(allowed.outcome, "allowed");
        assert_eq!(allowed.reason, "");
    }

    #[test]
    fn rejects_unbounded_reason() {
        let long = "x".repeat(REASON_LIMIT + 1);
        assert!(event(&long, AccessOutcome::Denied).validate().is_err());
    }
}
