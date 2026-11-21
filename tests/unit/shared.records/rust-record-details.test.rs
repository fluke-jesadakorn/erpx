//! Native Records drawer-detail read policy and orchestration negatives.
use erpx_server_records::public::record_context::{
    BacklinkEdge, HistoryRow, LinkTarget, MentionRow, RecordContextActor, RecordContextFuture,
    RecordContextGuard, RecordContextRepository, RecordRow, ReferenceEdge, SuggestionRow,
};
use erpx_server_records::public::record_details::{
    PAGE_SIZE, RecordDetailsQuery, RecordDetailsRepository, record_details,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeSet;

#[derive(Default)]
struct FakeRepository {
    authority: bool,
    row: Option<RecordRow>,
    references: Vec<ReferenceEdge>,
    history: Vec<HistoryRow>,
    history_calls: Vec<(i64, usize)>,
}

impl FakeRepository {
    fn ready() -> Self {
        Self {
            authority: true,
            row: Some(record()),
            ..Self::default()
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
    ) -> RecordContextFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(self.authority) })
    }
    fn record<'a>(
        &'a mut self,
        _: &'a RecordContextActor,
        _: &'a str,
        _: &'a str,
    ) -> RecordContextFuture<'a, Option<RecordRow>, Self::Error> {
        Box::pin(async move { Ok(self.row.clone()) })
    }
    fn references<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
    ) -> RecordContextFuture<'a, Vec<ReferenceEdge>, Self::Error> {
        Box::pin(async move { Ok(self.references.clone()) })
    }
    fn backlinks<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
    ) -> RecordContextFuture<'a, Vec<BacklinkEdge>, Self::Error> {
        Box::pin(async move { Ok(Vec::new()) })
    }
    fn mentions<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
    ) -> RecordContextFuture<'a, Vec<MentionRow>, Self::Error> {
        Box::pin(async move { Ok(Vec::new()) })
    }
    fn suggestions<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
    ) -> RecordContextFuture<'a, Vec<SuggestionRow>, Self::Error> {
        Box::pin(async move { Ok(Vec::new()) })
    }
    fn history<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
    ) -> RecordContextFuture<'a, Vec<HistoryRow>, Self::Error> {
        Box::pin(async move { Err(failure()) })
    }
    fn link_target<'a>(
        &'a mut self,
        _: &'a RecordContextActor,
        _: &'a str,
        _: &'a str,
    ) -> RecordContextFuture<'a, Option<LinkTarget>, Self::Error> {
        Box::pin(async move { Ok(None) })
    }
}

impl RecordDetailsRepository for FakeRepository {
    fn history_page<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
        offset: i64,
        limit: usize,
    ) -> RecordContextFuture<'a, Vec<HistoryRow>, Self::Error> {
        self.history_calls.push((offset, limit));
        Box::pin(async move {
            let start = offset as usize;
            let mut rows = self.history.clone();
            if start >= rows.len() {
                return Ok(Vec::new());
            }
            rows.drain(..start);
            rows.truncate(limit);
            Ok(rows)
        })
    }
}

#[derive(Default)]
struct FakeGuard {
    visible: BTreeSet<(String, String)>,
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
    ) -> RecordContextFuture<'a, (), Self::Error> {
        Box::pin(async move {
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
    ) -> RecordContextFuture<'a, bool, Self::Error> {
        Box::pin(async move {
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

fn history(index: i64) -> HistoryRow {
    HistoryRow {
        from_status: "draft".into(),
        to_status: "in_review".into(),
        action: "submit".into(),
        note: format!("event {index}"),
        actor_id: "maker".into(),
        record_version: 4,
        created_at: 500 + index,
    }
}

async fn read(
    repository: &mut FakeRepository,
    guard: &mut FakeGuard,
    page: Option<&str>,
) -> Result<serde_json::Value, BusinessError> {
    let query = RecordDetailsQuery::new("operations", "rec-1", page)?;
    record_details(repository, guard, &actor(), &query, 500).await
}

#[tokio::test]
async fn returns_the_scoped_row_visible_links_and_one_history_page() {
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
        history: (0..=(PAGE_SIZE as i64)).map(history).collect(),
        ..FakeRepository::ready()
    };
    let mut guard = FakeGuard::default().allow("operations", "rec-2");

    let data = read(&mut repository, &mut guard, Some("1")).await.unwrap();
    assert_eq!(data["kind"], "workspace_record");
    assert_eq!(data["row"]["moduleId"], "operations");
    assert_eq!(data["row"]["id"], "rec-1");
    assert_eq!(data["row"]["version"], 3);
    assert_eq!(data["row"]["fields"][0]["value"], "first");
    assert_eq!(data["related"].as_array().unwrap().len(), 1);
    assert_eq!(data["related"][0]["kind"], "operations");
    assert_eq!(data["related"][0]["id"], "rec-2");
    assert_eq!(data["page"], 1);
    assert_eq!(data["hasNext"], true);
    assert_eq!(data["history"].as_array().unwrap().len(), PAGE_SIZE);
    assert_eq!(repository.history_calls, vec![(0, PAGE_SIZE + 1)]);
}

#[tokio::test]
async fn last_page_has_no_next_witness_and_first_page_starts_at_zero() {
    let mut repository = FakeRepository {
        history: (0..PAGE_SIZE as i64).map(history).collect(),
        ..FakeRepository::ready()
    };
    let mut guard = FakeGuard::default();
    let data = read(&mut repository, &mut guard, None).await.unwrap();
    assert_eq!(data["page"], 1);
    assert_eq!(data["hasNext"], false);
    assert_eq!(data["history"].as_array().unwrap().len(), PAGE_SIZE);
    assert_eq!(repository.history_calls, vec![(0, PAGE_SIZE + 1)]);
}

#[tokio::test]
async fn permission_module_expiry_and_changed_authority_precede_source_reads() {
    let mut repository = FakeRepository::ready();
    let mut guard = FakeGuard::default();
    let query = RecordDetailsQuery::new("operations", "rec-1", None).unwrap();

    let mut scoped = actor();
    scoped.permissions.clear();
    let error = record_details(&mut repository, &mut guard, &scoped, &query, 500)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PERMISSION_REQUIRED");

    let mut scoped = actor();
    scoped.modules = vec!["crm".into()];
    let error = record_details(&mut repository, &mut guard, &scoped, &query, 500)
        .await
        .unwrap_err();
    assert_eq!(error.code, "MODULE_FORBIDDEN");

    let mut scoped = actor();
    scoped.expires_at = 500;
    let error = record_details(&mut repository, &mut guard, &scoped, &query, 500)
        .await
        .unwrap_err();
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");

    repository.authority = false;
    let error = record_details(&mut repository, &mut guard, &actor(), &query, 500)
        .await
        .unwrap_err();
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");

    repository.authority = true;
    repository.row = None;
    let error = record_details(&mut repository, &mut guard, &actor(), &query, 500)
        .await
        .unwrap_err();
    assert_eq!(error.code, "NOT_FOUND");
}

#[tokio::test]
async fn denied_source_scope_and_foreign_rows_fail_closed() {
    let mut repository = FakeRepository::ready();
    let mut guard = FakeGuard {
        source_denied: true,
        ..FakeGuard::default()
    };
    let error = read(&mut repository, &mut guard, None).await.unwrap_err();
    assert_eq!(error.code, "SOURCE_FORBIDDEN");

    let mut repository = FakeRepository::ready();
    let mut guard = FakeGuard::default();
    repository.row = Some(RecordRow {
        company_id: "foreign".into(),
        ..record()
    });
    let error = read(&mut repository, &mut guard, None).await.unwrap_err();
    assert_eq!(error.code, "RECORD_CONTEXT_SOURCE_INVALID");

    let mut repository = FakeRepository::ready();
    let mut guard = FakeGuard::default();
    repository.row = Some(RecordRow {
        module_id: "crm".into(),
        ..record()
    });
    let error = read(&mut repository, &mut guard, None).await.unwrap_err();
    assert_eq!(error.code, "RECORD_CONTEXT_SOURCE_INVALID");

    let mut repository = FakeRepository::ready();
    let mut guard = FakeGuard::default();
    let mut scoped = actor();
    scoped.branch_id = Some("branch-a".into());
    let query = RecordDetailsQuery::new("operations", "rec-1", None).unwrap();
    let error = record_details(&mut repository, &mut guard, &scoped, &query, 500)
        .await
        .unwrap_err();
    assert_eq!(error.code, "RECORD_CONTEXT_SOURCE_INVALID");
}
