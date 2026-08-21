//! Fail-closed product-completion policy for current catalog record families.
//!
//! This module composes the owner registries that already govern publication,
//! transport, authorization, collaboration, and editable fields. It does not
//! create a second list of record types: a current family is complete only when
//! every authoritative registry resolves the same catalog identity.

use crate::{
    RecordCollaborationPolicy, RecordEditableFieldSchema, RecordPolicyDefinition,
    catalog::{
        ACTIVE_SERVICES, ContextId, ContextLifecycle, LIVE_CURRENT_RECORD_PUBLISHERS,
        RecordPublisherMapping, RecordTypeDescriptor, ServiceDescriptor, ServiceKind, ServiceOwner,
        record_publisher, record_type,
    },
    editable_fields::EditableFieldAvailability,
    record_collaboration_policy, record_editable_field_schema,
    reference::CURRENT_RECORD_POLICIES,
};

/// One catalog-backed Console shell serves every complete current family.
///
/// The strings are registration markers consumed by the architecture gate;
/// they are component identities rather than presentation labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrentRecordSurfaceRegistration {
    pub canonical_detail_route_shape: &'static str,
    pub shell_component: &'static str,
    pub references_component: &'static str,
    pub activity_component: &'static str,
}

pub const CURRENT_RECORD_SURFACE: CurrentRecordSurfaceRegistration =
    CurrentRecordSurfaceRegistration {
        canonical_detail_route_shape: "/{module}/{record}/{id}",
        shell_component: "RecordDetailHost",
        references_component: "RecordReferences",
        activity_component: "AuthorizedRecordActivityRail",
    };

/// Typed-token kinds an authorized Item Directory result may produce for a
/// record family. Field tokens stay unavailable unless the owner publishes a
/// stable editable-field schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordTokenAutocompleteEligibility {
    AuthorizedRecordAndVersion,
    AuthorizedRecordVersionAndOwnerFields { schema_version: u64 },
}

impl RecordTokenAutocompleteEligibility {
    pub const fn record_and_version_tokens(self) -> bool {
        true
    }

    pub const fn field_tokens(self) -> bool {
        matches!(self, Self::AuthorizedRecordVersionAndOwnerFields { .. })
    }
}

/// Owner policy for Agent-authored field suggestions. A denial is a stable
/// policy outcome, never an invitation to fall back to an untyped mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordAgentEligibility {
    AllowedTypedFieldSuggestions { schema_version: u64 },
    DeniedNoOwnerPublishedFields,
}

/// All policy-classified business decisions on current records require
/// authenticated human evidence. Ordinary lifecycle commands that are not a
/// business decision remain governed by their owner command policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordDecisionPolicyClassification {
    HumanOnlyForDecisionShapedCommands,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnerTransportRegistration {
    pub command: &'static ServiceDescriptor,
    pub query: &'static ServiceDescriptor,
}

/// Projection of the authoritative registries for one complete current
/// record family. No payload or synthetic descriptor is created here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrentRecordCompletionRegistration {
    pub record: &'static RecordTypeDescriptor,
    pub publisher: &'static RecordPublisherMapping,
    pub transport: OwnerTransportRegistration,
    pub access_policy: &'static RecordPolicyDefinition,
    pub collaboration_policy: &'static RecordCollaborationPolicy,
    pub editable_fields: RecordEditableFieldSchema,
    pub surface: CurrentRecordSurfaceRegistration,
    pub token_autocomplete: RecordTokenAutocompleteEligibility,
    pub agent_eligibility: RecordAgentEligibility,
    pub decision_policy: RecordDecisionPolicyClassification,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrentRecordCompletionGap {
    UnknownRecord,
    FutureRecord,
    MissingLiveV2Publisher,
    MissingOwnerCommandTransport,
    MissingOwnerQueryTransport,
    MissingAccessPolicy,
    InvalidAccessMatrix,
    MissingCollaborationPolicy,
    InvalidCollaborationPolicy,
    MissingAgentPolicy,
    OwnerMismatch,
}

/// Resolves one current family only after every product-completion registry
/// agrees on its identity and owner. Future catalog entries deliberately fail
/// before policy or surface registration.
pub fn current_record_completion_registration(
    record_type_id: &str,
) -> Result<CurrentRecordCompletionRegistration, CurrentRecordCompletionGap> {
    let record = record_type(record_type_id).ok_or(CurrentRecordCompletionGap::UnknownRecord)?;
    if record.owner.lifecycle() != ContextLifecycle::Current {
        return Err(CurrentRecordCompletionGap::FutureRecord);
    }
    if !LIVE_CURRENT_RECORD_PUBLISHERS.contains(&record.id) {
        return Err(CurrentRecordCompletionGap::MissingLiveV2Publisher);
    }
    let publisher = record_publisher(record.id)
        .filter(|publisher| {
            publisher.descriptor_contract_version == 2
                && publisher.event_version == crate::ITEM_DIRECTORY_FACT_VERSION
        })
        .ok_or(CurrentRecordCompletionGap::MissingLiveV2Publisher)?;
    let transport = owner_transport(record.id, record.owner)?;
    let access_policy = CURRENT_RECORD_POLICIES
        .iter()
        .find(|policy| policy.record_type == record.id)
        .ok_or(CurrentRecordCompletionGap::MissingAccessPolicy)?;
    if access_policy.version == 0
        || access_policy.permission.trim().is_empty()
        || access_policy.principal_kinds.is_empty()
        || access_policy.realms.is_empty()
    {
        return Err(CurrentRecordCompletionGap::InvalidAccessMatrix);
    }
    let collaboration_policy = record_collaboration_policy(record.id)
        .ok_or(CurrentRecordCompletionGap::MissingCollaborationPolicy)?;
    if collaboration_policy.version == 0
        || collaboration_policy.allowed_relation_kinds.is_empty()
        || collaboration_policy.custom_relations_enabled
        || !collaboration_policy.discussion_threads_enabled
        || !collaboration_policy.attachments_enabled
        || !collaboration_policy.human_internal_mutations_only
        || collaboration_policy.retention.minimum_days == 0
    {
        return Err(CurrentRecordCompletionGap::InvalidCollaborationPolicy);
    }
    let editable_fields = record_editable_field_schema(record.id)
        .ok_or(CurrentRecordCompletionGap::MissingAgentPolicy)?;
    if publisher.owner != record.owner
        || access_policy.owner != record.owner
        || collaboration_policy.owner != record.owner
        || editable_fields.owner != record.owner
    {
        return Err(CurrentRecordCompletionGap::OwnerMismatch);
    }

    let (token_autocomplete, agent_eligibility) = match editable_fields.availability {
        EditableFieldAvailability::OwnerPublished if !editable_fields.fields.is_empty() => (
            RecordTokenAutocompleteEligibility::AuthorizedRecordVersionAndOwnerFields {
                schema_version: editable_fields.version,
            },
            RecordAgentEligibility::AllowedTypedFieldSuggestions {
                schema_version: editable_fields.version,
            },
        ),
        EditableFieldAvailability::OwnerUnsupported if editable_fields.fields.is_empty() => (
            RecordTokenAutocompleteEligibility::AuthorizedRecordAndVersion,
            RecordAgentEligibility::DeniedNoOwnerPublishedFields,
        ),
        _ => return Err(CurrentRecordCompletionGap::MissingAgentPolicy),
    };

    Ok(CurrentRecordCompletionRegistration {
        record,
        publisher,
        transport,
        access_policy,
        collaboration_policy,
        editable_fields,
        surface: CURRENT_RECORD_SURFACE,
        token_autocomplete,
        agent_eligibility,
        decision_policy: RecordDecisionPolicyClassification::HumanOnlyForDecisionShapedCommands,
    })
}

pub fn current_record_completion_registrations()
-> impl Iterator<Item = Result<CurrentRecordCompletionRegistration, CurrentRecordCompletionGap>> {
    crate::catalog::RECORD_TYPES
        .iter()
        .filter(|record| record.owner.lifecycle() == ContextLifecycle::Current)
        .map(|record| current_record_completion_registration(record.id))
}

fn owner_transport(
    record_type_id: &str,
    owner: ContextId,
) -> Result<OwnerTransportRegistration, CurrentRecordCompletionGap> {
    let command = owner_service(record_type_id, owner, ServiceKind::Command)
        .ok_or(CurrentRecordCompletionGap::MissingOwnerCommandTransport)?;
    let query = owner_service(record_type_id, owner, ServiceKind::Query)
        .ok_or(CurrentRecordCompletionGap::MissingOwnerQueryTransport)?;
    Ok(OwnerTransportRegistration { command, query })
}

fn owner_service(
    record_type_id: &str,
    owner: ContextId,
    kind: ServiceKind,
) -> Option<&'static ServiceDescriptor> {
    let required_name = match (record_type_id, kind) {
        ("finance.vendor-invoice" | "finance.payment", ServiceKind::Command) => {
            Some("FinancePayablesCommandService")
        }
        ("finance.vendor-invoice" | "finance.payment", ServiceKind::Query) => {
            Some("FinancePayablesQueryService")
        }
        _ => None,
    };
    ACTIVE_SERVICES.iter().find(|service| {
        service.owner == ServiceOwner::Context(owner)
            && service.kind == kind
            && required_name.is_none_or(|name| service.name == name)
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::{
        catalog::{RECORD_TYPES, record_publisher},
        record_collaboration_policy, record_editable_field_schema,
    };

    #[test]
    fn all_current_families_resolve_the_same_authoritative_completion_registries() {
        let current_catalog = RECORD_TYPES
            .iter()
            .filter(|record| record.owner.lifecycle() == ContextLifecycle::Current)
            .map(|record| record.id)
            .collect::<BTreeSet<_>>();
        let registrations = current_record_completion_registrations()
            .collect::<Result<Vec<_>, _>>()
            .expect("every current record family must be product-complete");
        let registered = registrations
            .iter()
            .map(|registration| registration.record.id)
            .collect::<BTreeSet<_>>();

        assert_eq!(registered, current_catalog);
        assert_eq!(
            registered,
            LIVE_CURRENT_RECORD_PUBLISHERS
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
        );
        for registration in registrations {
            let record = registration.record;
            assert_eq!(
                registration.publisher,
                record_publisher(record.id).expect("live V2 publisher")
            );
            assert_eq!(registration.publisher.descriptor_contract_version, 2);
            assert_eq!(registration.access_policy.record_type, record.id);
            assert_eq!(registration.collaboration_policy.record_type, record.id);
            assert_eq!(registration.editable_fields.record_type, record.id);
            assert_eq!(registration.surface, CURRENT_RECORD_SURFACE);
            assert_eq!(
                registration.surface.canonical_detail_route_shape,
                "/{module}/{record}/{id}"
            );
            assert!(registration.token_autocomplete.record_and_version_tokens());
            assert_eq!(
                registration.token_autocomplete.field_tokens(),
                registration.editable_fields.supports_agent_patches()
            );
            assert_eq!(
                matches!(
                    registration.agent_eligibility,
                    RecordAgentEligibility::AllowedTypedFieldSuggestions { .. }
                ),
                registration.editable_fields.supports_agent_patches()
            );
            assert_eq!(
                registration.decision_policy,
                RecordDecisionPolicyClassification::HumanOnlyForDecisionShapedCommands
            );
            assert_eq!(
                registration.transport.command.owner,
                ServiceOwner::Context(record.owner)
            );
            assert_eq!(registration.transport.command.kind, ServiceKind::Command);
            assert_eq!(registration.transport.query.kind, ServiceKind::Query);
        }
    }

    #[test]
    fn future_families_are_excluded_until_their_live_completion_registries_exist() {
        let future = RECORD_TYPES
            .iter()
            .filter(|record| record.owner.lifecycle() != ContextLifecycle::Current)
            .collect::<Vec<_>>();
        assert!(!future.is_empty());
        for record in future {
            assert_eq!(
                current_record_completion_registration(record.id),
                Err(CurrentRecordCompletionGap::FutureRecord),
                "{} escaped the future-record fence",
                record.id
            );
            assert!(!LIVE_CURRENT_RECORD_PUBLISHERS.contains(&record.id));
            assert!(record_publisher(record.id).is_none());
            assert!(record_collaboration_policy(record.id).is_none());
            assert!(record_editable_field_schema(record.id).is_none());
        }
    }

    #[test]
    fn agent_denial_and_owner_schema_allowance_are_both_explicit() {
        let allowed = current_record_completion_registration("sales.quote").unwrap();
        assert!(matches!(
            allowed.agent_eligibility,
            RecordAgentEligibility::AllowedTypedFieldSuggestions { schema_version: 1 }
        ));
        assert!(allowed.token_autocomplete.field_tokens());

        let denied = current_record_completion_registration("finance.payment").unwrap();
        assert_eq!(
            denied.agent_eligibility,
            RecordAgentEligibility::DeniedNoOwnerPublishedFields
        );
        assert!(!denied.token_autocomplete.field_tokens());
    }

    #[test]
    fn payables_records_require_the_dedicated_finance_transport_pair() {
        for record_type in ["finance.vendor-invoice", "finance.payment"] {
            let registration = current_record_completion_registration(record_type).unwrap();
            assert_eq!(
                registration.transport.command.name,
                "FinancePayablesCommandService"
            );
            assert_eq!(
                registration.transport.query.name,
                "FinancePayablesQueryService"
            );
        }
        let receivable = current_record_completion_registration("finance.invoice").unwrap();
        assert_eq!(receivable.transport.command.name, "FinanceCommandService");
        assert_eq!(receivable.transport.query.name, "FinanceQueryService");
    }
}
