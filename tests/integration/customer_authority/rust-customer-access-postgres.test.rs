//! Native Customer Authority customer-access commands/reads over real
//! migrated PostgreSQL: current IAM reviewer eligibility, source-bound grants,
//! versions/CAS, idempotency, audit/outbox, rollback and persisted readback.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_customer_authority::public::access::{
    AccessAction, AccessActor, AccessChange, AccessRequest, PgCustomerAccess, access_detail,
    access_workspace, change_access, request_access,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, HoldingId, RecordId, UserId,
};
use sqlx::{Connection, Row};

const NOW: i64 = 1_800_000_000_000;

fn actor(
    user: &str,
    company: &str,
    branch: Option<&str>,
    department: Option<&str>,
    assignment: &str,
    permissions: &[&str],
) -> AccessActor {
    AccessActor {
        user_id: UserId::new(user).unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: branch.map(|value| RecordId::new(value).unwrap()),
        department_id: department.map(|value| RecordId::new(value).unwrap()),
        assignment_id: AssignmentId::new(assignment).unwrap(),
        role: Some("sales_manager".into()),
        authority_revision: Some(3),
        assignment_version: Some(1),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        expires_at: NOW + 60_000,
    }
}

fn requester() -> AccessActor {
    actor(
        "requester",
        "company",
        None,
        None,
        "assignment-requester",
        &["commerce.read", "commerce.sales", "commerce.approve"],
    )
}

fn reviewer() -> AccessActor {
    actor(
        "reviewer",
        "company",
        None,
        None,
        "assignment-reviewer",
        &["commerce.read", "commerce.sales", "commerce.approve"],
    )
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn input() -> AccessRequest {
    AccessRequest {
        order_id: "order-main".into(),
        source_version: 3,
        email: "Customer@Example.Test".into(),
        expires_at: NOW + 86_400_000,
        reason: "  Verified source-bound customer authority  ".into(),
    }
}

fn change(action: AccessAction, expected: i64) -> AccessChange {
    AccessChange {
        action,
        expected_version: expected,
        reason: "Independent customer authority decision".into(),
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
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
         ('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('branch','branch','branch','Branch','company','company',NULL,'active',1),\
         ('other-branch','branch','other-branch','Other branch','company','company',NULL,'active',1),\
         ('department','department','department','Department','branch','company','branch','active',1),\
         ('foreign','company','foreign','Foreign',NULL,'foreign',NULL,'active',1);\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at) VALUES \
         ('customer-main','company','customer','C-001','Synthetic Customer','THB','active',1,10,10),\
         ('customer-foreign','foreign','customer','F-001','Foreign Customer','THB','active',1,10,10);\
         INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at) VALUES \
         ('order-main','company','branch',NULL,'quote','QT-100','customer-main',NULL,'THB','2026-09-20','2026-09-25','Main','confirmed',100,0,3,'requester',100,100),\
         ('order-other','company','other-branch',NULL,'quote','QT-200','customer-main',NULL,'THB','2026-09-20','2026-09-25','Other','confirmed',100,0,1,'requester',200,200),\
         ('order-dept','company','branch','department','quote','QT-300','customer-main',NULL,'THB','2026-09-20','2026-09-25','Dept','confirmed',100,0,1,'requester',300,300),\
         ('order-draft','company','branch',NULL,'quote','QT-400','customer-main',NULL,'THB','2026-09-20','2026-09-25','Draft','draft',100,0,1,'requester',400,400),\
         ('order-cancelled','company','branch',NULL,'quote','QT-500','customer-main',NULL,'THB','2026-09-20','2026-09-25','Cancelled','cancelled',100,0,1,'requester',500,500),\
         ('order-foreign','foreign','foreign',NULL,'quote','FQT-1','customer-foreign',NULL,'THB','2026-09-20','2026-09-25','Foreign','confirmed',100,0,1,'foreign',600,600);\
         INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) VALUES \
         ('company','requester','local:requester','active','local',1),\
         ('company','reviewer','local:reviewer','active','local',1),\
         ('company','ended-reviewer','local:ended-reviewer','active','local',1),\
         ('company','plain','local:plain','active','local',1),\
         ('company','branch-reviewer','local:branch-reviewer','active','local',1);\
         INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at) VALUES \
         ('person-requester','requester','company','Requester','TH','active','fixture',0,0),\
         ('person-reviewer','reviewer','company','Reviewer','TH','active','fixture',0,0),\
         ('person-ended','ended-reviewer','company','Ended Reviewer','TH','active','fixture',0,0),\
         ('person-plain','plain','company','Plain User','TH','active','fixture',0,0),\
         ('person-branch','branch-reviewer','company','Branch Reviewer','TH','active','fixture',0,0);\
         INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,end_date,job_title,currency,created_by,created_at,updated_at) VALUES \
         ('engagement-requester','person-requester','company','employee','salaried','active','2020-01-01',NULL,'Sales Manager','THB','fixture',0,0),\
         ('engagement-reviewer','person-reviewer','company','employee','salaried','active','2020-01-01',NULL,'Sales Manager','THB','fixture',0,0),\
         ('engagement-ended','person-ended','company','employee','salaried','ended','2020-01-01','2026-01-01','Former Manager','THB','fixture',0,0),\
         ('engagement-plain','person-plain','company','employee','salaried','active','2020-01-01',NULL,'Sales User','THB','fixture',0,0),\
         ('engagement-branch','person-branch','company','employee','salaried','active','2020-01-01',NULL,'Branch Manager','THB','fixture',0,0);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,scope_kind,parent_id,source) VALUES \
         ('assignment-requester','requester','sales_manager','[]','company',NULL,0,9000000000000,'holding',NULL,'manual'),\
         ('assignment-reviewer','reviewer','sales_manager','[]','company',NULL,0,9000000000000,'holding',NULL,'manual'),\
         ('assignment-ended','ended-reviewer','sales_manager','[]','company',NULL,0,9000000000000,'holding',NULL,'manual'),\
         ('assignment-plain','plain','sales_manager','[]','company',NULL,0,9000000000000,'holding',NULL,'manual'),\
         ('assignment-branch','branch-reviewer','sales_manager','[]','company','branch',0,9000000000000,'branch',NULL,'manual');\
         UPDATE assignments SET scope_node_id='branch' WHERE id='assignment-branch';",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn command_error(
    pool: &DatabasePool,
    actor: AccessActor,
    id: &str,
    action: AccessAction,
    command_key: &str,
) -> BusinessError {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let id = id.to_owned();
            let command_key = command_key.to_owned();
            Box::pin(async move {
                let mut repository = PgCustomerAccess::new(connection);
                change_access(
                    &mut repository,
                    &actor,
                    &key(&command_key),
                    &id,
                    &change(action, 1),
                    NOW,
                )
                .await
            })
        })
        .await
        .map_err(|error| match error {
            PgTransactionError::Business(error) => error,
            other => panic!("expected an owner business failure, got {other:?}"),
        })
        .unwrap_err()
}

#[tokio::test(flavor = "multi_thread")]
async fn request_commit_is_source_bound_idempotent_and_fully_evidenced() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let key_value = "customer-access-req-0001";
        let result = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let actor = requester();
                let key_value = key_value.to_owned();
                move |connection| {
                    let actor = actor.clone();
                    let key_value = key_value.clone();
                    Box::pin(async move {
                        let mut repository = PgCustomerAccess::new(connection);
                        let result = request_access(
                            &mut repository,
                            &actor,
                            &key(&key_value),
                            &input(),
                            NOW,
                        )
                        .await?;
                        let replay = request_access(
                            &mut repository,
                            &actor,
                            &key(&key_value),
                            &input(),
                            NOW,
                        )
                        .await?;
                        assert_eq!(replay, result);
                        let row = sqlx::query(
                            "SELECT b.id,b.company_id,b.order_id,b.customer_id,b.email,b.status,\
                             b.version,b.reason,b.created_by,b.approved_by,b.created_at,b.updated_at,\
                             o.number order_number,o.state order_state \
                             FROM customer_access_bindings b JOIN commerce_orders o ON o.id=b.order_id \
                             WHERE b.id=$1",
                        )
                        .bind(format!("customer-access-{key_value}"))
                        .fetch_one(&mut *connection)
                        .await?;
                        assert_eq!(row.get::<String, _>("company_id"), "company");
                        assert_eq!(row.get::<String, _>("order_id"), "order-main");
                        assert_eq!(row.get::<String, _>("customer_id"), "customer-main");
                        assert_eq!(row.get::<String, _>("email"), "customer@example.test");
                        assert_eq!(row.get::<String, _>("status"), "pending");
                        assert_eq!(row.get::<i64, _>("version"), 1);
                        assert_eq!(row.get::<String, _>("reason"), "Verified source-bound customer authority");
                        assert_eq!(row.get::<String, _>("created_by"), "requester");
                        assert_eq!(row.get::<Option<String>, _>("approved_by"), None);
                        assert_eq!(row.get::<i64, _>("created_at"), NOW);
                        assert_eq!(row.get::<i64, _>("updated_at"), NOW);
                        assert_eq!(row.get::<String, _>("order_number"), "QT-100");
                        assert_eq!(row.get::<String, _>("order_state"), "confirmed");
                        assert_eq!(
                            sqlx::query_scalar::<_, i64>(
                                "SELECT count(*) FROM customer_access_events WHERE binding_id=$1",
                            )
                            .bind(format!("customer-access-{key_value}"))
                            .fetch_one(&mut *connection)
                            .await?,
                            1
                        );
                        Ok(result)
                    })
                }
            })
            .await
            .unwrap();
        assert_eq!(result["status"], "pending");
        assert_eq!(result["version"], 1);
        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),\
             (SELECT count(*) FROM audit_events WHERE command_id=$1),\
             (SELECT count(*) FROM outbox_events WHERE command_id=$1)",
        )
        .bind(key_value)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (1, 1, 1));
        let payload: String = sqlx::query_scalar(
            "SELECT payload_json FROM outbox_events WHERE command_id=$1",
        )
        .bind(key_value)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(payload["state"], "pending");
        assert_eq!(payload["orderId"], "order-main");
        assert_eq!(payload["companyId"], "company");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn request_denials_are_company_scope_state_and_version_bound_without_effects() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        for (name, actor, request, code) in [
            (
                "foreign-company",
                requester(),
                AccessRequest {
                    order_id: "order-foreign".into(),
                    source_version: 1,
                    ..input()
                },
                "CUSTOMER_ACCESS_SOURCE_NOT_FOUND",
            ),
            (
                "stale-version",
                requester(),
                AccessRequest {
                    source_version: 2,
                    ..input()
                },
                "CUSTOMER_ACCESS_SOURCE_CHANGED",
            ),
            (
                "draft-source",
                requester(),
                AccessRequest {
                    order_id: "order-draft".into(),
                    source_version: 1,
                    ..input()
                },
                "CUSTOMER_ACCESS_SOURCE_CHANGED",
            ),
            (
                "branch-scope",
                actor(
                    "requester",
                    "company",
                    Some("other-branch"),
                    None,
                    "assignment-requester",
                    &["commerce.read", "commerce.sales"],
                ),
                input(),
                "CUSTOMER_ACCESS_SOURCE_NOT_FOUND",
            ),
            (
                "department-scope",
                actor(
                    "requester",
                    "company",
                    None,
                    Some("department"),
                    "assignment-requester",
                    &["commerce.read", "commerce.sales"],
                ),
                input(),
                "CUSTOMER_ACCESS_SOURCE_NOT_FOUND",
            ),
        ] {
            let command_key = format!("customer-access-denied-{name}");
            let error = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), {
                    let actor = actor.clone();
                    let request = request.clone();
                    let command_key = command_key.clone();
                    move |connection| {
                        let actor = actor.clone();
                        let request = request.clone();
                        let command_key = command_key.clone();
                        Box::pin(async move {
                            let mut repository = PgCustomerAccess::new(connection);
                            request_access(
                                &mut repository,
                                &actor,
                                &key(&command_key),
                                &request,
                                NOW,
                            )
                            .await
                        })
                    }
                })
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    other => panic!("expected an owner business failure, got {other:?}"),
                })
                .unwrap_err();
            assert_eq!(error.code, code, "{name}");
            let effects: (i64, i64, i64, i64) = sqlx::query_as(
                "SELECT (SELECT count(*) FROM customer_access_bindings WHERE id LIKE $1),\
                 (SELECT count(*) FROM command_receipts WHERE command_id=$2),\
                 (SELECT count(*) FROM audit_events WHERE command_id=$2),\
                 (SELECT count(*) FROM outbox_events WHERE command_id=$2)",
            )
            .bind(format!("customer-access-{command_key}%"))
            .bind(&command_key)
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(effects, (0, 0, 0, 0), "{name}");
        }

        let missing = actor(
            "requester",
            "company",
            None,
            None,
            "assignment-requester",
            &["commerce.sales"],
        );
        let error = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let actor = missing.clone();
                Box::pin(async move {
                    let mut repository = PgCustomerAccess::new(connection);
                    request_access(
                        &mut repository,
                        &actor,
                        &key("customer-access-permission-0001"),
                        &input(),
                        NOW,
                    )
                    .await
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                other => panic!("expected an owner business failure, got {other:?}"),
            })
            .unwrap_err();
        assert_eq!(error.code, "CUSTOMER_ACCESS_PERMISSION_REQUIRED");
        let receipts: i64 =
            sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id='customer-access-permission-0001'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(receipts, 0);
    })
    .await;
}

async fn create_binding(pool: &DatabasePool, key_value: &str) -> String {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), {
            let key_value = key_value.to_owned();
            move |connection| {
                let key_value = key_value.clone();
                Box::pin(async move {
                    let mut repository = PgCustomerAccess::new(connection);
                    let result = request_access(
                        &mut repository,
                        &requester(),
                        &key(&key_value),
                        &input(),
                        NOW,
                    )
                    .await?;
                    Ok(result["id"].as_str().unwrap().to_owned())
                })
            }
        })
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn independent_review_cas_separation_ended_employment_and_revoke_are_enforced() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let id = create_binding(&pool, "customer-access-create-0001").await;

        let self_error = command_error(
            &pool,
            requester(),
            &id,
            AccessAction::Approve,
            "customer-access-self-0001",
        )
        .await;
        assert_eq!(self_error.code, "INDEPENDENT_APPROVER_REQUIRED");

        let ended_error = command_error(
            &pool,
            actor(
                "ended-reviewer",
                "company",
                None,
                None,
                "assignment-ended",
                &["commerce.read", "commerce.sales", "commerce.approve"],
            ),
            &id,
            AccessAction::Approve,
            "customer-access-ended-0001",
        )
        .await;
        assert_eq!(ended_error.code, "CUSTOMER_ACCESS_REVIEWER_UNAVAILABLE");

        let approved = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let id = id.clone();
                move |connection| {
                    let id = id.clone();
                    Box::pin(async move {
                        let mut repository = PgCustomerAccess::new(connection);
                        change_access(
                            &mut repository,
                            &reviewer(),
                            &key("customer-access-approve-0001"),
                            &id,
                            &change(AccessAction::Approve, 1),
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await
            .unwrap();
        assert_eq!(approved["status"], "approved");
        assert_eq!(approved["version"], 2);
        let row: (String, i64, Option<String>, Option<i64>) = sqlx::query_as(
            "SELECT status,version,approved_by,reviewed_at FROM customer_access_bindings WHERE id=$1",
        )
        .bind(&id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            row,
            ("approved".into(), 2, Some("reviewer".into()), Some(NOW))
        );

        let stale = command_error(
            &pool,
            reviewer(),
            &id,
            AccessAction::Approve,
            "customer-access-stale-0001",
        )
        .await;
        assert_eq!(stale.code, "CUSTOMER_ACCESS_CHANGED");

        let revoked = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let id = id.clone();
                move |connection| {
                    let id = id.clone();
                    Box::pin(async move {
                        let mut repository = PgCustomerAccess::new(connection);
                        change_access(
                            &mut repository,
                            &requester(),
                            &key("customer-access-revoke-0001"),
                            &id,
                            &change(AccessAction::Revoke, 2),
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await
            .unwrap();
        assert_eq!(revoked["status"], "revoked");
        assert_eq!(revoked["version"], 3);
        let events: Vec<(i64, String)> = sqlx::query_as(
            "SELECT version,action FROM customer_access_events WHERE binding_id=$1 ORDER BY version",
        )
        .bind(&id)
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            events,
            vec![(1, "request".into()), (2, "approve".into()), (3, "revoke".into())]
        );

        let id = create_binding(&pool, "customer-access-create-0002").await;
        let other = actor(
            "plain",
            "company",
            None,
            None,
            "assignment-plain",
            &["commerce.read", "commerce.sales"],
        );
        let revoke_error = command_error(
            &pool,
            other,
            &id,
            AccessAction::Revoke,
            "customer-access-other-revoke-0001",
        )
        .await;
        assert_eq!(revoke_error.code, "CUSTOMER_ACCESS_PERMISSION_REQUIRED");

        let cancelled = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let id = id.clone();
                move |connection| {
                    let id = id.clone();
                    Box::pin(async move {
                        sqlx::query("UPDATE commerce_orders SET state='cancelled',version=version+1 WHERE id='order-main'")
                            .execute(&mut *connection)
                            .await?;
                        let mut repository = PgCustomerAccess::new(connection);
                        change_access(
                            &mut repository,
                            &reviewer(),
                            &key("customer-access-cancelled-0001"),
                            &id,
                            &change(AccessAction::Approve, 1),
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                other => panic!("expected an owner business failure, got {other:?}"),
            })
            .unwrap_err();
        assert_eq!(cancelled.code, "CUSTOMER_ACCESS_SOURCE_CHANGED");
        let untouched: (String, i64) =
            sqlx::query_as("SELECT status,version FROM customer_access_bindings WHERE id=$1")
                .bind(&id)
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(untouched, ("pending".into(), 1));
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn revoke_cancels_queued_portal_invitations_in_the_same_transaction() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let id = create_binding(&pool, "customer-access-invite-create-0001").await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let id = id.clone();
                move |connection| {
                    let id = id.clone();
                    Box::pin(async move {
                        let mut repository = PgCustomerAccess::new(connection);
                        change_access(
                            &mut repository,
                            &reviewer(),
                            &key("customer-access-invite-approve-0001"),
                            &id,
                            &change(AccessAction::Approve, 1),
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES('invitation-issue-0001','customer.portal.invite','requester','hash',$1,\
             '{\"id\":\"customer-portal-invite-1\"}',1799999999000,'{}')",
        )
        .bind(&id)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO customer_portal_invitations(id,binding_id,holding_slug,generation,email,token_hash,\
             token_expires_at,state,attempts,next_attempt_at,created_by,created_at,command_id) VALUES \
             ('customer-portal-invite-1',$1,'acme',1,'customer@example.test',\
             repeat('a',64),1800000000000,'queued',0,1800000000000,'requester',1799999999000,'invitation-issue-0001')",
        )
        .bind(&id)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO customer_portal_invitation_events(id,invitation_id,action,actor_id,reason,created_at) VALUES \
             ('customer-portal-invite-event-issued-1','customer-portal-invite-1','issued','requester','Issued',1799999999000)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let id_for_revoke = id.clone();
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let id = id_for_revoke.clone();
                Box::pin(async move {
                    let mut repository = PgCustomerAccess::new(connection);
                    change_access(
                        &mut repository,
                        &requester(),
                        &key("customer-access-invite-revoke-0001"),
                        &id,
                        &change(AccessAction::Revoke, 2),
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        let invitation: (String, Option<String>, Option<i64>, Option<String>) = sqlx::query_as(
            "SELECT state,lease_token,lease_until,last_error FROM customer_portal_invitations WHERE id='customer-portal-invite-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(invitation, ("revoked".into(), None, None, Some("REVOKED".into())));
        let event: (String, String) = sqlx::query_as(
            "SELECT action,reason FROM customer_portal_invitation_events \
             WHERE invitation_id='customer-portal-invite-1' AND action='revoked'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            event,
            (
                "revoked".into(),
                "Independent customer authority decision".into()
            )
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn late_outbox_failure_rolls_back_the_decision_then_retry_persists() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let id = create_binding(&pool, "customer-access-rollback-create-0001").await;
        sqlx::raw_sql(
            "CREATE FUNCTION customer_access_fault() RETURNS trigger LANGUAGE plpgsql AS $$ \
             BEGIN IF NEW.event_type='customer.access.approve' THEN \
             RAISE EXCEPTION 'CUSTOMER_ACCESS_LATE_HANDOFF' USING ERRCODE='23514'; END IF; RETURN NEW; END $$;\
             CREATE TRIGGER customer_access_fault AFTER INSERT ON outbox_events \
             FOR EACH ROW EXECUTE FUNCTION customer_access_fault();",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let failed = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let id = id.clone();
                move |connection| {
                    let id = id.clone();
                    Box::pin(async move {
                        let mut repository = PgCustomerAccess::new(connection);
                        change_access(
                            &mut repository,
                            &reviewer(),
                            &key("customer-access-rollback-approve-0001"),
                            &id,
                            &change(AccessAction::Approve, 1),
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await;
        assert!(failed.is_err());
        let rolled: (String, i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT b.status,b.version,\
             (SELECT count(*) FROM customer_access_events WHERE binding_id=b.id),\
             (SELECT count(*) FROM command_receipts WHERE command_id='customer-access-rollback-approve-0001'),\
             (SELECT count(*) FROM audit_events WHERE command_id='customer-access-rollback-approve-0001'),\
             (SELECT count(*) FROM outbox_events WHERE command_id='customer-access-rollback-approve-0001') \
             FROM customer_access_bindings b WHERE b.id=$1",
        )
        .bind(&id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(rolled, ("pending".into(), 1, 1, 0, 0, 0));
        sqlx::raw_sql(
            "DROP TRIGGER customer_access_fault ON outbox_events; DROP FUNCTION customer_access_fault();",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let approved = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let id = id.clone();
                move |connection| {
                    let id = id.clone();
                    Box::pin(async move {
                        let mut repository = PgCustomerAccess::new(connection);
                        change_access(
                            &mut repository,
                            &reviewer(),
                            &key("customer-access-rollback-approve-0001"),
                            &id,
                            &change(AccessAction::Approve, 1),
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await
            .unwrap();
        assert_eq!(approved["status"], "approved");
        let effects: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id='customer-access-rollback-approve-0001'),\
             (SELECT count(*) FROM audit_events WHERE command_id='customer-access-rollback-approve-0001'),\
             (SELECT count(*) FROM outbox_events WHERE command_id='customer-access-rollback-approve-0001')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(effects, (1, 1, 1));
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn workspace_and_detail_reads_preserve_company_scope_flags_and_audit_history() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let id = create_binding(&pool, "customer-access-read-create-0001").await;
        let id_for_audit = id.clone();
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let id = id_for_audit.clone();
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO customer_access_reads(id,binding_id,binding_version,user_id,read_at,projection_version) \
                         VALUES('read-1',$1,1,'portal-user',1799999998000,'v1')",
                    )
                    .bind(&id)
                    .execute(&mut *connection)
                    .await?;
                    let mut repository = PgCustomerAccess::new(connection);
                    let workspace = access_workspace(&mut repository, &reviewer(), NOW).await?;
                    let rows = workspace["rows"].as_array().unwrap();
                    assert_eq!(rows.len(), 1);
                    assert_eq!(rows[0]["id"], id);
                    assert_eq!(rows[0]["number"], "QT-100");
                    assert_eq!(rows[0]["canReview"], true);
                    assert_eq!(rows[0]["canInvite"], false);
                    assert_eq!(workspace["sources"].as_array().unwrap().len(), 3);
                    let detail = access_detail(&mut repository, &reviewer(), &id, NOW).await?;
                    assert_eq!(detail["binding"]["id"], id);
                    assert_eq!(detail["events"].as_array().unwrap().len(), 1);
                    assert_eq!(detail["reads"][0]["projection_version"], "v1");
                    Ok(())
                })
            })
            .await
            .unwrap();
        for (label, actor) in [
            (
                "foreign-company",
                actor(
                    "requester",
                    "foreign",
                    None,
                    None,
                    "assignment-requester",
                    &["commerce.read", "commerce.sales"],
                ),
            ),
            (
                "other-branch",
                actor(
                    "requester",
                    "company",
                    Some("other-branch"),
                    None,
                    "assignment-requester",
                    &["commerce.read", "commerce.sales"],
                ),
            ),
            (
                "department",
                actor(
                    "requester",
                    "company",
                    None,
                    Some("department"),
                    "assignment-requester",
                    &["commerce.read", "commerce.sales"],
                ),
            ),
        ] {
            let workspace = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), {
                    let actor = actor.clone();
                    move |connection| {
                        let actor = actor.clone();
                        Box::pin(async move {
                            let mut repository = PgCustomerAccess::new(connection);
                            access_workspace(&mut repository, &actor, NOW).await
                        })
                    }
                })
                .await
                .unwrap();
            assert_eq!(
                workspace["rows"].as_array().unwrap().len(),
                0,
                "{label}"
            );
            let detail = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), {
                    let actor = actor.clone();
                    let id = id.clone();
                    move |connection| {
                        let actor = actor.clone();
                        let id = id.clone();
                        Box::pin(async move {
                            let mut repository = PgCustomerAccess::new(connection);
                            access_detail(&mut repository, &actor, &id, NOW).await
                        })
                    }
                })
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    other => panic!("expected an owner business failure, got {other:?}"),
                })
                .unwrap_err();
            assert_eq!(detail.code, "CUSTOMER_ACCESS_NOT_FOUND", "{label}");
        }
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn branch_scoped_manager_can_review_only_its_branch_source() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let id = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                move |connection| {
                    Box::pin(async move {
                        let mut repository = PgCustomerAccess::new(connection);
                        let result = request_access(
                            &mut repository,
                            &requester(),
                            &key("customer-access-branch-create-0001"),
                            &input(),
                            NOW,
                        )
                        .await?;
                        Ok(result["id"].as_str().unwrap().to_owned())
                    })
                }
            })
            .await
            .unwrap();
        let branch_reviewer = actor(
            "branch-reviewer",
            "company",
            Some("branch"),
            None,
            "assignment-branch",
            &["commerce.read", "commerce.sales", "commerce.approve"],
        );
        let approved = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let id = id.clone();
                let branch_reviewer = branch_reviewer.clone();
                move |connection| {
                    let id = id.clone();
                    let branch_reviewer = branch_reviewer.clone();
                    Box::pin(async move {
                        let mut repository = PgCustomerAccess::new(connection);
                        change_access(
                            &mut repository,
                            &branch_reviewer,
                            &key("customer-access-branch-approve-0001"),
                            &id,
                            &change(AccessAction::Approve, 1),
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await
            .unwrap();
        assert_eq!(approved["status"], "approved");

        let other_id = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                move |connection| {
                    Box::pin(async move {
                        let mut repository = PgCustomerAccess::new(connection);
                        let result = request_access(
                            &mut repository,
                            &requester(),
                            &key("customer-access-branch-create-0002"),
                            &AccessRequest {
                                order_id: "order-other".into(),
                                source_version: 1,
                                ..input()
                            },
                            NOW,
                        )
                        .await?;
                        Ok(result["id"].as_str().unwrap().to_owned())
                    })
                }
            })
            .await
            .unwrap();
        let error = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let other_id = other_id.clone();
                let branch_reviewer = branch_reviewer.clone();
                move |connection| {
                    let other_id = other_id.clone();
                    let branch_reviewer = branch_reviewer.clone();
                    Box::pin(async move {
                        let mut repository = PgCustomerAccess::new(connection);
                        change_access(
                            &mut repository,
                            &branch_reviewer,
                            &key("customer-access-branch-approve-0002"),
                            &other_id,
                            &change(AccessAction::Approve, 1),
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                other => panic!("expected an owner business failure, got {other:?}"),
            })
            .unwrap_err();
        assert_eq!(error.code, "CUSTOMER_ACCESS_NOT_FOUND");
        let untouched: (String, i64) =
            sqlx::query_as("SELECT status,version FROM customer_access_bindings WHERE id=$1")
                .bind(&other_id)
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(untouched, ("pending".into(), 1));
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn adapter_requires_an_existing_serializable_holding_transaction() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        {
            let mut repository = PgCustomerAccess::new(&mut connection);
            let error = request_access(
                &mut repository,
                &requester(),
                &key("customer-access-guard-0001"),
                &input(),
                NOW,
            )
            .await
            .unwrap_err();
            assert!(matches!(
                error,
                PgTransactionError::Persistence(ref error)
                    if error.code == "CUSTOMER_ACCESS_TRANSACTION_REQUIRED"
            ));
        }
        let mut transaction = connection
            .begin_with("BEGIN ISOLATION LEVEL READ COMMITTED")
            .await
            .unwrap();
        {
            let mut repository = PgCustomerAccess::new(&mut transaction);
            let error = request_access(
                &mut repository,
                &requester(),
                &key("customer-access-guard-0002"),
                &input(),
                NOW,
            )
            .await
            .unwrap_err();
            assert!(matches!(
                error,
                PgTransactionError::Persistence(ref error)
                    if error.code == "CUSTOMER_ACCESS_SERIALIZABLE_REQUIRED"
            ));
        }
        transaction.rollback().await.unwrap();
        drop(connection);
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id LIKE 'customer-access-guard-%'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipts, 0);
    })
    .await;
}
