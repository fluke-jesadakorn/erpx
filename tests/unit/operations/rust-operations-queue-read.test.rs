//! Native Operations queue read policy and application facts. No database,
//! browser, command, purchase or Workflow acceptance is inferred.
use erpx_server_operations::public::queue_read::*;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

fn row(id: &str, status: &str, area: &str, due_at: i64) -> QueueException {
    QueueException {
        id: id.into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        area: area.into(),
        source_kind: "staff_task".into(),
        source_id: format!("source-{id}"),
        source_version: 1,
        source_reference_json: "{}".into(),
        policy_id: "policy".into(),
        policy_snapshot_json: "{}".into(),
        title: format!("Synthetic {id}"),
        issue: "Synthetic issue".into(),
        priority: "normal".into(),
        status: status.into(),
        owner_id: "owner".into(),
        escalation_owner_id: None,
        resolution: None,
        resolved_by: None,
        reviewed_by: None,
        version: 1,
        created_by: "maker".into(),
        created_at: 0,
        updated_at: 1,
        due_at,
    }
}
fn source(row: &QueueException) -> SourceView {
    SourceView {
        kind: row.source_kind.clone(),
        id: row.source_id.clone(),
        version: 1,
        title: format!("Source {}", row.id),
        href: format!("/tasks/{}", row.source_id),
        label: "Task · Synthetic issue".into(),
        company_id: row.company_id.clone(),
        branch_id: row.branch_id.clone(),
        department_id: row.department_id.clone(),
        status: "open".into(),
    }
}
fn actor() -> OperationsReadActor {
    OperationsReadActor {
        user_id: "user".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        permissions: vec!["records.read".into(), "records.write".into()],
        modules: vec!["operations".into()],
        expires_at: 10_000,
    }
}
fn policy(id: &str) -> QueuePolicyRow {
    QueuePolicyRow {
        id: id.into(),
        company_id: "company".into(),
        area: "service".into(),
        name: "Service queue".into(),
        revision: 1,
        source_kinds_json: "[\"staff_task\"]".into(),
        target_minutes: 60,
        status: "published".into(),
        created_by: "maker".into(),
        reviewed_by: Some("reviewer".into()),
        version: 2,
        created_at: 0,
        updated_at: 1,
    }
}
#[derive(Clone)]
struct Repo {
    data: WorkspaceData,
    history: Vec<OperationsEventRow>,
    seen: Arc<Mutex<Vec<String>>>,
}
impl OperationsReadRepository for Repo {
    type Error = BusinessError;
    fn workspace<'a>(
        &'a mut self,
        company_id: &'a str,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
    ) -> OperationsReadFuture<'a, WorkspaceData, Self::Error> {
        self.seen.lock().unwrap().push(format!(
            "{company_id}:{}:{}",
            branch_id.unwrap_or("-"),
            department_id.unwrap_or("-")
        ));
        let data = self.data.clone();
        Box::pin(async move { Ok(data) })
    }
    fn history<'a>(
        &'a mut self,
        exception_id: &'a str,
    ) -> OperationsReadFuture<'a, Vec<OperationsEventRow>, Self::Error> {
        let rows = self.history.clone();
        let id = exception_id.to_owned();
        Box::pin(async move {
            if rows.iter().all(|row| row.exception_id == id) {
                Ok(rows)
            } else {
                Ok(Vec::new())
            }
        })
    }
}
struct Sources(BTreeMap<String, Option<SourceView>>);
impl OperationsSourceVisibility for Sources {
    type Error = BusinessError;
    fn visible<'a>(
        &'a mut self,
        row: &'a QueueException,
    ) -> OperationsReadFuture<'a, Option<SourceView>, Self::Error> {
        let value = self
            .0
            .get(&row.id)
            .cloned()
            .unwrap_or_else(|| Some(source(row)));
        Box::pin(async move { Ok(value) })
    }
}
struct People(Vec<PersonOption>);
impl OperationsPeopleDirectory for People {
    type Error = BusinessError;
    fn people(&mut self) -> OperationsReadFuture<'_, Vec<PersonOption>, Self::Error> {
        let rows = self.0.clone();
        Box::pin(async move { Ok(rows) })
    }
}

#[test]
fn read_requires_permission_module_and_current_work_context() {
    let mut missing = actor();
    missing.permissions = vec!["records.write".into()];
    assert_eq!(
        missing.require_read(0).unwrap_err().code,
        "PERMISSION_REQUIRED"
    );
    let mut no_module = actor();
    no_module.modules = vec!["assets".into()];
    assert_eq!(
        no_module.require_read(0).unwrap_err().code,
        "MODULE_FORBIDDEN"
    );
    let mut expired = actor();
    expired.expires_at = 10;
    let error = expired.require_read(10).unwrap_err();
    assert_eq!(error.kind, FailureKind::Conflict);
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
    assert!(actor().require_read(9_999).is_ok());
}

#[tokio::test]
async fn workspace_uses_actor_scope_filters_hidden_sources_and_summarizes_current_rows() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut repo = Repo {
        data: WorkspaceData {
            exceptions: vec![
                row("open", "open", "service", 100),
                row("closed", "closed", "service", 50),
                row("hidden", "escalated", "service", 100),
            ],
            policies: vec![policy("policy")],
        },
        history: vec![],
        seen: seen.clone(),
    };
    let mut sources = Sources(BTreeMap::from([("hidden".into(), None)]));
    let mut people = People(vec![
        PersonOption {
            id: "user-b".into(),
            name: "Beta".into(),
        },
        PersonOption {
            id: "user-a".into(),
            name: "Alpha".into(),
        },
        PersonOption {
            id: "user-a".into(),
            name: "Alpha".into(),
        },
    ]);
    let result = operations_workspace(&actor(), &mut repo, &mut sources, &mut people, 200)
        .await
        .unwrap();
    assert_eq!(seen.lock().unwrap().as_slice(), ["company:branch:-"]);
    assert_eq!(result.exceptions.len(), 2);
    assert!(result.exceptions.iter().all(|view| view.row.id != "hidden"));
    assert_eq!(result.exceptions[0].source.label, "Task · Synthetic issue");
    assert_eq!(result.policies.len(), 1);
    assert_eq!(result.actor_id, "user");
    assert!(result.can_write);
    assert!(!result.can_review);
    assert_eq!(result.summary.active, 1);
    assert_eq!(result.summary.overdue, 1);
    assert_eq!(result.summary.by_area.get("service"), Some(&1));
    assert_eq!(result.people.len(), 2);
    assert_eq!(result.people[0].name, "Alpha");
    assert_eq!(result.people[1].name, "Beta");
}

#[tokio::test]
async fn workspace_json_shape_matches_the_next_transport_contract() {
    let mut repo = Repo {
        data: WorkspaceData {
            exceptions: vec![row("visible", "open", "service", 100)],
            policies: vec![policy("policy")],
        },
        history: vec![],
        seen: Arc::new(Mutex::new(Vec::new())),
    };
    let mut sources = Sources(BTreeMap::new());
    let mut people = People(vec![PersonOption {
        id: "owner".into(),
        name: "Owner".into(),
    }]);
    let value = operations_workspace(&actor(), &mut repo, &mut sources, &mut people, 200)
        .await
        .unwrap()
        .value();
    assert_eq!(value["exceptions"][0]["created_at"], 0);
    assert_eq!(
        value["exceptions"][0]["source"]["label"],
        "Task · Synthetic issue"
    );
    assert_eq!(
        value["policies"][0]["source_kinds_json"],
        "[\"staff_task\"]"
    );
    assert_eq!(value["summary"]["byArea"]["service"], 1);
    assert_eq!(value["actorId"], "user");
    assert_eq!(value["canWrite"], true);
}

#[tokio::test]
async fn detail_preserves_history_and_returns_scoped_not_found_for_hidden_rows() {
    let mut repo = Repo {
        data: WorkspaceData {
            exceptions: vec![row("visible", "open", "service", 100)],
            policies: vec![],
        },
        history: vec![OperationsEventRow {
            id: "event".into(),
            exception_id: "visible".into(),
            version: 1,
            action: "open".into(),
            actor_id: "maker".into(),
            reason: "Synthetic issue".into(),
            status: "open".into(),
            details_json: "{}".into(),
            created_at: 0,
        }],
        seen: Arc::new(Mutex::new(Vec::new())),
    };
    let mut sources = Sources(BTreeMap::new());
    let mut people = People(vec![]);
    let detail = operations_detail(
        &actor(),
        &mut repo,
        &mut sources,
        &mut people,
        "visible",
        200,
    )
    .await
    .unwrap();
    assert_eq!(detail.row.row.id, "visible");
    assert_eq!(detail.history.len(), 1);
    assert_eq!(detail.value()["history"][0]["action"], "open");
    let error = operations_detail(
        &actor(),
        &mut repo,
        &mut sources,
        &mut people,
        "missing",
        200,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind, FailureKind::NotFound);
    assert_eq!(error.code, "OPERATIONS_EXCEPTION_NOT_FOUND");
}

#[tokio::test]
async fn a_mismatched_or_foreign_source_projection_fails_closed() {
    for invalid in [
        SourceView {
            id: "different".into(),
            ..source(&row("visible", "open", "service", 100))
        },
        SourceView {
            company_id: "foreign".into(),
            ..source(&row("visible", "open", "service", 100))
        },
        SourceView {
            version: 0,
            ..source(&row("visible", "open", "service", 100))
        },
    ] {
        let mut repo = Repo {
            data: WorkspaceData {
                exceptions: vec![row("visible", "open", "service", 100)],
                policies: vec![],
            },
            history: vec![],
            seen: Arc::new(Mutex::new(Vec::new())),
        };
        let mut sources = Sources(BTreeMap::from([("visible".into(), Some(invalid))]));
        let mut people = People(vec![]);
        let error = operations_workspace(&actor(), &mut repo, &mut sources, &mut people, 200)
            .await
            .unwrap_err();
        assert_eq!(error.code, "OPERATIONS_SOURCE_INVALID");
    }
}

#[test]
fn summary_ignores_terminal_status_and_counts_overdue_by_current_time() {
    let rows = vec![
        ExceptionView {
            row: row("open", "open", "service", 100),
            source: source(&row("open", "open", "service", 100)),
        },
        ExceptionView {
            row: row("review", "awaiting_review", "finance", 500),
            source: source(&row("review", "awaiting_review", "finance", 500)),
        },
        ExceptionView {
            row: row("closed", "closed", "service", 100),
            source: source(&row("closed", "closed", "service", 100)),
        },
    ];
    let summary = summary(&rows, 200);
    assert_eq!(summary.active, 2);
    assert_eq!(summary.awaiting_review, 1);
    assert_eq!(summary.overdue, 1);
    assert_eq!(summary.by_area.len(), 2);
}
