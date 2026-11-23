//! Native CRM activity workspace read over real migrated PostgreSQL.
//!
//! Owner-side parity only: the exact repository/directory SELECTs are exercised
//! on the caller's serializable unit and their projected JSON is compared with
//! the retained TypeScript reader shape. No composition, purchase gate,
//! Workflow ledger, selected-actor authority or browser evidence is claimed.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_crm::public::activity_workspace::{
    ActivityActor, PgActivityDirectory, PgActivityWorkspace, activity_work, activity_workspace,
    due_date_millis,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    // Current Identity assignment, People workforce facts and one eligible
    // salesperson; a second activity owner has no assignment (ineligible).
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
          ('company','company','company','Company',NULL,'company',NULL,'active',1),\
          ('branch','branch','branch','Branch','company','company',NULL,'active',1),\
          ('other','branch','other','Other','company','company',NULL,'active',1);\
         INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','ws_sales','Workspace sales',false,1,'active');\
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','ws_sales',1,'[\"commerce.read\",\"commerce.sales\",\"commerce.approve\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('ws-grant','owner','ws_sales','[]','company',0,9007199254740991,1,'holding');\
         INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES('ws-person','owner','company','Owner Name','TH','fixture',0,0);\
         INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES('ws-engagement','ws-person','company','employee','salaried','active','2000-01-01','Sales','THB','fixture',0,0);\
         INSERT INTO crm_leads(id,company_id,branch_id,department_id,title,contact_name,contact_detail,source,brief,owner_id,due_date,status,version,created_by,created_at,updated_at) VALUES \
          ('lead','company','branch',NULL,'Lead','Customer','Contact','Staff','Brief','owner','2026-12-01','open',1,'maker',1,1),\
          ('lead2','company','branch',NULL,'Second lead','Customer','Contact','Staff','Brief','stranger','2026-12-02','open',1,'maker',1,1);\
         INSERT INTO crm_lead_activities(id,company_id,lead_id,activity_type,subject,details,owner_id,due_date,status,outcome,version,created_by,created_at,updated_at) VALUES \
          ('activity','company','lead','call','Call','Discuss','owner','2026-11-20','open',NULL,1,'owner',1,2),\
          ('other-activity','company','lead2','email','Email','Follow up','stranger','2026-11-21','open',NULL,1,'maker',1,2);\
         INSERT INTO crm_lead_activity_events(id,activity_id,version,action,actor_id,owner_id,due_date,note,snapshot_json,created_at) VALUES \
          ('event-activity','activity',1,'create','owner','owner','2026-11-20','Created','{}',1),\
          ('event-other','other-activity',1,'create','maker','stranger','2026-11-21','Created','{}',1);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn actor() -> ActivityActor {
    ActivityActor {
        user_id: "owner".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        can_sales: true,
        can_manage: true,
        has_read: true,
        same_person_ids: vec![],
    }
}

#[tokio::test]
async fn native_workspace_and_work_match_the_retained_reader_shape() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut rows_connection = pool.raw().begin().await.unwrap();
        let mut directory_connection = pool.raw().begin().await.unwrap();
        for connection in [&mut rows_connection, &mut directory_connection] {
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
                .execute(&mut **connection)
                .await
                .unwrap();
        }
        let a = actor();
        let mut repository = PgActivityWorkspace::new(&mut rows_connection);
        let mut directory = PgActivityDirectory::new(&mut directory_connection);
        let data = activity_workspace(&a, "lead", &mut repository, &mut directory)
            .await
            .unwrap();

        let value = serde_json::to_value(&data).unwrap();
        assert_eq!(value["lead"], serde_json::json!({
            "id": "lead", "title": "Lead", "status": "open", "stage": null, "version": 1
        }));
        assert_eq!(value["canCreate"], true);
        assert_eq!(value["canManage"], true);
        assert_eq!(value["items"].as_array().unwrap().len(), 1);
        assert_eq!(value["items"][0], serde_json::json!({
            "id": "activity", "company_id": "company", "lead_id": "lead",
            "activity_type": "call", "subject": "Call", "details": "Discuss",
            "owner_id": "owner", "due_date": "2026-11-20", "status": "open", "outcome": null,
            "version": 1, "created_by": "owner", "created_at": 1, "updated_at": 2,
            "lead_title": "Lead", "lead_status": "open", "lead_stage": null,
            "branch_id": "branch", "department_id": null, "owner_name": "Owner Name",
            "owner_eligible": 1, "ownerEligible": true, "canComplete": true,
            "canManage": true, "canChange": true
        }));
        assert_eq!(value["recipients"], serde_json::json!([
            {"id": "owner", "name": "Owner Name"}
        ]));
        assert_eq!(data.history.len(), 1);
        assert_eq!(data.history[0]["action"], "create");

        // The second lead's owner has no active sales assignment: ineligible
        // owner, no completion control, but the manager still sees a row.
        let other = activity_workspace(&a, "lead2", &mut repository, &mut directory)
            .await
            .unwrap();
        let other_value = serde_json::to_value(&other).unwrap();
        assert_eq!(other_value["items"][0]["owner_eligible"], 0);
        assert_eq!(other_value["items"][0]["ownerEligible"], false);
        assert_eq!(other_value["items"][0]["canComplete"], false);
        assert_eq!(other_value["items"][0]["canChange"], true);
        assert_eq!(other_value["history"][0]["activity_id"], "other-activity");

        let work = activity_work(&a, &mut repository).await.unwrap();
        assert_eq!(work.len(), 2);
        let work_value = serde_json::to_value(&work).unwrap();
        assert_eq!(work_value[0]["type"], "crm_activity");
        assert_eq!(work_value[0]["queue"], "todo");
        assert_eq!(work_value[0]["href"], "/crm/lead#crm-activities");
        assert_eq!(work_value[0]["dueAt"], due_date_millis("2026-11-20"));
        assert!(work_value[0]["blocker"].is_null());
        assert_eq!(work_value[1]["queue"], "waiting");
        assert_eq!(
            work_value[1]["blocker"],
            "Assigned owner is no longer eligible; a sales manager must reassign with a reason"
        );

        rows_connection.rollback().await.unwrap();
        directory_connection.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn scoped_read_requires_current_unit_and_in_scope_lead() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;

        // A non-serializable unit is refused before any row is observed.
        let mut plain = pool.raw().begin().await.unwrap();
        let denied = PgActivityWorkspace::new(&mut plain)
            .lead(&actor(), "lead")
            .await;
        assert!(matches!(denied, Err(PgTransactionError::Persistence(_))));
        plain.rollback().await.unwrap();

        let mut connection = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *connection)
            .await
            .unwrap();
        let mut foreign = actor();
        foreign.branch_id = Some("other".into());
        assert!(PgActivityWorkspace::new(&mut connection)
            .lead(&foreign, "lead")
            .await
            .unwrap()
            .is_none());
        assert!(PgActivityWorkspace::new(&mut connection)
            .activities(&foreign, "lead")
            .await
            .unwrap()
            .is_empty());
        assert!(PgActivityWorkspace::new(&mut connection)
            .work_rows(&foreign)
            .await
            .unwrap()
            .is_empty());
        connection.rollback().await.unwrap();
    })
    .await;
}
