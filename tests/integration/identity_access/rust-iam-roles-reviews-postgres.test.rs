//! Native Identity Access role/review slice over migrated PostgreSQL.
//! These prove owner SQL, catalogue delegation, version/CAS, independent review
//! publication, campaign certification and rollback-sensitive effects. They are
//! not a browser or Account/SSO journey.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_identity_access::public::iam_roles::{
    AccessReviewCampaignInput, AccessReviewCancelInput, AccessReviewDecisionInput, IamRoleActor,
    PgIamRolesReviews, ReviewDecisionInput, RoleSaveInput, cancel_access_review_campaign,
    decide_access_review_item, decide_review, save_role, start_access_review,
};
use erpx_server_identity_access::public::permission_catalog::permission_catalog;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, HoldingId, RecordId, UserId,
};
use serde_json::Value;

const NOW: i64 = 500;

fn id(value: &str) -> RecordId {
    RecordId::new(value).unwrap()
}

fn actor(user: &str, session: &str, assignment: &str, role: &str, revision: i64) -> IamRoleActor {
    IamRoleActor {
        user_id: UserId::new(user).unwrap(),
        session_id: id(session),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        assignment_id: AssignmentId::new(assignment).unwrap(),
        scope_kind: "holding".into(),
        scope_node_id: None,
        role: Some(role.into()),
        role_revision: Some(99),
        assignment_version: Some(1),
        identity_version: Some(1),
        authority_revision: Some(revision),
        permissions: vec![
            "iam.read".into(),
            "iam.manage".into(),
            "iam.roles.manage".into(),
            "iam.review".into(),
            "iam.recovery".into(),
        ],
        grantable_permissions: vec![
            "iam.read".into(),
            "iam.manage".into(),
            "iam.roles.manage".into(),
        ],
        person_user_ids: vec![UserId::new(user).unwrap()],
        person_id: None,
        platform: false,
        emergency_until: None,
        expires_at: 1_000_000,
    }
}

fn manager() -> IamRoleActor {
    actor("actor", "session-actor", "assignment-actor", "it_admin", 9)
}
fn reviewer() -> IamRoleActor {
    actor(
        "reviewer",
        "session-reviewer",
        "assignment-reviewer",
        "holding_owner",
        4,
    )
}
fn creator() -> IamRoleActor {
    actor(
        "creator",
        "session-creator",
        "assignment-creator",
        "holding_owner",
        7,
    )
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
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
        "INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES \
         ('*','it_admin','IT administrator',true,99,'active'),\
         ('*','holding_owner','Holding owner',true,99,'active'),\
         ('*','employee','Employee',true,1,'active') \
         ON CONFLICT(company_id,id) DO UPDATE SET published_revision=EXCLUDED.published_revision,status='active';\
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','it_admin',99,'[\"iam.read\",\"iam.manage\",\"iam.roles.manage\",\"iam.review\",\"iam.recovery\"]','[\"iam.read\",\"iam.manage\",\"iam.roles.manage\"]','[\"holding\"]','system',0),\
         ('*','holding_owner',99,'[\"iam.read\",\"iam.manage\",\"iam.roles.manage\",\"iam.review\",\"iam.recovery\"]','[\"iam.read\",\"iam.manage\",\"iam.roles.manage\"]','[\"holding\"]','system',0),\
         ('*','employee',1,'[\"iam.self\"]','[]','[\"holding\",\"branch\",\"department\"]','system',0) \
         ON CONFLICT(company_id,role_id,revision) DO NOTHING;\
         INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version) VALUES \
         ('company','actor','local:actor',NULL,'active','local',1),\
         ('company','reviewer','local:reviewer',NULL,'active','local',1),\
         ('company','creator','local:creator',NULL,'active','local',1),\
         ('company','target','local:target',NULL,'active','local',1),\
         ('company','owner','local:owner',NULL,'active','local',1);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,version,scope_kind,scope_node_id,parent_id,source,source_id) VALUES \
         ('assignment-actor','actor','it_admin','[]','company',NULL,0,1000000,NULL,1,'holding',NULL,NULL,'manual',NULL),\
         ('assignment-reviewer','reviewer','holding_owner','[]','company',NULL,0,1000000,NULL,1,'holding',NULL,NULL,'manual',NULL),\
         ('assignment-creator','creator','holding_owner','[]','company',NULL,0,1000000,NULL,1,'holding',NULL,NULL,'manual',NULL),\
         ('assignment-target','target','employee','[\"collaboration\"]','company',NULL,0,1000000,NULL,1,'holding',NULL,NULL,'manual',NULL),\
         ('assignment-owner','owner','holding_owner','[]','company',NULL,0,1000000,NULL,1,'holding',NULL,NULL,'manual',NULL);\
         INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES \
         ('session-actor','actor','company','assignment-actor',1,9,1000000,0),\
         ('session-reviewer','reviewer','company','assignment-reviewer',1,4,1000000,0),\
         ('session-creator','creator','company','assignment-creator',1,7,1000000,0),\
         ('session-target','target','company','assignment-target',1,1,1000000,0),\
         ('session-owner','owner','company','assignment-owner',1,1,1000000,0);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn role_input(name: &str, permissions: Vec<&str>, grantable: Vec<&str>) -> RoleSaveInput {
    RoleSaveInput {
        id: None,
        name: name.into(),
        expected_version: 0,
        permissions: permissions.into_iter().map(str::to_owned).collect(),
        grantable_permissions: grantable.into_iter().map(str::to_owned).collect(),
        scopes: vec!["holding".into()],
        reason: "  Independent role revision  ".into(),
    }
}

async fn save(
    pool: &DatabasePool,
    actor: &IamRoleActor,
    key: &str,
    input: &RoleSaveInput,
) -> Result<Value, PgTransactionError<BusinessError>> {
    let mut tx = pool
        .raw()
        .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
        .await
        .unwrap();
    let mut repository = PgIamRolesReviews::new(&mut tx);
    let result = save_role(
        &mut repository,
        actor,
        permission_catalog().unwrap(),
        &key_ref(key),
        input,
        NOW,
    )
    .await;
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

fn key_ref(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

async fn decide(
    pool: &DatabasePool,
    actor: &IamRoleActor,
    key: &str,
    review_id: &str,
    input: &ReviewDecisionInput,
) -> Result<Value, PgTransactionError<BusinessError>> {
    let mut tx = pool
        .raw()
        .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
        .await
        .unwrap();
    let mut repository = PgIamRolesReviews::new(&mut tx);
    let result = decide_review(
        &mut repository,
        actor,
        permission_catalog().unwrap(),
        &key_ref(key),
        review_id,
        input,
        NOW,
    )
    .await;
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

async fn start(
    pool: &DatabasePool,
    actor: &IamRoleActor,
    key: &str,
    input: &AccessReviewCampaignInput,
) -> Result<Value, PgTransactionError<BusinessError>> {
    let mut tx = pool
        .raw()
        .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
        .await
        .unwrap();
    let mut repository = PgIamRolesReviews::new(&mut tx);
    let result = start_access_review(&mut repository, actor, &key_ref(key), input, NOW).await;
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

async fn decide_item(
    pool: &DatabasePool,
    actor: &IamRoleActor,
    key: &str,
    item_id: &str,
    input: &AccessReviewDecisionInput,
) -> Result<Value, PgTransactionError<BusinessError>> {
    let mut tx = pool
        .raw()
        .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
        .await
        .unwrap();
    let mut repository = PgIamRolesReviews::new(&mut tx);
    let result =
        decide_access_review_item(&mut repository, actor, &key_ref(key), item_id, input, NOW).await;
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

async fn cancel(
    pool: &DatabasePool,
    actor: &IamRoleActor,
    key: &str,
    campaign_id: &str,
    input: &AccessReviewCancelInput,
) -> Result<Value, PgTransactionError<BusinessError>> {
    let mut tx = pool
        .raw()
        .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
        .await
        .unwrap();
    let mut repository = PgIamRolesReviews::new(&mut tx);
    let result = cancel_access_review_campaign(
        &mut repository,
        actor,
        &key_ref(key),
        campaign_id,
        input,
        NOW,
    )
    .await;
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

async fn effects(pool: &DatabasePool, key: &str) -> (i64, i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),\
         (SELECT count(*) FROM audit_events WHERE command_id=$1),\
         (SELECT count(*) FROM outbox_events WHERE command_id=$1)",
    )
    .bind(key)
    .fetch_one(pool.raw())
    .await
    .unwrap()
}

#[tokio::test]
async fn role_save_persists_revision_review_receipt_and_catalogue_gate() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let input = role_input(
            "Finance reader",
            vec!["iam.manage", "iam.read", "iam.read"],
            vec!["iam.read"],
        );
        let value = save(&pool, &manager(), "iam-role-key-1", &input).await.unwrap();
        assert_eq!(value["status"], "pending");
        assert_eq!(value["revision"], 1);
        let role_id = value["roleId"].as_str().unwrap().to_owned();
        assert!(role_id.starts_with("custom_"), "{role_id}");
        let stored: (String, bool, i64) = sqlx::query_as(
            "SELECT name,built_in,published_revision::bigint FROM iam_roles WHERE company_id='company' AND id=$1",
        )
        .bind(&role_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(stored, ("Finance reader".into(), false, 0));
        let revision: (String, String, String) = sqlx::query_as(
            "SELECT permissions_json::text,grantable_json::text,scopes_json::text \
             FROM iam_role_revisions WHERE company_id='company' AND role_id=$1 AND revision=1",
        )
        .bind(&role_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(revision.0, "[\"iam.manage\", \"iam.read\"]");
        assert_eq!(revision.1, "[\"iam.read\"]");
        let review: (String, String, i64) = sqlx::query_as(
            "SELECT status,kind,version::bigint FROM iam_reviews WHERE id=$1 AND company_id='company'",
        )
        .bind(value["id"].as_str().unwrap())
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(review, ("pending".into(), "role.publish".into(), 1));
        assert_eq!(effects(&pool, "iam-role-key-1").await, (1, 1, 1));
        let replay = save(&pool, &manager(), "iam-role-key-1", &input).await.unwrap();
        assert_eq!(replay, value);
        assert_eq!(effects(&pool, "iam-role-key-1").await, (1, 1, 1));

        for (name, permissions, code) in [
            ("unknown", vec!["invented.permission"], "DELEGATION_EXCEEDED"),
            ("preview", vec!["preview.brand"], "DELEGATION_EXCEEDED"),
            ("undelegated", vec!["finance.read"], "DELEGATION_EXCEEDED"),
        ] {
            let error = save(
                &pool,
                &manager(),
                &format!("iam-role-bad-{name}"),
                &role_input("Bad role", permissions, vec!["iam.read"]),
            )
            .await
            .unwrap_err();
            assert_eq!(business(error).code, code, "{name}");
        }
        let diff = role_input("Different role", vec!["iam.read"], vec!["iam.read"]);
        assert_eq!(
            business(
                save(&pool, &manager(), "iam-role-key-1", &diff)
                    .await
                    .unwrap_err()
            )
            .code,
            "IDEMPOTENCY_CONFLICT"
        );
    })
    .await;
}

#[tokio::test]
async fn role_review_approval_publishes_and_invalidates_assignments() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let value = save(
            &pool,
            &manager(),
            "iam-role-key-2",
            &role_input("Custom reviewer", vec!["iam.read"], vec!["iam.read"]),
        )
        .await
        .unwrap();
        let review_id = value["id"].as_str().unwrap().to_owned();
        let role_id = value["roleId"].as_str().unwrap().to_owned();
        let approve = ReviewDecisionInput {
            expected_version: 1,
            decision: "approve".into(),
            reason: "  Independent publication  ".into(),
        };
        let self_error = decide(&pool, &manager(), "iam-review-key-2", &review_id, &approve)
            .await
            .unwrap_err();
        assert_eq!(business(self_error).code, "INDEPENDENT_REVIEW_REQUIRED");
        let decided = decide(&pool, &reviewer(), "iam-review-key-2", &review_id, &approve)
            .await
            .unwrap();
        assert_eq!(
            decided,
            serde_json::json!({"id": review_id, "status": "approved"})
        );
        let role: (i64, String) = sqlx::query_as(
            "SELECT published_revision::bigint,name FROM iam_roles WHERE company_id='company' AND id=$1",
        )
        .bind(&role_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(role, (1, "Custom reviewer".into()));
        assert_eq!(effects(&pool, "iam-review-key-2").await, (1, 1, 1));
        let review: (String, i64) = sqlx::query_as(
            "SELECT status,version::bigint FROM iam_reviews WHERE id=$1",
        )
        .bind(&review_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(review, ("approved".into(), 2));
        let replay = decide(&pool, &reviewer(), "iam-review-key-2", &review_id, &approve)
            .await
            .unwrap();
        assert_eq!(replay, decided);
        assert_eq!(effects(&pool, "iam-review-key-2").await, (1, 1, 1));

        sqlx::query(
            "INSERT INTO assignments\
             (id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,\
             version,scope_kind,scope_node_id,parent_id,source,source_id) \
             VALUES('assignment-custom','target',$1,'[]','company',NULL,0,1000000,NULL,1,\
             'holding',NULL,NULL,'manual',NULL)",
        )
        .bind(&role_id)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::raw_sql(
            "INSERT INTO active_work_contexts\
             (session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) \
             VALUES('session-custom','target','company','assignment-custom',1,1,1000000,0);",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let second = save(
            &pool,
            &manager(),
            "iam-role-key-3",
            &RoleSaveInput {
                id: Some(role_id.clone()),
                expected_version: 1,
                ..role_input("Custom reviewer", vec!["iam.read"], vec!["iam.read"])
            },
        )
        .await
        .unwrap();
        assert_eq!(second["revision"], 2);
        let second_approve = ReviewDecisionInput {
            expected_version: 1,
            decision: "approve".into(),
            reason: "  Independent revision  ".into(),
        };
        decide(
            &pool,
            &reviewer(),
            "iam-review-key-3",
            second["id"].as_str().unwrap(),
            &second_approve,
        )
        .await
        .unwrap();
        let assignment: (i64, Option<i64>) = sqlx::query_as(
            "SELECT version,revoked_at FROM assignments WHERE id='assignment-custom'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(assignment, (2, None));
        let context: (Option<String>, Option<i64>, i64) = sqlx::query_as(
            "SELECT assignment_id,assignment_version,revision FROM active_work_contexts WHERE session_id='session-custom'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(context, (None, None, 2));
        assert_eq!(effects(&pool, "iam-review-key-3").await, (1, 1, 1));
    })
    .await;
}

#[tokio::test]
async fn access_review_certifies_keeps_revokes_and_cancels_with_cas() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let campaign = AccessReviewCampaignInput {
            name: "  Quarterly certification  ".into(),
            due_at: NOW + 86_400_000,
            reason: "  Quarterly review  ".into(),
        };
        let started = start(&pool, &creator(), "iam-campaign-key-1", &campaign)
            .await
            .unwrap();
        assert_eq!(started["status"], "open");
        assert_eq!(started["version"], 1);
        let campaign_id = started["id"].as_str().unwrap().to_owned();
        let item_count = started["itemCount"].as_i64().unwrap();
        assert!(item_count >= 5);
        let item: (String, i64) = sqlx::query_as(
            "SELECT id,assignment_version FROM iam_access_review_items \
             WHERE campaign_id=$1 AND assignment_id='assignment-target'",
        )
        .bind(&campaign_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        let decision = AccessReviewDecisionInput {
            expected_campaign_version: 1,
            decision: "keep".into(),
            reason: "  Certify current access  ".into(),
        };
        let kept = decide_item(&pool, &reviewer(), "iam-item-key-1", &item.0, &decision)
            .await
            .unwrap();
        assert_eq!(kept["decision"], "keep");
        assert_eq!(kept["campaignStatus"], "open");
        assert_eq!(kept["campaignVersion"], 2);
        let stale = AccessReviewDecisionInput {
            expected_campaign_version: 1,
            ..decision.clone()
        };
        assert_eq!(
            business(
                decide_item(&pool, &reviewer(), "iam-item-key-2", &item.0, &stale)
                    .await
                    .unwrap_err()
            )
            .code,
            "ACCESS_REVIEW_ITEM_CHANGED"
        );

        let revoke_campaign = start(&pool, &creator(), "iam-campaign-key-2", &campaign)
            .await
            .unwrap();
        let revoke_campaign_id = revoke_campaign["id"].as_str().unwrap().to_owned();
        let target_item: String = sqlx::query_scalar(
            "SELECT id FROM iam_access_review_items \
             WHERE campaign_id=$1 AND assignment_id='assignment-target'",
        )
        .bind(&revoke_campaign_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        let revoke = AccessReviewDecisionInput {
            expected_campaign_version: 1,
            decision: "revoke".into(),
            reason: "  Certified removal  ".into(),
        };
        let revoked = decide_item(&pool, &reviewer(), "iam-item-key-3", &target_item, &revoke)
            .await
            .unwrap();
        assert_eq!(revoked["decision"], "revoke");
        assert_eq!(revoked["campaignVersion"], 2);
        let assignment: (Option<i64>, i64) = sqlx::query_as(
            "SELECT revoked_at,version FROM assignments WHERE id='assignment-target'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(assignment, (Some(NOW), 2));
        let context: (Option<String>, i64) = sqlx::query_as(
            "SELECT assignment_id,revision FROM active_work_contexts WHERE session_id='session-target'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(context, (None, 2));
        let event: String = sqlx::query_scalar(
            "SELECT action FROM iam_access_review_events WHERE campaign_id=$1 AND item_id=$2",
        )
        .bind(&revoke_campaign_id)
        .bind(&target_item)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(event, "revoked");
        assert_eq!(effects(&pool, "iam-item-key-3").await, (1, 1, 1));

        let cancel_campaign = start(&pool, &creator(), "iam-campaign-key-3", &campaign)
            .await
            .unwrap();
        let cancel_campaign_id = cancel_campaign["id"].as_str().unwrap().to_owned();
        let cancel_input = AccessReviewCancelInput {
            expected_version: 1,
            reason: "  Superseded campaign  ".into(),
        };
        assert_eq!(
            business(
                cancel(
                    &pool,
                    &reviewer(),
                    "iam-cancel-key-1",
                    &cancel_campaign_id,
                    &cancel_input
                )
                .await
                .unwrap_err()
            )
            .code,
            "CAMPAIGN_OWNER_REQUIRED"
        );
        assert_eq!(
            business(
                cancel(
                    &pool,
                    &creator(),
                    "iam-cancel-key-2",
                    &campaign_id,
                    &AccessReviewCancelInput {
                        expected_version: 2,
                        ..cancel_input.clone()
                    }
                )
                .await
                .unwrap_err()
            )
            .code,
            "CAMPAIGN_DECISIONS_EXIST"
        );
        let cancelled = cancel(
            &pool,
            &creator(),
            "iam-cancel-key-3",
            &cancel_campaign_id,
            &cancel_input,
        )
        .await
        .unwrap();
        assert_eq!(
            cancelled,
            serde_json::json!({"id": cancel_campaign_id, "status": "cancelled", "version": 2})
        );
        assert_eq!(effects(&pool, "iam-cancel-key-3").await, (1, 1, 1));
    })
    .await;
}

#[tokio::test]
async fn transaction_and_serializable_guards_fail_closed() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut plain = pool.raw().acquire().await.unwrap();
        let mut repository = PgIamRolesReviews::new(&mut plain);
        let error = save_role(
            &mut repository,
            &manager(),
            permission_catalog().unwrap(),
            &key("iam-role-key-tx"),
            &role_input("Tx role", vec!["iam.read"], vec!["iam.read"]),
            NOW,
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref error)
                if error.code == "IAM_ROLES_REVIEWS_TRANSACTION_REQUIRED"
        ));
        let mut read_committed = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL READ COMMITTED")
            .await
            .unwrap();
        let mut repository = PgIamRolesReviews::new(&mut read_committed);
        let error = save_role(
            &mut repository,
            &manager(),
            permission_catalog().unwrap(),
            &key("iam-role-key-tx"),
            &role_input("Tx role", vec!["iam.read"], vec!["iam.read"]),
            NOW,
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref error)
                if error.code == "IAM_ROLES_REVIEWS_SERIALIZABLE_REQUIRED"
        ));
        read_committed.rollback().await.unwrap();
    })
    .await;
}
