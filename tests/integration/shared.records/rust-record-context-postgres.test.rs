//! Native Records context read over migrated PostgreSQL. These prove owner SQL,
//! Company/scope isolation, current IAM, guard filtering and bounded sources;
//! they are not an HTTP, browser or Account journey.
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_records::public::record_context::{
    PgRecordContext, RecordContextActor, RecordContextFuture, RecordContextGuard, record_context,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeSet;

const NOW: i64 = 500;

#[derive(Default)]
struct Guard {
    visible: BTreeSet<(String, String)>,
    deny_source: bool,
    fail_target: bool,
}

impl Guard {
    fn allow(mut self, module_id: &str, id: &str) -> Self {
        self.visible.insert((module_id.into(), id.into()));
        self
    }
}

impl RecordContextGuard for Guard {
    type Error = BusinessError;

    fn authorize_source<'a>(
        &'a mut self,
        _: &'a RecordContextActor,
        _: &'a str,
        _: &'a str,
    ) -> RecordContextFuture<'a, (), Self::Error> {
        Box::pin(async move {
            if self.deny_source {
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
            if self.fail_target {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "TARGET_GUARD_UNAVAILABLE",
                    "Synthetic target guard outage.",
                ));
            }
            Ok(self
                .visible
                .contains(&(module_id.to_owned(), id.to_owned())))
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
         INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,branch_id,department_id,version,created_by,updated_by,created_at,updated_at,archived_at) VALUES \
         ('operations','rec-1','overview','Service visit','Bounded record','draft','[{\"key\":\"note\",\"label\":\"Note\",\"type\":\"text\",\"value\":\"first\"}]','company',NULL,NULL,3,'maker','maker',100,500,NULL),\
         ('operations','rec-2','overview','Target record','Link target','approved','[{\"key\":\"note\",\"label\":\"Note\",\"type\":\"text\",\"value\":\"target\"}]','company',NULL,NULL,2,'maker','maker',100,480,NULL),\
         ('operations','rec-3','overview','Host record','Backlink host','in_review','[{\"key\":\"note\",\"label\":\"Note\",\"type\":\"text\",\"value\":\"host\"}]','company',NULL,NULL,3,'maker','maker',100,470,NULL),\
         ('operations','rec-archived','overview','Archived record','Archived source','draft','[{\"key\":\"note\",\"label\":\"Note\",\"type\":\"text\",\"value\":\"old\"}]','company',NULL,NULL,2,'maker','maker',100,410,420),\
         ('operations','rec-branch-1','overview','Branch one','Branch source','draft','[{\"key\":\"note\",\"label\":\"Note\",\"type\":\"text\",\"value\":\"branch\"}]','company','branch-1',NULL,1,'maker','maker',100,430,NULL),\
         ('operations','rec-branch-2','overview','Branch two','Other branch','draft','[{\"key\":\"note\",\"label\":\"Note\",\"type\":\"text\",\"value\":\"other\"}]','company','branch-2',NULL,1,'maker','maker',100,430,NULL),\
         ('operations','rec-dept-1','overview','Department one','Department source','draft','[{\"key\":\"note\",\"label\":\"Note\",\"type\":\"text\",\"value\":\"department\"}]','company','branch-1','dept-1',1,'maker','maker',100,430,NULL),\
         ('crm','party-1','overview','Foreign owner party','CRM party','draft','[{\"key\":\"name\",\"label\":\"Name\",\"type\":\"text\",\"value\":\"Party\"}]','company',NULL,NULL,1,'maker','maker',100,440,NULL),\
         ('operations','rec-foreign','overview','Foreign record','Foreign company','draft','[{\"key\":\"note\",\"label\":\"Note\",\"type\":\"text\",\"value\":\"foreign\"}]','foreign',NULL,NULL,1,'maker','maker',100,450,NULL);\
         INSERT INTO record_reference_edges(host_module,host_id,target_module,target_id,kind,ordinal,created_by,created_at) VALUES \
         ('operations','rec-1','operations','rec-2','dependsOn',0,'maker',400),\
         ('operations','rec-1','crm','party-1','authoredWith',1,'maker',410),\
         ('operations','rec-1','operations','rec-foreign','billedBy',2,'maker',420),\
         ('operations','rec-3','operations','rec-1','blocks',0,'maker',430),\
         ('operations','rec-foreign','operations','rec-1','fulfills',0,'maker',440),\
         ('operations','rec-archived','operations','rec-1','dependsOn',0,'maker',450);\
         INSERT INTO record_principal_mentions(host_module,host_id,target_kind,target_id,target_label,created_by,created_at) VALUES \
         ('operations','rec-1','person','user-1','Person One','maker',400),\
         ('operations','rec-1','group','team-1','Team One','maker',410);\
         INSERT INTO record_field_suggestions(id,module_id,record_id,field_key,proposed_value_json,previous_value_digest,source_versions_json,provenance_json,state,created_by,applied_by,created_at,updated_at) VALUES \
         ('suggestion-1','operations','rec-1','note','\"next\"','digest','[{\"record\":\"operations/rec-2\",\"version\":2}]','{\"provider_kind\":\"development\"}','ready','maker',NULL,420,420);\
         INSERT INTO workspace_record_transitions(id,module_id,record_id,from_status,to_status,action,note,actor_id,assignment_id,record_version,created_at) VALUES \
         ('transition-1','operations','rec-1','draft','in_review','submit','Submitted','maker','assignment-maker',2,460),\
         ('transition-2','operations','rec-1','in_review','approved','approve','Approved','reviewer','assignment-reviewer',3,470);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn read(
    pool: &DatabasePool,
    actor: &RecordContextActor,
    module_id: &str,
    id: &str,
    guard: &mut Guard,
) -> Result<serde_json::Value, PgTransactionError<BusinessError>> {
    let mut tx = pool
        .raw()
        .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
        .await
        .unwrap();
    let mut repository = PgRecordContext::new(&mut tx);
    let result = record_context(&mut repository, guard, actor, module_id, id, NOW).await;
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
async fn context_reads_scoped_registry_links_timeline_and_denied_sources() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut guard = Guard::default()
            .allow("operations", "rec-2")
            .allow("operations", "rec-3");
        let value = read(
            &pool,
            &actor("assignment-actor", "actor", "records_reader"),
            "operations",
            "rec-1",
            &mut guard,
        )
        .await
        .unwrap();
        assert_eq!(value["record"]["moduleId"], "operations");
        assert_eq!(value["record"]["id"], "rec-1");
        assert_eq!(value["record"]["title"], "Service visit");
        assert_eq!(value["record"]["version"], 3);
        assert_eq!(value["record"]["fields"][0]["value"], "first");
        assert_eq!(value["references"][0]["target_id"], "rec-2");
        assert_eq!(value["references"][0]["title"], "Target record");
        assert_eq!(value["references"][0]["access"], "available");
        assert_eq!(value["references"][1]["target_id"], "party-1");
        assert_eq!(value["references"][1]["title"], "Unavailable record");
        assert_eq!(value["references"][1]["access"], "denied");
        assert_eq!(value["references"][2]["target_id"], "rec-foreign");
        assert_eq!(value["references"][2]["access"], "denied");
        assert_eq!(value["backlinks"].as_array().unwrap().len(), 1);
        assert_eq!(value["backlinks"][0]["host_id"], "rec-3");
        assert_eq!(value["mentions"].as_array().unwrap().len(), 2);
        assert_eq!(value["suggestions"][0]["state"], "ready");
        assert_eq!(value["history"][0]["action"], "approve");
        assert_eq!(value["history"][1]["action"], "submit");
    })
    .await;
}

#[tokio::test]
async fn foreign_archived_branch_and_department_sources_are_isolated() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut guard = Guard::default();
        let actor_value = actor("assignment-actor", "actor", "records_reader");
        assert_eq!(
            business(
                read(&pool, &actor_value, "operations", "rec-foreign", &mut guard)
                    .await
                    .unwrap_err()
            )
            .code,
            "NOT_FOUND"
        );
        assert_eq!(
            business(
                read(
                    &pool,
                    &actor_value,
                    "operations",
                    "rec-archived",
                    &mut guard
                )
                .await
                .unwrap_err()
            )
            .code,
            "NOT_FOUND"
        );

        let branch_actor = RecordContextActor {
            branch_id: Some("branch-1".into()),
            scope_kind: "branch".into(),
            ..actor("assignment-branch", "branch-user", "records_reader")
        };
        assert!(
            read(
                &pool,
                &branch_actor,
                "operations",
                "rec-branch-1",
                &mut guard
            )
            .await
            .is_ok()
        );
        assert_eq!(
            business(
                read(
                    &pool,
                    &branch_actor,
                    "operations",
                    "rec-branch-2",
                    &mut guard
                )
                .await
                .unwrap_err()
            )
            .code,
            "NOT_FOUND"
        );

        let dept_actor = RecordContextActor {
            branch_id: Some("branch-1".into()),
            department_id: Some("dept-1".into()),
            scope_kind: "department".into(),
            ..actor("assignment-dept", "dept-user", "records_reader")
        };
        assert!(
            read(&pool, &dept_actor, "operations", "rec-dept-1", &mut guard)
                .await
                .is_ok()
        );
        assert_eq!(
            business(
                read(&pool, &dept_actor, "operations", "rec-branch-1", &mut guard)
                    .await
                    .unwrap_err()
            )
            .code,
            "NOT_FOUND"
        );
    })
    .await;
}

#[tokio::test]
async fn revoked_expired_role_and_foreign_assignments_are_denied_before_source() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut guard = Guard::default();
        for actor_value in [
            actor("assignment-revoked", "revoked-user", "records_reader"),
            actor("assignment-limited", "limited-user", "limited_reader"),
            actor("assignment-foreign", "foreign-user", "records_reader"),
        ] {
            assert_eq!(
                business(
                    read(&pool, &actor_value, "operations", "rec-1", &mut guard)
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
            business(
                read(&pool, &expired, "operations", "rec-1", &mut guard)
                    .await
                    .unwrap_err()
            )
            .code,
            "WORK_CONTEXT_CHANGED"
        );
        let mut foreign_scope = actor("assignment-actor", "actor", "records_reader");
        foreign_scope.company_id = "foreign".into();
        assert_eq!(
            business(
                read(&pool, &foreign_scope, "operations", "rec-1", &mut guard)
                    .await
                    .unwrap_err()
            )
            .code,
            "WORK_CONTEXT_CHANGED"
        );
    })
    .await;
}

#[tokio::test]
async fn source_and_target_guard_failures_fail_closed() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let actor_value = actor("assignment-actor", "actor", "records_reader");
        let mut denied = Guard {
            deny_source: true,
            ..Guard::default()
        };
        assert_eq!(
            business(
                read(&pool, &actor_value, "operations", "rec-1", &mut denied)
                    .await
                    .unwrap_err()
            )
            .code,
            "SOURCE_FORBIDDEN"
        );
        let mut failing = Guard {
            fail_target: true,
            ..Guard::default()
        };
        assert_eq!(
            business(
                read(&pool, &actor_value, "operations", "rec-1", &mut failing)
                    .await
                    .unwrap_err()
            )
            .code,
            "TARGET_GUARD_UNAVAILABLE"
        );
    })
    .await;
}

#[tokio::test]
async fn mention_and_timeline_limits_are_bounded_sources() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO record_principal_mentions(host_module,host_id,target_kind,target_id,target_label,created_by,created_at) \
             SELECT 'operations','rec-1','person','limit-'||value,'Person '||value,'maker',value FROM generate_series(1,201) value;",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let mut guard = Guard::default();
        assert_eq!(
            business(
                read(
                    &pool,
                    &actor("assignment-actor", "actor", "records_reader"),
                    "operations",
                    "rec-1",
                    &mut guard,
                )
                .await
                .unwrap_err()
            )
            .code,
            "RECORD_CONTEXT_SOURCE_INVALID"
        );
    })
    .await;
}

#[tokio::test]
async fn caller_must_supply_serializable_owner_unit() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let actor_value = actor("assignment-actor", "actor", "records_reader");
        let mut guard = Guard::default();
        let mut connection = pool.raw().acquire().await.unwrap();
        let mut repository = PgRecordContext::new(&mut connection);
        requires(
            record_context(
                &mut repository,
                &mut guard,
                &actor_value,
                "operations",
                "rec-1",
                NOW,
            )
            .await
            .unwrap_err(),
            "RECORDS_CONTEXT_TRANSACTION_REQUIRED",
        );
        drop(connection);

        let mut tx = pool.raw().begin().await.unwrap();
        let mut repository = PgRecordContext::new(&mut tx);
        requires(
            record_context(
                &mut repository,
                &mut guard,
                &actor_value,
                "operations",
                "rec-1",
                NOW,
            )
            .await
            .unwrap_err(),
            "RECORDS_CONTEXT_SERIALIZABLE_REQUIRED",
        );
        tx.rollback().await.unwrap();
    })
    .await;
}
