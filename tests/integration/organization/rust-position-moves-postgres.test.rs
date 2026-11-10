#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_organization::public::organization_nodes::{
    OrganizationActor, OrganizationAuthority, OrganizationCommand, OrganizationCommandActor,
    OrganizationScopeKind,
};
use erpx_server_organization::public::position_moves::*;
use erpx_server_runtime::public::postgres::{
    load_migrations, migrate_database, DatabasePool, MigrationTrack, PgTransactionError,
    PgUnitOfWork, TransactionOptions,
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

fn actor() -> OrganizationActor {
    OrganizationActor {
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        user_id: UserId::new("owner").unwrap(),
        assignment_id: AssignmentId::new("assignment-owner").unwrap(),
        role: "custom_organization".into(),
        branch_id: None,
        department_id: None,
        scope_kind: OrganizationScopeKind::Holding,
        assignment_scope_kind: OrganizationScopeKind::Holding,
        permissions: vec!["organization.manage".into(), "people.manage".into()],
        authority: None,
    }
}

fn command_actor() -> OrganizationCommandActor {
    let actor = actor();
    OrganizationCommandActor {
        holding_id: actor.holding_id.clone(),
        command: OrganizationCommand {
            user_id: actor.user_id.clone(),
            assignment_id: actor.assignment_id.clone(),
            role: actor.role.clone(),
            company_id: actor.company_id.clone(),
            branch_id: None,
            department_id: None,
            authority: Some(OrganizationAuthority {
                session_id: "session-owner".into(),
                revision: 7,
                assignment_version: 1,
            }),
            iam_json: json!({
                "principalId": "local:owner",
                "permissions": ["organization.manage", "people.manage"],
                "grantablePermissions": [],
                "personId": "owner",
                "personUserIds": ["owner"],
                "freshUntil": null
            })
            .to_string(),
            assignment_scope_kind: "holding".into(),
            assignment_scope_node_id: None,
        },
        modules: vec!["people".into()],
        permissions: vec!["organization.manage".into(), "people.manage".into()],
    }
}

fn input(expected_version: i64) -> PositionMoveInput {
    PositionMoveInput::parse(&json!({
        "positionId": "director",
        "expectedVersion": expected_version,
        "targetKind": "level",
        "targetId": "level-3",
        "effectiveAt": 1_800_003_600_000_i64,
        "timezone": "Asia/Bangkok",
        "managerAssignments": [],
        "reason": "Promote the reviewed position"
    }))
    .unwrap()
}

async fn seed(pool: &DatabasePool) {
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
               ('department','department','service','Service','branch','company','branch','active');\
         INSERT INTO staff_levels(id,company_id,code,name,rank,level_number,active,created_at,updated_at) \
         VALUES('level-1','company','ceo','CEO',1,1,1,0,0), \
               ('level-2','company','director','Executive / Director',2,2,1,0,0), \
               ('level-3','company','manager','Branch / Department Manager',3,3,1,0,0);\
         INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,reports_to_position_id,is_ceo,active,version,created_by,created_at,updated_at) \
         VALUES('ceo','company','branch','department','Chief','level-1',NULL,1,1,1,'fixture',0,0), \
               ('director','company','branch','department','Director','level-2','ceo',0,1,3,'fixture',0,0);\
         INSERT INTO hr_people(id,user_id,company_id,legal_name,email,country_code,status,version,created_by,created_at,updated_at) \
         VALUES('ceo-person','ceo-user','company','Chief','chief@example.test','TH','active',1,'fixture',0,0), \
               ('holder','holder-user','company','Holder','holder@example.test','TH','active',1,'fixture',0,0), \
               ('owner','owner','company','Owner','owner@example.test','TH','active',1,'fixture',0,0);\
         INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,manager_person_id,job_title,currency,version,created_by,created_at,updated_at,position_id,level_id) \
         VALUES('engagement-ceo','ceo-person','company','branch','department','employee','salaried','active','2024-01-01',NULL,'Chief','THB',1,'fixture',0,0,'ceo','level-1'), \
               ('engagement-holder','holder','company','branch','department','employee','salaried','active','2024-01-01','ceo-person','Director','THB',1,'fixture',0,0,'director','level-2');\
         INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version) \
         VALUES('company','owner','local:owner','owner','active','local',1);\
         INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) \
         VALUES('*','custom_organization','Synthetic organization authority',false,1,'active');\
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) \
         VALUES('*','custom_organization',1,'[\"organization.manage\",\"people.manage\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) \
         VALUES('assignment-owner','owner','custom_organization','[\"people\"]','company',0,9999999999999,1,'holding');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn run<T, F>(pool: DatabasePool, f: F) -> Result<T, PgTransactionError<BusinessError>>
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

fn finish<T>(result: Result<T, PgTransactionError<BusinessError>>) -> Result<T, BusinessError> {
    match result {
        Ok(value) => Ok(value),
        Err(PgTransactionError::Business(error)) => Err(error),
        Err(PgTransactionError::Persistence(error)) => {
            panic!("unexpected persistence {error:?}")
        }
    }
}

fn business<T>(result: Result<T, BusinessError>, code: &str) {
    match result {
        Err(error) => assert_eq!(error.code, code),
        Ok(_) => panic!("expected business denial {code}, got success"),
    }
}

async fn preview(pool: DatabasePool, version: i64) -> Result<PositionMovePreview, BusinessError> {
    finish(
        run(pool, move |connection| {
            Box::pin(async move {
                let mut ports = PgPositionMoves::new(connection);
                ports
                    .preview(&actor(), &input(version), &TestClock)
                    .await
                    .map_err(|error| match error {
                        PgTransactionError::Business(error) => error,
                        error => panic!("unexpected persistence {error:?}"),
                    })
            })
        })
        .await,
    )
}

async fn submit(
    pool: DatabasePool,
    version: i64,
    token: String,
    expires_at: i64,
    key: &'static str,
) -> Result<serde_json::Value, BusinessError> {
    finish(
        run(pool, move |connection| {
            Box::pin(async move {
                let mut ports = PgPositionMoves::new(connection);
                ports
                    .submit(
                        &command_actor(),
                        &input(version),
                        &token,
                        expires_at,
                        key,
                        &TestClock,
                    )
                    .await
                    .map_err(|error| match error {
                        PgTransactionError::Business(error) => error,
                        error => panic!("unexpected persistence {error:?}"),
                    })
            })
        })
        .await,
    )
}

async fn list(pool: DatabasePool) -> Result<Vec<PositionMoveRecord>, BusinessError> {
    finish(
        run(pool, move |connection| {
            Box::pin(async move {
                let mut ports = PgPositionMoves::new(connection);
                ports
                    .list(&actor(), None)
                    .await
                    .map_err(|error| match error {
                        PgTransactionError::Business(error) => error,
                        error => panic!("unexpected persistence {error:?}"),
                    })
            })
        })
        .await,
    )
}

async fn cancel(
    pool: DatabasePool,
    expected_version: i64,
    key: &'static str,
) -> Result<serde_json::Value, BusinessError> {
    finish(
        run(pool, move |connection| {
            Box::pin(async move {
                let mut ports = PgPositionMoves::new(connection);
                ports
                    .cancel(
                        &command_actor(),
                        "position-move-position-submit-0001",
                        expected_version,
                        key,
                        &TestClock,
                    )
                    .await
                    .map_err(|error| match error {
                        PgTransactionError::Business(error) => error,
                        error => panic!("unexpected persistence {error:?}"),
                    })
            })
        })
        .await,
    )
}

#[tokio::test]
async fn position_moves_preview_submit_cancel_keep_cas_replay_and_receipt() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let preview = preview(pool.clone(), 3).await.unwrap();
        assert!(preview.ready, "{:?}", preview.blockers);
        assert_eq!(preview.affected_count, 1);
        assert_eq!(preview.preview_token.len(), 64);
        assert_eq!(preview.after.level_id.as_deref(), Some("level-3"));
        assert_eq!(preview.people[0].name, "Holder");

        business(
            submit(
                pool.clone(),
                3,
                "0".repeat(64),
                preview.expires_at,
                "position-submit-stale-0001",
            )
            .await,
            "POSITION_MOVE_CHANGED",
        );

        let submitted = submit(
            pool.clone(),
            3,
            preview.preview_token.clone(),
            preview.expires_at,
            "position-submit-0001",
        )
        .await
        .unwrap();
        assert_eq!(submitted["status"], "scheduled");
        assert_eq!(submitted["version"], 1);

        let replay = submit(
            pool.clone(),
            3,
            preview.preview_token.clone(),
            preview.expires_at,
            "position-submit-0001",
        )
        .await
        .unwrap();
        assert_eq!(replay, submitted);

        let counts: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM organization_position_moves WHERE id='position-move-position-submit-0001'), \
             (SELECT count(*) FROM organization_position_move_members WHERE move_id='position-move-position-submit-0001'), \
             (SELECT count(*) FROM command_receipts WHERE command_id='position-submit-0001'), \
             (SELECT count(*) FROM audit_events WHERE command_id='position-submit-0001')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (1, 1, 1, 1));
        let outbox: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM outbox_events WHERE command_id='position-submit-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(outbox, 1);

        business(
            submit(
                pool.clone(),
                2,
                preview.preview_token.clone(),
                preview.expires_at,
                "position-submit-0001",
            )
            .await,
            "IDEMPOTENCY_CONFLICT",
        );

        let rows = list(pool.clone()).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "position-move-position-submit-0001");
        assert_eq!(rows[0].status, "scheduled");
        assert_eq!(rows[0].definition.expected_version, 3);
        assert_eq!(rows[0].preview.preview_token, preview.preview_token);

        let cancelled = cancel(pool.clone(), 1, "position-cancel-0001")
            .await
            .unwrap();
        assert_eq!(cancelled["status"], "cancelled");
        assert_eq!(cancelled["version"], 2);

        business(
            cancel(pool.clone(), 2, "position-cancel-0002").await,
            "POSITION_MOVE_LOCKED",
        );

        let position: (i64, String) = sqlx::query_as(
            "SELECT version::bigint version,level_id FROM organization_positions WHERE id='director'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(position, (3, "level-2".into()));
        let audits: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit_events WHERE command_id IN ('position-submit-0001','position-cancel-0001')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(audits, 2);
    })
    .await;
}

#[tokio::test]
async fn position_move_preview_rejects_out_of_scope_authority() {
    fixture::with_database(1, |pool| async move {
        seed(&pool).await;
        let denied = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgPositionMoves::new(connection);
                let mut reader = actor();
                reader.role = "custom_reader".into();
                ports
                    .preview(&reader, &input(3), &TestClock)
                    .await
                    .map_err(|error| match error {
                        PgTransactionError::Business(error) => error,
                        error => panic!("unexpected persistence {error:?}"),
                    })
            })
        })
        .await;
        business(finish(denied), "ORGANIZATION_AUTHORITY_CHANGED");
    })
    .await;
}

async fn add_managed_leave_default(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO leave_policy_versions(id,company_id,policy_key,version,name,status,effective_from,created_by,created_at) \
         VALUES('policy-level-3','company','level-3-default',1,'Level 3 default leave','active','2020-01-01','owner',1);\
         INSERT INTO level_leave_defaults(id,company_id,level_id,version,policy_version_id,created_by,created_at) \
         VALUES('leave-default-level-3','company','level-3',1,'policy-level-3','owner',1);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn native_apply_is_atomic_idempotent_and_writes_leave_and_evidence() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        add_managed_leave_default(&pool).await;
        let preview = preview(pool.clone(), 3).await.unwrap();
        assert!(preview.ready, "{:?}", preview.blockers);
        let submitted = submit(
            pool.clone(),
            3,
            preview.preview_token.clone(),
            preview.expires_at,
            "position-submit-apply-0001",
        )
        .await
        .unwrap();
        let id = "position-move-position-submit-apply-0001";

        let before = PgPositionMoveDrain::new(pool.clone())
            .drain(1_800_003_600_000 - 1)
            .await
            .unwrap();
        assert_eq!(before, 0);

        let applied = PgPositionMoveDrain::new(pool.clone())
            .drain(1_800_003_600_000 + 1)
            .await
            .unwrap();
        assert_eq!(applied, 1);
        let again = PgPositionMoveDrain::new(pool.clone())
            .drain(1_800_003_600_000 + 2)
            .await
            .unwrap();
        assert_eq!(again, 0);

        let move_row: (String, i64, i64) = sqlx::query_as(
            "SELECT status,version::bigint version,applied_at FROM organization_position_moves WHERE id=$1",
        )
        .bind(id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(move_row, ("applied".into(), 2, 1_800_003_600_001));
        let position: (String, i64) = sqlx::query_as(
            "SELECT level_id,version::bigint version FROM organization_positions WHERE id='director'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(position, ("level-3".into(), 4));
        let engagement: (String, String, i64) = sqlx::query_as(
            "SELECT level_id,status,version::bigint version FROM hr_engagements WHERE id='engagement-holder'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(engagement, ("level-3".into(), "active".into(), 2));
        let history: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM staff_organization_history WHERE command_id=$1), \
             (SELECT count(*) FROM staff_level_history WHERE command_id=$1)",
        )
        .bind(id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(history, (1, 1));
        let leave: (String, String) = sqlx::query_as(
            "SELECT policy_version_id,effective_from FROM leave_policy_assignments WHERE id=$1",
        )
        .bind(format!("leave-default-{id}-engagement-holder"))
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(leave, ("policy-level-3".into(), "2028-01-01".into()));
        let evidence: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM audit_events WHERE command_id=$1), \
             (SELECT count(*) FROM outbox_events WHERE command_id=$1)",
        )
        .bind(id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(evidence, (1, 1));

        // The reviewed row is no longer claimable; no second apply is possible.
        let _ = submitted;
        let unchanged: (i64,) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM staff_organization_history WHERE command_id=$1)",
        )
        .bind(id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(unchanged.0, 1);
    })
    .await;
}

#[tokio::test]
async fn native_apply_fails_closed_on_changed_snapshot_and_revoked_authority() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let first = preview(pool.clone(), 3).await.unwrap();
        submit(
            pool.clone(),
            3,
            first.preview_token.clone(),
            first.expires_at,
            "position-submit-apply-0002",
        )
        .await
        .unwrap();
        sqlx::query("UPDATE hr_engagements SET status='ended',version=version+1 WHERE id='engagement-holder'")
            .execute(pool.raw())
            .await
            .unwrap();
        assert_eq!(
            PgPositionMoveDrain::new(pool.clone())
                .drain(1_800_003_600_001)
                .await
                .unwrap(),
            0
        );
        let stale: (String, Option<String>) = sqlx::query_as(
            "SELECT status,last_error FROM organization_position_moves WHERE id='position-move-position-submit-apply-0002'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(stale.0, "needs_review");
        assert!(stale
            .1
            .as_deref()
            .is_some_and(|error| error.contains("POSITION_MOVE_REVIEW_CHANGED")));
        let position: (String, i64) = sqlx::query_as(
            "SELECT level_id,version::bigint version FROM organization_positions WHERE id='director'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(position, ("level-2".into(), 3));
        let history: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM staff_organization_history WHERE command_id='position-move-position-submit-apply-0002'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(history, 0);
    })
    .await;
}

#[tokio::test]
async fn native_apply_marks_needs_review_when_authority_was_revoked() {
    fixture::with_database(1, |pool| async move {
        seed(&pool).await;
        let revoke = preview(pool.clone(), 3).await.unwrap();
        submit(
            pool.clone(),
            3,
            revoke.preview_token.clone(),
            revoke.expires_at,
            "position-submit-apply-0003",
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE assignments SET revoked_at=1_800_000_000_000,version=version+1 WHERE id='assignment-owner'",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            PgPositionMoveDrain::new(pool.clone())
                .drain(1_800_003_600_001)
                .await
                .unwrap(),
            0
        );
        let revoked: (String, Option<String>) = sqlx::query_as(
            "SELECT status,last_error FROM organization_position_moves WHERE id='position-move-position-submit-apply-0003'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(revoked.0, "needs_review");
        assert!(revoked
            .1
            .as_deref()
            .is_some_and(|error| error.contains("POSITION_MOVE_AUTHORITY_CHANGED")));
    })
    .await;
}
