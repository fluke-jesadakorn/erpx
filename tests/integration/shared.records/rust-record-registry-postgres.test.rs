//! Native Records registry read over migrated PostgreSQL. These prove owner
//! SQL, Company/scope isolation, current IAM, cross-owner row admission,
//! search, forward/backward paging and read-only serializable bounds; they are
//! not an HTTP, browser or Account journey.
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_records::public::record_context::{RecordContextActor, RecordRow};
use erpx_server_records::public::record_registry::{
    MAX_FETCH, PgRecordRegistry, RecordRegistryFuture, RecordRegistryGuard,
    RecordRegistryRepository, RecordView, RegistryPage, RegistryQuery, record_registry_list,
    record_registry_page,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeSet;

const NOW: i64 = 500;

#[derive(Default)]
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
                    "Synthetic module denial.",
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

fn actor(assignment: &str, user: &str, role: &str) -> RecordContextActor {
    RecordContextActor {
        user_id: user.into(),
        holding_id: "h001".into(),
        company_id: "company".into(),
        branch_id: None,
        department_id: None,
        assignment_id: assignment.into(),
        assignment_version: Some(2),
        authority_revision: Some(9),
        role: role.into(),
        scope_kind: "holding".into(),
        modules: vec!["operations".into(), "crm".into()],
        permissions: vec!["records.read".into()],
        expires_at: 1_000_000,
    }
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

fn requires(error: PgTransactionError<BusinessError>, code: &str) {
    assert!(
        matches!(
            error,
            PgTransactionError::Persistence(ref value) if value.to_string().contains(code)
        ),
        "expected {code}, got {error:?}"
    );
}

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES \
         ('*','records_reader','Records reader',false,1,'active'),\
         ('*','limited_reader','Limited reader',false,1,'active') \
         ON CONFLICT(company_id,id) DO UPDATE SET published_revision=excluded.published_revision,status='active';\
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','records_reader',1,'[\"records.read\"]','[]','[\"holding\",\"branch\",\"department\"]','system',0),\
         ('*','limited_reader',1,'[\"iam.self\"]','[]','[\"holding\"]','system',0) \
         ON CONFLICT(company_id,role_id,revision) DO NOTHING;\
         INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version) VALUES \
         ('company','actor','local:actor',NULL,'active','local',1),\
         ('company','branch-user','local:branch',NULL,'active','local',1),\
         ('company','dept-user','local:dept',NULL,'active','local',1),\
         ('company','revoked-user','local:revoked',NULL,'active','local',1),\
         ('company','limited-user','local:limited',NULL,'active','local',1),\
         ('foreign','foreign-user','local:foreign',NULL,'active','local',1);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,version,scope_kind,scope_node_id,parent_id,source,source_id) VALUES \
         ('assignment-actor','actor','records_reader','[\"operations\",\"crm\"]','company',NULL,0,1000000,NULL,2,'holding',NULL,NULL,'manual',NULL),\
         ('assignment-branch','branch-user','records_reader','[\"operations\"]','company','branch-1',0,1000000,NULL,2,'branch','branch-1',NULL,'manual',NULL),\
         ('assignment-dept','dept-user','records_reader','[\"operations\"]','company','branch-1',0,1000000,NULL,2,'department','dept-1',NULL,'manual',NULL),\
         ('assignment-revoked','revoked-user','records_reader','[\"operations\"]','company',NULL,0,1000000,400,2,'holding',NULL,NULL,'manual',NULL),\
         ('assignment-limited','limited-user','limited_reader','[\"operations\"]','company',NULL,0,1000000,NULL,2,'holding',NULL,NULL,'manual',NULL),\
         ('assignment-foreign','foreign-user','records_reader','[\"operations\"]','foreign',NULL,0,1000000,NULL,2,'holding',NULL,NULL,'manual',NULL);\
         INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,branch_id,department_id,version,created_by,updated_by,created_at,updated_at,archived_at) \
         SELECT 'operations','rec-'||lpad(value::text,3,'0'),'overview',\
           CASE WHEN value=20 THEN 'Quarterly NEEDLE review' ELSE 'Service record '||value END,\
           'Bounded registry row '||value,'draft',\
           '[{\"key\":\"note\",\"label\":\"Note\",\"type\":\"text\",\"value\":\"ok\"}]',\
           'company',NULL,NULL,1,'maker','maker',100,1000+(value/3),NULL \
         FROM generate_series(1,65) value;\
         INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,branch_id,department_id,version,created_by,updated_by,created_at,updated_at,archived_at) VALUES \
         ('operations','rec-foreign','overview','Foreign record','Foreign company','draft','[{\"key\":\"note\",\"label\":\"Note\",\"type\":\"text\",\"value\":\"foreign\"}]','foreign',NULL,NULL,1,'maker','maker',100,2000,NULL),\
         ('operations','rec-archived','overview','Archived record','Archived source','draft','[{\"key\":\"note\",\"label\":\"Note\",\"type\":\"text\",\"value\":\"old\"}]','company',NULL,NULL,1,'maker','maker',100,900,910),\
         ('operations','rec-draft','drafts','Draft record','Other section','draft','[{\"key\":\"note\",\"label\":\"Note\",\"type\":\"text\",\"value\":\"draft\"}]','company',NULL,NULL,1,'maker','maker',100,700,NULL),\
         ('operations','rec-branch-1','branching','Branch one','Branch source','draft','[{\"key\":\"note\",\"label\":\"Note\",\"type\":\"text\",\"value\":\"branch\"}]','company','branch-1',NULL,1,'maker','maker',100,430,NULL),\
         ('operations','rec-branch-2','branching','Branch two','Other branch','draft','[{\"key\":\"note\",\"label\":\"Note\",\"type\":\"text\",\"value\":\"other\"}]','company','branch-2',NULL,1,'maker','maker',100,430,NULL),\
         ('operations','rec-dept-1','branching','Department one','Department source','draft','[{\"key\":\"note\",\"label\":\"Note\",\"type\":\"text\",\"value\":\"department\"}]','company','branch-1','dept-1',1,'maker','maker',100,430,NULL),\
         ('operations','rec-dept-2','branching','Department two','Other department','draft','[{\"key\":\"note\",\"label\":\"Note\",\"type\":\"text\",\"value\":\"other\"}]','company','branch-2','dept-2',1,'maker','maker',100,430,NULL),\
         ('crm','party-1','overview','Foreign owner party','CRM party','draft','[{\"key\":\"name\",\"label\":\"Name\",\"type\":\"text\",\"value\":\"Party\"}]','company',NULL,NULL,1,'maker','maker',100,440,NULL);\
         INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,branch_id,department_id,version,created_by,updated_by,created_at,updated_at,archived_at) \
         SELECT 'operations','large-'||lpad(value::text,3,'0'),'large','Large record '||value,\
           'Large section row','draft',\
           '[{\"key\":\"note\",\"label\":\"Note\",\"type\":\"text\",\"value\":\"large\"}]',\
           'company',NULL,NULL,1,'maker','maker',100,5000+value,NULL \
         FROM generate_series(1,501) value;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn page(
    pool: &DatabasePool,
    actor: &RecordContextActor,
    query: &RegistryQuery,
    guard: &mut Guard,
) -> Result<RegistryPage, PgTransactionError<BusinessError>> {
    let mut tx = pool
        .raw()
        .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE READ ONLY")
        .await
        .unwrap();
    let mut repository = PgRecordRegistry::new(&mut tx);
    let result = record_registry_page(&mut repository, guard, actor, query, NOW).await;
    match result {
        Ok(value) => {
            tx.commit().await.unwrap();
            Ok(value)
        }
        Err(error) => {
            tx.rollback().await.unwrap();
            Err(error)
        }
    }
}

async fn list(
    pool: &DatabasePool,
    actor: &RecordContextActor,
    query: &RegistryQuery,
    guard: &mut Guard,
) -> Result<Vec<RecordView>, PgTransactionError<BusinessError>> {
    let mut tx = pool
        .raw()
        .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE READ ONLY")
        .await
        .unwrap();
    let mut repository = PgRecordRegistry::new(&mut tx);
    let result = record_registry_list(&mut repository, guard, actor, query, NOW).await;
    match result {
        Ok(value) => {
            tx.commit().await.unwrap();
            Ok(value)
        }
        Err(error) => {
            tx.rollback().await.unwrap();
            Err(error)
        }
    }
}

#[tokio::test]
async fn page_reads_scoped_searchable_records_and_navigates_both_ways() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let actor_value = actor("assignment-actor", "actor", "records_reader");
        let mut guard = Guard::default();
        let first = page(
            &pool,
            &actor_value,
            &RegistryQuery::page("operations", "overview", None, None).unwrap(),
            &mut guard,
        )
        .await
        .unwrap();
        assert_eq!(first.items.len(), 30);
        assert_eq!(first.items[0].id, "rec-065");
        assert_eq!(first.items[29].id, "rec-036");
        assert!(first.has_more);
        assert!(first.previous_cursor.is_none());
        assert!(first.items.iter().all(|item| item.section_id == "overview"));
        assert!(
            first
                .items
                .iter()
                .all(|item| item.module_id == "operations")
        );
        assert!(!first.items.iter().any(|item| item.id == "rec-foreign"));
        assert!(!first.items.iter().any(|item| item.id == "rec-archived"));

        let second = page(
            &pool,
            &actor_value,
            &RegistryQuery::page("operations", "overview", None, first.next_cursor.as_deref())
                .unwrap(),
            &mut guard,
        )
        .await
        .unwrap();
        assert_eq!(second.items[0].id, "rec-035");
        assert_eq!(second.items[29].id, "rec-006");
        let third = page(
            &pool,
            &actor_value,
            &RegistryQuery::page(
                "operations",
                "overview",
                None,
                second.next_cursor.as_deref(),
            )
            .unwrap(),
            &mut guard,
        )
        .await
        .unwrap();
        assert_eq!(third.items.len(), 5);
        assert_eq!(third.items[4].id, "rec-001");
        assert!(!third.has_more);

        let back = page(
            &pool,
            &actor_value,
            &RegistryQuery::page(
                "operations",
                "overview",
                None,
                second.previous_cursor.as_deref(),
            )
            .unwrap(),
            &mut guard,
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

        let search = page(
            &pool,
            &actor_value,
            &RegistryQuery::page("operations", "overview", Some("needle review"), None).unwrap(),
            &mut guard,
        )
        .await
        .unwrap();
        assert_eq!(search.items.len(), 1);
        assert_eq!(search.items[0].id, "rec-020");
        let none = page(
            &pool,
            &actor_value,
            &RegistryQuery::page("operations", "overview", Some("%"), None).unwrap(),
            &mut guard,
        )
        .await
        .unwrap();
        assert!(none.items.is_empty());
    })
    .await;
}

#[tokio::test]
async fn company_branch_department_and_archived_rows_are_isolated() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut guard = Guard::default();
        let branch_actor = RecordContextActor {
            branch_id: Some("branch-1".into()),
            scope_kind: "branch".into(),
            ..actor("assignment-branch", "branch-user", "records_reader")
        };
        let branch = page(
            &pool,
            &branch_actor,
            &RegistryQuery::page("operations", "branching", None, None).unwrap(),
            &mut guard,
        )
        .await
        .unwrap();
        assert_eq!(
            branch
                .items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["rec-dept-1", "rec-branch-1"]
        );

        let dept_actor = RecordContextActor {
            branch_id: Some("branch-1".into()),
            department_id: Some("dept-1".into()),
            scope_kind: "department".into(),
            ..actor("assignment-dept", "dept-user", "records_reader")
        };
        let dept = page(
            &pool,
            &dept_actor,
            &RegistryQuery::page("operations", "branching", None, None).unwrap(),
            &mut guard,
        )
        .await
        .unwrap();
        assert_eq!(
            dept.items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["rec-dept-1"]
        );

        let all = list(
            &pool,
            &actor("assignment-actor", "actor", "records_reader"),
            &RegistryQuery::list("operations", "overview").unwrap(),
            &mut guard,
        )
        .await
        .unwrap();
        assert_eq!(all.len(), 65);
        assert!(!all.iter().any(|item| item.id == "rec-foreign"));
        assert!(!all.iter().any(|item| item.id == "rec-archived"));
        assert!(!all.iter().any(|item| item.id == "rec-draft"));
    })
    .await;
}

#[tokio::test]
async fn revoked_expired_role_and_foreign_assignments_are_denied_before_rows() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut guard = Guard::default();
        let query = RegistryQuery::page("operations", "overview", None, None).unwrap();
        for actor_value in [
            actor("assignment-revoked", "revoked-user", "records_reader"),
            actor("assignment-limited", "limited-user", "limited_reader"),
            actor("assignment-foreign", "foreign-user", "records_reader"),
        ] {
            assert_eq!(
                business(
                    page(&pool, &actor_value, &query, &mut guard)
                        .await
                        .unwrap_err()
                )
                .code,
                "WORK_CONTEXT_CHANGED"
            );
        }
        let mut expired = actor("assignment-actor", "actor", "records_reader");
        expired.expires_at = NOW;
        assert_eq!(
            business(page(&pool, &expired, &query, &mut guard).await.unwrap_err()).code,
            "WORK_CONTEXT_CHANGED"
        );
        let mut no_module = actor("assignment-actor", "actor", "records_reader");
        no_module.modules = vec!["crm".into()];
        assert_eq!(
            business(
                page(&pool, &no_module, &query, &mut guard)
                    .await
                    .unwrap_err()
            )
            .code,
            "MODULE_FORBIDDEN"
        );
    })
    .await;
}

#[tokio::test]
async fn cross_owner_module_and_row_guards_filter_or_fail_closed() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let actor_value = actor("assignment-actor", "actor", "records_reader");
        let query = RegistryQuery::page("operations", "overview", None, None).unwrap();
        let mut denied_module = Guard {
            deny_module: true,
            ..Guard::default()
        };
        assert_eq!(
            business(
                page(&pool, &actor_value, &query, &mut denied_module)
                    .await
                    .unwrap_err()
            )
            .code,
            "MODULE_NOT_FOUND"
        );

        let visible_ids = (1..=25).map(|index| format!("rec-{index:03}")).collect();
        let mut filtered = Guard {
            visible: Some(visible_ids),
            ..Guard::default()
        };
        let visible = page(&pool, &actor_value, &query, &mut filtered)
            .await
            .unwrap();
        assert_eq!(visible.items.len(), 25);
        assert_eq!(visible.items[0].id, "rec-025");
        assert_eq!(visible.items[24].id, "rec-001");
        assert!(!visible.has_more);

        let mut failing = Guard {
            fail: true,
            ..Guard::default()
        };
        assert_eq!(
            business(
                page(&pool, &actor_value, &query, &mut failing)
                    .await
                    .unwrap_err()
            )
            .code,
            "SOURCE_GUARD_UNAVAILABLE"
        );
    })
    .await;
}

#[tokio::test]
async fn cursor_scope_and_read_only_serializable_transaction_are_required() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let actor_value = actor("assignment-actor", "actor", "records_reader");
        let mut guard = Guard::default();
        let first = page(
            &pool,
            &actor_value,
            &RegistryQuery::page("operations", "overview", None, None).unwrap(),
            &mut guard,
        )
        .await
        .unwrap();
        let wrong_scope = page(
            &pool,
            &actor_value,
            &RegistryQuery::page(
                "operations",
                "overview",
                Some("needle"),
                first.next_cursor.as_deref(),
            )
            .unwrap(),
            &mut guard,
        )
        .await
        .unwrap_err();
        assert_eq!(business(wrong_scope).code, "INVALID_CURSOR");

        let mut connection = pool.raw().acquire().await.unwrap();
        let mut repository = PgRecordRegistry::new(&mut connection);
        requires(
            record_registry_page(
                &mut repository,
                &mut guard,
                &actor_value,
                &RegistryQuery::page("operations", "overview", None, None).unwrap(),
                NOW,
            )
            .await
            .unwrap_err(),
            "RECORDS_REGISTRY_TRANSACTION_REQUIRED",
        );
        drop(connection);

        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let mut repository = PgRecordRegistry::new(&mut tx);
        requires(
            record_registry_page(
                &mut repository,
                &mut guard,
                &actor_value,
                &RegistryQuery::page("operations", "overview", None, None).unwrap(),
                NOW,
            )
            .await
            .unwrap_err(),
            "RECORDS_REGISTRY_READ_ONLY_REQUIRED",
        );
        tx.rollback().await.unwrap();

        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL READ COMMITTED READ ONLY")
            .await
            .unwrap();
        let mut repository = PgRecordRegistry::new(&mut tx);
        requires(
            record_registry_page(
                &mut repository,
                &mut guard,
                &actor_value,
                &RegistryQuery::page("operations", "overview", None, None).unwrap(),
                NOW,
            )
            .await
            .unwrap_err(),
            "RECORDS_REGISTRY_SERIALIZABLE_REQUIRED",
        );
        tx.rollback().await.unwrap();

        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE READ ONLY")
            .await
            .unwrap();
        let mut repository = PgRecordRegistry::new(&mut tx);
        requires(
            repository
                .read_rows(
                    &actor_value,
                    "operations",
                    "overview",
                    None,
                    None,
                    None,
                    true,
                    0,
                )
                .await
                .unwrap_err(),
            "RECORDS_REGISTRY_LIMIT_INVALID",
        );
        requires(
            repository
                .read_rows(
                    &actor_value,
                    "operations",
                    "overview",
                    None,
                    None,
                    None,
                    true,
                    MAX_FETCH + 1,
                )
                .await
                .unwrap_err(),
            "RECORDS_REGISTRY_LIMIT_INVALID",
        );
        requires(
            repository
                .read_rows(
                    &actor_value,
                    "operations",
                    "overview",
                    None,
                    Some(500),
                    None,
                    true,
                    30,
                )
                .await
                .unwrap_err(),
            "RECORDS_REGISTRY_CURSOR_INVALID",
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn unpaged_list_is_ordered_and_bounded() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let actor_value = actor("assignment-actor", "actor", "records_reader");
        let mut guard = Guard::default();
        let records = list(
            &pool,
            &actor_value,
            &RegistryQuery::list("operations", "overview").unwrap(),
            &mut guard,
        )
        .await
        .unwrap();
        assert_eq!(records.len(), 65);
        assert_eq!(records[0].id, "rec-065");
        assert_eq!(records[64].id, "rec-001");
        let error = list(
            &pool,
            &actor_value,
            &RegistryQuery::list("operations", "large").unwrap(),
            &mut guard,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "RECORDS_REGISTRY_SOURCE_INVALID");
    })
    .await;
}
