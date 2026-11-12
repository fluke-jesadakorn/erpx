#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_organization::public::organization_nodes::{
    OrganizationAuthority, OrganizationCommand, OrganizationCommandActor, OrganizationFuture,
};
use erpx_server_organization::public::position::*;
use erpx_server_runtime::public::postgres::{
    MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions, load_migrations,
    migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CompanyId, HoldingId, TimestampMillis, UserId,
};
use serde_json::{Value, json};

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
) -> OrganizationCommandActor {
    OrganizationCommandActor {
        holding_id: HoldingId::new("h001").unwrap(),
        command: OrganizationCommand {
            user_id: UserId::new(user).unwrap(),
            assignment_id: AssignmentId::new(assignment).unwrap(),
            role: "custom_organization".into(),
            company_id: CompanyId::new(company).unwrap(),
            branch_id: None,
            department_id: None,
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
            assignment_scope_kind: "holding".into(),
            assignment_scope_node_id: None,
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
    )
}

fn position_input(
    title: &str,
    level: &str,
    reports_to: Option<&str>,
    branch: Option<&str>,
    department: Option<&str>,
    roster_required: Option<bool>,
    expected_version: i64,
) -> PositionInput {
    let mut payload = json!({
        "requirements": format!("{title} requirements"),
        "title": title,
        "branchId": branch,
        "departmentId": department,
        "levelId": level,
        "reportsToPositionId": reports_to,
        "isCeo": false,
        "active": true,
        "expectedVersion": expected_version,
        "reason": "Review the organization position"
    });
    if let Some(value) = roster_required {
        payload["rosterRequired"] = json!(value);
    }
    PositionInput::parse(&payload).unwrap()
}

struct NoExit;
impl RosterExitPort for NoExit {
    fn execute<'a>(
        &'a mut self,
        _actor: &'a OrganizationCommandActor,
        _engagement_ids: &'a [String],
        _key: &'a str,
        _reason: &'a str,
        _now: i64,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, ()> {
        Box::pin(async move { Ok(()) })
    }
}

#[derive(Default)]
struct RecordingExit {
    calls: Vec<(Vec<String>, String, String, i64)>,
}

impl RosterExitPort for RecordingExit {
    fn execute<'a>(
        &'a mut self,
        _actor: &'a OrganizationCommandActor,
        engagement_ids: &'a [String],
        key: &'a str,
        reason: &'a str,
        now: i64,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, ()> {
        self.calls.push((
            engagement_ids.to_vec(),
            key.to_owned(),
            reason.to_owned(),
            now,
        ));
        Box::pin(async move { Ok(()) })
    }
}


struct Unit<'a, E> {
    ports: PgOrganizationPosition<'a>,
    exit: E,
}

impl<E: RosterExitPort> OrganizationPositionRepository for Unit<'_, E> {
    type Error = erpx_server_runtime::public::postgres::PgTransactionError<BusinessError>;

    fn replay<'a>(
        &'a mut self,
        actor: &'a OrganizationCommandActor,
        key: &'a str,
        operation: &'a str,
        record_id: &'a str,
        input: &'a PositionInput,
    ) -> OrganizationFuture<'a, Option<Value>, Self::Error> {
        self.ports.replay(actor, key, operation, record_id, input)
    }
    fn roster<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> OrganizationFuture<'a, PositionRoster, Self::Error> {
        self.ports.roster(company)
    }
    fn branch_active<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch: &'a str,
    ) -> OrganizationFuture<'a, bool, Self::Error> {
        self.ports.branch_active(company, branch)
    }
    fn department_active<'a>(
        &'a mut self,
        company: &'a CompanyId,
        department: &'a str,
        branch: Option<&'a str>,
    ) -> OrganizationFuture<'a, bool, Self::Error> {
        self.ports.department_active(company, department, branch)
    }
    fn position_level<'a>(
        &'a mut self,
        company: &'a CompanyId,
        level: &'a str,
    ) -> OrganizationFuture<'a, Option<i64>, Self::Error> {
        self.ports.position_level(company, level)
    }
    fn graph_guard<'a>(
        &'a mut self,
        company: &'a CompanyId,
        key: &'a str,
    ) -> OrganizationFuture<'a, PositionGraphGuard, Self::Error> {
        self.ports.graph_guard(company, key)
    }
    fn exiting_engagements<'a>(
        &'a mut self,
        company: &'a CompanyId,
        position: &'a str,
    ) -> OrganizationFuture<'a, Vec<String>, Self::Error> {
        self.ports.exiting_engagements(company, position)
    }
    fn persist<'a>(
        &'a mut self,
        actor: &'a OrganizationCommandActor,
        id: Option<&'a str>,
        input: &'a PositionInput,
        key: &'a str,
        guard: &'a PositionGraphGuard,
        roster_required: bool,
        exiting: &'a [String],
        now: i64,
    ) -> OrganizationFuture<'a, Value, Self::Error> {
        self.ports
            .persist(actor, id, input, key, guard, roster_required, exiting, now)
    }
}

impl<E: RosterExitPort> RosterExitPort for Unit<'_, E> {
    fn execute<'a>(
        &'a mut self,
        actor: &'a OrganizationCommandActor,
        engagement_ids: &'a [String],
        key: &'a str,
        reason: &'a str,
        now: i64,
    ) -> OrganizationFuture<'a, ()> {
        self.exit.execute(actor, engagement_ids, key, reason, now)
    }
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
               ('foreign','company','foreign','Foreign',NULL,'foreign',NULL,'active');\
         INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) \
         VALUES('*','custom_organization','Synthetic organization authority',false,1,'active'), \
               ('*','custom_people_only','Synthetic people-only authority',false,1,'active');\
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) \
         VALUES('*','custom_organization',1,'[\"organization.manage\"]','[]','[\"holding\"]','fixture',0), \
               ('*','custom_people_only',1,'[\"people.manage\"]','[]','[\"holding\"]','fixture',0);\
         UPDATE iam_roles SET published_revision=1 WHERE company_id='*' AND id IN ('custom_organization','custom_people_only');\
         INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,version,created_by,created_at,updated_at) \
         VALUES('person-owner','owner','company','Owner','TH','active',1,'fixture',0,0), \
               ('person-foreign','foreign-owner','foreign','Foreign owner','TH','active',1,'fixture',0,0), \
               ('person-holder','holder','company','Holder','TH','active',1,'fixture',0,0);\
         INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version) \
         VALUES('company','owner','local:owner','person-owner','active','local',1), \
               ('foreign','foreign-owner','local:foreign-owner','person-foreign','active','local',1);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) \
         VALUES('assignment-owner','owner','custom_organization','[\"people\"]','company',0,9999999999999,1,'holding'), \
               ('assignment-foreign','foreign-owner','custom_organization','[\"people\"]','foreign',0,9999999999999,1,'holding');\
         INSERT INTO staff_levels(id,company_id,code,name,rank,active,version,created_at,updated_at,level_number) \
         VALUES('level-1','company','ORG_L1','CEO',-1,1,1,0,0,1), \
               ('level-2','company','ORG_L2','Executive / Director',-2,1,1,0,0,2), \
               ('level-3','company','ORG_L3','Branch / Department Manager',-3,1,1,0,0,3), \
               ('level-4','company','ORG_L4','Supervisor',-4,1,1,0,0,4), \
               ('level-5','company','ORG_L5','Senior Staff',-5,1,1,0,0,5), \
               ('level-6','company','ORG_L6','Staff',-6,1,1,0,0,6);\
         INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,reports_to_position_id,is_ceo,roster_required,active,version,created_by,created_at,updated_at) \
         VALUES('position-ceo','company',NULL,NULL,'CEO','level-1',NULL,1,0,1,1,'fixture',0,0), \
               ('position-manager','company','branch','department','Manager','level-3','position-ceo',0,0,1,1,'fixture',0,0), \
               ('position-staff','company','branch','department','Staff','level-6','position-manager',0,1,1,1,'fixture',0,0), \
               ('position-equal','company',NULL,NULL,'Equal Level','level-3',NULL,0,0,1,1,'fixture',0,0), \
               ('position-archived','company',NULL,NULL,'Archived','level-2',NULL,0,0,0,1,'fixture',0,0);\
         INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,manager_person_id,job_title,currency,version,created_by,created_at,updated_at,level_id,position_id) \
         VALUES('engagement-staff','person-holder','company','branch','department','employee','salaried','active','2020-01-01',NULL,'Staff','THB',1,'fixture',0,0,'level-6','position-staff');",
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
async fn position_save_persists_replays_updates_and_rejects_stale_versions() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let created = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut unit = Unit { ports: PgOrganizationPosition::new(connection), exit: NoExit };
                save_position(
                    &mut unit,
                    owner(),
                    "position-create-0001",
                    None,
                    position_input(
                        "Operations Lead",
                        "level-3",
                        Some("position-ceo"),
                        Some("branch"),
                        Some("department"),
                        None,
                        0,
                    ),
                    &TestClock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await
        .unwrap();
        assert_eq!(
            created,
            json!({"id": "position-position-create-0001", "version": 1})
        );
        let row: (String, String, Option<String>, Option<String>, i64, i64, i64) = sqlx::query_as(
            "SELECT title,requirements,branch_id,department_id,version::bigint,roster_required::bigint,active::bigint \
             FROM organization_positions WHERE id='position-position-create-0001' AND company_id='company'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            row,
            (
                "Operations Lead".into(),
                "Operations Lead requirements".into(),
                Some("branch".into()),
                Some("department".into()),
                1,
                0,
                1
            )
        );
        let audit: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id='position-create-0001'), \
             (SELECT count(*) FROM audit_events WHERE command_id='position-create-0001')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(audit, (1, 1));

        let replayed = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut unit = Unit { ports: PgOrganizationPosition::new(connection), exit: NoExit };
                save_position(
                    &mut unit,
                    owner(),
                    "position-create-0001",
                    None,
                    position_input(
                        "Operations Lead",
                        "level-3",
                        Some("position-ceo"),
                        Some("branch"),
                        Some("department"),
                        None,
                        0,
                    ),
                    &TestClock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await
        .unwrap();
        assert_eq!(replayed, created);
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM organization_positions WHERE id='position-position-create-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(count, 1);

        let updated = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut unit = Unit { ports: PgOrganizationPosition::new(connection), exit: NoExit };
                save_position(
                    &mut unit,
                    owner(),
                    "position-update-0001",
                    Some("position-position-create-0001"),
                    position_input(
                        "Operations Director",
                        "level-2",
                        Some("position-ceo"),
                        Some("branch"),
                        Some("department"),
                        Some(false),
                        1,
                    ),
                    &TestClock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await
        .unwrap();
        assert_eq!(
            updated,
            json!({"id": "position-position-create-0001", "version": 2})
        );
        let row: (String, String, i64) = sqlx::query_as(
            "SELECT title,requirements,version::bigint FROM organization_positions \
             WHERE id='position-position-create-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            row,
            (
                "Operations Director".into(),
                "Operations Director requirements".into(),
                2
            )
        );

        let stale = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut unit = Unit { ports: PgOrganizationPosition::new(connection), exit: NoExit };
                save_position(
                    &mut unit,
                    owner(),
                    "position-stale-0001",
                    Some("position-position-create-0001"),
                    position_input(
                        "Stale update",
                        "level-3",
                        Some("position-ceo"),
                        None,
                        None,
                        None,
                        1,
                    ),
                    &TestClock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await;
        business(stale, "CONFLICT");
    })
    .await;
}

#[tokio::test]
async fn position_save_enforces_level_and_reporting_guards() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let missing_level = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut unit = Unit { ports: PgOrganizationPosition::new(connection), exit: NoExit };
                save_position(
                    &mut unit,
                    owner(),
                    "position-level-0001",
                    None,
                    position_input(
                        "Missing level",
                        "missing-level",
                        Some("position-ceo"),
                        None,
                        None,
                        None,
                        0,
                    ),
                    &TestClock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await;
        business(missing_level, "LEVEL_REQUIRED");

        let inactive_manager = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut unit = Unit { ports: PgOrganizationPosition::new(connection), exit: NoExit };
                save_position(
                    &mut unit,
                    owner(),
                    "position-reporting-0001",
                    None,
                    position_input(
                        "Inactive manager",
                        "level-3",
                        Some("position-archived"),
                        None,
                        None,
                        None,
                        0,
                    ),
                    &TestClock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await;
        business(inactive_manager, "REPORTING_POSITION_INVALID");

        let equal_manager = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut unit = Unit { ports: PgOrganizationPosition::new(connection), exit: NoExit };
                save_position(
                    &mut unit,
                    owner(),
                    "position-seniority-0001",
                    None,
                    position_input(
                        "Equal manager",
                        "level-3",
                        Some("position-equal"),
                        None,
                        None,
                        None,
                        0,
                    ),
                    &TestClock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await;
        business(equal_manager, "MANAGER_LEVEL_INVALID");
    })
    .await;
}

#[tokio::test]
async fn position_roster_transition_hands_exiting_engagements_to_the_port() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let (receipt, calls) = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut unit = Unit { ports: PgOrganizationPosition::new(connection), exit: RecordingExit::default() };
                let receipt = save_position(
                    &mut unit,
                    owner(),
                    "position-roster-exit-0001",
                    Some("position-staff"),
                    position_input(
                        "Staff",
                        "level-6",
                        Some("position-manager"),
                        Some("branch"),
                        Some("department"),
                        Some(false),
                        1,
                    ),
                    &TestClock,
                )
                .await
                .map_err(expect_business)?;
                Ok((receipt, unit.exit.calls))
            })
        })
        .await
        .unwrap();
        assert_eq!(receipt, json!({"id": "position-staff", "version": 2}));
        assert_eq!(
            calls,
            vec![(
                vec!["engagement-staff".into()],
                "position-roster-exit-0001".to_owned(),
                "Review the organization position".to_owned(),
                1_800_000_000_000
            )]
        );
        let roster_required: i64 = sqlx::query_scalar(
            "SELECT roster_required::bigint FROM organization_positions WHERE id='position-staff'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(roster_required, 0);
    })
    .await;
}

#[tokio::test]
async fn position_save_requires_current_organization_authority_and_company_isolation() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let limited = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut unit = Unit { ports: PgOrganizationPosition::new(connection), exit: NoExit };
                save_position(
                    &mut unit,
                    actor("company", "owner", "assignment-owner", &["people.manage"]),
                    "position-limited-0001",
                    None,
                    position_input(
                        "Limited",
                        "level-3",
                        Some("position-ceo"),
                        None,
                        None,
                        None,
                        0,
                    ),
                    &TestClock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await;
        business(limited, "PERMISSION_REQUIRED");

        let foreign_update = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut unit = Unit { ports: PgOrganizationPosition::new(connection), exit: NoExit };
                save_position(
                    &mut unit,
                    actor(
                        "foreign",
                        "foreign-owner",
                        "assignment-foreign",
                        &["organization.manage"],
                    ),
                    "position-foreign-0001",
                    Some("position-position-create-0001"),
                    PositionInput::parse(&json!({
                        "title": "Foreign update",
                        "branchId": null,
                        "departmentId": null,
                        "levelId": "level-3",
                        "reportsToPositionId": "position-ceo",
                        "expectedVersion": 1,
                        "reason": "Attempt the foreign update"
                    }))
                    .unwrap(),
                    &TestClock,
                )
                .await
                .map_err(expect_business)
            })
        })
        .await;
        business(foreign_update, "POSITION_NOT_FOUND");
    })
    .await;
}

#[test]
fn native_position_input_serializes_the_retained_command_shape() {
    let parsed = position_input(
        "Operations Manager",
        "level-3",
        Some("position-ceo"),
        None,
        None,
        Some(true),
        4,
    );
    let value: Value = parsed.to_value();
    assert_eq!(value["rosterRequired"], json!(true));
    assert_eq!(value["reportsToPositionId"], json!("position-ceo"));
    assert_eq!(value["expectedVersion"], json!(4));
}
