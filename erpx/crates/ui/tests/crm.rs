use erpx_ui::{
    CrmAttributionModel, CrmHandoffDisplayModel, CrmOpportunityDisplayModel, CrmOpportunityStatus,
    CrmProjectionMetadata, CrmSurface, CrmViewState, ScopeKind, UiStateKind,
};

fn fresh_company_metadata() -> CrmProjectionMetadata {
    CrmProjectionMetadata::fresh("2026-07-29T00:00:00Z")
        .with_company_scope("holding-1", "company-1")
        .expect("safe company scope")
}

#[test]
fn crm_scope_binding_is_typed_and_fail_closed_for_sibling_records() {
    let holding = CrmProjectionMetadata::fresh("2026-07-29T00:00:00Z")
        .with_holding_scope("holding-1")
        .expect("safe holding scope");
    assert_eq!(holding.scope_kind(), Some(ScopeKind::Holding));
    assert!(holding.permits_record_scope("holding-1", "company-1", None));
    assert!(holding.permits_record_scope("holding-1", "company-2", Some("branch-2")));
    assert!(!holding.permits_record_scope("holding-2", "company-1", None));

    let company = fresh_company_metadata();
    assert_eq!(company.scope_kind(), Some(ScopeKind::Company));
    assert!(company.permits_record_scope("holding-1", "company-1", None));
    assert!(company.permits_record_scope("holding-1", "company-1", Some("branch-1")));
    assert!(!company.permits_record_scope("holding-1", "company-2", None));
    assert!(!company.permits_record_scope("holding-2", "company-1", None));

    let branch = CrmProjectionMetadata::fresh("2026-07-29T00:00:00Z")
        .with_branch_scope("holding-1", "company-1", "branch-1")
        .expect("safe branch scope");
    assert_eq!(branch.scope_kind(), Some(ScopeKind::Branch));
    assert!(branch.permits_record_scope("holding-1", "company-1", Some("branch-1")));
    assert!(!branch.permits_record_scope("holding-1", "company-1", Some("branch-2")));
    assert!(!branch.permits_record_scope("holding-1", "company-1", None));

    assert!(
        CrmProjectionMetadata::fresh("2026-07-29T00:00:00Z")
            .with_company_scope("holding-1", "database_url=secret")
            .is_none()
    );
}

#[test]
fn crm_current_and_cached_payloads_require_scope_binding() {
    let unscoped_ready = CrmViewState::Ready {
        data: "unbound",
        metadata: CrmProjectionMetadata::fresh("2026-07-29T00:00:00Z"),
    };
    assert!(unscoped_ready.data().is_none());
    assert!(!unscoped_ready.can_render_data());

    let scoped_data = fresh_company_metadata();
    let scoped_ready = CrmViewState::Ready {
        data: "bound",
        metadata: scoped_data.clone(),
    };
    assert_eq!(scoped_ready.data(), Some(&"bound"));
    assert_eq!(
        scoped_ready.data_for_record_scope("holding-1", "company-1", None),
        Some(&"bound")
    );
    assert!(
        scoped_ready
            .data_for_record_scope("holding-1", "company-2", None)
            .is_none()
    );

    for state in [
        CrmViewState::Conflict {
            data: Some("cached conflict"),
            metadata: Some(scoped_data.clone()),
            reason: "revision changed".into(),
        },
        CrmViewState::Offline {
            data: Some("cached offline"),
            metadata: Some(scoped_data),
            reason: "backend unavailable".into(),
        },
    ] {
        assert!(state.data().is_some());
        assert!(state.is_read_only());
    }

    let unscoped_offline = CrmViewState::Offline {
        data: Some("must not render"),
        metadata: Some(CrmProjectionMetadata::fresh("2026-07-29T00:00:00Z")),
        reason: "backend unavailable".into(),
    };
    assert_eq!(unscoped_offline.kind(), Some(UiStateKind::Offline));
    assert!(unscoped_offline.data().is_none());
}

#[test]
fn crm_mutation_preflight_blocks_stale_conflict_offline_and_bad_versions() {
    let writable = CrmViewState::Ready {
        data: (),
        metadata: fresh_company_metadata().with_read_only(false),
    };

    assert!(writable.mutation_allowed(
        CrmSurface::AcceptedLeadHandoffs,
        None,
        "holding-1",
        "company-1",
        None,
    ));
    assert!(!writable.mutation_allowed(
        CrmSurface::AcceptedLeadHandoffs,
        Some(1),
        "holding-1",
        "company-1",
        None,
    ));
    assert!(writable.mutation_allowed(
        CrmSurface::Opportunities,
        None,
        "holding-1",
        "company-1",
        None,
    ));
    assert!(writable.mutation_allowed(
        CrmSurface::Opportunities,
        Some(4),
        "holding-1",
        "company-1",
        None,
    ));
    assert!(!writable.mutation_allowed(
        CrmSurface::Opportunities,
        Some(0),
        "holding-1",
        "company-1",
        None,
    ));
    assert!(!writable.mutation_allowed(
        CrmSurface::Attribution,
        None,
        "holding-1",
        "company-1",
        None,
    ));
    assert!(writable.attribution_mutation_allowed("linear", 4, "holding-1", "company-1", None,));
    assert!(!writable.attribution_mutation_allowed(
        "not-a-domain-model",
        4,
        "holding-1",
        "company-1",
        None,
    ));
    assert!(!writable.mutation_allowed(
        CrmSurface::Opportunities,
        Some(4),
        "holding-1",
        "company-2",
        None,
    ));

    let stale = CrmViewState::Ready {
        data: (),
        metadata: CrmProjectionMetadata::stale("2026-07-28T00:00:00Z")
            .with_company_scope("holding-1", "company-1")
            .expect("safe scope")
            .with_read_only(false),
    };
    assert!(!stale.mutation_allowed(
        CrmSurface::Opportunities,
        Some(4),
        "holding-1",
        "company-1",
        None,
    ));

    let conflict = CrmViewState::Conflict {
        data: Some(()),
        metadata: Some(fresh_company_metadata().with_read_only(false)),
        reason: "revision changed".into(),
    };
    assert!(conflict.reconciliation_required());
    assert!(!conflict.mutation_allowed(
        CrmSurface::Opportunities,
        Some(4),
        "holding-1",
        "company-1",
        None,
    ));

    let offline = CrmViewState::Offline {
        data: Some(()),
        metadata: Some(fresh_company_metadata().with_read_only(false)),
        reason: "backend unavailable".into(),
    };
    assert!(!offline.reconciliation_required());
    assert!(!offline.mutation_allowed(
        CrmSurface::Opportunities,
        Some(4),
        "holding-1",
        "company-1",
        None,
    ));
}

#[test]
fn crm_redaction_and_domain_lifecycle_guards_remain_visible() {
    let handoff = CrmHandoffDisplayModel::from_untrusted_fields(
        "handoff-1",
        [
            erpx_ui::CrmFieldInput::new("name", "safe"),
            erpx_ui::CrmFieldInput::new("name", "{\"secret\":true}"),
            erpx_ui::CrmFieldInput::new("account_label", "client_secret=hidden"),
        ],
    )
    .expect("safe handoff id");
    assert_eq!(handoff.field("name"), Some("safe"));
    assert_eq!(handoff.redacted_field_count(), 2);

    let opportunity = CrmOpportunityDisplayModel::from_untrusted_fields(
        "opportunity-1",
        "draft",
        [erpx_ui::CrmFieldInput::new("name", "Demo")],
    )
    .expect("safe opportunity");
    assert!(opportunity.can_transition_to(CrmOpportunityStatus::Qualified));
    assert!(!opportunity.can_transition_to(CrmOpportunityStatus::Won));
    assert_eq!(
        CrmAttributionModel::from_transport("weighted"),
        Some(CrmAttributionModel::Weighted)
    );

    let unsafe_error = CrmViewState::<()>::Error {
        reason: "database_url=secret".into(),
    };
    assert_eq!(
        unsafe_error.redacted_reason().as_deref(),
        Some("Unavailable")
    );
}
