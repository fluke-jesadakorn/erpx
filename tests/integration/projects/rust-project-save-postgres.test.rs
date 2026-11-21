//! Native Projects create/edit commands on migrated PostgreSQL.
//! These tests prove the owner SQL, exact version guard, receipt/audit/outbox
//! presence, replay, Company isolation, frozen lifecycle and complete member
//! replacement; they do not prove purchase, People or browser journeys.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_projects::public::project_save::{
    PgProjectSaveCommands, ProjectSavePurchase, SaveProjectActor, SaveProjectCommand,
    save_project,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use std::collections::BTreeSet;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

fn actor(user: &str, company: &str) -> SaveProjectActor {
    SaveProjectActor {
        command: erpx_server_projects::public::project_commands::ProjectCommandActor {
            user_id: user.into(),
            company_id: CompanyId::new(company).unwrap(),
            branch_id: None,
            department_id: None,
            assignment_id: "assignment".into(),
            permissions: vec!["records.write".into()],
            modules: vec!["projects".into()],
            expires_at: NOW.get() + 60_000,
            active_staff: true,
            wbs_new_work: true,
            wbs_settle_existing: true,
        },
        staff_branch_id: Some("staff-branch".into()),
        staff_department_id: Some("staff-department".into()),
        active_staff_users: ["owner", "alias-user", "member", "second"]
            .into_iter()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>(),
        purchase: ProjectSavePurchase {
            new_work: true,
            settle_existing: true,
            history: true,
        },
    }
}

fn input(title: &str, members: &[&str], expected_version: i64) -> SaveProjectCommand {
    SaveProjectCommand {
        title: title.into(),
        description: "Bounded project brief".into(),
        member_user_ids: members.iter().map(|user| (*user).to_owned()).collect(),
        expected_version,
    }
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
        "INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status) VALUES \
         ('company','owner','principal-owner','person-owner','active'),\
         ('company','alias-user','principal-alias','person-owner','active'),\
         ('company','member','principal-member','person-member','active'),\
         ('company','second','principal-second','person-second','active'),\
         ('company','stranger','principal-stranger','person-stranger','active'),\
         ('foreign','owner','principal-foreign','person-foreign','active');\
         INSERT INTO staff_projects(id,company_id,branch_id,department_id,title,description,\
           owner_user_id,version,created_at,updated_at,lifecycle_status) VALUES \
         ('project-owned','company','branch','department','Owned project','Bounded source','owner',3,10,10,'active'),\
         ('project-frozen','company',NULL,NULL,'Frozen project','Closed source','owner',5,10,10,'completed'),\
         ('project-member','company',NULL,NULL,'Member project','Member source','member',1,10,10,'active'),\
         ('project-foreign','foreign',NULL,NULL,'Foreign project','Foreign source','owner',1,10,10,'active');\
         INSERT INTO staff_project_members(project_id,user_id,added_by,created_at) VALUES \
         ('project-owned','member','owner',10),\
         ('project-owned','stale-member','owner',10);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn create_on(
    pool: DatabasePool,
    key: &'static str,
    command: SaveProjectCommand,
) -> Result<(String, i64), PgTransactionError<BusinessError>> {
    PgUnitOfWork::new(pool)
        .run::<_, BusinessError, _>(
            TransactionOptions::default(),
            move |connection| {
                let command = command.clone();
                Box::pin(async move {
                    let actor = actor("owner", "company");
                    let mut repository = PgProjectSaveCommands::new(connection);
                    let receipt =
                        save_project(&actor, &mut repository, None, &command, key, NOW).await?;
                    Ok((receipt.id, receipt.version))
                })
            },
        )
        .await
}

#[tokio::test]
async fn create_writes_brief_membership_receipt_audit_and_outbox() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let (id, version) = create_on(
            pool.clone(),
            "command-create",
            input("Customer delivery", &["member", "member", "second"], 1),
        )
        .await
        .unwrap();
        assert_eq!(id, "project-command-create");
        assert_eq!(version, 1);
        let row: (String, Option<String>, Option<String>, String, i64, String, i64, i64) =
            sqlx::query_as(
                "SELECT title,branch_id,department_id,owner_user_id,version,lifecycle_status,\
                 created_at,updated_at FROM staff_projects WHERE id=$1 AND company_id='company'",
            )
            .bind(&id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(row.0, "Customer delivery");
        // Create uses the actor's active-staff placement, not a client scope.
        assert_eq!(row.1.as_deref(), Some("staff-branch"));
        assert_eq!(row.2.as_deref(), Some("staff-department"));
        assert_eq!(row.3, "owner");
        assert_eq!(row.4, 1);
        assert_eq!(row.5, "active");
        assert_eq!(row.6, NOW.get());
        assert_eq!(row.7, NOW.get());

        let members: Vec<(String, String, i64)> = sqlx::query_as(
            "SELECT user_id,added_by,created_at FROM staff_project_members \
             WHERE project_id=$1 ORDER BY user_id",
        )
        .bind(&id)
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            members,
            vec![
                ("member".into(), "owner".into(), NOW.get()),
                ("second".into(), "owner".into(), NOW.get()),
            ]
        );

        let receipt: (String, String, String, String, i64) = sqlx::query_as(
            "SELECT operation,record_id,result_json,request_hash,created_at \
             FROM command_receipts WHERE command_id='command-create'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipt.0, "project.create");
        assert_eq!(receipt.1, id);
        assert_eq!(
            receipt.2,
            serde_json::json!({"id": id, "version": 1}).to_string()
        );
        assert!(receipt.3.contains("project.create"));
        assert_eq!(receipt.4, NOW.get());

        let audit: Vec<(String, String, String, i64)> = sqlx::query_as(
            "SELECT action,record_type,record_id,record_version FROM audit_events \
             WHERE command_id='command-create'",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            audit,
            vec![(
                "project.create".into(),
                "project".into(),
                id.clone(),
                1
            )]
        );
        let outbox: Vec<(String, String, String, i64)> = sqlx::query_as(
            "SELECT event_type,aggregate_type,aggregate_id,aggregate_version FROM outbox_events \
             WHERE command_id='command-create'",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            outbox,
            vec![("project.create".into(), "project".into(), id, 1)]
        );
    })
    .await;
}

#[tokio::test]
async fn edit_uses_the_exact_version_guard_and_replaces_membership() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let (id, version) = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = actor("owner", "company");
                    let mut repository = PgProjectSaveCommands::new(connection);
                    let receipt = save_project(
                        &actor,
                        &mut repository,
                        Some("project-owned"),
                        &input("Edited delivery", &["second"], 3),
                        "command-edit",
                        NOW,
                    )
                    .await?;
                    Ok((receipt.id, receipt.version))
                })
            })
            .await
            .unwrap();
        assert_eq!(id, "project-owned");
        assert_eq!(version, 4);
        let row: (String, String, i64, i64, String, Option<String>, Option<String>) =
            sqlx::query_as(
                "SELECT title,description,version,updated_at,owner_user_id,branch_id,department_id \
                 FROM staff_projects WHERE id='project-owned'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(row.0, "Edited delivery");
        assert_eq!(row.1, "Bounded project brief");
        assert_eq!(row.2, 4);
        assert_eq!(row.3, NOW.get());
        // The retained revision never rewrites the current source scope.
        assert_eq!(row.4, "owner");
        assert_eq!(row.5.as_deref(), Some("branch"));
        assert_eq!(row.6.as_deref(), Some("department"));

        let members: Vec<String> = sqlx::query_scalar(
            "SELECT user_id FROM staff_project_members WHERE project_id='project-owned' ORDER BY user_id",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(members, vec!["second".to_owned()]);

        let receipt: String = sqlx::query_scalar(
            "SELECT result_json FROM command_receipts WHERE command_id='command-edit'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            receipt,
            serde_json::json!({"id": "project-owned", "version": 4}).to_string()
        );
        let outbox: i64 = sqlx::query_scalar(
            "SELECT aggregate_version FROM outbox_events WHERE command_id='command-edit'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(outbox, 4);
    })
    .await;
}

#[tokio::test]
async fn stale_version_fails_without_partial_membership_or_receipt() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let error = PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = actor("owner", "company");
                    let mut repository = PgProjectSaveCommands::new(connection);
                    save_project(
                        &actor,
                        &mut repository,
                        Some("project-owned"),
                        &input("Stale edit", &["second"], 2),
                        "command-stale",
                        NOW,
                    )
                    .await?;
                    Ok(())
                })
            })
            .await
            .map_err(business)
            .unwrap_err();
        assert_eq!(error.code, "CONFLICT");
        let title: String =
            sqlx::query_scalar("SELECT title FROM staff_projects WHERE id='project-owned'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(title, "Owned project");
        let members: Vec<String> = sqlx::query_scalar(
            "SELECT user_id FROM staff_project_members WHERE project_id='project-owned' ORDER BY user_id",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(members, vec!["member".to_owned(), "stale-member".to_owned()]);
        let receipts: i64 =
            sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id='command-stale'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(receipts, 0);
    })
    .await;
}

#[tokio::test]
async fn replay_returns_the_retained_receipt_and_conflicts_on_a_different_request() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let command = input("Customer delivery", &["member"], 1);
        let first = create_on(pool.clone(), "command-replay", command.clone())
            .await
            .unwrap();
        let second = create_on(pool.clone(), "command-replay", command)
            .await
            .unwrap();
        assert_eq!(first, second);
        let members: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM staff_project_members WHERE project_id=$1",
        )
        .bind(&first.0)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(members, 1);
        let audits: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit_events WHERE command_id='command-replay'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(audits, 1);

        let error = create_on(
            pool.clone(),
            "command-replay",
            input("Different delivery", &["member"], 1),
        )
        .await
        .map_err(business)
        .unwrap_err();
        assert_eq!(error.code, "COMMAND_KEY_CONFLICT");
    })
    .await;
}

#[tokio::test]
async fn foreign_and_member_only_projects_stay_out_of_scope() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let error = PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = actor("owner", "company");
                    let mut repository = PgProjectSaveCommands::new(connection);
                    save_project(
                        &actor,
                        &mut repository,
                        Some("project-foreign"),
                        &input("Foreign edit", &[], 1),
                        "command-foreign",
                        NOW,
                    )
                    .await?;
                    Ok(())
                })
            })
            .await
            .map_err(business)
            .unwrap_err();
        assert_eq!(error.code, "PROJECT_NOT_FOUND");
        let title: String =
            sqlx::query_scalar("SELECT title FROM staff_projects WHERE id='project-foreign'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(title, "Foreign project");

        // A member may read the project but may not rewrite its brief.
        let error = PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = actor("member", "company");
                    let mut repository = PgProjectSaveCommands::new(connection);
                    save_project(
                        &actor,
                        &mut repository,
                        Some("project-owned"),
                        &input("Member edit", &[], 3),
                        "command-member",
                        NOW,
                    )
                    .await?;
                    Ok(())
                })
            })
            .await
            .map_err(business)
            .unwrap_err();
        assert_eq!(error.code, "PROJECT_OWNER_REQUIRED");
    })
    .await;
}

#[tokio::test]
async fn a_closed_project_keeps_its_brief_and_membership_frozen() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let error = PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = actor("owner", "company");
                    let mut repository = PgProjectSaveCommands::new(connection);
                    save_project(
                        &actor,
                        &mut repository,
                        Some("project-frozen"),
                        &input("Frozen edit", &["second"], 5),
                        "command-frozen",
                        NOW,
                    )
                    .await?;
                    Ok(())
                })
            })
            .await
            .map_err(business)
            .unwrap_err();
        assert_eq!(error.code, "PROJECT_STRUCTURE_FROZEN");
        let row: (String, i64) =
            sqlx::query_as("SELECT title,version FROM staff_projects WHERE id='project-frozen'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(row, ("Frozen project".to_owned(), 5));
        let members: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM staff_project_members WHERE project_id='project-frozen'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(members, 0);
    })
    .await;
}

#[tokio::test]
async fn an_empty_membership_clears_the_current_set() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = actor("owner", "company");
                    let mut repository = PgProjectSaveCommands::new(connection);
                    save_project(
                        &actor,
                        &mut repository,
                        Some("project-owned"),
                        &input("Owned project", &[], 3),
                        "command-empty",
                        NOW,
                    )
                    .await?;
                    Ok(())
                })
            })
            .await
            .unwrap();
        let members: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM staff_project_members WHERE project_id='project-owned'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(members, 0);
        let version: i64 =
            sqlx::query_scalar("SELECT version FROM staff_projects WHERE id='project-owned'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(version, 4);
    })
    .await;
}

#[tokio::test]
async fn a_member_identity_alias_keeps_the_same_owner_and_membership_scope() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        // `alias-user` is the same natural Person as `owner` through IAM.
        let (_, version) = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = actor("alias-user", "company");
                    let mut repository = PgProjectSaveCommands::new(connection);
                    let receipt = save_project(
                        &actor,
                        &mut repository,
                        Some("project-owned"),
                        &input("Alias edit", &["member"], 3),
                        "command-alias",
                        NOW,
                    )
                    .await?;
                    Ok((receipt.id, receipt.version))
                })
            })
            .await
            .unwrap();
        assert_eq!(version, 4);
        let owner: String =
            sqlx::query_scalar("SELECT owner_user_id FROM staff_projects WHERE id='project-owned'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        // The retained revision cannot transfer ownership.
        assert_eq!(owner, "owner");
    })
    .await;
}
