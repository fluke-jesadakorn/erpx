//! Portable Records mention policy.
//!
//! Owns the exact input shape, UTF-16 bounds and positional write payload
//! behind `record.mention`. Cross-owner principal resolution (eligibility and
//! control-side display name), the record-version guard, transaction, command
//! receipt, audit and outbox stay with composition. No SQL, HTTP or IO here.
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::record_context::{ReferenceView, validate_request};
use super::record_write::{MAX_SAFE, RecordFieldInput};

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

// ---------------------------------------------------------------------------
// record.suggestion
// ---------------------------------------------------------------------------

const SUGGESTION_FIELD_KEY_MAX: usize = 80;
const PROVIDER_KIND: &str = "development";
const MODEL_REF: &str = "erpx-evidence-assistant-v1";
const READY: &str = "ready";

pub const SUGGESTION_STATEMENT: &str = "records_stmt_19";
pub const FIELD_WRITE_STATEMENT: &str = "records_stmt_20";
pub const SUGGESTION_STATE_STATEMENT: &str = "records_stmt_21";

pub const OP_SUGGESTION: &str = "record.suggestion";
pub const OP_SUGGESTION_DECIDE: &str = "record.suggestion.decide";

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

/// The `suggestionSchema.safeParse` failure, kept identical for native parity.
pub fn suggestion_input_invalid() -> BusinessError {
    invalid("VALIDATION_FAILED", "Choose a field.")
}

/// The `suggestionDecisionSchema.safeParse` failure.
pub fn decision_input_invalid() -> BusinessError {
    invalid("VALIDATION_FAILED", "Choose apply or reject.")
}

fn unknown_field() -> BusinessError {
    invalid("UNKNOWN_FIELD", "Choose an editable field.")
}

fn evidence_required() -> BusinessError {
    conflict(
        "EVIDENCE_REQUIRED",
        "Add at least one readable @ reference before requesting a suggestion.",
    )
}

pub fn suggestion_unavailable() -> BusinessError {
    conflict(
        "SUGGESTION_UNAVAILABLE",
        "This suggestion is no longer ready.",
    )
}

pub fn suggestion_conflict() -> BusinessError {
    conflict(
        "SUGGESTION_CONFLICT",
        "The record changed. Request a new suggestion.",
    )
}

fn suggestion_source_invalid() -> BusinessError {
    invalid(
        "SUGGESTION_SOURCE_INVALID",
        "The suggestion evidence is no longer available.",
    )
}

fn legal_owner_required() -> BusinessError {
    conflict(
        "LEGAL_OWNER_REQUIRED",
        "Use Legal workflows; preview records remain read-only.",
    )
}

fn module_preview_only() -> BusinessError {
    conflict(
        "MODULE_PREVIEW_ONLY",
        "This module is a preview and cannot execute business commands.",
    )
}

fn field_source_native_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FIELD_RECORD_SOURCE_NATIVE_UNIT_REQUIRED",
        "Use the native Field Service workflows.",
    )
}

fn finance_owner_required() -> BusinessError {
    conflict(
        "FINANCE_OWNER_REQUIRED",
        "Use the Finance API for accounting records; the generic workbench is read-only.",
    )
}

/// Pure part of `assertGenericRecordOwner`; permission and module-access reads
/// stay composition-owned. Called only for `apply`.
pub fn guard_social_owner(module_id: &str, preview: bool) -> Result<(), BusinessError> {
    if module_id == "legal" {
        return Err(legal_owner_required());
    }
    if preview {
        return Err(module_preview_only());
    }
    if module_id == "field_service" {
        return Err(field_source_native_required());
    }
    if module_id == "finance" {
        return Err(finance_owner_required());
    }
    Ok(())
}

/// `suggestionSchema`: zod string bounds count UTF-16 code units. Unknown keys
/// are stripped by zod, so they are accepted here rather than rejected.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionInput {
    pub field_key: String,
}

pub fn validate_suggestion_input(input: &SuggestionInput) -> Result<(), BusinessError> {
    let key = units(&input.field_key);
    if key == 0 || key > SUGGESTION_FIELD_KEY_MAX {
        return Err(suggestion_input_invalid());
    }
    Ok(())
}

/// Typed source version: field order is the legacy `JSON.stringify` order.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SuggestionSource {
    pub record: String,
    pub version: i64,
}

/// Ordered prompt DTO: `moduleId` first, so the digest input is byte-stable.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct SuggestionPrompt<'a> {
    pub module_id: &'a str,
    pub field: &'a str,
    pub sources: &'a [SuggestionSource],
}

/// Ordered provenance DTO: snake_case keys in the retained legacy order.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct SuggestionProvenance {
    pub provider_kind: String,
    pub model_ref: String,
    pub prompt_digest: String,
}

/// Positional write for `records_stmt_19`; state is always `ready`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SuggestionRequest {
    pub suggestion_id: String,
    pub module_id: String,
    pub record_id: String,
    pub field_key: String,
    pub proposed_value_json: String,
    pub previous_value_digest: String,
    pub source_versions_json: String,
    pub provenance_json: String,
}

impl SuggestionRequest {
    pub fn values(&self, actor_id: &str, now: i64) -> Vec<Value> {
        vec![
            json!(self.suggestion_id),
            json!(self.module_id),
            json!(self.record_id),
            json!(self.field_key),
            json!(self.proposed_value_json),
            json!(self.previous_value_digest),
            json!(self.source_versions_json),
            json!(self.provenance_json),
            json!(READY),
            json!(actor_id),
            json!(now),
            json!(now),
        ]
    }
}

/// Only readable references become suggestion evidence.
fn available_references(references: &[ReferenceView]) -> Vec<&ReferenceView> {
    references
        .iter()
        .filter(|reference| reference.access == "available")
        .collect()
}

/// Exact legacy proposal: reference count for numeric fields, otherwise an
/// `@module/id` list followed by the lower-cased field label.
fn proposed_value(field_type: &str, label: &str, references: &[&ReferenceView]) -> Value {
    if field_type == "number" {
        return Value::Number(serde_json::Number::from(references.len() as u64));
    }
    let list = references
        .iter()
        .map(|reference| format!("@{}/{}", reference.target_module, reference.target_id))
        .collect::<Vec<_>>()
        .join(", ");
    Value::String(format!("{list} — reviewed for {}", label.to_lowercase()))
}

/// Owns serialization of the digest inputs; the composition supplies hashing
/// (CommandJson + sha2). No hashing or IO belongs to this policy.
pub fn plan_suggestion_request(
    input: &SuggestionInput,
    module_id: &str,
    record_id: &str,
    fields: &[RecordFieldInput],
    references: &[ReferenceView],
    suggestion_id: &str,
    mut digest: impl FnMut(&str) -> Result<String, BusinessError>,
) -> Result<SuggestionRequest, BusinessError> {
    validate_suggestion_input(input)?;
    validate_request(module_id, record_id).map_err(|_| suggestion_input_invalid())?;
    let field = fields
        .iter()
        .find(|field| field.key == input.field_key)
        .ok_or_else(unknown_field)?;
    if !(field.value.is_string() || field.value.is_number() || field.value.is_boolean()) {
        return Err(unknown_field());
    }
    let references = available_references(references);
    if references.is_empty() {
        return Err(evidence_required());
    }
    let sources = references
        .iter()
        .map(|reference| SuggestionSource {
            record: format!("{}/{}", reference.target_module, reference.target_id),
            version: reference.version,
        })
        .collect::<Vec<_>>();
    let proposed = proposed_value(&field.field_type, &field.label, &references);
    let proposed_value_json =
        serde_json::to_string(&proposed).map_err(|_| suggestion_input_invalid())?;
    let previous_value_digest =
        digest(&serde_json::to_string(&field.value).map_err(|_| suggestion_input_invalid())?)?;
    let prompt = SuggestionPrompt {
        module_id,
        field: &field.key,
        sources: &sources,
    };
    let provenance = SuggestionProvenance {
        provider_kind: PROVIDER_KIND.to_owned(),
        model_ref: MODEL_REF.to_owned(),
        prompt_digest: digest(
            &serde_json::to_string(&prompt).map_err(|_| suggestion_input_invalid())?,
        )?,
    };
    Ok(SuggestionRequest {
        suggestion_id: suggestion_id.to_owned(),
        module_id: module_id.to_owned(),
        record_id: record_id.to_owned(),
        field_key: field.key.clone(),
        proposed_value_json,
        previous_value_digest,
        source_versions_json: serde_json::to_string(&sources)
            .map_err(|_| suggestion_input_invalid())?,
        provenance_json: serde_json::to_string(&provenance)
            .map_err(|_| suggestion_input_invalid())?,
    })
}

// ---------------------------------------------------------------------------
// record.suggestion.decide
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SuggestionDecision {
    Apply,
    Reject,
}

impl SuggestionDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Apply => "apply",
            Self::Reject => "reject",
        }
    }
}

/// `suggestionDecisionSchema`: positive integer version and a two-way action.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionDecisionInput {
    #[serde(deserialize_with = "deserialize_suggestion_version")]
    pub expected_version: i64,
    pub action: SuggestionDecision,
}

fn deserialize_suggestion_version<'de, D: serde::Deserializer<'de>>(
    input: D,
) -> Result<i64, D::Error> {
    let number = f64::deserialize(input)?;
    if number.fract() == 0.0 && (1.0..=MAX_SAFE as f64).contains(&number) {
        Ok(number as i64)
    } else {
        Err(serde::de::Error::custom("expected a positive safe integer"))
    }
}

pub fn validate_decision_input(input: &SuggestionDecisionInput) -> Result<(), BusinessError> {
    if !(1..=MAX_SAFE).contains(&input.expected_version) {
        return Err(decision_input_invalid());
    }
    Ok(())
}

pub fn parse_sources(raw: &str) -> Result<Vec<SuggestionSource>, BusinessError> {
    let value: Value = serde_json::from_str(raw).map_err(|_| suggestion_source_invalid())?;
    let Some(entries) = value.as_array() else {
        return Err(suggestion_source_invalid());
    };
    entries
        .iter()
        .map(|entry| {
            let record = entry
                .get("record")
                .and_then(Value::as_str)
                .ok_or_else(suggestion_source_invalid)?;
            let version = entry
                .get("version")
                .and_then(Value::as_i64)
                .filter(|version| *version > 0)
                .ok_or_else(suggestion_source_invalid)?;
            Ok(SuggestionSource {
                record: record.to_owned(),
                version,
            })
        })
        .collect()
}

pub fn parse_proposed_value(raw: &str) -> Result<Value, BusinessError> {
    let value: Value = serde_json::from_str(raw).map_err(|_| suggestion_source_invalid())?;
    if value.is_string() || value.is_number() || value.is_boolean() {
        Ok(value)
    } else {
        Err(suggestion_source_invalid())
    }
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

    fn field(key: &str, label: &str, kind: &str, value: Value) -> RecordFieldInput {
        RecordFieldInput {
            key: key.to_owned(),
            label: label.to_owned(),
            field_type: kind.to_owned(),
            value,
            options: None,
        }
    }

    fn reference(module_id: &str, id: &str, version: i64, access: &'static str) -> ReferenceView {
        ReferenceView {
            target_module: module_id.to_owned(),
            target_id: id.to_owned(),
            kind: "dependsOn".to_owned(),
            ordinal: 0,
            title: "ref".to_owned(),
            version,
            access,
        }
    }

    #[test]
    fn suggestion_inputs_keep_legacy_bounds_and_actions() {
        let parsed: SuggestionInput = serde_json::from_value(json!({
            "fieldKey": "title", "extra": 1
        }))
        .unwrap();
        assert_eq!(
            parsed,
            SuggestionInput {
                field_key: "title".into()
            }
        );
        assert!(validate_suggestion_input(&parsed).is_ok());
        assert!(
            validate_suggestion_input(&SuggestionInput {
                field_key: "😀".repeat(40)
            })
            .is_ok()
        );
        assert!(
            validate_suggestion_input(&SuggestionInput {
                field_key: "😀".repeat(41)
            })
            .is_err()
        );
        assert!(
            serde_json::from_value::<SuggestionDecisionInput>(json!({
                "expectedVersion": 1, "action": "undo"
            }))
            .is_err()
        );
        let decision: SuggestionDecisionInput = serde_json::from_value(json!({
            "expectedVersion": 2, "action": "apply", "extra": 1
        }))
        .unwrap();
        assert_eq!(decision.action, SuggestionDecision::Apply);
        assert_eq!(decision.action.as_str(), "apply");
        assert!(validate_decision_input(&decision).is_ok());
        assert!(
            validate_decision_input(&SuggestionDecisionInput {
                expected_version: 0,
                action: SuggestionDecision::Reject,
            })
            .is_err()
        );
    }

    #[test]
    fn proposal_uses_readable_evidence_and_field_type() {
        let refs = [
            reference("ops", "rec-a", 3, "available"),
            reference("files", "rec-b", 7, "denied"),
        ];
        let available = available_references(&refs);
        assert_eq!(available.len(), 1);
        assert_eq!(proposed_value("number", "Count", &available), json!(1));
        assert_eq!(
            proposed_value("text", "Short Title", &available),
            json!("@ops/rec-a — reviewed for short title")
        );
    }

    #[test]
    fn request_plan_pins_ordered_prompt_and_provenance() {
        let fields = [field("title", "Title", "text", json!("draft"))];
        let refs = [
            reference("ops", "rec-a", 3, "available"),
            reference("files", "rec-b", 7, "available"),
        ];
        let mut seen = Vec::new();
        let plan = plan_suggestion_request(
            &SuggestionInput {
                field_key: "title".into(),
            },
            "ops",
            "rec-1",
            &fields,
            &refs,
            "sug-1",
            |value: &str| {
                seen.push(value.to_owned());
                Ok("D".to_owned())
            },
        )
        .unwrap();
        assert_eq!(seen[0], "\"draft\"");
        assert_eq!(
            seen[1],
            r#"{"moduleId":"ops","field":"title","sources":[{"record":"ops/rec-a","version":3},{"record":"files/rec-b","version":7}]}"#
        );
        assert_eq!(plan.previous_value_digest, "D");
        assert_eq!(
            plan.source_versions_json,
            json!([
                {"record":"ops/rec-a","version":3},
                {"record":"files/rec-b","version":7}
            ])
            .to_string()
        );
        assert_eq!(
            plan.proposed_value_json,
            json!("@ops/rec-a, @files/rec-b — reviewed for title").to_string()
        );
        assert_eq!(
            plan.provenance_json,
            r#"{"provider_kind":"development","model_ref":"erpx-evidence-assistant-v1","prompt_digest":"D"}"#
        );
        let values = plan.values("actor-1", 42);
        assert_eq!(values.len(), 12);
        assert_eq!(values[8], json!(READY));
    }

    #[test]
    fn request_requires_known_field_and_readable_evidence() {
        let fields = [field("title", "Title", "text", json!("draft"))];
        let missing = plan_suggestion_request(
            &SuggestionInput {
                field_key: "other".into(),
            },
            "ops",
            "rec-1",
            &fields,
            &[],
            "sug-1",
            |_| Ok("D".to_owned()),
        );
        assert_eq!(missing.unwrap_err().code, "UNKNOWN_FIELD");
        let no_refs = plan_suggestion_request(
            &SuggestionInput {
                field_key: "title".into(),
            },
            "ops",
            "rec-1",
            &fields,
            &[reference("ops", "rec-a", 3, "denied")],
            "sug-1",
            |_| Ok("D".to_owned()),
        );
        assert_eq!(no_refs.unwrap_err().code, "EVIDENCE_REQUIRED");
    }

    #[test]
    fn typed_sources_and_owner_guard_are_bounded() {
        let sources = parse_sources(r#"[{"record":"files/rec-b/extra","version":7}]"#).unwrap();
        let (module_id, record_id) = sources[0].record.split_once('/').unwrap();
        assert_eq!(module_id, "files");
        assert_eq!(record_id, "rec-b/extra");
        assert!(parse_sources(r#"[{"record":"x/y","version":0}]"#).is_err());
        assert!(parse_proposed_value(r#"{"a":1}"#).is_err());
        assert_eq!(
            guard_social_owner("legal", false).unwrap_err().code,
            "LEGAL_OWNER_REQUIRED"
        );
        assert_eq!(
            guard_social_owner("ops", true).unwrap_err().code,
            "MODULE_PREVIEW_ONLY"
        );
        assert!(guard_social_owner("ops", false).is_ok());
    }
}
