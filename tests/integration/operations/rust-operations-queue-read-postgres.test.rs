//! Native Operations queue reads over real migrated PostgreSQL. These are
//! owner read projections, not commands, source purchase or browser evidence.
#![allow(dead_code)]
#[path = "../../support/operations/rust-operations-database.rs"]
mod fixture;
use erpx_server_operations::public::queue_read::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::Connection;
use std::{future::Future, pin::Pin};

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
         ('policy-field','company','field','Field queue',1,'[\"field_service_order\"]',30,'published','maker','reviewer',2,0,1),\
         ('policy-foreign','other','service','Foreign queue',1,'[\"staff_task\"]',60,'published','maker','reviewer',2,0,1);\
         INSERT INTO operations_exceptions(id,company_id,branch_id,department_id,area,source_kind,source_id,source_version,source_reference_json,policy_id,policy_snapshot_json,title,issue,priority,status,owner_id,due_at,version,created_by,created_at,updated_at)VALUES\
         ('exception-open','company','branch',NULL,'service','staff_task','task-open',1,'{\"kind\":\"staff_task\",\"id\":\"task-open\"}','policy-service','{\"id\":\"policy-service\"}','Open task','Synthetic open issue','normal','open','owner',100,1,'maker',0,10),\
         ('exception-closed','company','branch',NULL,'service','asset','asset-closed',1,'{\"kind\":\"asset\",\"id\":\"asset-closed\"}','policy-service','{\"id\":\"policy-service\"}','Closed asset','Synthetic closed issue','high','open','owner',50,3,'maker',0,5),\
         ('exception-field','company','branch',NULL,'field','field_service_order','service-order',1,'{\"kind\":\"field_service_order\",\"id\":\"service-order\"}','policy-field','{\"id\":\"policy-field\"}','Field visit','Field source','normal','open','owner',100,1,'maker',0,10),\
         ('exception-foreign-branch','company','south',NULL,'service','staff_task','task-south',1,'{\"kind\":\"staff_task\",\"id\":\"task-south\"}','policy-service','{\"id\":\"policy-service\"}','Foreign branch','Synthetic foreign branch','normal','open','owner',100,1,'maker',0,10),\
         ('exception-foreign-company','other','branch',NULL,'service','staff_task','task-other',1,'{\"kind\":\"staff_task\",\"id\":\"task-other\"}','policy-foreign','{\"id\":\"policy-foreign\"}','Foreign company','Synthetic foreign company','normal','open','owner',100,1,'maker',0,10);\
         INSERT INTO operations_exception_events(id,exception_id,version,action,actor_id,reason,status,details_json,created_at)VALUES\
         ('event-open','exception-open',1,'open','maker','Synthetic open issue','open','{}',0),\
         ('event-closed','exception-closed',1,'open','maker','Synthetic closed issue','open','{}',0),\
         ('event-closed-2','exception-closed',2,'resolve','owner','Resolved evidence','awaiting_review','{}',1),\
         ('event-closed-3','exception-closed',3,'approve','reviewer','Reviewed evidence','resolved','{}',2),\
         ('event-closed-4','exception-closed',4,'close','owner','Closed evidence','closed','{}',3);\
         UPDATE operations_exceptions SET status='closed',resolution='Reviewed evidence',resolved_by='owner',reviewed_by='reviewer' WHERE id='exception-closed';",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn actor() -> OperationsReadActor {
    OperationsReadActor {
        user_id: "user".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        permissions: vec![
            "records.read".into(),
            "records.write".into(),
            "records.review".into(),
        ],
        modules: vec!["operations".into()],
        expires_at: 9_007_199_254_740_991,
    }
}

struct Sources {
    hidden: Vec<String>,
}
impl OperationsSourceVisibility for Sources {
    type Error = BusinessError;
    fn visible<'a>(
        &'a mut self,
        row: &'a QueueException,
    ) -> Pin<Box<dyn Future<Output = Result<Option<SourceView>, Self::Error>> + Send + 'a>> {
        let hidden = self.hidden.iter().any(|id| id == &row.id);
        Box::pin(async move {
            if hidden {
                return Ok(None);
            }
            Ok(Some(SourceView {
                kind: row.source_kind.clone(),
                id: row.source_id.clone(),
                version: 1,
                title: format!("Current {}", row.title),
                href: format!("/records/{}", row.source_id),
                label: format!("Source · {}", row.title),
                company_id: row.company_id.clone(),
                branch_id: row.branch_id.clone(),
                department_id: row.department_id.clone(),
                status: row.status.clone(),
            }))
        })
    }
}
struct People;
impl OperationsPeopleDirectory for People {
    type Error = BusinessError;
    fn people(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<PersonOption>, Self::Error>> + Send + '_>> {
        Box::pin(async {
            Ok(vec![PersonOption {
                id: "owner".into(),
                name: "Synthetic Owner".into(),
            }])
        })
    }
}

#[tokio::test]
async fn workspace_read_is_company_branch_scoped_and_preserves_owner_rows() {
    fixture::with_database(2, |holding| async move {
        migrate(&holding).await;
        seed(&holding).await;
        let mut tx = serializable(&holding).await;
        lock_operations_read_sources(&mut tx).await.unwrap();
        let mut repository = PgOperationsRead::new(&mut tx);
        let mut sources = Sources { hidden: vec![] };
        let mut people = People;
        let workspace =
            operations_workspace(&actor(), &mut repository, &mut sources, &mut people, 200)
                .await
                .unwrap();
        assert_eq!(workspace.exceptions.len(), 2);
        assert!(
            workspace
                .exceptions
                .iter()
                .all(|row| row.row.id != "exception-field"
                    && row.row.id != "exception-foreign-branch"
                    && row.row.id != "exception-foreign-company")
        );
        let open = workspace
            .exceptions
            .iter()
            .find(|row| row.row.id == "exception-open")
            .unwrap();
        assert_eq!(open.row.company_id, "company");
        assert_eq!(open.row.branch_id.as_deref(), Some("branch"));
        assert_eq!(open.source.kind, "staff_task");
        assert_eq!(open.source.title, "Current Open task");
        assert_eq!(workspace.policies.len(), 1);
        assert_eq!(workspace.policies[0].id, "policy-service");
        assert_eq!(workspace.summary.active, 1);
        assert_eq!(workspace.summary.overdue, 1);
        assert_eq!(workspace.people[0].name, "Synthetic Owner");
        assert!(workspace.can_write && workspace.can_review);

        let detail = operations_detail(
            &actor(),
            &mut repository,
            &mut sources,
            &mut people,
            "exception-closed",
            200,
        )
        .await
        .unwrap();
        assert_eq!(detail.row.row.status, "closed");
        assert_eq!(detail.history.len(), 4);
        assert_eq!(detail.history[0].action, "open");
        assert_eq!(detail.history[3].action, "close");
        assert_eq!(detail.value()["row"]["source"]["id"], "asset-closed");
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn hidden_sources_are_excluded_and_detail_is_a_scoped_not_found() {
    fixture::with_database(2, |holding| async move {
        migrate(&holding).await;
        seed(&holding).await;
        let mut tx = serializable(&holding).await;
        let mut repository = PgOperationsRead::new(&mut tx);
        let mut sources = Sources {
            hidden: vec!["exception-open".into()],
        };
        let mut people = People;
        let workspace =
            operations_workspace(&actor(), &mut repository, &mut sources, &mut people, 200)
                .await
                .unwrap();
        assert_eq!(workspace.exceptions.len(), 1);
        assert_eq!(workspace.exceptions[0].row.id, "exception-closed");
        assert_eq!(workspace.summary.active, 0);
        let error = operations_detail(
            &actor(),
            &mut repository,
            &mut sources,
            &mut people,
            "exception-open",
            200,
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            erpx_server_runtime::public::postgres::PgTransactionError::Business(ref error)
                if error.kind == FailureKind::NotFound
                    && error.code == "OPERATIONS_EXCEPTION_NOT_FOUND"
        ));

        // A different Company is separated by the owner SQL before visibility.
        let mut foreign = actor();
        foreign.company_id = "other".into();
        foreign.branch_id = None;
        let mut sources = Sources { hidden: vec![] };
        let other = operations_workspace(&foreign, &mut repository, &mut sources, &mut people, 200)
            .await
            .unwrap();
        assert_eq!(other.exceptions.len(), 1);
        assert_eq!(other.exceptions[0].row.id, "exception-foreign-company");
        assert_eq!(other.policies[0].id, "policy-foreign");
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn reads_require_a_serializable_owner_transaction() {
    fixture::with_database(1, |holding| async move {
        migrate(&holding).await;
        seed(&holding).await;
        let mut connection = holding.raw().acquire().await.unwrap();
        let error = read_operations_workspace(&mut connection, "company", Some("branch"), None)
            .await
            .unwrap_err();
        assert!(format!("{error:?}").contains("OPERATIONS_TRANSACTION_REQUIRED"));
        let mut tx = connection.begin().await.unwrap();
        let error = read_operations_workspace(&mut tx, "company", Some("branch"), None)
            .await
            .unwrap_err();
        assert!(format!("{error:?}").contains("OPERATIONS_SERIALIZABLE_REQUIRED"));
        tx.rollback().await.unwrap();
    })
    .await;
}
