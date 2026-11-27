//! Portable Records mention policy.
//!
//! Owns the exact input shape, UTF-16 bounds and positional write payload
//! behind `record.mention`. Cross-owner principal resolution (eligibility and
//! control-side display name), the record-version guard, transaction, command
//! receipt, audit and outbox stay with composition. No SQL, HTTP or IO here.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::record_context::validate_request;

pub const MENTION_TARGET_ID_MAX: usize = 128;
pub const MENTION_TARGET_LABEL_MAX: usize = 160;

/// Registered owner write: seven binds, holding scope.
pub const MENTION_STATEMENT: &str = "records_stmt_18";
pub const OP_MENTION: &str = "record.mention";

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
}

fn not_found(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::NotFound, code, message)
}

/// The transport's `safeParse` failure, kept identical for native parity.
pub fn mention_input_invalid() -> BusinessError {
    invalid("VALIDATION_FAILED", "Choose a valid mention target.")
}

pub fn mention_target_not_eligible() -> BusinessError {
    not_found(
        "MENTION_TARGET_NOT_FOUND",
        "That person is not available in this Holding.",
    )
}

pub fn mention_target_unavailable() -> BusinessError {
    not_found("MENTION_TARGET_NOT_FOUND", "That person is unavailable.")
}

/// Zod string limits count UTF-16 code units, not scalar values.
fn units(value: &str) -> usize {
    value.encode_utf16().count()
}

/// `addPrincipalMentionSchema`: the only admitted target kind is a person.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MentionTargetKind {
    Person,
}

impl MentionTargetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddPrincipalMentionInput {
    pub target_kind: MentionTargetKind,
    pub target_id: String,
    pub target_label: String,
}

pub fn validate_mention_input(input: &AddPrincipalMentionInput) -> Result<(), BusinessError> {
    let id = units(&input.target_id);
    let label = units(&input.target_label);
    if id == 0 || id > MENTION_TARGET_ID_MAX || label == 0 || label > MENTION_TARGET_LABEL_MAX {
        return Err(mention_input_invalid());
    }
    Ok(())
}

/// The body's `targetLabel` is advisory: the owner writes the resolved
/// control-side principal display name, never the caller-supplied label.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MentionWrite {
    pub module_id: String,
    pub record_id: String,
    pub target_id: String,
    pub display_name: String,
}

impl MentionWrite {
    /// Positional payload for `records_stmt_18`.
    pub fn values(&self, actor_id: &str, now: i64) -> Vec<Value> {
        vec![
            json!(self.module_id),
            json!(self.record_id),
            json!(MentionTargetKind::Person.as_str()),
            json!(self.target_id),
            json!(self.display_name),
            json!(actor_id),
            json!(now),
        ]
    }
}

pub fn plan_mention(
    module_id: &str,
    record_id: &str,
    input: &AddPrincipalMentionInput,
    display_name: &str,
) -> Result<MentionWrite, BusinessError> {
    validate_request(module_id, record_id).map_err(|_| mention_input_invalid())?;
    validate_mention_input(input)?;
    Ok(MentionWrite {
        module_id: module_id.to_owned(),
        record_id: record_id.to_owned(),
        target_id: input.target_id.clone(),
        display_name: display_name.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mention(target_id: &str, label: &str) -> AddPrincipalMentionInput {
        AddPrincipalMentionInput {
            target_kind: MentionTargetKind::Person,
            target_id: target_id.to_owned(),
            target_label: label.to_owned(),
        }
    }

    #[test]
    fn mention_input_accepts_only_person_and_strips_unknown_fields() {
        let parsed: AddPrincipalMentionInput = serde_json::from_value(json!({
            "targetKind": "person",
            "targetId": "u1",
            "targetLabel": "Alice",
        }))
        .unwrap();
        assert_eq!(parsed, mention("u1", "Alice"));
        assert!(validate_mention_input(&parsed).is_ok());
        assert!(
            serde_json::from_value::<AddPrincipalMentionInput>(json!({
                "targetKind": "group",
                "targetId": "u1",
                "targetLabel": "Alice",
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<AddPrincipalMentionInput>(json!({
                "targetKind": "person",
                "targetId": "u1",
                "targetLabel": "Alice",
                "extra": 1,
            }))
            .is_ok()
        );
    }

    #[test]
    fn mention_bounds_count_utf16_units() {
        assert_eq!(mention_input_invalid().code, "VALIDATION_FAILED");
        assert_eq!(mention_input_invalid().kind, FailureKind::InvalidInput);
        assert!(validate_mention_input(&mention("", "Alice")).is_err());
        assert!(validate_mention_input(&mention(&"😀".repeat(64), "Alice")).is_ok());
        assert!(validate_mention_input(&mention(&"😀".repeat(65), "Alice")).is_err());
        assert!(validate_mention_input(&mention("u1", &"😀".repeat(80))).is_ok());
        assert!(validate_mention_input(&mention("u1", &"😀".repeat(81))).is_err());
    }

    #[test]
    fn mention_plan_writes_the_resolved_display_name() {
        let write = plan_mention(
            "ops",
            "rec-1",
            &mention("u1", "caller label"),
            "Control Alice",
        )
        .unwrap();
        let values = write.values("actor-1", 42);
        assert_eq!(write.module_id, "ops");
        assert_eq!(write.record_id, "rec-1");
        assert_eq!(values.len(), 7);
        assert_eq!(values[2], json!("person"));
        assert_eq!(values[4], json!("Control Alice"));
        assert_eq!(values[5], json!("actor-1"));
        assert_eq!(values[6], json!(42));
    }

    #[test]
    fn mention_errors_keep_the_owner_codes() {
        assert_eq!(
            mention_target_not_eligible().code,
            "MENTION_TARGET_NOT_FOUND"
        );
        assert_eq!(mention_target_not_eligible().kind, FailureKind::NotFound);
        assert_eq!(
            mention_target_unavailable().code,
            "MENTION_TARGET_NOT_FOUND"
        );
        assert_eq!(
            mention_target_unavailable().message,
            "That person is unavailable."
        );
        assert!(plan_mention("bad module", "rec-1", &mention("u1", "Alice"), "Alice").is_err());
    }
}
