//! Portable Records registry list/search/page decisions.
//!
//! These tests use an in-memory owner repository and a synthetic cross-owner
//! guard. PostgreSQL, HTTP and browser behavior are covered by the separate
//! owner targets; this target proves bounds, authorization order, filtering,
//! page assembly and cursor behavior without a database.
use erpx_server_records::public::record_context::{RecordContextActor, RecordRow};
use erpx_server_records::public::record_registry::{
    MAX_LIST_ROWS, PAGE_SIZE, RecordRegistryFuture, RecordRegistryGuard, RecordRegistryRepository,
    RegistryCursor, RegistryQuery, encode_cursor, record_registry_list, record_registry_page,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

const NOW: i64 = 500;

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
        role: "records_reader".into(),
        scope_kind: "holding".into(),
        modules: vec!["operations".into(), "crm".into()],
        permissions: vec!["records.read".into()],
        expires_at: 1_000_000,
    }
}

fn row(module_id: &str, section_id: &str, id: &str, updated_at: i64) -> RecordRow {
    RecordRow {
        module_id: module_id.into(),
        id: id.into(),
        section_id: section_id.into(),
        title: format!("Record {id}"),
        description: "Bounded registry row".into(),
        status: "draft".into(),
        fields_json: "[{\"key\":\"note\",\"label\":\"Note\",\"type\":\"text\",\"value\":\"ok\"}]"
            .into(),
        company_id: "company".into(),
        branch_id: None,
        department_id: None,
        version: 1,
        created_by: "maker".into(),
        updated_at,
    }
}

#[derive(Clone, Debug)]
struct ReadCall {
    section_id: String,
    q: Option<String>,
    after: Option<(i64, String)>,
    descending: bool,
    limit: usize,
}

#[derive(Clone)]
struct Repository {
    rows: Vec<RecordRow>,
    authority: bool,
    calls: Arc<Mutex<Vec<ReadCall>>>,
}

impl Repository {
    fn new(rows: Vec<RecordRow>) -> Self {
        Self {
            rows,
            authority: true,
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl RecordRegistryRepository for Repository {
    type Error = BusinessError;

    fn current_authority<'a>(
        &'a mut self,
        _: &'a RecordContextActor,
        _: i64,
    ) -> RecordRegistryFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(self.authority) })
    }

    fn read_rows<'a>(
        &'a mut self,
        _: &'a RecordContextActor,
        module_id: &'a str,
        section_id: &'a str,
        q: Option<&'a str>,
        after_key: Option<i64>,
        after_id: Option<&'a str>,
        descending: bool,
        limit: usize,
    ) -> RecordRegistryFuture<'a, Vec<RecordRow>, Self::Error> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(ReadCall {
                section_id: section_id.into(),
                q: q.map(str::to_owned),
                after: after_key.map(|key| (key, after_id.unwrap_or_default().to_owned())),
                descending,
                limit,
            });
            let needle = q.map(str::to_lowercase);
            let mut rows = self
                .rows
                .iter()
                .filter(|row| row.module_id == module_id && row.section_id == section_id)
                .filter(|row| {
                    needle.as_ref().is_none_or(|needle| {
                        format!("{} {}", row.title, row.description)
                            .to_lowercase()
                            .contains(needle)
                    })
                })
                .filter(|row| match (after_key, after_id) {
                    (Some(key), Some(id)) if descending => {
                        (row.updated_at, row.id.as_str()) < (key, id)
                    }
                    (Some(key), Some(id)) => (row.updated_at, row.id.as_str()) > (key, id),
                    _ => true,
                })
                .cloned()
                .collect::<Vec<_>>();
            rows.sort_by(|left, right| {
                (left.updated_at, left.id.as_str()).cmp(&(right.updated_at, right.id.as_str()))
            });
            if descending {
                rows.reverse();
            }
            rows.truncate(limit);
            Ok(rows)
        })
    }
}

#[derive(Clone, Default)]
struct Guard {
    visible: Option<BTreeSet<String>>,
    deny_module: bool,
    fail: bool,
}

impl RecordRegistryGuard for Guard {
    type Error = BusinessError;

    fn authorize_module<'a>(
        &'a mut self,
        _: &'a RecordContextActor,
        _: &'a str,
    ) -> RecordRegistryFuture<'a, (), Self::Error> {
        Box::pin(async move {
            if self.deny_module {
                return Err(BusinessError::new(
                    FailureKind::NotFound,
                    "MODULE_NOT_FOUND",
                    "Module not found.",
                ));
            }
            Ok(())
        })
    }

    fn row_visible<'a>(
        &'a mut self,
        _: &'a RecordContextActor,
        row: &'a RecordRow,
    ) -> RecordRegistryFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "SOURCE_GUARD_UNAVAILABLE",
                    "Synthetic row guard outage.",
                ));
            }
            Ok(self
                .visible
                .as_ref()
                .is_none_or(|visible| visible.contains(&row.id)))
        })
    }
}

async fn page(
    repository: &mut Repository,
    guard: &mut Guard,
    query: &RegistryQuery,
) -> Result<erpx_server_records::public::record_registry::RegistryPage, BusinessError> {
    record_registry_page(repository, guard, &actor(), query, NOW).await
}

#[test]
fn query_bounds_and_cursor_identity_are_strict() {
    assert!(RegistryQuery::list("operations", "overview").is_ok());
    assert!(RegistryQuery::page("operations", "overview", None, None).is_ok());
    for query in [
        RegistryQuery::page("Operations", "overview", None, None),
        RegistryQuery::page("operations", "", None, None),
        RegistryQuery::page("operations", " overview", None, None),
        RegistryQuery::page("operations", "overview", Some("\0"), None),
        RegistryQuery::page("operations", "overview", Some(&"x".repeat(201)), None),
        RegistryQuery::page("operations", "overview", None, Some("")),
        RegistryQuery::page("operations", "overview", None, Some(&"x".repeat(4_097))),
    ] {
        assert!(query.is_err());
    }
    let query = RegistryQuery::list("operations", "overview").unwrap();
    let scope = erpx_server_records::public::record_registry::registry_scope(&actor(), &query);
    let cursor = RegistryCursor {
        key: 700,
        id: "rec-1".into(),
        reverse: false,
        scope: scope.clone(),
    };
    let encoded = encode_cursor(&cursor);
    assert!(!encoded.contains('='));
    assert_eq!(
        erpx_server_records::public::record_registry::decode_cursor(&encoded, &scope).unwrap(),
        cursor
    );
    assert!(
        erpx_server_records::public::record_registry::decode_cursor(&encoded, "other").is_err()
    );
    assert!(erpx_server_records::public::record_registry::decode_cursor("a", &scope).is_err());
    assert!(erpx_server_records::public::record_registry::decode_cursor("!!!!", &scope).is_err());
}

#[tokio::test]
async fn page_scans_in_descending_order_and_navigates_both_ways() {
    let rows = (0..65)
        .map(|index| {
            row(
                "operations",
                "overview",
                &format!("rec-{index:03}"),
                1_000 + index as i64,
            )
        })
        .collect();
    let mut repository = Repository::new(rows);
    let mut guard = Guard::default();
    let first = page(
        &mut repository,
        &mut guard,
        &RegistryQuery::page("operations", "overview", None, None).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(first.items.len(), PAGE_SIZE);
    assert_eq!(first.items[0].id, "rec-064");
    assert_eq!(first.items[29].id, "rec-035");
    assert!(first.has_more);
    assert!(first.previous_cursor.is_none());
    let next = first.next_cursor.as_ref().unwrap();
    let wire = first.to_wire();
    assert_eq!(wire["items"][0]["moduleId"], "operations");
    assert_eq!(wire["items"][0]["fields"][0]["key"], "note");
    assert_eq!(wire["hasMore"], true);

    let second = page(
        &mut repository,
        &mut guard,
        &RegistryQuery::page("operations", "overview", None, Some(next)).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(second.items[0].id, "rec-034");
    assert_eq!(second.items[29].id, "rec-005");
    assert!(second.has_more);
    assert!(second.previous_cursor.is_some());

    let third = page(
        &mut repository,
        &mut guard,
        &RegistryQuery::page(
            "operations",
            "overview",
            None,
            second.next_cursor.as_deref(),
        )
        .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(third.items.len(), 5);
    assert_eq!(third.items[4].id, "rec-000");
    assert!(!third.has_more);

    let back = page(
        &mut repository,
        &mut guard,
        &RegistryQuery::page(
            "operations",
            "overview",
            None,
            second.previous_cursor.as_deref(),
        )
        .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(
        back.items
            .iter()
            .map(|item| item.id.clone())
            .collect::<Vec<_>>(),
        first
            .items
            .iter()
            .map(|item| item.id.clone())
            .collect::<Vec<_>>()
    );
    let calls = repository.calls.lock().unwrap().clone();
    assert_eq!(calls[0].limit, PAGE_SIZE + 1);
    assert!(calls[0].after.is_none());
    assert_eq!(
        calls[1].after.as_ref().map(|(_, id)| id.as_str()),
        Some("rec-035")
    );
    assert!(calls[1].descending);
    assert!(!calls[3].descending);
}

#[tokio::test]
async fn exact_page_boundary_has_no_next_cursor() {
    let rows = (0..30)
        .map(|index| {
            row(
                "operations",
                "overview",
                &format!("rec-{index:03}"),
                1_000 + index as i64,
            )
        })
        .collect();
    let mut repository = Repository::new(rows);
    let mut guard = Guard::default();
    let first = page(
        &mut repository,
        &mut guard,
        &RegistryQuery::page("operations", "overview", None, None).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(first.items.len(), PAGE_SIZE);
    assert!(!first.has_more);
    assert!(first.next_cursor.is_none());
}

#[tokio::test]
async fn search_is_bounded_to_the_owner_query_and_never_reads_another_section() {
    let mut needle = row("operations", "overview", "rec-needle", 900);
    needle.title = "Quarterly NEEDLE review".into();
    let mut repository = Repository::new(vec![
        needle,
        row("operations", "overview", "rec-other", 800),
        row("operations", "drafts", "rec-draft", 700),
        row("crm", "overview", "party-1", 600),
    ]);
    let mut guard = Guard::default();
    let result = page(
        &mut repository,
        &mut guard,
        &RegistryQuery::page("operations", "overview", Some("needle"), None).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].id, "rec-needle");
    let calls = repository.calls.lock().unwrap().clone();
    assert_eq!(calls[0].q.as_deref(), Some("needle"));
    assert_eq!(calls[0].section_id, "overview");
}

#[tokio::test]
async fn current_authority_permission_and_module_are_checked_before_rows() {
    let mut repository = Repository::new(vec![row("operations", "overview", "rec-1", 100)]);
    let mut guard = Guard::default();
    for (actor_value, code) in [
        (
            RecordContextActor {
                permissions: vec![],
                ..actor()
            },
            "PERMISSION_REQUIRED",
        ),
        (
            RecordContextActor {
                expires_at: NOW,
                ..actor()
            },
            "WORK_CONTEXT_CHANGED",
        ),
        (
            RecordContextActor {
                modules: vec!["crm".into()],
                ..actor()
            },
            "MODULE_FORBIDDEN",
        ),
    ] {
        let query = RegistryQuery::list("operations", "overview").unwrap();
        let error = record_registry_page(&mut repository, &mut guard, &actor_value, &query, NOW)
            .await
            .unwrap_err();
        assert_eq!(error.code, code);
    }
    assert!(repository.calls.lock().unwrap().is_empty());

    repository.authority = false;
    let query = RegistryQuery::list("operations", "overview").unwrap();
    let error = page(&mut repository, &mut guard, &query).await.unwrap_err();
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
    assert!(repository.calls.lock().unwrap().is_empty());

    repository.authority = true;
    let mut denied = Guard {
        deny_module: true,
        ..Guard::default()
    };
    let error = page(&mut repository, &mut denied, &query)
        .await
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::NotFound);
    assert_eq!(error.code, "MODULE_NOT_FOUND");
    assert!(repository.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn cross_owner_guard_filters_rows_and_failures_fail_closed() {
    let rows = (0..65)
        .map(|index| {
            row(
                "operations",
                "overview",
                &format!("rec-{index:03}"),
                1_000 + index as i64,
            )
        })
        .collect();
    let mut repository = Repository::new(rows);
    let visible = (0..30).map(|index| format!("rec-{index:03}")).collect();
    let mut guard = Guard {
        visible: Some(visible),
        ..Guard::default()
    };
    let result = page(
        &mut repository,
        &mut guard,
        &RegistryQuery::page("operations", "overview", None, None).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(result.items.len(), 30);
    assert_eq!(result.items[0].id, "rec-029");
    assert!(!result.has_more);

    let mut failing = Guard {
        fail: true,
        ..Guard::default()
    };
    let error = page(
        &mut repository,
        &mut failing,
        &RegistryQuery::page("operations", "overview", None, None).unwrap(),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "SOURCE_GUARD_UNAVAILABLE");
}

#[tokio::test]
async fn foreign_or_wrong_section_rows_fail_closed() {
    let mut foreign = row("operations", "overview", "rec-1", 100);
    foreign.company_id = "foreign".into();
    let mut repository = Repository::new(vec![foreign]);
    let mut guard = Guard::default();
    let error = page(
        &mut repository,
        &mut guard,
        &RegistryQuery::page("operations", "overview", None, None).unwrap(),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "RECORDS_REGISTRY_SOURCE_INVALID");

    let mut wrong = row("operations", "overview", "rec-2", 90);
    wrong.section_id = "drafts".into();
    let error = erpx_server_records::public::record_registry::validate_registry_row(
        &wrong,
        &actor(),
        "operations",
        "overview",
    )
    .unwrap_err();
    assert_eq!(error.code, "RECORDS_REGISTRY_SOURCE_INVALID");
}

#[tokio::test]
async fn cursor_scope_and_tampering_are_rejected() {
    let mut repository = Repository::new(vec![row("operations", "overview", "rec-1", 100)]);
    let mut guard = Guard::default();
    let query = RegistryQuery::page("operations", "overview", None, Some("not-a-cursor")).unwrap();
    let error = page(&mut repository, &mut guard, &query).await.unwrap_err();
    assert_eq!(error.code, "INVALID_CURSOR");

    let scope = erpx_server_records::public::record_registry::registry_scope(&actor(), &query);
    let wrong = encode_cursor(&RegistryCursor {
        key: 100,
        id: "rec-1".into(),
        reverse: false,
        scope: format!("{scope}-other"),
    });
    let query = RegistryQuery::page("operations", "overview", None, Some(&wrong)).unwrap();
    let error = page(&mut repository, &mut guard, &query).await.unwrap_err();
    assert_eq!(error.code, "INVALID_CURSOR");
}

#[tokio::test]
async fn hidden_row_scan_and_unpaged_list_are_bounded() {
    let rows = (0..2_001)
        .map(|index| {
            row(
                "operations",
                "overview",
                &format!("rec-{index:04}"),
                10_000 + index as i64,
            )
        })
        .collect();
    let mut repository = Repository::new(rows);
    let mut guard = Guard {
        visible: Some(BTreeSet::new()),
        ..Guard::default()
    };
    let error = page(
        &mut repository,
        &mut guard,
        &RegistryQuery::page("operations", "overview", None, None).unwrap(),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "RECORDS_REGISTRY_SOURCE_INVALID");

    let make = |count: usize| {
        Repository::new(
            (0..count)
                .map(|index| {
                    row(
                        "operations",
                        "overview",
                        &format!("rec-{index:04}"),
                        10_000 + index as i64,
                    )
                })
                .collect(),
        )
    };
    let mut exact = make(MAX_LIST_ROWS);
    let list = record_registry_list(
        &mut exact,
        &mut Guard::default(),
        &actor(),
        &RegistryQuery::list("operations", "overview").unwrap(),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(list.len(), MAX_LIST_ROWS);
    let mut overflow = make(MAX_LIST_ROWS + 1);
    let error = record_registry_list(
        &mut overflow,
        &mut Guard::default(),
        &actor(),
        &RegistryQuery::list("operations", "overview").unwrap(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "RECORDS_REGISTRY_SOURCE_INVALID");
}
