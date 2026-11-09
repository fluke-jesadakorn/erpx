#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_organization::public::organization_nodes::{
    OrganizationAuthority, OrganizationCommand, OrganizationCommandActor,
};
use erpx_server_organization::public::organization_setup::*;
use erpx_server_runtime::public::postgres::{
    MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions, load_migrations,
    migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CompanyId, HoldingId, TimestampMillis, UserId,
};
use serde_json::json;

#[derive(Clone, Copy)]
struct TestClock;
impl Clock for TestClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(1_800_000_000_000)
    }
}

fn actor(
    company: &str,
    user: &str,
    assignment: &str,
    permissions: &[&str],
    branch: Option<&str>,
    department: Option<&str>,
) -> OrganizationCommandActor {
    OrganizationCommandActor {
        holding_id: HoldingId::new("h001").unwrap(),
        command: OrganizationCommand {
            user_id: UserId::new(user).unwrap(),
            assignment_id: AssignmentId::new(assignment).unwrap(),
            role: "custom_organization".into(),
            company_id: CompanyId::new(company).unwrap(),
            branch_id: branch.map(str::to_owned),
            department_id: department.map(str::to_owned),
            authority: Some(OrganizationAuthority {
                session_id: format!("session-{user}"),
                revision: 7,
                assignment_version: 1,
            }),
            iam_json: json!({
                "principalId": format!("principal-{user}"),
                "permissions": permissions,
                "grantablePermissions": [],
                "personId": format!("person-{user}"),
                "personUserIds": [user],
                "freshUntil": null
            })
            .to_string(),
            assignment_scope_kind: if department.is_some() {
                "department".into()
            } else if branch.is_some() {
                "branch".into()
            } else {
                "holding".into()
            },
            assignment_scope_node_id: department.or(branch).map(str::to_owned),
        },
        modules: vec!["people".into()],
        permissions: permissions.iter().map(|value| (*value).into()).collect(),
    }
}

fn owner() -> OrganizationCommandActor {
    actor(
        "company",
        "owner",
        "assignment-owner",
        &["organization.manage"],
        None,
        None,
    )
}

fn template(name: &str, expected_version: i64, positions: serde_json::Value) -> TemplateInput {
    TemplateInput::parse(&json!({
        "name": name,
        "positions": positions,
        "expectedVersion": expected_version
    }))
    .unwrap()
}

fn standard_template(expected_version: i64) -> TemplateInput {
    template(
        "Standard organization",
        expected_version,
        json!([
            {"key":"ceo","title":"CEO","levelNumber":1,"reportsToKey":null,"isCeo":true,"rosterRequired":false},
            {"key":"manager","title":"Manager","levelNumber":3,"reportsToKey":"ceo","isCeo":false,"rosterRequired":true},
            {"key":"staff","title":"Staff","levelNumber":6,"reportsToKey":"manager","isCeo":false,"rosterRequired":false}
        ]),
    )
}

async fn seed(pool: &erpx_server_runtime::public::postgres::DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) \
         VALUES('company','company','acme','Synthetic Acme',NULL,'company',NULL,'active'), \
               ('branch','branch','north','North','company','company','branch','active'), \
               ('department','department','service','Service','branch','company','branch','active'), \
               ('destination','branch','destination','Destination','company','company','destination','active'), \
               ('foreign','company','foreign','Foreign',NULL,'foreign',NULL,'active');\
         INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) \
         VALUES('*','custom_organization','Synthetic organization authority',false,1,'active'), \
               ('*','custom_people_only','Synthetic people-only authority',false,1,'active');\
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) \
         VALUES('*','custom_organization',1,'[\"organization.manage\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0), \
               ('*','custom_people_only',1,'[\"people.manage\"]','[]','[\"holding\"]','fixture',0);\
         UPDATE iam_roles SET published_revision=1 WHERE company_id='*' AND id IN ('custom_organization','custom_people_only');\
         INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,version,created_by,created_at,updated_at) \
         VALUES('person-owner','owner','company','Owner','TH','active',1,'fixture',0,0), \
               ('person-foreign','foreign-owner','foreign','Foreign owner','TH','active',1,'fixture',0,0), \
               ('person-limited','limited','company','Limited','TH','active',1,'fixture',0,0);\
         INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version) \
         VALUES('company','owner','local:owner','person-owner','active','local',1), \
               ('foreign','foreign-owner','local:foreign-owner','person-foreign','active','local',1), \
               ('company','limited','local:limited','person-limited','active','local',1);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) \
         VALUES('assignment-owner','owner','custom_organization','[\"people\"]','company',0,9999999999999,1,'holding'), \
               ('assignment-foreign','foreign-owner','custom_organization','[\"people\"]','foreign',0,9999999999999,1,'holding'), \
               ('assignment-limited','limited','custom_people_only','[\"people\"]','company',0,9999999999999,1,'holding');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn run<T, F>(
    pool: erpx_server_runtime::public::postgres::DatabasePool,
    f: F,
) -> Result<T, PgTransactionError<BusinessError>>
where
    F: for<'c> FnOnce(
            &'c mut sqlx::PgConnection,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<T, BusinessError>> + Send + 'c>,
        > + Send
        + 'static,
    T: Send + 'static,
{
    PgUnitOfWork::new(pool)
        .run_once(TransactionOptions::default(), move |connection| {
            Box::pin(async move { f(connection).await.map_err(PgTransactionError::Business) })
        })
        .await
}

fn business<T>(result: Result<T, PgTransactionError<BusinessError>>, code: &str) {
    match result {
        Err(PgTransactionError::Business(error)) => assert_eq!(error.code, code),
        Err(PgTransactionError::Persistence(error)) => {
            panic!("expected business denial {code}, got persistence {error:?}")
        }
        Ok(_) => panic!("expected business denial {code}, got success"),
    }
}

fn expect_business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(error) => {
            panic!("unexpected persistence {error:?}")
        }
    }
}

#[tokio::test]
async fn install_levels_persists_the_fixed_catalog_and_replays_without_duplicates() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let clock = TestClock;
        let installed = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationSetup::new(connection);
                install_levels(&mut ports, owner(), "organization-levels-0001", &clock)
                    .await
                    .map_err(expect_business)
            })
        })
        .await
        .unwrap();
        assert_eq!(installed, json!({"id": "company"}));
        let rows: Vec<(i64, String, String)> = sqlx::query_as(
            "SELECT level_number::bigint level_number,name,code FROM staff_levels WHERE company_id='company' ORDER BY level_number",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            rows,
            vec![
                (1, "CEO".into(), "ORG_L1".into()),
                (2, "Executive / Director".into(), "ORG_L2".into()),
                (3, "Branch / Department Manager".into(), "ORG_L3".into()),
                (4, "Supervisor".into(), "ORG_L4".into()),
                (5, "Senior Staff".into(), "ORG_L5".into()),
                (6, "Staff".into(), "ORG_L6".into())
            ]
        );
        let replayed = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationSetup::new(connection);
                install_levels(&mut ports, owner(), "organization-levels-0001", &clock)
                    .await
                    .map_err(expect_business)
            })
        })
        .await
        .unwrap();
        assert_eq!(replayed, installed);
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM staff_levels WHERE company_id='company'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(count, 6);
        let audit: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM audit_events WHERE command_id='organization-levels-0001'), \
             (SELECT count(*) FROM outbox_events WHERE command_id='organization-levels-0001')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(audit, (1, 1));
    })
    .await;
}

#[tokio::test]
async fn template_save_uses_exact_versions_and_company_isolation() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let clock = TestClock;
        let created = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationSetup::new(connection);
                save_template(
                    &mut ports,
                    owner(),
                    "organization-template-0001",
                    None,
                    standard_template(0),
                    &clock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await
        .unwrap();
        assert_eq!(
            created,
            json!({"id": "org-template-organization-template-0001", "version": 1})
        );
        let idempotency_conflict = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationSetup::new(connection);
                save_template(
                    &mut ports,
                    owner(),
                    "organization-template-0001",
                    None,
                    template(
                        "Different organization",
                        0,
                        json!([
                            {"key":"ceo","title":"CEO","levelNumber":1,"reportsToKey":null,"isCeo":true}
                        ]),
                    ),
                    &clock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await;
        business(idempotency_conflict, "IDEMPOTENCY_CONFLICT");
        let updated = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationSetup::new(connection);
                save_template(
                    &mut ports,
                    owner(),
                    "organization-template-0002",
                    Some("org-template-organization-template-0001"),
                    template(
                        "Renamed organization",
                        1,
                        json!([
                            {"key":"ceo","title":"CEO","levelNumber":1,"reportsToKey":null,"isCeo":true}
                        ]),
                    ),
                    &clock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await
        .unwrap();
        assert_eq!(updated["version"], 2);
        let saved: (String, i64) = sqlx::query_as(
            "SELECT name,version::bigint version FROM organization_templates WHERE id='org-template-organization-template-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(saved, ("Renamed organization".into(), 2));

        let stale = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationSetup::new(connection);
                save_template(
                    &mut ports,
                    owner(),
                    "organization-template-0003",
                    Some("org-template-organization-template-0001"),
                    standard_template(1),
                    &clock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await;
        business(stale, "CONFLICT");

        let foreign = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationSetup::new(connection);
                save_template(
                    &mut ports,
                    actor(
                        "foreign",
                        "foreign-owner",
                        "assignment-foreign",
                        &["organization.manage"],
                        None,
                        None,
                    ),
                    "organization-template-foreign-0001",
                    Some("org-template-organization-template-0001"),
                    template(
                        "Foreign update",
                        1,
                        json!([
                            {"key":"ceo","title":"CEO","levelNumber":1,"reportsToKey":null,"isCeo":true}
                        ]),
                    ),
                    &clock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await;
        business(foreign, "TEMPLATE_NOT_FOUND");

        let limited = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationSetup::new(connection);
                save_template(
                    &mut ports,
                    actor(
                        "company",
                        "limited",
                        "assignment-limited",
                        &["organization.manage"],
                        None,
                        None,
                    ),
                    "organization-template-limited-0001",
                    None,
                    standard_template(0),
                    &clock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await;
        business(limited, "ORGANIZATION_AUTHORITY_CHANGED");
    })
    .await;
}

#[tokio::test]
async fn apply_template_persists_positions_replays_and_refuses_stale_or_foreign_sources() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let clock = TestClock;
        run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationSetup::new(connection);
                install_levels(&mut ports, owner(), "organization-levels-apply-0001", &clock)
                    .await
                    .map_err(expect_business)
            })
        })
        .await
        .unwrap();
        let template_id = "org-template-apply-template-0001";
        run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationSetup::new(connection);
                save_template(
                    &mut ports,
                    owner(),
                    "apply-template-0001",
                    None,
                    standard_template(0),
                    &clock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await
        .unwrap();
        let applied = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationSetup::new(connection);
                apply_template(
                    &mut ports,
                    owner(),
                    "organization-apply-0001",
                    template_id,
                    ApplyTemplateInput {
                        expected_version: 1,
                        branch_id: Some("branch".into()),
                        department_id: Some("department".into()),
                        reason: "Apply the reviewed organization template".to_owned(),
                    },
                    &clock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await
        .unwrap();
        assert_eq!(
            applied,
            json!({
                "id": template_id,
                "positionIds": [
                    "position-organization-apply-0001-0",
                    "position-organization-apply-0001-1",
                    "position-organization-apply-0001-2"
                ]
            })
        );
        type PositionRow = (String, String, Option<String>, Option<String>, i64, i64);
        let positions: Vec<PositionRow> = sqlx::query_as(
            "SELECT id,title,reports_to_position_id,department_id,is_ceo::bigint is_ceo,roster_required::bigint roster_required \
             FROM organization_positions WHERE company_id='company' ORDER BY id",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(positions.len(), 3);
        assert!(positions.iter().any(|row| {
            row.0 == "position-organization-apply-0001-0"
                && row.1 == "CEO"
                && row.2.is_none()
                && row.4 == 1
        }));
        assert!(positions.iter().any(|row| {
            row.0 == "position-organization-apply-0001-1"
                && row.2.as_deref() == Some("position-organization-apply-0001-0")
                && row.3.as_deref() == Some("department")
                && row.5 == 1
        }));
        assert!(positions.iter().any(|row| {
            row.0 == "position-organization-apply-0001-2"
                && row.2.as_deref() == Some("position-organization-apply-0001-1")
        }));
        let replayed = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationSetup::new(connection);
                apply_template(
                    &mut ports,
                    owner(),
                    "organization-apply-0001",
                    template_id,
                    ApplyTemplateInput {
                        expected_version: 1,
                        branch_id: Some("branch".into()),
                        department_id: Some("department".into()),
                        reason: "Apply the reviewed organization template".to_owned(),
                    },
                    &clock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await
        .unwrap();
        assert_eq!(replayed, applied);
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM organization_positions WHERE company_id='company'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(count, 3);

        let stale = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationSetup::new(connection);
                apply_template(
                    &mut ports,
                    owner(),
                    "organization-apply-stale-0001",
                    template_id,
                    ApplyTemplateInput {
                        expected_version: 2,
                        branch_id: None,
                        department_id: None,
                        reason: "Apply the reviewed organization template".to_owned(),
                    },
                    &clock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await;
        business(stale, "CONFLICT");

        let foreign = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationSetup::new(connection);
                apply_template(
                    &mut ports,
                    actor(
                        "foreign",
                        "foreign-owner",
                        "assignment-foreign",
                        &["organization.manage"],
                        None,
                        None,
                    ),
                    "organization-apply-foreign-0001",
                    template_id,
                    ApplyTemplateInput {
                        expected_version: 1,
                        branch_id: None,
                        department_id: None,
                        reason: "Apply the reviewed organization template".to_owned(),
                    },
                    &clock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await;
        business(foreign, "TEMPLATE_NOT_FOUND");
    })
    .await;
}

#[tokio::test]
async fn apply_template_rolls_back_every_position_and_receipt_on_a_late_conflict() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let clock = TestClock;
        run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationSetup::new(connection);
                install_levels(&mut ports, owner(), "organization-levels-rollback-0001", &clock)
                    .await
                    .map_err(expect_business)
            })
        })
        .await
        .unwrap();
        let template_id = "org-template-rollback-template-0001";
        run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationSetup::new(connection);
                save_template(
                    &mut ports,
                    owner(),
                    "rollback-template-0001",
                    None,
                    standard_template(0),
                    &clock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,created_by,created_at,updated_at) \
             VALUES('position-organization-rollback-0001-1','company','branch','department','Conflict','org-level-company-3','fixture',0,0)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let failed = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationSetup::new(connection);
                apply_template(
                    &mut ports,
                    owner(),
                    "organization-rollback-0001",
                    template_id,
                    ApplyTemplateInput {
                        expected_version: 1,
                        branch_id: Some("branch".into()),
                        department_id: Some("department".into()),
                        reason: "Apply the reviewed organization template".to_owned(),
                    },
                    &clock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await;
        business(failed, "CONFLICT");
        let ceo: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM organization_positions WHERE id='position-organization-rollback-0001-0'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(ceo, 0);
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id='organization-rollback-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipts, 0);
    })
    .await;
}

#[tokio::test]
async fn apply_template_requires_active_placement_and_the_six_levels() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let clock = TestClock;
        let template_id = "org-template-placement-template-0001";
        run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationSetup::new(connection);
                save_template(
                    &mut ports,
                    owner(),
                    "placement-template-0001",
                    None,
                    standard_template(0),
                    &clock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await
        .unwrap();
        let missing_levels = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationSetup::new(connection);
                apply_template(
                    &mut ports,
                    owner(),
                    "organization-placement-levels-0001",
                    template_id,
                    ApplyTemplateInput {
                        expected_version: 1,
                        branch_id: None,
                        department_id: None,
                        reason: "Apply the reviewed organization template".to_owned(),
                    },
                    &clock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await;
        business(missing_levels, "LEVEL_REQUIRED");

        let inactive_branch = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationSetup::new(connection);
                apply_template(
                    &mut ports,
                    owner(),
                    "organization-placement-branch-0001",
                    template_id,
                    ApplyTemplateInput {
                        expected_version: 1,
                        branch_id: Some("missing-branch".into()),
                        department_id: None,
                        reason: "Apply the reviewed organization template".to_owned(),
                    },
                    &clock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await;
        business(inactive_branch, "BRANCH_INVALID");
    })
    .await;
}

#[tokio::test]
async fn records_department_head_preserves_actual_typescript_seat_and_legacy_precedence() {
    use erpx_server_organization::public::records_request_department_head::{
        lock_records_request_department_head, observe_records_request_department_head,
    };
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src/adapters/department-authority-reads.ts"),
        ).unwrap();
        let seat_prefix = "seat:(kind,scope)=>db.prepare('";
        let legacy_prefix = "legacyHead:async(id)=>(await db.prepare('";
        assert_eq!(source.matches(seat_prefix).count(), 1);
        assert_eq!(source.matches(legacy_prefix).count(), 1);
        let seat_sql = source.split_once(seat_prefix).unwrap().1.split_once('\'').unwrap().0;
        let legacy_sql = source.split_once(legacy_prefix).unwrap().1.split_once('\'').unwrap().0;
        assert_eq!(seat_sql.matches('?').count(), 3);
        assert_eq!(legacy_sql.matches('?').count(), 2);
        let mut seat_index = 0;
        let seat_sql = seat_sql.replace('?', "\u{1}").chars().map(|c| {
            if c == '\u{1}' { seat_index += 1; format!("${seat_index}") } else { c.to_string() }
        }).collect::<String>();
        let mut legacy_index = 0;
        let legacy_sql = legacy_sql.replace('?', "\u{1}").chars().map(|c| {
            if c == '\u{1}' { legacy_index += 1; format!("${legacy_index}") } else { c.to_string() }
        }).collect::<String>();
        let company = CompanyId::new("company").unwrap();
        let mut unlocked = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unlocked).await.unwrap();
        assert!(observe_records_request_department_head(&mut unlocked, &company, "department").await.is_err());
        unlocked.rollback().await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_records_request_department_head(&mut tx).await.unwrap();
        for phase in 0..4 {
            match phase {
                1 => { sqlx::query("INSERT INTO department_heads(department_id,company_id,person_id,version,updated_by,updated_at) VALUES('department','company','person-owner',1,'fixture',0)").execute(&mut *tx).await.unwrap(); },
                2 => { sqlx::query("INSERT INTO authority_seats(company_id,kind,scope_id,version,person_id,effective_at) VALUES('company','head','department',1,NULL,9999999999999)").execute(&mut *tx).await.unwrap(); },
                3 => { sqlx::query("UPDATE authority_seats SET person_id='person-limited',version=version+1 WHERE company_id='company' AND kind='head' AND scope_id='department'").execute(&mut *tx).await.unwrap(); },
                _ => {}
            }
            let seat: Option<sqlx::postgres::PgRow> = sqlx::query(sqlx::AssertSqlSafe(seat_sql.as_str()))
                .bind("company").bind("head").bind("department").fetch_optional(&mut *tx).await.unwrap();
            use sqlx::Row;
            let (seat_exists, expected): (bool, Option<String>) = if let Some(row) = seat {
                (true, row.try_get("person_id").unwrap())
            } else {
                let value = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(legacy_sql.as_str()))
                    .bind("company").bind("department").fetch_optional(&mut *tx).await.unwrap();
                (false, value)
            };
            assert_eq!(expected.as_deref(), [None, Some("person-owner"), None, Some("person-limited")][phase]);
            let witness = observe_records_request_department_head(&mut tx, &company, "department").await.unwrap();
            assert_eq!(witness.seat_exists(), seat_exists);
            assert_eq!(witness.person_id(), expected.as_deref());
            witness.require_current(&mut tx).await.unwrap();
            if phase == 3 {
                sqlx::query("SAVEPOINT head_drift").execute(&mut *tx).await.unwrap();
                sqlx::query("UPDATE authority_seats SET person_id=NULL WHERE company_id='company' AND kind='head' AND scope_id='department'").execute(&mut *tx).await.unwrap();
                assert!(witness.require_current(&mut tx).await.is_err());
                sqlx::query("ROLLBACK TO SAVEPOINT head_drift").execute(&mut *tx).await.unwrap();
                witness.require_current(&mut tx).await.unwrap();
            }
        }
        let foreign = CompanyId::new("foreign").unwrap();
        let absent = observe_records_request_department_head(&mut tx, &foreign, "department").await.unwrap();
        assert!(!absent.seat_exists());
        assert_eq!(absent.person_id(), None);
        assert!(observe_records_request_department_head(&mut tx, &company, &"x".repeat(8193)).await.is_err());
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn records_department_head_fences_shadow_rls_cross_unit_and_writer() {
    use erpx_server_organization::public::records_request_department_head::{lock_records_request_department_head, observe_records_request_department_head};
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let company = CompanyId::new("company").unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_records_request_department_head(&mut tx).await.unwrap();
        let source = observe_records_request_department_head(&mut tx, &company, "department").await.unwrap();
        assert!(observe_records_request_department_head(&mut tx, &company, &"x".repeat(8192)).await.is_ok());
        assert!(observe_records_request_department_head(&mut tx, &company, &"x".repeat(8193)).await.is_err());
        sqlx::query("SAVEPOINT head_shadow").execute(&mut *tx).await.unwrap();
        sqlx::query("CREATE TEMP TABLE authority_seats(company_id text,kind text,scope_id text,person_id text)").execute(&mut *tx).await.unwrap();
        assert!(observe_records_request_department_head(&mut tx, &company, "department").await.is_err());
        assert!(source.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT head_shadow").execute(&mut *tx).await.unwrap();
        source.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT head_rls").execute(&mut *tx).await.unwrap();
        let bypass: bool = sqlx::query_scalar("SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user").fetch_one(&mut *tx).await.unwrap();
        assert!(!bypass, "leased owner must not bypass RLS");
        sqlx::raw_sql("ALTER TABLE public.authority_seats ENABLE ROW LEVEL SECURITY; ALTER TABLE public.authority_seats FORCE ROW LEVEL SECURITY").execute(&mut *tx).await.unwrap();
        assert!(sqlx::query_scalar::<_, bool>("SELECT row_security_active('public.authority_seats'::regclass)").fetch_one(&mut *tx).await.unwrap());
        assert!(observe_records_request_department_head(&mut tx, &company, "department").await.is_err());
        assert!(source.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT head_rls").execute(&mut *tx).await.unwrap();
        source.require_current(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();
        let mut other = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        lock_records_request_department_head(&mut other).await.unwrap();
        assert!(source.require_current(&mut other).await.is_err());
        other.rollback().await.unwrap();
        let mut save = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *save).await.unwrap();
        sqlx::query("SAVEPOINT head_locks").execute(&mut *save).await.unwrap();
        lock_records_request_department_head(&mut save).await.unwrap();
        let retained = observe_records_request_department_head(&mut save, &company, "department").await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT head_locks").execute(&mut *save).await.unwrap();
        assert!(retained.require_current(&mut save).await.is_err());
        save.rollback().await.unwrap();
        let mut writer = pool.raw().begin().await.unwrap();
        let writer_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        sqlx::query("INSERT INTO department_heads(department_id,company_id,person_id,version,updated_by,updated_at) VALUES('department','company','person-owner',1,'fixture',0)").execute(&mut *writer).await.unwrap();
        let mut blocked = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *blocked).await.unwrap();
        let writer_holds: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=$1 AND relation='public.department_heads'::regclass AND mode='RowExclusiveLock' AND granted)").bind(writer_pid).fetch_one(&mut *blocked).await.unwrap();
        assert!(writer_holds, "the identified writer must hold the conflicting relation lock");
        let error = lock_records_request_department_head(&mut blocked).await.unwrap_err();
        assert!(matches!(error, PgTransactionError::Persistence(ref e) if e.sql_state()==Some("55P03") && e.retryable()));
        blocked.rollback().await.unwrap();
        writer.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn records_department_head_writer_first_and_legacy_source_fences() {
    use erpx_server_organization::public::records_request_department_head::{lock_records_request_department_head, observe_records_request_department_head};
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let company = CompanyId::new("company").unwrap();
        let mut writer = pool.raw().begin().await.unwrap();
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        sqlx::query("INSERT INTO public.department_heads(department_id,company_id,person_id,version,updated_by,updated_at) VALUES('department','company','person-owner',1,'fixture',0)").execute(&mut *writer).await.unwrap();
        let held: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=$1 AND relation='public.department_heads'::regclass AND mode='RowExclusiveLock' AND granted)").bind(pid).fetch_one(&mut *writer).await.unwrap();
        assert!(held);
        let mut blocked = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *blocked).await.unwrap();
        // No SELECT or source observation occurs in this transaction before the lock.
        let error = lock_records_request_department_head(&mut blocked).await.unwrap_err();
        assert!(matches!(error, PgTransactionError::Persistence(ref e) if e.sql_state()==Some("55P03") && e.retryable()));
        blocked.rollback().await.unwrap();
        writer.commit().await.unwrap();
        let mut retry = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *retry).await.unwrap();
        lock_records_request_department_head(&mut retry).await.unwrap();
        let source = observe_records_request_department_head(&mut retry, &company, "department").await.unwrap();
        assert!(!source.seat_exists());
        assert_eq!(source.person_id(), Some("person-owner"));
        sqlx::query("SAVEPOINT legacy_shadow").execute(&mut *retry).await.unwrap();
        sqlx::query("CREATE TEMP TABLE department_heads(department_id text,company_id text,person_id text)").execute(&mut *retry).await.unwrap();
        assert!(observe_records_request_department_head(&mut retry, &company, "department").await.is_err());
        assert!(source.require_current(&mut retry).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT legacy_shadow").execute(&mut *retry).await.unwrap();
        source.require_current(&mut retry).await.unwrap();
        sqlx::query("SAVEPOINT legacy_rls").execute(&mut *retry).await.unwrap();
        let bypass: bool = sqlx::query_scalar("SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user").fetch_one(&mut *retry).await.unwrap();
        assert!(!bypass);
        sqlx::raw_sql("ALTER TABLE public.department_heads ENABLE ROW LEVEL SECURITY; ALTER TABLE public.department_heads FORCE ROW LEVEL SECURITY").execute(&mut *retry).await.unwrap();
        assert!(sqlx::query_scalar::<_, bool>("SELECT row_security_active('public.department_heads'::regclass)").fetch_one(&mut *retry).await.unwrap());
        assert!(observe_records_request_department_head(&mut retry, &company, "department").await.is_err());
        assert!(source.require_current(&mut retry).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT legacy_rls").execute(&mut *retry).await.unwrap();
        source.require_current(&mut retry).await.unwrap();
        sqlx::query("UPDATE public.department_heads SET updated_by=repeat('x',2097152) WHERE department_id='department'").execute(&mut *retry).await.unwrap();
        let image_bytes: i64 = sqlx::query_scalar("SELECT octet_length(to_jsonb(h)::text)::bigint FROM public.department_heads h WHERE department_id='department'").fetch_one(&mut *retry).await.unwrap();
        assert!(image_bytes > 2097152);
        let limit = observe_records_request_department_head(&mut retry, &company, "department").await;
        assert!(matches!(limit, Err(PgTransactionError::Business(ref e)) if e.code == "RECORDS_HEAD_SOURCE_LIMIT"));
        let retained_limit = source.require_current(&mut retry).await;
        assert!(matches!(retained_limit, Err(PgTransactionError::Business(ref e)) if e.code == "RECORDS_HEAD_SOURCE_LIMIT"));
        retry.rollback().await.unwrap();
    }).await;
}
