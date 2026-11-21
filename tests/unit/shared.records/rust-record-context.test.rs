//! Native Records context-read policy and orchestration negatives.
use erpx_server_records::public::record_context::{
    BacklinkEdge, HistoryRow, LinkTarget, MentionRow, RecordContextActor, RecordContextGuard,
    RecordContextRepository, RecordRow, ReferenceEdge, SuggestionRow, record_context,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::{BTreeMap, BTreeSet};

struct FakeRepository {
    authority: bool,
    row: Option<RecordRow>,
    references: Vec<ReferenceEdge>,
    backlinks: Vec<BacklinkEdge>,
    mentions: Vec<MentionRow>,
    suggestions: Vec<SuggestionRow>,
    history: Vec<HistoryRow>,
    targets: BTreeMap<(String, String), LinkTarget>,
}

impl Default for FakeRepository {
    fn default() -> Self {
        Self {
            authority: true,
            row: Some(record()),
            references: Vec::new(),
            backlinks: Vec::new(),
            mentions: Vec::new(),
            suggestions: Vec::new(),
            history: Vec::new(),
            targets: BTreeMap::new(),
        }
    }
}

fn failure() -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, "SYNTHETIC", "synthetic failure")
}

impl RecordContextRepository for FakeRepository {
    type Error = BusinessError;

    fn current_authority<'a>(
        &'a mut self,
        _: &'a RecordContextActor,
        _: i64,
    ) -> erpx_server_records::public::record_context::RecordContextFuture<'a, bool, Self::Error>
    {
        Box::pin(async move { Ok(self.authority) })
    }
    fn record<'a>(
        &'a mut self,
        _: &'a RecordContextActor,
        _: &'a str,
        _: &'a str,
    ) -> erpx_server_records::public::record_context::RecordContextFuture<
        'a,
        Option<RecordRow>,
        Self::Error,
    > {
        Box::pin(async move { Ok(self.row.clone()) })
    }
    fn references<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
    ) -> erpx_server_records::public::record_context::RecordContextFuture<
        'a,
        Vec<ReferenceEdge>,
        Self::Error,
    > {
        Box::pin(async move { Ok(self.references.clone()) })
    }
    fn backlinks<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
    ) -> erpx_server_records::public::record_context::RecordContextFuture<
        'a,
        Vec<BacklinkEdge>,
        Self::Error,
    > {
        Box::pin(async move { Ok(self.backlinks.clone()) })
    }
    fn mentions<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
    ) -> erpx_server_records::public::record_context::RecordContextFuture<
        'a,
        Vec<MentionRow>,
        Self::Error,
    > {
        Box::pin(async move { Ok(self.mentions.clone()) })
    }
    fn suggestions<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
    ) -> erpx_server_records::public::record_context::RecordContextFuture<
        'a,
        Vec<SuggestionRow>,
        Self::Error,
    > {
        Box::pin(async move { Ok(self.suggestions.clone()) })
    }
    fn history<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
    ) -> erpx_server_records::public::record_context::RecordContextFuture<
        'a,
        Vec<HistoryRow>,
        Self::Error,
    > {
        Box::pin(async move { Ok(self.history.clone()) })
    }
    fn link_target<'a>(
        &'a mut self,
        _: &'a RecordContextActor,
        module_id: &'a str,
        id: &'a str,
    ) -> erpx_server_records::public::record_context::RecordContextFuture<
        'a,
        Option<LinkTarget>,
        Self::Error,
    > {
        Box::pin(async move {
            Ok(self
                .targets
                .get(&(module_id.to_owned(), id.to_owned()))
                .cloned())
        })
    }
}

#[derive(Default)]
struct FakeGuard {
    visible: BTreeSet<(String, String)>,
    source_fails: bool,
    target_fails: bool,
    source_denied: bool,
}

impl FakeGuard {
    fn allow(mut self, module_id: &str, id: &str) -> Self {
        self.visible.insert((module_id.into(), id.into()));
        self
    }
}

impl RecordContextGuard for FakeGuard {
    type Error = BusinessError;

    fn authorize_source<'a>(
        &'a mut self,
        _: &'a RecordContextActor,
        _: &'a str,
        _: &'a str,
    ) -> erpx_server_records::public::record_context::RecordContextFuture<'a, (), Self::Error> {
        Box::pin(async move {
            if self.source_fails {
                return Err(failure());
            }
            if self.source_denied {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "SOURCE_FORBIDDEN",
                    "Synthetic source denial.",
                ));
            }
            Ok(())
        })
    }
    fn target_visible<'a>(
        &'a mut self,
        _: &'a RecordContextActor,
        module_id: &'a str,
        id: &'a str,
    ) -> erpx_server_records::public::record_context::RecordContextFuture<'a, bool, Self::Error>
    {
        Box::pin(async move {
            if self.target_fails {
                return Err(failure());
            }
            Ok(self
                .visible
                .contains(&(module_id.to_owned(), id.to_owned())))
        })
    }
}

fn actor() -> RecordContextActor {
    RecordContextActor {
        user_id: "actor".into(),
        holding_id: "h001".into(),
        company_id: "company".into(),
        branch_id: None,
        department_id: None,
        assignment_id: "assignment-actor".into(),
        assignment_version: Some(2),
        authority_revision: Some(9),
        role: "operations_manager".into(),
        scope_kind: "holding".into(),
        modules: vec!["operations".into(), "crm".into()],
        permissions: vec!["records.read".into()],
        expires_at: 1_000,
    }
}

fn record() -> RecordRow {
    RecordRow {
        module_id: "operations".into(),
        id: "rec-1".into(),
        section_id: "overview".into(),
        title: "Service visit".into(),
        description: "Bounded record".into(),
        status: "draft".into(),
        fields_json: r#"[{"key":"note","label":"Note","type":"text","value":"first"}]"#.into(),
        company_id: "company".into(),
        branch_id: None,
        department_id: None,
        version: 3,
        created_by: "maker".into(),
        updated_at: 500,
    }
}

async fn read(
    repository: &mut FakeRepository,
    guard: &mut FakeGuard,
) -> Result<serde_json::Value, BusinessError> {
    record_context(repository, guard, &actor(), "operations", "rec-1", 500).await
}

#[tokio::test]
async fn exact_document_orders_links_and_hides_denied_targets() {
    let mut repository = FakeRepository {
        references: vec![
            ReferenceEdge {
                target_module: "operations".into(),
                target_id: "rec-2".into(),
                kind: "dependsOn".into(),
                ordinal: 0,
            },
            ReferenceEdge {
                target_module: "crm".into(),
                target_id: "party-1".into(),
                kind: "authoredWith".into(),
                ordinal: 1,
            },
        ],
        backlinks: vec![
            BacklinkEdge {
                host_module: "operations".into(),
                host_id: "rec-3".into(),
                kind: "blocks".into(),
            },
            BacklinkEdge {
                host_module: "operations".into(),
                host_id: "rec-4".into(),
                kind: "billedBy".into(),
            },
        ],
        mentions: vec![MentionRow {
            target_kind: "person".into(),
            target_id: "user-1".into(),
            target_label: "Person One".into(),
            created_at: 400,
        }],
        suggestions: vec![SuggestionRow {
            id: "suggestion-1".into(),
            field_key: "note".into(),
            proposed_value_json: "\"next\"".into(),
            source_versions_json: "[{\"record\":\"operations/rec-2\",\"version\":2}]".into(),
            provenance_json: "{\"provider_kind\":\"development\"}".into(),
            state: "ready".into(),
            created_at: 420,
        }],
        history: vec![HistoryRow {
            from_status: "draft".into(),
            to_status: "in_review".into(),
            action: "submit".into(),
            note: "Submitted".into(),
            actor_id: "maker".into(),
            record_version: 4,
            created_at: 480,
        }],
        targets: BTreeMap::from([
            (
                ("operations".into(), "rec-2".into()),
                LinkTarget {
                    title: "Target record".into(),
                    version: 2,
                },
            ),
            (
                ("operations".into(), "rec-3".into()),
                LinkTarget {
                    title: "Host record".into(),
                    version: 3,
                },
            ),
        ]),
        ..FakeRepository::default()
    };
    let mut guard = FakeGuard::default()
        .allow("operations", "rec-2")
        .allow("operations", "rec-3")
        .allow("operations", "rec-4");

    let data = read(&mut repository, &mut guard).await.unwrap();
    assert_eq!(data["record"]["moduleId"], "operations");
    assert_eq!(data["record"]["id"], "rec-1");
    assert_eq!(data["record"]["version"], 3);
    assert_eq!(data["record"]["fields"][0]["value"], "first");
    assert_eq!(data["references"][0]["kind"], "dependsOn");
    assert_eq!(data["references"][0]["title"], "Target record");
    assert_eq!(data["references"][0]["access"], "available");
    assert_eq!(data["references"][1]["title"], "Unavailable record");
    assert_eq!(data["references"][1]["version"], 0);
    assert_eq!(data["references"][1]["access"], "denied");
    assert_eq!(data["backlinks"].as_array().unwrap().len(), 1);
    assert_eq!(data["backlinks"][0]["host_id"], "rec-3");
    assert_eq!(data["mentions"][0]["target_label"], "Person One");
    assert_eq!(data["suggestions"][0]["state"], "ready");
    assert_eq!(data["history"][0]["action"], "submit");
}

#[tokio::test]
async fn permission_module_expiry_and_changed_authority_precede_source_reads() {
    let mut repository = FakeRepository::default();
    let mut guard = FakeGuard::default();
    let mut scoped = actor();
    scoped.permissions.clear();
    let error = record_context(
        &mut repository,
        &mut guard,
        &scoped,
        "operations",
        "rec-1",
        500,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PERMISSION_REQUIRED");

    let mut scoped = actor();
    scoped.modules = vec!["crm".into()];
    let error = record_context(
        &mut repository,
        &mut guard,
        &scoped,
        "operations",
        "rec-1",
        500,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "MODULE_FORBIDDEN");

    let mut scoped = actor();
    scoped.expires_at = 500;
    let error = record_context(
        &mut repository,
        &mut guard,
        &scoped,
        "operations",
        "rec-1",
        500,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");

    repository.authority = false;
    let error = read(&mut repository, &mut guard).await.unwrap_err();
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
}

#[tokio::test]
async fn invalid_targets_and_missing_records_fail_closed() {
    let mut repository = FakeRepository::default();
    let mut guard = FakeGuard::default();
    for (module, id) in [
        ("../operations", "rec-1"),
        ("operations", ""),
        ("UPPER", "rec-1"),
    ] {
        let error = record_context(&mut repository, &mut guard, &actor(), module, id, 500)
            .await
            .unwrap_err();
        assert_eq!(error.code, "RECORD_CONTEXT_REQUEST_INVALID");
    }
    repository.row = None;
    let error = read(&mut repository, &mut guard).await.unwrap_err();
    assert_eq!(error.code, "NOT_FOUND");
}

#[tokio::test]
async fn cross_owner_source_and_target_guard_failures_propagate() {
    let mut repository = FakeRepository::default();
    let mut guard = FakeGuard {
        source_fails: true,
        ..FakeGuard::default()
    };
    assert_eq!(
        read(&mut repository, &mut guard).await.unwrap_err().code,
        "SYNTHETIC"
    );
    let mut guard = FakeGuard {
        source_denied: true,
        ..FakeGuard::default()
    };
    assert_eq!(
        read(&mut repository, &mut guard).await.unwrap_err().code,
        "SOURCE_FORBIDDEN"
    );
    repository.references = vec![ReferenceEdge {
        target_module: "crm".into(),
        target_id: "party-1".into(),
        kind: "billedBy".into(),
        ordinal: 0,
    }];
    let mut guard = FakeGuard {
        target_fails: true,
        ..FakeGuard::default()
    };
    assert_eq!(
        read(&mut repository, &mut guard).await.unwrap_err().code,
        "SYNTHETIC"
    );
}

#[tokio::test]
async fn foreign_duplicate_and_oversized_sources_are_rejected() {
    let mut repository = FakeRepository::default();
    let mut guard = FakeGuard::default();
    let mut row = record();
    row.company_id = "foreign".into();
    repository.row = Some(row);
    assert_eq!(
        read(&mut repository, &mut guard).await.unwrap_err().code,
        "RECORD_CONTEXT_SOURCE_INVALID"
    );

    repository.row = Some(record());
    repository.references = vec![
        ReferenceEdge {
            target_module: "operations".into(),
            target_id: "rec-2".into(),
            kind: "dependsOn".into(),
            ordinal: 0,
        },
        ReferenceEdge {
            target_module: "operations".into(),
            target_id: "rec-2".into(),
            kind: "blocks".into(),
            ordinal: 1,
        },
    ];
    assert_eq!(
        read(&mut repository, &mut guard).await.unwrap_err().code,
        "RECORD_CONTEXT_SOURCE_INVALID"
    );

    repository.references = (0..51)
        .map(|ordinal| ReferenceEdge {
            target_module: "operations".into(),
            target_id: format!("rec-{ordinal}"),
            kind: "dependsOn".into(),
            ordinal: ordinal as i64,
        })
        .collect();
    assert_eq!(
        read(&mut repository, &mut guard).await.unwrap_err().code,
        "RECORD_CONTEXT_SOURCE_INVALID"
    );
}

#[tokio::test]
async fn malformed_fields_suggestions_and_history_are_rejected() {
    let mut repository = FakeRepository::default();
    let mut guard = FakeGuard::default();
    repository.row.as_mut().unwrap().fields_json = "{}".into();
    assert_eq!(
        read(&mut repository, &mut guard).await.unwrap_err().code,
        "RECORD_CONTEXT_SOURCE_INVALID"
    );

    repository.row = Some(record());
    repository.suggestions = vec![SuggestionRow {
        id: "suggestion-1".into(),
        field_key: "note".into(),
        proposed_value_json: "\"next\"".into(),
        source_versions_json: "{}".into(),
        provenance_json: "{}".into(),
        state: "ready".into(),
        created_at: 420,
    }];
    assert_eq!(
        read(&mut repository, &mut guard).await.unwrap_err().code,
        "RECORD_CONTEXT_SOURCE_INVALID"
    );

    repository.suggestions.clear();
    repository.history = vec![HistoryRow {
        from_status: "draft".into(),
        to_status: "in_review".into(),
        action: "submit".into(),
        note: "Submitted".into(),
        actor_id: "maker".into(),
        record_version: 0,
        created_at: 480,
    }];
    assert_eq!(
        read(&mut repository, &mut guard).await.unwrap_err().code,
        "RECORD_CONTEXT_SOURCE_INVALID"
    );
}

#[tokio::test]
async fn branch_and_department_scope_match_retained_selection() {
    let mut scoped = actor();
    scoped.branch_id = Some("branch-1".into());
    scoped.department_id = None;
    let mut row = record();
    row.branch_id = Some("branch-1".into());
    let mut repository = FakeRepository {
        row: Some(row),
        ..FakeRepository::default()
    };
    let mut guard = FakeGuard::default();
    assert!(
        record_context(
            &mut repository,
            &mut guard,
            &scoped,
            "operations",
            "rec-1",
            500
        )
        .await
        .is_ok()
    );
    repository.row.as_mut().unwrap().branch_id = Some("branch-2".into());
    assert_eq!(
        record_context(
            &mut repository,
            &mut guard,
            &scoped,
            "operations",
            "rec-1",
            500
        )
        .await
        .unwrap_err()
        .code,
        "RECORD_CONTEXT_SOURCE_INVALID"
    );

    let mut scoped = actor();
    scoped.branch_id = Some("branch-1".into());
    scoped.department_id = Some("department-1".into());
    let mut row = record();
    row.branch_id = None;
    row.department_id = Some("department-1".into());
    repository.row = Some(row);
    assert!(
        record_context(
            &mut repository,
            &mut guard,
            &scoped,
            "operations",
            "rec-1",
            500
        )
        .await
        .is_ok()
    );
}
