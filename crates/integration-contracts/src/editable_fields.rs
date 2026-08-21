//! Owner-published editable-field schemas and typed Agent patch contracts.
//!
//! A catalog record is never implicitly editable. Each current owner either
//! publishes a bounded field list here or remains explicitly unavailable.
//! Suggested values are a closed enum, so an Agent cannot smuggle an untyped
//! owner command through an opaque JSON document.

use std::collections::BTreeSet;

use chrono::{DateTime, NaiveDate, Utc};
use erpx_kernel::{DataClassification, SessionId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    RecordPolicyDefinition, RecordVersionRef, StableFieldRef, catalog::ContextId,
    reference::CURRENT_RECORD_POLICIES,
};

pub const MAX_FIELD_PATCH_OPERATIONS: usize = 64;
pub const MAX_FIELD_EVIDENCE_REFS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditableFieldValueKind {
    Text,
    Integer,
    Boolean,
    Date,
    DateTime,
    Choice,
    Money,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditableFieldRisk {
    Low,
    Moderate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditableFieldDefinition {
    pub field_id: &'static str,
    pub display_label: &'static str,
    pub value_kind: EditableFieldValueKind,
    pub required: bool,
    pub max_text_bytes: Option<usize>,
    pub minimum_integer: Option<i64>,
    pub maximum_integer: Option<i64>,
    pub choices: &'static [&'static str],
    pub classification: DataClassification,
    pub risk: EditableFieldRisk,
    pub agent_suggestible: bool,
    pub human_apply_permission: &'static str,
}

const NO_CHOICES: &[&str] = &[];
const QUOTE_LANGUAGES: &[&str] = &["en", "th"];

const SALES_QUOTE_FIELDS: &[EditableFieldDefinition] = &[
    EditableFieldDefinition {
        field_id: "quote.notes",
        display_label: "Notes",
        value_kind: EditableFieldValueKind::Text,
        required: false,
        max_text_bytes: Some(2_048),
        minimum_integer: None,
        maximum_integer: None,
        choices: NO_CHOICES,
        classification: DataClassification::Internal,
        risk: EditableFieldRisk::Low,
        agent_suggestible: true,
        human_apply_permission: "sales.quote.suggestion.apply",
    },
    EditableFieldDefinition {
        field_id: "quote.payment_terms_days",
        display_label: "Payment terms (days)",
        value_kind: EditableFieldValueKind::Integer,
        required: true,
        max_text_bytes: None,
        minimum_integer: Some(0),
        maximum_integer: Some(365),
        choices: NO_CHOICES,
        classification: DataClassification::Internal,
        risk: EditableFieldRisk::Moderate,
        agent_suggestible: true,
        human_apply_permission: "sales.quote.suggestion.apply",
    },
    EditableFieldDefinition {
        field_id: "quote.valid_until",
        display_label: "Valid until",
        value_kind: EditableFieldValueKind::DateTime,
        required: true,
        max_text_bytes: None,
        minimum_integer: None,
        maximum_integer: None,
        choices: NO_CHOICES,
        classification: DataClassification::Internal,
        risk: EditableFieldRisk::Moderate,
        agent_suggestible: true,
        human_apply_permission: "sales.quote.suggestion.apply",
    },
    EditableFieldDefinition {
        field_id: "quote.language",
        display_label: "Document language",
        value_kind: EditableFieldValueKind::Choice,
        required: true,
        max_text_bytes: None,
        minimum_integer: None,
        maximum_integer: None,
        choices: QUOTE_LANGUAGES,
        classification: DataClassification::Internal,
        risk: EditableFieldRisk::Low,
        agent_suggestible: true,
        human_apply_permission: "sales.quote.suggestion.apply",
    },
];

const CUSTOMER_NOTE_FIELDS: &[EditableFieldDefinition] = &[EditableFieldDefinition {
    field_id: "note.body",
    display_label: "Note",
    value_kind: EditableFieldValueKind::Text,
    required: true,
    max_text_bytes: Some(8_192),
    minimum_integer: None,
    maximum_integer: None,
    choices: NO_CHOICES,
    classification: DataClassification::Confidential,
    risk: EditableFieldRisk::Moderate,
    agent_suggestible: true,
    human_apply_permission: "customer_360.note.suggestion.apply",
}];

const REQUISITION_FIELDS: &[EditableFieldDefinition] = &[EditableFieldDefinition {
    field_id: "requisition.justification",
    display_label: "Business justification",
    value_kind: EditableFieldValueKind::Text,
    required: true,
    max_text_bytes: Some(4_096),
    minimum_integer: None,
    maximum_integer: None,
    choices: NO_CHOICES,
    classification: DataClassification::Internal,
    risk: EditableFieldRisk::Moderate,
    agent_suggestible: true,
    human_apply_permission: "procurement.requisition.suggestion.apply",
}];

const MAINTENANCE_REQUEST_FIELDS: &[EditableFieldDefinition] = &[EditableFieldDefinition {
    field_id: "request.description",
    display_label: "Request description",
    value_kind: EditableFieldValueKind::Text,
    required: true,
    max_text_bytes: Some(4_096),
    minimum_integer: None,
    maximum_integer: None,
    choices: NO_CHOICES,
    classification: DataClassification::Internal,
    risk: EditableFieldRisk::Low,
    agent_suggestible: true,
    human_apply_permission: "maintenance.request.suggestion.apply",
}];

const MARKETING_CONTENT_FIELDS: &[EditableFieldDefinition] = &[
    EditableFieldDefinition {
        field_id: "content.title",
        display_label: "Title",
        value_kind: EditableFieldValueKind::Text,
        required: true,
        max_text_bytes: Some(512),
        minimum_integer: None,
        maximum_integer: None,
        choices: NO_CHOICES,
        classification: DataClassification::Internal,
        risk: EditableFieldRisk::Low,
        agent_suggestible: true,
        human_apply_permission: "marketing.content.suggestion.apply",
    },
    EditableFieldDefinition {
        field_id: "content.body",
        display_label: "Body",
        value_kind: EditableFieldValueKind::Text,
        required: true,
        max_text_bytes: Some(32_768),
        minimum_integer: None,
        maximum_integer: None,
        choices: NO_CHOICES,
        classification: DataClassification::Internal,
        risk: EditableFieldRisk::Moderate,
        agent_suggestible: true,
        human_apply_permission: "marketing.content.suggestion.apply",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditableFieldAvailability {
    OwnerPublished,
    OwnerUnsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordEditableFieldSchema {
    pub record_type: &'static str,
    pub owner: ContextId,
    pub version: u64,
    pub availability: EditableFieldAvailability,
    pub fields: &'static [EditableFieldDefinition],
}

impl RecordEditableFieldSchema {
    pub fn field(self, field_id: &str) -> Option<&'static EditableFieldDefinition> {
        self.fields.iter().find(|field| field.field_id == field_id)
    }

    pub const fn supports_agent_patches(self) -> bool {
        matches!(self.availability, EditableFieldAvailability::OwnerPublished)
            && !self.fields.is_empty()
    }
}

fn schema_from_policy(policy: RecordPolicyDefinition) -> RecordEditableFieldSchema {
    let fields = match policy.record_type {
        "sales.quote" => SALES_QUOTE_FIELDS,
        "customer-360.note" => CUSTOMER_NOTE_FIELDS,
        "procurement.requisition" => REQUISITION_FIELDS,
        "maintenance.request" => MAINTENANCE_REQUEST_FIELDS,
        "marketing.content" => MARKETING_CONTENT_FIELDS,
        _ => &[],
    };
    RecordEditableFieldSchema {
        record_type: policy.record_type,
        owner: policy.owner,
        version: 1,
        availability: if fields.is_empty() {
            EditableFieldAvailability::OwnerUnsupported
        } else {
            EditableFieldAvailability::OwnerPublished
        },
        fields,
    }
}

/// Exact current-only owner registry. Future catalog entries fail closed.
pub fn record_editable_field_schemas() -> impl Iterator<Item = RecordEditableFieldSchema> {
    CURRENT_RECORD_POLICIES
        .iter()
        .copied()
        .map(schema_from_policy)
}

pub fn record_editable_field_schema(record_type: &str) -> Option<RecordEditableFieldSchema> {
    CURRENT_RECORD_POLICIES
        .iter()
        .copied()
        .find(|policy| policy.record_type == record_type)
        .map(schema_from_policy)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SuggestedFieldValue {
    Unset,
    Text(String),
    Integer(i64),
    Boolean(bool),
    Date(NaiveDate),
    DateTime(DateTime<Utc>),
    Choice(String),
    Money { amount_minor: i64, currency: String },
}

/// Canonical digest used by owner adapters for optimistic field-level
/// conflict detection. `Unset` is a real typed state, never an empty-string
/// sentinel.
pub fn owner_field_value_digest(value: &SuggestedFieldValue) -> Result<String, EditableFieldError> {
    let bytes = serde_json::to_vec(value).map_err(|_| EditableFieldError::InvalidValue)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FieldSuggestionEvidence {
    Record {
        record: RecordVersionRef,
    },
    ProtectedMaterial {
        evidence_ref: String,
        digest: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuggestedFieldChange {
    pub field: StableFieldRef,
    pub previous_value_digest: String,
    pub proposed_value: SuggestedFieldValue,
    pub confidence_basis_points: u16,
    pub evidence: Vec<FieldSuggestionEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordFieldPatch {
    pub source: RecordVersionRef,
    pub schema_version: u64,
    pub changes: Vec<SuggestedFieldChange>,
}

impl RecordFieldPatch {
    pub fn validate(&self) -> Result<(), EditableFieldError> {
        self.source
            .validate()
            .map_err(|_| EditableFieldError::InvalidRecord)?;
        let record_type = self.source.record().record_type().as_str();
        let schema = record_editable_field_schema(record_type)
            .filter(|schema| schema.supports_agent_patches())
            .ok_or(EditableFieldError::SchemaUnavailable)?;
        if self.schema_version != schema.version
            || self.changes.is_empty()
            || self.changes.len() > MAX_FIELD_PATCH_OPERATIONS
        {
            return Err(EditableFieldError::InvalidPatch);
        }
        let mut field_ids = BTreeSet::new();
        for change in &self.changes {
            change
                .field
                .validate()
                .map_err(|_| EditableFieldError::InvalidField)?;
            if change.field.record() != self.source.record()
                || change.field.schema_version() != schema.version
                || !field_ids.insert(change.field.field_id())
                || !valid_digest(&change.previous_value_digest)
                || change.confidence_basis_points > 10_000
                || change.evidence.len() > MAX_FIELD_EVIDENCE_REFS
            {
                return Err(EditableFieldError::InvalidPatch);
            }
            let definition = schema
                .field(change.field.field_id())
                .filter(|field| field.agent_suggestible)
                .ok_or(EditableFieldError::FieldUnavailable)?;
            validate_value(definition, &change.proposed_value)?;
            for evidence in &change.evidence {
                validate_evidence(evidence)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldPatchConflictReason {
    RecordVersionChanged,
    FieldValueChanged,
    SchemaChanged,
    AuthorizationChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldPatchConflict {
    pub field: StableFieldRef,
    pub reason: FieldPatchConflictReason,
    pub current_record_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanFieldPatchReview {
    pub reviewer_principal_ref: String,
    pub reviewer_session_id: SessionId,
    pub reviewed_at: DateTime<Utc>,
    pub edited_patch_digest: String,
}

impl HumanFieldPatchReview {
    pub fn validate(&self) -> Result<(), EditableFieldError> {
        if !valid_text(&self.reviewer_principal_ref, 256)
            || self.reviewer_session_id.as_uuid().is_nil()
            || !valid_digest(&self.edited_patch_digest)
        {
            return Err(EditableFieldError::InvalidHumanReview);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EditableFieldError {
    #[error("record field schema is unavailable")]
    SchemaUnavailable,
    #[error("record reference is invalid")]
    InvalidRecord,
    #[error("field reference is invalid")]
    InvalidField,
    #[error("field is not owner-published for Agent suggestions")]
    FieldUnavailable,
    #[error("typed field patch is invalid")]
    InvalidPatch,
    #[error("typed field value is invalid")]
    InvalidValue,
    #[error("field suggestion evidence is invalid")]
    InvalidEvidence,
    #[error("human field-patch review is invalid")]
    InvalidHumanReview,
}

fn validate_value(
    definition: &EditableFieldDefinition,
    value: &SuggestedFieldValue,
) -> Result<(), EditableFieldError> {
    if matches!(value, SuggestedFieldValue::Unset) {
        return if definition.required {
            Err(EditableFieldError::InvalidValue)
        } else {
            Ok(())
        };
    }
    let valid = match (definition.value_kind, value) {
        (EditableFieldValueKind::Text, SuggestedFieldValue::Text(value)) => definition
            .max_text_bytes
            .is_some_and(|maximum| valid_text(value, maximum)),
        (EditableFieldValueKind::Integer, SuggestedFieldValue::Integer(value)) => {
            definition
                .minimum_integer
                .is_none_or(|minimum| *value >= minimum)
                && definition
                    .maximum_integer
                    .is_none_or(|maximum| *value <= maximum)
        }
        (EditableFieldValueKind::Boolean, SuggestedFieldValue::Boolean(_))
        | (EditableFieldValueKind::Date, SuggestedFieldValue::Date(_))
        | (EditableFieldValueKind::DateTime, SuggestedFieldValue::DateTime(_)) => true,
        (EditableFieldValueKind::Choice, SuggestedFieldValue::Choice(value)) => {
            definition.choices.contains(&value.as_str())
        }
        (
            EditableFieldValueKind::Money,
            SuggestedFieldValue::Money {
                amount_minor,
                currency,
            },
        ) => {
            *amount_minor >= 0
                && currency.len() == 3
                && currency.bytes().all(|byte| byte.is_ascii_uppercase())
        }
        _ => false,
    };
    valid.then_some(()).ok_or(EditableFieldError::InvalidValue)
}

fn validate_evidence(value: &FieldSuggestionEvidence) -> Result<(), EditableFieldError> {
    match value {
        FieldSuggestionEvidence::Record { record } => record
            .validate()
            .map_err(|_| EditableFieldError::InvalidEvidence),
        FieldSuggestionEvidence::ProtectedMaterial {
            evidence_ref,
            digest,
        } if valid_text(evidence_ref, 256) && valid_digest(digest) => Ok(()),
        FieldSuggestionEvidence::ProtectedMaterial { .. } => {
            Err(EditableFieldError::InvalidEvidence)
        }
    }
}

fn valid_text(value: &str, maximum: usize) -> bool {
    let value = value.trim();
    !value.is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use erpx_kernel::RecordId;
    use uuid::Uuid;

    use super::*;
    use crate::{
        RecordTypeId, StableRecordRef,
        catalog::{ContextLifecycle, RECORD_TYPES},
    };

    #[test]
    fn every_current_family_declares_editability_and_future_families_fail_closed() {
        let schemas = record_editable_field_schemas().collect::<Vec<_>>();
        assert_eq!(schemas.len(), 93);
        assert!(
            schemas
                .iter()
                .all(|schema| schema.owner.lifecycle() == ContextLifecycle::Current)
        );
        assert!(schemas.iter().any(|schema| {
            schema.record_type == "sales.quote" && schema.supports_agent_patches()
        }));
        assert!(schemas.iter().any(|schema| {
            schema.record_type == "marketing.content" && schema.supports_agent_patches()
        }));
        assert!(
            RECORD_TYPES
                .iter()
                .filter(|record| record.owner.lifecycle() != ContextLifecycle::Current)
                .all(|record| record_editable_field_schema(record.id).is_none())
        );
    }

    #[test]
    fn typed_patch_rejects_undeclared_fields_and_wrong_value_kinds() {
        let record = StableRecordRef::try_new(
            RecordId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            ContextId::Sales,
            RecordTypeId::try_new("sales.quote").unwrap(),
        )
        .unwrap();
        let source = RecordVersionRef::try_new(record.clone(), 7).unwrap();
        let mut patch = RecordFieldPatch {
            source,
            schema_version: 1,
            changes: vec![SuggestedFieldChange {
                field: StableFieldRef::try_new(record, "quote.payment_terms_days", 1).unwrap(),
                previous_value_digest: owner_field_value_digest(&SuggestedFieldValue::Unset)
                    .unwrap(),
                proposed_value: SuggestedFieldValue::Integer(30),
                confidence_basis_points: 9_000,
                evidence: Vec::new(),
            }],
        };
        assert!(patch.validate().is_ok());
        patch.changes[0].proposed_value = SuggestedFieldValue::Text("30".into());
        assert_eq!(patch.validate(), Err(EditableFieldError::InvalidValue));
        patch.changes[0].field =
            StableFieldRef::try_new(patch.source.record().clone(), "quote.release_decision", 1)
                .unwrap();
        assert_eq!(patch.validate(), Err(EditableFieldError::FieldUnavailable));
    }
}
