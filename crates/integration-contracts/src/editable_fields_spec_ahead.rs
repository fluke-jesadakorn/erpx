//! Spec-ahead editable field schemas for the 26 contexts not yet OwnerPublished.
//! Runtime remains fail-closed (`supports_agent_patches()==false`) until owning
//! context flips `schema_from_policy` to `OwnerPublished`. This file is the
//! single source for Plan/03-cross-cutting/@-and-ai field tables — no per-context
//! drift. See `crates/integration-contracts/src/editable_fields.rs` and
//! `Plan/traceability.md` (OwnerUnsupported rows).

use super::editable_fields::{EditableFieldDefinition, EditableFieldRisk, EditableFieldValueKind};
use erpx_kernel::DataClassification;

const NO_CHOICES: &[&str] = &[];

// Finance — spec-ahead (target: journal.description, journal.notes, posting memo)
pub const FINANCE_JOURNAL_FIELDS_SPEC_AHEAD: &[EditableFieldDefinition] = &[
    EditableFieldDefinition {
        field_id: "journal.description",
        display_label: "Journal description",
        value_kind: EditableFieldValueKind::Text,
        required: true,
        max_text_bytes: Some(2_048),
        minimum_integer: None,
        maximum_integer: None,
        choices: NO_CHOICES,
        classification: DataClassification::Internal,
        risk: EditableFieldRisk::Low,
        agent_suggestible: true,
        human_apply_permission: "finance.journal.suggestion.apply",
    },
    EditableFieldDefinition {
        field_id: "journal.notes",
        display_label: "Notes",
        value_kind: EditableFieldValueKind::Text,
        required: false,
        max_text_bytes: Some(4_096),
        minimum_integer: None,
        maximum_integer: None,
        choices: NO_CHOICES,
        classification: DataClassification::Internal,
        risk: EditableFieldRisk::Low,
        agent_suggestible: true,
        human_apply_permission: "finance.journal.suggestion.apply",
    },
];

// Projects — spec-ahead (task title/notes)
pub const PROJECTS_TASK_FIELDS_SPEC_AHEAD: &[EditableFieldDefinition] = &[
    EditableFieldDefinition {
        field_id: "task.title",
        display_label: "Task title",
        value_kind: EditableFieldValueKind::Text,
        required: true,
        max_text_bytes: Some(512),
        minimum_integer: None,
        maximum_integer: None,
        choices: NO_CHOICES,
        classification: DataClassification::Internal,
        risk: EditableFieldRisk::Low,
        agent_suggestible: true,
        human_apply_permission: "projects.task.suggestion.apply",
    },
    EditableFieldDefinition {
        field_id: "task.notes",
        display_label: "Notes",
        value_kind: EditableFieldValueKind::Text,
        required: false,
        max_text_bytes: Some(8_192),
        minimum_integer: None,
        maximum_integer: None,
        choices: NO_CHOICES,
        classification: DataClassification::Internal,
        risk: EditableFieldRisk::Low,
        agent_suggestible: true,
        human_apply_permission: "projects.task.suggestion.apply",
    },
];

// Support — spec-ahead (incident/ticket)
pub const SUPPORT_TICKET_FIELDS_SPEC_AHEAD: &[EditableFieldDefinition] = &[
    EditableFieldDefinition {
        field_id: "ticket.summary",
        display_label: "Summary",
        value_kind: EditableFieldValueKind::Text,
        required: true,
        max_text_bytes: Some(1_024),
        minimum_integer: None,
        maximum_integer: None,
        choices: NO_CHOICES,
        classification: DataClassification::Internal,
        risk: EditableFieldRisk::Low,
        agent_suggestible: true,
        human_apply_permission: "support.ticket.suggestion.apply",
    },
    EditableFieldDefinition {
        field_id: "ticket.notes",
        display_label: "Internal notes",
        value_kind: EditableFieldValueKind::Text,
        required: false,
        max_text_bytes: Some(8_192),
        minimum_integer: None,
        maximum_integer: None,
        choices: NO_CHOICES,
        classification: DataClassification::Internal,
        risk: EditableFieldRisk::Low,
        agent_suggestible: true,
        human_apply_permission: "support.ticket.suggestion.apply",
    },
];

// Inventory — spec-ahead
pub const INVENTORY_MOVE_FIELDS_SPEC_AHEAD: &[EditableFieldDefinition] =
    &[EditableFieldDefinition {
        field_id: "move.notes",
        display_label: "Move notes",
        value_kind: EditableFieldValueKind::Text,
        required: false,
        max_text_bytes: Some(2_048),
        minimum_integer: None,
        maximum_integer: None,
        choices: NO_CHOICES,
        classification: DataClassification::Internal,
        risk: EditableFieldRisk::Low,
        agent_suggestible: true,
        human_apply_permission: "inventory.move.suggestion.apply",
    }];

// Remaining 22 contexts share the same shape — single low-risk notes/text field
// spec-ahead so Plan deep files have a bounded schema without inventing runtime.
// Owners flip to `OwnerPublished` per slice (Q14=A) by moving their const into
// `editable_fields.rs` `schema_from_policy` match.

pub const GENERIC_NOTES_FIELD_SPEC_AHEAD: &[EditableFieldDefinition] = &[EditableFieldDefinition {
    field_id: "record.notes",
    display_label: "Notes",
    value_kind: EditableFieldValueKind::Text,
    required: false,
    max_text_bytes: Some(4_096),
    minimum_integer: None,
    maximum_integer: None,
    choices: NO_CHOICES,
    classification: DataClassification::Internal,
    risk: EditableFieldRisk::Low,
    agent_suggestible: true,
    human_apply_permission: "record.notes.suggestion.apply",
}];
