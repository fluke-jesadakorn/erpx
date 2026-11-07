//! Native Talent requisition commands/reads over real migrated PostgreSQL:
//! current IAM authority, Company/branch/department isolation, versions/CAS,
//! idempotency, audit/outbox, rollback and persisted readback.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, HoldingId, RecordId, UserId,
};
use erpx_server_talent::public::requisition::{
    PgTalentRequisitions, RequisitionAction, RequisitionChange, RequisitionRequest, TalentActor,
    change_requisition, create_requisition, requisition_detail, requisition_workspace,
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
) -> TalentActor {
    TalentActor {
        user_id: UserId::new(user).unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: branch.map(|value| RecordId::new(value).unwrap()),
        department_id: department.map(|value| RecordId::new(value).unwrap()),
        assignment_id: AssignmentId::new(assignment).unwrap(),
        role: Some("people_manager".into()),
        authority_revision: Some(3),
        assignment_version: Some(1),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        expires_at: NOW + 60_000,
    }
}

fn writer() -> TalentActor {
    actor(
        "creator",
        "company",
        Some("branch"),
        None,
        "assignment-creator",
        &["records.read", "records.write", "records.review"],
    )
}

fn reviewer() -> TalentActor {
    actor(
        "reviewer",
        "company",
        Some("branch"),
        None,
        "assignment-reviewer",
        &["records.read", "records.write", "records.review"],
    )
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn request() -> RequisitionRequest {
    RequisitionRequest {
        job_title: "  Service specialist  ".into(),
        openings: 2,
        target_start_date: "2026-11-01".into(),
        description: "  Serve customer cases and complete field handoffs.  ".into(),
    }
}

fn change(action: RequisitionAction, expected: i64) -> RequisitionChange {
    RequisitionChange {
        action,
        expected_version: expected,
        reason: "Headcount and scope reviewed".into(),
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
        "INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) VALUES \
         ('company','creator','local:creator','active','local',1),\
         ('company','reviewer','local:reviewer','active','local',1),\
         ('company','revoked','local:revoked','active','local',1),\
         ('company','other-branch','local:other-branch','active','local',1),\
         ('foreign','foreign-user','local:foreign-user','active','local',1);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,scope_kind,scope_node_id,source) VALUES \
         ('assignment-creator','creator','people_manager','[]','company','branch',0,9000000000000,NULL,'branch','branch','manual'),\
         ('assignment-reviewer','reviewer','people_manager','[]','company','branch',0,9000000000000,NULL,'branch','branch','manual'),\
         ('assignment-revoked','revoked','people_manager','[]','company','branch',0,9000000000000,1,'branch','branch','manual'),\
         ('assignment-other-branch','other-branch','people_manager','[]','company','other-branch',0,9000000000000,NULL,'branch','other-branch','manual'),\
         ('assignment-foreign','foreign-user','people_manager','[]','foreign','foreign-branch',0,9000000000000,NULL,'branch','foreign-branch','manual');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn create(pool: &DatabasePool, actor: TalentActor, key_value: &str) -> String {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), {
            let key_value = key_value.to_owned();
            move |connection| {
                let actor = actor.clone();
                let key_value = key_value.clone();
                Box::pin(async move {
                    let mut repository = PgTalentRequisitions::new(connection);
                    let result = create_requisition(
                        &mut repository,
                        &actor,
                        &key(&key_value),
                        &request(),
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

async fn command_error(
    pool: &DatabasePool,
    actor: TalentActor,
    id: &str,
    action: RequisitionAction,
    expected: i64,
    key_value: &str,
) -> BusinessError {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), {
            let id = id.to_owned();
            let key_value = key_value.to_owned();
            move |connection| {
                let actor = actor.clone();
                let id = id.clone();
                let key_value = key_value.clone();
                Box::pin(async move {
                    let mut repository = PgTalentRequisitions::new(connection);
                    change_requisition(
                        &mut repository,
                        &actor,
                        &key(&key_value),
                        &id,
                        &change(action, expected),
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
        .unwrap_err()
}

#[tokio::test(flavor = "multi_thread")]
async fn create_persists_exact_requisition_and_fully_evidenced_receipt() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let key_value = "talent-requisition-create-0001";
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let key_value = key_value.to_owned();
                move |connection| {
                    let key_value = key_value.clone();
                    Box::pin(async move {
                        let mut repository = PgTalentRequisitions::new(connection);
                        let result = create_requisition(
                            &mut repository,
                            &writer(),
                            &key(&key_value),
                            &request(),
                            NOW,
                        )
                        .await?;
                        let replay = create_requisition(
                            &mut repository,
                            &writer(),
                            &key(&key_value),
                            &request(),
                            NOW,
                        )
                        .await?;
                        assert_eq!(replay, result);
                        let row = sqlx::query(
                            "SELECT id,company_id,branch_id,department_id,job_title,openings,\
                             target_start_date,description,status,created_by,reviewed_by,version,\
                             created_at,updated_at FROM talent_requisitions WHERE id=$1",
                        )
                        .bind(format!("talent-req-{key_value}"))
                        .fetch_one(&mut *connection)
                        .await?;
                        assert_eq!(row.get::<String, _>("company_id"), "company");
                        assert_eq!(row.get::<String, _>("branch_id"), "branch");
                        assert_eq!(row.get::<Option<String>, _>("department_id"), None);
                        assert_eq!(row.get::<String, _>("job_title"), "Service specialist");
                        assert_eq!(row.get::<i64, _>("openings"), 2);
                        assert_eq!(row.get::<String, _>("target_start_date"), "2026-11-01");
                        assert_eq!(
                            row.get::<String, _>("description"),
                            "Serve customer cases and complete field handoffs."
                        );
                        assert_eq!(row.get::<String, _>("status"), "draft");
                        assert_eq!(row.get::<String, _>("created_by"), "creator");
                        assert_eq!(row.get::<Option<String>, _>("reviewed_by"), None);
                        assert_eq!(row.get::<i64, _>("version"), 1);
                        assert_eq!(row.get::<i64, _>("created_at"), NOW);
                        assert_eq!(row.get::<i64, _>("updated_at"), NOW);
                        let events: Vec<(i64, String, String)> = sqlx::query_as(
                            "SELECT version,action,status FROM talent_requisition_events \
                             WHERE requisition_id=$1 ORDER BY version",
                        )
                        .bind(format!("talent-req-{key_value}"))
                        .fetch_all(&mut *connection)
                        .await?;
                        assert_eq!(events, vec![(1, "create".into(), "draft".into())]);
                        Ok(result)
                    })
                }
            })
            .await
            .unwrap();
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
        let payload: String =
            sqlx::query_scalar("SELECT payload_json FROM outbox_events WHERE command_id=$1")
                .bind(key_value)
                .fetch_one(pool.raw())
                .await
                .unwrap();
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(payload["state"], "draft");
        assert_eq!(payload["companyId"], "company");
        assert_eq!(payload["branchId"], "branch");
        assert_eq!(payload["version"], 1);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn create_denials_are_permission_scope_and_assignment_bound_without_effects() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        for (name, actor, expected) in [
            (
                "no-write",
                actor(
                    "creator",
                    "company",
                    Some("branch"),
                    None,
                    "assignment-creator",
                    &["records.read", "records.review"],
                ),
                "TALENT_PERMISSION_REQUIRED",
            ),
            (
                "no-branch",
                actor(
                    "creator",
                    "company",
                    None,
                    None,
                    "assignment-creator",
                    &["records.read", "records.write", "records.review"],
                ),
                "BRANCH_SCOPE_REQUIRED",
            ),
            (
                "revoked",
                actor(
                    "revoked",
                    "company",
                    Some("branch"),
                    None,
                    "assignment-revoked",
                    &["records.read", "records.write", "records.review"],
                ),
                "TALENT_ACCESS_CHANGED",
            ),
        ] {
            let key_value = format!("talent-requisition-denied-{name}");
            let error = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), {
                    let actor = actor.clone();
                    let key_value = key_value.clone();
                    move |connection| {
                        let actor = actor.clone();
                        let key_value = key_value.clone();
                        Box::pin(async move {
                            let mut repository = PgTalentRequisitions::new(connection);
                            create_requisition(
                                &mut repository,
                                &actor,
                                &key(&key_value),
                                &request(),
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
            assert_eq!(error.code, expected, "{name}");
            let effects: (i64, i64, i64, i64) = sqlx::query_as(
                "SELECT (SELECT count(*) FROM talent_requisitions WHERE id=$1),\
                 (SELECT count(*) FROM command_receipts WHERE command_id=$2),\
                 (SELECT count(*) FROM audit_events WHERE command_id=$2),\
                 (SELECT count(*) FROM outbox_events WHERE command_id=$2)",
            )
            .bind(format!("talent-req-{key_value}"))
            .bind(&key_value)
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(effects, (0, 0, 0, 0), "{name}");
        }

        let expired = TalentActor {
            expires_at: NOW,
            ..writer()
        };
        let error = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let expired = expired.clone();
                Box::pin(async move {
                    let mut repository = PgTalentRequisitions::new(connection);
                    create_requisition(
                        &mut repository,
                        &expired,
                        &key("talent-requisition-expired-0001"),
                        &request(),
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
        assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn independent_review_cas_and_every_transition_are_enforced() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let id = create(&pool, writer(), "talent-requisition-create-0002").await;

        let self_error = command_error(
            &pool,
            writer(),
            &id,
            RequisitionAction::Approve,
            1,
            "talent-requisition-self-0001",
        )
        .await;
        assert_eq!(self_error.code, "CONFLICT");

        let approved = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let id = id.clone();
                move |connection| {
                    let id = id.clone();
                    Box::pin(async move {
                        let mut repository = PgTalentRequisitions::new(connection);
                        change_requisition(
                            &mut repository,
                            &reviewer(),
                            &key("talent-requisition-approve-0001"),
                            &id,
                            &change(RequisitionAction::Approve, 1),
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
            "SELECT status,version,reviewed_by,updated_at FROM talent_requisitions WHERE id=$1",
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
            RequisitionAction::Approve,
            1,
            "talent-requisition-stale-0001",
        )
        .await;
        assert_eq!(stale.code, "CONFLICT");

        let opened = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let id = id.clone();
                move |connection| {
                    let id = id.clone();
                    Box::pin(async move {
                        let mut repository = PgTalentRequisitions::new(connection);
                        change_requisition(
                            &mut repository,
                            &reviewer(),
                            &key("talent-requisition-open-0001"),
                            &id,
                            &change(RequisitionAction::Open, 2),
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await
            .unwrap();
        assert_eq!(opened["status"], "open");
        assert_eq!(opened["version"], 3);

        let closed = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let id = id.clone();
                move |connection| {
                    let id = id.clone();
                    Box::pin(async move {
                        let mut repository = PgTalentRequisitions::new(connection);
                        change_requisition(
                            &mut repository,
                            &writer(),
                            &key("talent-requisition-close-0001"),
                            &id,
                            &change(RequisitionAction::Close, 3),
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await
            .unwrap();
        assert_eq!(closed["status"], "closed");

        let events: Vec<(i64, String)> = sqlx::query_as(
            "SELECT version,action FROM talent_requisition_events WHERE requisition_id=$1 \
             ORDER BY version",
        )
        .bind(&id)
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            events,
            vec![
                (1, "create".into()),
                (2, "approve".into()),
                (3, "open".into()),
                (4, "close".into())
            ]
        );

        let other = create(&pool, writer(), "talent-requisition-create-0003").await;
        let cancelled = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let other = other.clone();
                move |connection| {
                    let other = other.clone();
                    Box::pin(async move {
                        let mut repository = PgTalentRequisitions::new(connection);
                        change_requisition(
                            &mut repository,
                            &writer(),
                            &key("talent-requisition-cancel-0001"),
                            &other,
                            &change(RequisitionAction::Cancel, 1),
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await
            .unwrap();
        assert_eq!(cancelled["status"], "cancelled");

        let rejected = create(&pool, writer(), "talent-requisition-create-0004").await;
        let rejected = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let rejected = rejected.clone();
                move |connection| {
                    let rejected = rejected.clone();
                    Box::pin(async move {
                        let mut repository = PgTalentRequisitions::new(connection);
                        change_requisition(
                            &mut repository,
                            &reviewer(),
                            &key("talent-requisition-reject-0001"),
                            &rejected,
                            &change(RequisitionAction::Reject, 1),
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await
            .unwrap();
        assert_eq!(rejected["status"], "rejected");

        let foreign = command_error(
            &pool,
            actor(
                "other-branch",
                "company",
                Some("other-branch"),
                None,
                "assignment-other-branch",
                &["records.read", "records.write", "records.review"],
            ),
            &id,
            RequisitionAction::Close,
            4,
            "talent-requisition-foreign-0001",
        )
        .await;
        assert_eq!(foreign.code, "TALENT_REQUISITION_NOT_FOUND");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn workspace_and_detail_enforce_company_branch_department_scope() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let own = create(&pool, writer(), "talent-requisition-create-0005").await;
        let other_branch = create(
            &pool,
            actor(
                "other-branch",
                "company",
                Some("other-branch"),
                None,
                "assignment-other-branch",
                &["records.read", "records.write", "records.review"],
            ),
            "talent-requisition-create-0006",
        )
        .await;
        let foreign = create(
            &pool,
            actor(
                "foreign-user",
                "foreign",
                Some("foreign-branch"),
                None,
                "assignment-foreign",
                &["records.read", "records.write", "records.review"],
            ),
            "talent-requisition-create-0007",
        )
        .await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let own = own.clone();
                let other_branch = other_branch.clone();
                let foreign = foreign.clone();
                move |connection| {
                    let own = own.clone();
                    let other_branch = other_branch.clone();
                    let foreign = foreign.clone();
                    Box::pin(async move {
                        let mut repository = PgTalentRequisitions::new(connection);
                        let workspace =
                            requisition_workspace(&mut repository, &writer(), NOW).await?;
                        let ids = workspace["requisitions"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|row| row["id"].as_str().unwrap().to_owned())
                            .collect::<Vec<_>>();
                        assert_eq!(ids, vec![own.clone()]);
                        assert_eq!(workspace["requisitions"][0]["canApprove"], false);
                        assert_eq!(workspace["requisitions"][0]["canCancel"], true);
                        let detail =
                            requisition_detail(&mut repository, &writer(), &own, NOW).await?;
                        assert_eq!(detail["requisition"]["id"], own);
                        assert_eq!(detail["events"].as_array().unwrap().len(), 1);
                        let foreign_error = match requisition_detail(
                            &mut repository,
                            &writer(),
                            &other_branch,
                            NOW,
                        )
                        .await
                        .unwrap_err()
                        {
                            PgTransactionError::Business(error) => error,
                            other => panic!("expected an owner business failure, got {other:?}"),
                        };
                        assert_eq!(foreign_error.code, "TALENT_REQUISITION_NOT_FOUND");
                        let other_error =
                            match requisition_detail(&mut repository, &writer(), &foreign, NOW)
                                .await
                                .unwrap_err()
                            {
                                PgTransactionError::Business(error) => error,
                                other => {
                                    panic!("expected an owner business failure, got {other:?}")
                                }
                            };
                        assert_eq!(other_error.code, "TALENT_REQUISITION_NOT_FOUND");
                        Ok(())
                    })
                }
            })
            .await
            .unwrap();

        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let other_branch = other_branch.clone();
                Box::pin(async move {
                    let mut repository = PgTalentRequisitions::new(connection);
                    let workspace = requisition_workspace(
                        &mut repository,
                        &actor(
                            "other-branch",
                            "company",
                            Some("other-branch"),
                            None,
                            "assignment-other-branch",
                            &["records.read", "records.write", "records.review"],
                        ),
                        NOW,
                    )
                    .await?;
                    let ids = workspace["requisitions"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|row| row["id"].as_str().unwrap().to_owned())
                        .collect::<Vec<_>>();
                    assert_eq!(ids, vec![other_branch]);
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn late_outbox_failure_rolls_back_and_exact_retry_persists_once() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let key_value = "talent-requisition-rollback-0001";
        let name = "talent_requisition_outbox_failure";
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "CREATE FUNCTION {name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN \
             RAISE EXCEPTION 'SYNTHETIC_TALENT_OUTBOX_FAILURE'; RETURN NEW; END $$;\
             CREATE TRIGGER {name} BEFORE INSERT ON outbox_events FOR EACH ROW \
             WHEN (NEW.command_id='{key_value}') EXECUTE FUNCTION {name}()"
        )))
        .execute(pool.raw())
        .await
        .unwrap();
        let result = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let key_value = key_value.to_owned();
                move |connection| {
                    let key_value = key_value.clone();
                    Box::pin(async move {
                        let mut repository = PgTalentRequisitions::new(connection);
                        create_requisition(
                            &mut repository,
                            &writer(),
                            &key(&key_value),
                            &request(),
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await;
        assert!(matches!(result, Err(PgTransactionError::Persistence(_))));
        let effects: (i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM talent_requisitions WHERE id=$1),\
             (SELECT count(*) FROM talent_requisition_events WHERE requisition_id=$1),\
             (SELECT count(*) FROM command_receipts WHERE command_id=$2),\
             (SELECT count(*) FROM audit_events WHERE command_id=$2),\
             (SELECT count(*) FROM outbox_events WHERE command_id=$2)",
        )
        .bind(format!("talent-req-{key_value}"))
        .bind(key_value)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(effects, (0, 0, 0, 0, 0));
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "DROP TRIGGER {name} ON outbox_events;DROP FUNCTION {name}()"
        )))
        .execute(pool.raw())
        .await
        .unwrap();
        let id = create(&pool, writer(), key_value).await;
        assert_eq!(id, format!("talent-req-{key_value}"));
        let effects: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM talent_requisitions WHERE id=$1),\
             (SELECT count(*) FROM command_receipts WHERE command_id=$2),\
             (SELECT count(*) FROM audit_events WHERE command_id=$2),\
             (SELECT count(*) FROM outbox_events WHERE command_id=$2)",
        )
        .bind(&id)
        .bind(key_value)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(effects, (1, 1, 1, 1));
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn serializable_owner_transaction_is_required_for_commands_and_reads() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let mut repository = PgTalentRequisitions::new(&mut connection);
        let error = create_requisition(
            &mut repository,
            &writer(),
            &key("talent-requisition-transaction-0001"),
            &request(),
            NOW,
        )
        .await
        .unwrap_err();
        match error {
            PgTransactionError::Persistence(error) => {
                assert_eq!(error.code, "TALENT_REQUISITION_TRANSACTION_REQUIRED")
            }
            other => panic!("expected a persistence failure, got {other:?}"),
        }
        drop(connection);

        let mut connection = pool.raw().acquire().await.unwrap();
        let mut transaction = connection
            .begin_with("BEGIN ISOLATION LEVEL READ COMMITTED")
            .await
            .unwrap();
        {
            let mut repository = PgTalentRequisitions::new(&mut transaction);
            let error = requisition_workspace(&mut repository, &writer(), NOW)
                .await
                .unwrap_err();
            match error {
                PgTransactionError::Persistence(error) => {
                    assert_eq!(error.code, "TALENT_REQUISITION_SERIALIZABLE_REQUIRED")
                }
                other => panic!("expected a persistence failure, got {other:?}"),
            }
        }
        transaction.rollback().await.unwrap();
    })
    .await;
}
