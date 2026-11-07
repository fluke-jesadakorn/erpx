//! Native Operations commands over real migrated PostgreSQL. These cases prove
//! owner SQL CAS, Company/scope isolation, serializable requirements and
//! rollback. Workflow receipts, audit/outbox and cross-owner source/recipient
//! witnesses remain composition responsibilities.
#![allow(dead_code)]
#[path = "../../support/operations/rust-operations-database.rs"]
mod fixture;
use erpx_server_operations::public::field_exceptions::*;
use erpx_server_operations::public::field_policy::*;
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
