//! Native Operations commands over real migrated PostgreSQL. These cases prove
//! owner SQL CAS, Company/scope isolation, serializable requirements and
//! rollback. Workflow receipts, audit/outbox and cross-owner source/recipient
//! witnesses remain composition responsibilities.
#![allow(dead_code)]
#[path = "../../support/operations/rust-operations-database.rs"]
mod fixture;
use erpx_server_operations::public::field_exceptions::*;
use erpx_server_operations::public::field_policy::*;
use erpx_server_operations::public::staff_tasks::{
    notify_staff_record, notify_staff_task_approval,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use erpx_server_shared::public::record_source_contract::{SourceKind, SourceRecord};
use erpx_server_shared::public::{CommandKey, CompanyId};
use sqlx::Connection;

async fn migrate(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(&root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
}

async fn serializable(pool: &DatabasePool) -> sqlx::Transaction<'static, sqlx::Postgres> {
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx
}

async fn seed(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO operations_queue_policies(id,company_id,area,name,revision,source_kinds_json,target_minutes,status,created_by,reviewed_by,version,created_at,updated_at)VALUES\
         ('policy-service','company','service','Service queue',1,'[\"staff_task\"]',60,'published','maker','reviewer',2,0,1),\
         ('policy-other','other','service','Foreign queue',1,'[\"staff_task\"]',60,'published','maker','reviewer',2,0,1);\
         INSERT INTO operations_exceptions(id,company_id,branch_id,department_id,area,source_kind,source_id,source_version,source_reference_json,policy_id,policy_snapshot_json,title,issue,priority,status,owner_id,due_at,version,created_by,created_at,updated_at)VALUES\
         ('exception-open','company','branch',NULL,'service','staff_task','task-open',1,'{\"kind\":\"staff_task\",\"id\":\"task-open\"}','policy-service','{\"id\":\"policy-service\"}','Open task','Synthetic open issue','normal','open','owner',100,1,'maker',0,10),\
         ('exception-other','other','branch',NULL,'service','staff_task','task-other',1,'{\"kind\":\"staff_task\",\"id\":\"task-other\"}','policy-other','{\"id\":\"policy-other\"}','Foreign task','Synthetic foreign issue','normal','open','owner',100,1,'maker',0,10);\
         INSERT INTO operations_exception_events(id,exception_id,version,action,actor_id,reason,status,details_json,created_at)VALUES\
         ('event-open','exception-open',1,'open','maker','Synthetic open issue','open','{}',0);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}
fn other_company() -> CompanyId {
    CompanyId::new("other").unwrap()
}
fn source(kind: SourceKind, company: &str, version: i64) -> SourceRecord {
    SourceRecord {
        kind,
        id: "task-open".into(),
        company_id: company.into(),
        branch_id: Some("branch".into()),
        department_id: None,
        version,
        title: "Current task".into(),
        status: "open".into(),
    }
}
fn source_json() -> serde_json::Value {
    serde_json::json!({"kind":"staff_task","id":"task-open","version":1})
}
fn open_input() -> OpenException {
    OpenException {
        source_id: "task-open".into(),
        source_version: 1,
        policy_id: "policy-service".into(),
        policy_version: 2,
        title: "Open task".into(),
        issue: "Synthetic open issue".into(),
        priority: "normal".into(),
        owner_id: "owner".into(),
    }
}
fn actor() -> Actor {
    Actor {
        user: "owner".into(),
        can_review: true,
        same_person_owner: false,
        same_person_resolver: false,
    }
}
fn transition_input(action: Action, version: i64) -> TransitionInput {
    TransitionInput {
        action,
        expected_version: version,
        reason: "Reviewed actual source".into(),
        owner_id: None,
        escalation_owner_id: None,
        resolution: None,
        priority: None,
    }
}

#[tokio::test]
async fn commands_require_serializable_and_rollback_owner_rows_and_events() {
    fixture::with_database(2, |holding| async move {
        migrate(&holding).await;
        seed(&holding).await;
        let mut connection = holding.raw().acquire().await.unwrap();
        let error = read_operations_exception(&mut connection, &company(), "exception-open", Some("branch"), None)
            .await
            .unwrap_err();
        assert!(format!("{error:?}").contains("OPERATIONS_TRANSACTION_REQUIRED"));
        let mut tx = connection.begin().await.unwrap();
        let error = read_operations_exception(&mut tx, &company(), "exception-open", Some("branch"), None)
            .await
            .unwrap_err();
        assert!(format!("{error:?}").contains("OPERATIONS_SERIALIZABLE_REQUIRED"));
        tx.rollback().await.unwrap();

        let mut tx = serializable(&holding).await;
        lock_field_exception_sources(&mut tx).await.unwrap();
        let area = read_policy(&mut tx, &company(), "policy-service")
            .await
            .unwrap()
            .unwrap();
        let policy = area.policy("policy-service").unwrap().clone();
        let key = CommandKey::new("command-key-01").unwrap();
        let opened = open_operations_exception(
            &mut tx,
            &company(),
            &key,
            "maker",
            &open_input(),
            &source(SourceKind::StaffTask, "company", 1),
            &source_json(),
            &policy,
            10,
        )
        .await
        .unwrap();
        assert_eq!(opened.data().source_kind, "staff_task");
        assert_eq!(opened.data().version, 1);

        let current = read_operations_exception(
            &mut tx,
            &company(),
            &format!("operations-exception-{}", key.as_str()),
            Some("branch"),
            None,
        )
        .await
        .unwrap()
        .unwrap();
        let plan = transition(
            &actor(),
            current.data(),
            &transition_input(Action::Acknowledge, 1),
        )
        .unwrap();
        let changed = apply_operations_exception_transition(
            &mut tx,
            &current,
            &actor(),
            &transition_input(Action::Acknowledge, 1),
            &plan,
            &CommandKey::new("command-key-02").unwrap(),
            20,
        )
        .await
        .unwrap();
        assert_eq!(changed.data().status, "acknowledged");
        assert_eq!(changed.data().version, 2);
        let stale = apply_operations_exception_transition(
            &mut tx,
            &current,
            &actor(),
            &transition_input(Action::Acknowledge, 1),
            &plan,
            &CommandKey::new("command-key-03").unwrap(),
            20,
        )
        .await
        .unwrap_err();
        assert!(format!("{stale:?}").contains("OPERATIONS_STATE_CHANGED"));
        let ids = sqlx::query_scalar::<_, String>(
            "SELECT id FROM operations_exceptions WHERE id LIKE 'operations-exception-command-key-01'",
        )
        .fetch_all(&mut *tx)
        .await
        .unwrap();
        assert_eq!(ids.len(), 1);
        tx.rollback().await.unwrap();

        let mut tx = serializable(&holding).await;
        let gone =
            read_operations_exception(&mut tx, &company(), "operations-exception-command-key-01", None, None)
                .await
                .unwrap();
        assert!(gone.is_none());
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn commands_reject_foreign_company_and_changed_source_versions() {
    fixture::with_database(1, |holding| async move {
        migrate(&holding).await;
        seed(&holding).await;
        let mut tx = serializable(&holding).await;
        lock_field_exception_sources(&mut tx).await.unwrap();
        let area = read_policy(&mut tx, &company(), "policy-service")
            .await
            .unwrap()
            .unwrap();
        let policy = area.policy("policy-service").unwrap().clone();
        let key = CommandKey::new("command-key-01").unwrap();
        let foreign = open_operations_exception(
            &mut tx,
            &company(),
            &key,
            "maker",
            &open_input(),
            &source(SourceKind::StaffTask, "other", 1),
            &source_json(),
            &policy,
            10,
        )
        .await
        .unwrap_err();
        assert!(format!("{foreign:?}").contains("SOURCE_RECORD_CHANGED"));
        let changed = open_operations_exception(
            &mut tx,
            &company(),
            &key,
            "maker",
            &open_input(),
            &source(SourceKind::StaffTask, "company", 2),
            &source_json(),
            &policy,
            10,
        )
        .await
        .unwrap_err();
        assert!(format!("{changed:?}").contains("SOURCE_RECORD_CHANGED"));
        assert!(
            read_operations_exception(&mut tx, &company(), "exception-other", None, None)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            read_operations_exception(&mut tx, &other_company(), "exception-other", None, None)
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            read_operations_exception(&mut tx, &company(), "exception-open", Some("south"), None)
                .await
                .unwrap()
                .is_none()
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn generic_policy_draft_and_review_are_company_scoped_and_rollback() {
    fixture::with_database(1, |holding| async move {
        migrate(&holding).await;
        seed(&holding).await;
        let mut tx = serializable(&holding).await;
        let area = read_policy_area(&mut tx, &company(), "service")
            .await
            .unwrap();
        let draft = DraftPolicy {
            area: "service".into(),
            name: "Revised service queue".into(),
            target_minutes: 30,
            source_kinds: vec!["staff_task".into(), "asset".into(), "staff_task".into()],
        };
        let after =
            draft_operations_policy(&mut tx, &area, &draft, "policy-service-2", "maker", 10)
                .await
                .unwrap();
        let created = after.policy("policy-service-2").unwrap();
        assert_eq!(created.status, "draft");
        assert_eq!(created.revision, 2);
        assert_eq!(created.source_kinds_json, "[\"asset\",\"staff_task\"]");
        let reviewed = review_operations_policy_area(
            &mut tx,
            &after,
            "policy-service-2",
            PolicyAction::Publish,
            1,
            false,
            "Independent policy review",
            "reviewer",
            20,
        )
        .await
        .unwrap();
        assert_eq!(
            reviewed.policy("policy-service-2").unwrap().status,
            "published"
        );
        assert_eq!(reviewed.policy("policy-service").unwrap().status, "retired");
        let after_second =
            draft_operations_policy(&mut tx, &reviewed, &draft, "policy-service-3", "maker", 25)
                .await
                .unwrap();
        let self_review = review_operations_policy_area(
            &mut tx,
            &after_second,
            "policy-service-3",
            PolicyAction::Publish,
            1,
            true,
            "Self review must not activate the policy",
            "maker",
            30,
        )
        .await
        .unwrap_err();
        assert!(format!("{self_review:?}").contains("CONFLICT"));
        let stale = review_operations_policy_area(
            &mut tx,
            &after_second,
            "policy-service-3",
            PolicyAction::Publish,
            2,
            false,
            "Obsolete decision must not pass",
            "reviewer",
            30,
        )
        .await
        .unwrap_err();
        assert!(format!("{stale:?}").contains("CONFLICT"));
        tx.rollback().await.unwrap();

        let mut tx = serializable(&holding).await;
        assert!(
            read_policy_area(&mut tx, &company(), "service")
                .await
                .unwrap()
                .policy("policy-service-2")
                .is_none()
        );
        let other = read_policy_area(&mut tx, &other_company(), "service")
            .await
            .unwrap();
        assert!(other.policy("policy-service").is_none());
        assert_eq!(other.policy("policy-other").unwrap().company_id, "other");
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn generic_open_rejects_policies_without_the_canonical_source_kind() {
    fixture::with_database(1, |holding| async move {
        migrate(&holding).await;
        seed(&holding).await;
        let mut tx = serializable(&holding).await;
        lock_field_exception_sources(&mut tx).await.unwrap();
        let area = read_policy(&mut tx, &company(), "policy-service")
            .await
            .unwrap()
            .unwrap();
        let policy = area.policy("policy-service").unwrap().clone();
        let error = open_operations_exception(
            &mut tx,
            &company(),
            &CommandKey::new("command-key-01").unwrap(),
            "maker",
            &open_input(),
            &source(SourceKind::Asset, "company", 1),
            &serde_json::json!({"kind":"asset","id":"task-open","version":1}),
            &policy,
            10,
        )
        .await
        .unwrap_err();
        assert!(format!("{error:?}").contains("OPERATIONS_POLICY_UNAVAILABLE"));
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn staff_record_notification_is_idempotent_and_owned() {
    fixture::with_database(2, |holding| async move {
        migrate(&holding).await;
        let mut tx = serializable(&holding).await;
        notify_staff_record(
            &mut tx,
            "command-key-01",
            "company",
            "user-a",
            "supplier_promise",
            "order-1",
            2,
            "Supplier delivery confirmation changed: review remaining quantities and dates",
            10,
        )
        .await
        .unwrap();
        // Same key/user/record is a no-op even with a later clock.
        notify_staff_record(
            &mut tx,
            "command-key-01",
            "company",
            "user-a",
            "supplier_promise",
            "order-1",
            2,
            "Supplier delivery confirmation changed: review remaining quantities and dates",
            20,
        )
        .await
        .unwrap();
        let rows: Vec<(String, String, String, String, i64, String, i64)> = sqlx::query_as(
            "SELECT id,company_id,user_id,record_type,record_version,message,created_at \
             FROM staff_notifications ORDER BY id",
        )
        .fetch_all(&mut *tx)
        .await
        .unwrap();
        assert_eq!(
            rows,
            vec![(
                "staff-record-notice-command-key-01-user-a".into(),
                "company".into(),
                "user-a".into(),
                "supplier_promise".into(),
                2,
                "Supplier delivery confirmation changed: review remaining quantities and dates"
                    .into(),
                10,
            )]
        );

        notify_staff_record(
            &mut tx,
            "command-key-02",
            "company",
            "user-b",
            "supplier_promise",
            "order-1",
            2,
            "Supplier delivery confirmation changed: review remaining quantities and dates",
            11,
        )
        .await
        .unwrap();
        notify_staff_task_approval(
            &mut tx,
            "task-key-01",
            "company",
            "user-a",
            "task-1",
            3,
            "start",
            12,
        )
        .await
        .unwrap();
        let owners: Vec<String> =
            sqlx::query_scalar("SELECT id FROM staff_notifications ORDER BY id")
                .fetch_all(&mut *tx)
                .await
                .unwrap();
        assert_eq!(
            owners,
            vec![
                "staff-record-notice-command-key-01-user-a".to_string(),
                "staff-record-notice-command-key-02-user-b".to_string(),
                "staff-record-notice-task-key-01-user-a".to_string(),
            ]
        );

        let error = notify_staff_record(
            &mut tx,
            "",
            "company",
            "user-a",
            "supplier_promise",
            "order-1",
            2,
            "Message",
            10,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "STAFF_NOTIFICATION_INVALID");
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn unicode_exception_owner_writes_readback_and_invalid_rollback() {
    // SourceRecord and Actor are owner-test fixtures; selected Identity and
    // cross-owner source authorization remain composition responsibilities.
    fixture::with_database(2, |holding| async move {
        migrate(&holding).await;seed(&holding).await;
        let mut tx=serializable(&holding).await;lock_field_exception_sources(&mut tx).await.unwrap();
        let area=read_policy(&mut tx,&company(),"policy-service").await.unwrap().unwrap();let policy=area.policy("policy-service").unwrap().clone();
        for (key,title,issue) in [("unicode-invalid-title","😀".repeat(161),"valid issue".to_owned()),("unicode-invalid-issue","valid title".to_owned(),"😀".repeat(4001))] {
            let mut invalid=open_input();invalid.title=title;invalid.issue=issue;
            let result=open_operations_exception(&mut tx,&company(),&CommandKey::new(key).unwrap(),"maker",&invalid,&source(SourceKind::StaffTask,"company",1),&source_json(),&policy,10).await;
            assert!(matches!(result,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(ref e)) if e.code=="OPERATIONS_INPUT_INVALID"));
            let count:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM operations_exceptions WHERE id=$1),(SELECT count(*) FROM operations_exception_events WHERE exception_id=$1)").bind(format!("operations-exception-{key}")).fetch_one(&mut *tx).await.unwrap();assert_eq!(count,(0,0));
        }
        tx.rollback().await.unwrap();
        let absent:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM operations_exceptions WHERE id IN ('operations-exception-unicode-invalid-title','operations-exception-unicode-invalid-issue')),(SELECT count(*) FROM operations_exception_events WHERE exception_id IN ('operations-exception-unicode-invalid-title','operations-exception-unicode-invalid-issue'))").fetch_one(holding.raw()).await.unwrap();assert_eq!(absent,(0,0));
        let mut tx=serializable(&holding).await;lock_field_exception_sources(&mut tx).await.unwrap();
        let area=read_policy(&mut tx,&company(),"policy-service").await.unwrap().unwrap();let policy=area.policy("policy-service").unwrap().clone();
        let title="😀".repeat(160);let issue="😀".repeat(4000);let reason="🧭".repeat(4000);let mut input=open_input();input.title=format!("\u{feff}{title}\u{feff}");input.issue=issue.clone();
        let key=CommandKey::new("unicode-owner-create").unwrap();let opened=open_operations_exception(&mut tx,&company(),&key,"maker",&input,&source(SourceKind::StaffTask,"company",1),&source_json(),&policy,10).await.unwrap();opened.require_current(&mut tx).await.unwrap();assert_eq!(opened.data().title,title);assert_eq!(opened.data().issue,issue);
        let id="operations-exception-unicode-owner-create";let current=read_operations_exception(&mut tx,&company(),id,Some("branch"),None).await.unwrap().unwrap();
        let mut action=transition_input(Action::Acknowledge,1);action.reason=reason.clone();let plan=transition(&actor(),current.data(),&action).unwrap();
        let changed=apply_operations_exception_transition(&mut tx,&current,&actor(),&action,&plan,&CommandKey::new("unicode-owner-ack").unwrap(),20).await.unwrap();changed.require_current(&mut tx).await.unwrap();
        let events:Vec<(i64,String,String)>=sqlx::query_as("SELECT version,action,reason FROM operations_exception_events WHERE exception_id=$1 ORDER BY version").bind(id).fetch_all(&mut *tx).await.unwrap();assert_eq!(events,vec![(1,"open".into(),issue.clone()),(2,"acknowledge".into(),reason.clone())]);tx.commit().await.unwrap();
        let mut read=serializable(&holding).await;lock_field_exception_sources(&mut read).await.unwrap();let persisted=read_operations_exception(&mut read,&company(),id,Some("branch"),None).await.unwrap().unwrap();assert_eq!(persisted.data().title,title);assert_eq!(persisted.data().issue,issue);assert_eq!((persisted.data().status.as_str(),persisted.data().version),("acknowledged",2));
        let actual_reason:String=sqlx::query_scalar("SELECT reason FROM operations_exception_events WHERE exception_id=$1 AND version=2").bind(id).fetch_one(&mut *read).await.unwrap();assert_eq!(actual_reason,reason);read.rollback().await.unwrap();
    }).await;
}
