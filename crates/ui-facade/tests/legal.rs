use erpx_ui::{
    LEGAL_ROUTES, LegalAccessAction, LegalAccessHistoryEntryDisplayModel,
    LegalAccessHistoryEntryId, LegalApprovalDisplayModel, LegalApprovalId, LegalApprovalStatus,
    LegalContractDisplayModel, LegalContractId, LegalContractStatus, LegalDisplayTextError,
    LegalDocumentDisplayModel, LegalDocumentId, LegalDocumentStatus,
    LegalDocumentVersionDisplayModel, LegalDocumentVersionId, LegalIdentifierError, LegalPageState,
    LegalPageStateKind, LegalPartyDisplayModel, LegalPartyId, LegalPartyRole, LegalPrincipalId,
    LegalProjectionMetadata, LegalRetentionDisplayModel, LegalRetentionId, LegalRetentionStatus,
    LegalRoute, LegalRouteId, LegalRouteRegistry, Locale, MAX_LEGAL_DISPLAY_TEXT_BYTES,
    MAX_LEGAL_IDENTIFIER_BYTES, ProjectionFreshness, UiStateKind,
};

#[test]
fn legal_route_registry_covers_the_allow_list_with_bilingual_labels() {
    let registry = LegalRouteRegistry::new();
    let expected = [
        (LegalRouteId::Legal, "/legal"),
        (LegalRouteId::Contracts, "/legal/contracts"),
        (LegalRouteId::Documents, "/legal/documents"),
        (LegalRouteId::AccessHistory, "/legal/access-history"),
        (LegalRouteId::Retention, "/legal/retention"),
    ];

    assert_eq!(LEGAL_ROUTES, registry.definitions());
    assert_eq!(registry.definitions().len(), expected.len());
    for (definition, (id, path)) in registry.definitions().iter().zip(expected) {
        assert_eq!(definition.id, id);
        assert_eq!(definition.template, path);
        assert!(!definition.label_for(Locale::En).is_empty());
        assert!(!definition.label_for(Locale::Th).is_empty());
        assert_ne!(
            definition.label_for(Locale::En),
            definition.label_for(Locale::Th)
        );
        assert_eq!(registry.resolve(path).map(LegalRoute::path), Ok(path));
    }
    assert_eq!(
        registry.resolve("/legal/documents?token=hidden"),
        Ok(LegalRoute::Documents)
    );
    assert!(registry.resolve("/legal/unknown").is_err());
}

#[test]
fn opaque_ids_and_scalars_are_bounded_and_redacted() {
    let valid_id = "contract-opaque-1";
    assert_eq!(LegalContractId::new(valid_id).unwrap().as_str(), valid_id);
    assert_eq!(
        LegalContractId::new("").unwrap_err(),
        LegalIdentifierError::Empty
    );
    assert_eq!(
        LegalContractId::new("id with spaces").unwrap_err(),
        LegalIdentifierError::UnsafeCharacters
    );
    assert_eq!(
        LegalContractId::new("access_token").unwrap_err(),
        LegalIdentifierError::SensitiveMarker
    );
    assert_eq!(
        LegalContractId::new("x".repeat(MAX_LEGAL_IDENTIFIER_BYTES + 1)).unwrap_err(),
        LegalIdentifierError::TooLong
    );

    assert!(erpx_ui::LegalScalar::new("scalar title").is_ok());
    assert_eq!(
        erpx_ui::LegalScalar::new("{\"document\":true}").unwrap_err(),
        LegalDisplayTextError::StructuredValue
    );
    assert_eq!(
        erpx_ui::LegalScalar::new("https://storage.example/document").unwrap_err(),
        LegalDisplayTextError::Url
    );
    assert_eq!(
        erpx_ui::LegalScalar::new("signed_url=secret").unwrap_err(),
        LegalDisplayTextError::SensitiveMarker
    );
    assert_eq!(
        erpx_ui::LegalScalar::new("x".repeat(MAX_LEGAL_DISPLAY_TEXT_BYTES + 1)).unwrap_err(),
        LegalDisplayTextError::TooLong
    );
}

#[test]
fn models_are_scalar_redacted_contracts_for_each_legal_surface() {
    let contract_id = LegalContractId::new("contract-1").unwrap();
    let party_id = LegalPartyId::new("party-1").unwrap();
    let principal_id = LegalPrincipalId::new("principal-1").unwrap();
    let document_id = LegalDocumentId::new("document-1").unwrap();
    let version_id = LegalDocumentVersionId::new("version-1").unwrap();
    let retention_id = LegalRetentionId::new("retention-1").unwrap();

    let contract = LegalContractDisplayModel::new(
        contract_id.clone(),
        "CTR-001",
        "Supply agreement",
        LegalContractStatus::Active,
        "2026-01-01",
        Some("2027-01-01".into()),
        2,
    )
    .unwrap();
    let party = LegalPartyDisplayModel::new(
        party_id,
        contract_id.clone(),
        "Example Company",
        LegalPartyRole::Counterparty,
        Some("party-ref".into()),
    )
    .unwrap();
    let approval = LegalApprovalDisplayModel::new(
        LegalApprovalId::new("approval-1").unwrap(),
        contract_id.clone(),
        principal_id.clone(),
        "Reviewer",
        LegalApprovalStatus::Approved,
        Some("2026-01-02".into()),
        Some("Reviewed".into()),
    )
    .unwrap();
    let retention = LegalRetentionDisplayModel::new(
        retention_id.clone(),
        "Contract retention",
        LegalRetentionStatus::Active,
        Some("2027-01-01".into()),
        Some(365),
        None,
    )
    .unwrap();
    let document = LegalDocumentDisplayModel::new(
        document_id.clone(),
        contract_id.clone(),
        "Signed agreement metadata",
        "Agreement",
        LegalDocumentStatus::Active,
        Some(version_id.clone()),
        Some(retention_id),
    )
    .unwrap();
    let version = LegalDocumentVersionDisplayModel::new(
        version_id,
        document_id.clone(),
        "v1",
        "2026-01-02",
        principal_id.clone(),
        "Reviewer",
        Some("Initial approved version".into()),
        Some(12),
    )
    .unwrap();
    let access = LegalAccessHistoryEntryDisplayModel::new(
        LegalAccessHistoryEntryId::new("access-1").unwrap(),
        document_id,
        principal_id,
        "Reviewer",
        LegalAccessAction::Viewed,
        "2026-01-03T00:00:00Z",
        "Recorded",
    )
    .unwrap();

    assert_eq!(contract.title(), "Supply agreement");
    assert_eq!(LegalContractStatus::Active.as_str(), "active");
    assert_eq!(party.role(), LegalPartyRole::Counterparty);
    assert_eq!(LegalPartyRole::Counterparty.as_str(), "counterparty");
    assert_eq!(approval.status(), LegalApprovalStatus::Approved);
    assert_eq!(retention.retention_days(), Some(365));
    assert_eq!(document.document_type(), "Agreement");
    assert_eq!(version.page_count(), Some(12));
    assert_eq!(access.action(), LegalAccessAction::Viewed);
}

#[test]
fn all_legal_states_have_explicit_safe_copy_and_read_only_metadata() {
    let metadata = LegalProjectionMetadata::fresh("2026-07-25T00:00:00Z")
        .unwrap()
        .with_read_only(true);
    let states = [
        LegalPageState::<()>::Loading {
            metadata: Some(metadata.clone()),
        },
        LegalPageState::<()>::Empty {
            metadata: metadata.clone(),
        },
        LegalPageState::<()>::Ready {
            data: (),
            metadata: metadata.clone(),
        },
        LegalPageState::<()>::Denied {
            metadata: Some(metadata.clone()),
        },
        LegalPageState::<()>::Stale {
            data: (),
            metadata: LegalProjectionMetadata::stale("2026-07-24T00:00:00Z").unwrap(),
        },
        LegalPageState::<()>::ReadOnly {
            data: (),
            metadata: metadata.clone(),
        },
        LegalPageState::<()>::Conflict {
            data: None,
            metadata: Some(metadata.clone()),
        },
        LegalPageState::<()>::Offline {
            data: None,
            metadata: Some(metadata.clone()),
        },
        LegalPageState::<()>::Error,
    ];
    let expected = [
        LegalPageStateKind::Loading,
        LegalPageStateKind::Empty,
        LegalPageStateKind::Ready,
        LegalPageStateKind::Denied,
        LegalPageStateKind::Stale,
        LegalPageStateKind::ReadOnly,
        LegalPageStateKind::Conflict,
        LegalPageStateKind::Offline,
        LegalPageStateKind::Error,
    ];

    for (state, expected_kind) in states.iter().zip(expected) {
        assert_eq!(state.state_kind(), expected_kind);
        assert!(
            state.state_label(Locale::En).is_some() || expected_kind == LegalPageStateKind::Ready
        );
        assert!(
            state.state_message(Locale::Th).is_some() || expected_kind == LegalPageStateKind::Ready
        );
        assert!(!state.can_mutate());
    }
    assert!(states[5].is_read_only());
    assert_eq!(states[0].kind(), Some(UiStateKind::Loading));
    assert_eq!(states[2].kind(), None);
    assert_eq!(metadata.freshness(), ProjectionFreshness::Fresh);
    assert!(metadata.is_read_only());
}
