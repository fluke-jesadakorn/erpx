#![doc = "Versioned, transport-neutral facts exchanged between ERPX bounded contexts."]

#[macro_use]
mod macros;

mod common;
mod editable_fields;
mod envelope;
mod foundation;

pub mod catalog;
pub mod reference;

pub mod assets;
pub mod authority;
pub mod brand;
pub mod commercial;
pub mod crm;
pub mod customer;
pub mod finance;
pub mod identity_access;
pub mod inventory;
pub mod legal;
pub mod maintenance;
pub mod marketing;
pub mod operations;
pub mod organization;
pub mod people_payroll;
pub mod platform_administration;
pub mod pos;
pub mod procurement;
pub mod quality;
pub mod sales;
pub mod support;

pub use authority::{
    AUTHORITY_EVIDENCE_EVENT_TYPE, AUTHORITY_EVIDENCE_VERSION, AuthorityAssignmentEvidence,
    AuthorityEvidenceFact, AuthorityPortalBindingStatus, AuthorityPortalUseCase,
    AuthorityPrincipalKind, AuthorityRealm, AuthoritySharingDirection, AuthoritySharingStatus,
    CustomerPortalBindingAuthorityEvidence, GroupMembershipAuthorityEvidence,
    OrganizationSharingAuthorityEvidence, PrincipalAuthorityEvidence, RoleAuthorityEvidence,
};
pub use common::{
    BoundedText, ContractValidationError, CurrencyCode, DisplayName, MoneyAmount, Quantity,
    ReferenceCode,
};
pub use editable_fields::{
    EditableFieldAvailability, EditableFieldDefinition, EditableFieldError, EditableFieldRisk,
    EditableFieldValueKind, FieldPatchConflict, FieldPatchConflictReason, FieldSuggestionEvidence,
    HumanFieldPatchReview, MAX_FIELD_EVIDENCE_REFS, MAX_FIELD_PATCH_OPERATIONS,
    RecordEditableFieldSchema, RecordFieldPatch, SuggestedFieldChange, SuggestedFieldValue,
    owner_field_value_digest, record_editable_field_schema, record_editable_field_schemas,
};
pub use envelope::{
    AggregateKind, BoundedContext, EventType, IntegrationEvent, IntegrationEventEnvelope,
};
pub use foundation::{
    CostCenterRef, DimensionSet, LocationRef, MonetaryAmountSet, PartyRef, ProjectRef, RecordRef,
};
pub use reference::{
    AuthorizedItemSummary, CURRENT_RECORD_COLLABORATION_POLICIES, CURRENT_RECORD_POLICIES,
    CanonicalRouteMoved, EffectiveGrantChanged, EffectiveGrantRevoked, FUTURE_RECORD_POLICIES,
    HoldingItemDirectoryFact, ITEM_DIRECTORY_FACT_EVENT_TYPE, ITEM_DIRECTORY_FACT_VERSION,
    ITEM_DIRECTORY_OWNER_EVENT_TYPE, ITEM_DIRECTORY_PROJECTION_NAME, ItemDescriptor,
    ItemDescriptorChanged, ItemDescriptorDraft, ItemDescriptorDraftV2, ItemDirectoryFact,
    ItemTombstoned, RecordCollaborationPolicy, RecordPolicyAccessRule, RecordPolicyDefinition,
    RecordPolicyPrincipalKind, RecordPolicyRealm, RecordPolicyScopeRule,
    RecordRef as StableRecordRef, RecordRelationPolicyKind, RecordThreadAudiencePolicy,
    RecordThreadRetentionPolicy, RecordTypeId, RecordVersionRef, ReferenceRelationChanged,
    ReferenceRelationRevoked, ReferenceValidationError, StableFieldRef,
    record_collaboration_policies, record_collaboration_policy, record_policies, record_policy,
    record_policy_id,
};

#[cfg(test)]
mod tests;
