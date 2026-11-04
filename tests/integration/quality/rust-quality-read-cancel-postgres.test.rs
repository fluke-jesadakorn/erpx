//! Native Quality reads and cancellation over real PostgreSQL units.
#[path = "../../support/quality/quality-facts.rs"]
mod facts;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_quality::public::quality_http::{
    PgQualityCancellation, PgQualityRead, apply_quality_cancellation, lock_quality_read_sources,
    read_quality_detail, read_quality_workspace,
};
use erpx_server_quality::public::quality_read::{
    CancellationAction, HoldStatus, QualityActor, QualityCancellationCommand, cancel_quality_hold,
    quality_detail, quality_workspace,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgUnitOfWork, TransactionOptions};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, RecordId, TimestampMillis, UserId,
};

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

fn branch() -> RecordId {
    RecordId::new("branch").unwrap()
}

fn user(value: &str) -> UserId {
    UserId::new(value).unwrap()
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn actor(value: &str, permissions: &[&str]) -> QualityActor {
    QualityActor {
        user_id: user(value),
        company_id: company(),
        branch_id: Some(branch()),
        department_id: None,
        permissions: permissions
            .iter()
            .map(|permission| (*permission).into())
            .collect(),
    }
}

struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(1_800_000_000_000)
    }
}

fn denied<T: std::fmt::Debug>(result: Result<T, PgTransactionError<BusinessError>>, code: &str) {
    match result {
        Err(PgTransactionError::Business(e)) => assert_eq!(e.code, code),
        other => panic!("expected {code}, got {other:?}"),
    }
}

async fn read_workspace_view(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
    actor: QualityActor,
) -> Result<
    erpx_server_quality::public::quality_read::QualityWorkspace,
    PgTransactionError<BusinessError>,
> {
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let actor = actor.clone();
            Box::pin(async move {
                lock_quality_read_sources(c).await?;
                let mut repository = PgQualityRead {
                    connection: &mut *c,
                };
                Ok(quality_workspace(&actor, &mut repository).await?)
            })
        })
        .await
}

async fn read_detail_view(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
    actor: QualityActor,
    id: &str,
) -> Result<
    erpx_server_quality::public::quality_read::QualityDetail,
    PgTransactionError<BusinessError>,
> {
    let id = id.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let actor = actor.clone();
            let id = id.clone();
            Box::pin(async move {
                lock_quality_read_sources(c).await?;
                let mut repository = PgQualityRead {
                    connection: &mut *c,
                };
                Ok(quality_detail(&actor, &id, &mut repository).await?)
            })
        })
        .await
}

async fn cancel(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
    actor: QualityActor,
    command: QualityCancellationCommand,
) -> Result<
    erpx_server_quality::public::quality_read::QualityCancellationReceipt,
    PgTransactionError<BusinessError>,
> {
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let actor = actor.clone();
            let command = command.clone();
            Box::pin(async move {
                lock_quality_read_sources(c).await?;
                let mut repository = PgQualityCancellation {
                    connection: &mut *c,
                };
                Ok(cancel_quality_hold(&actor, command, &mut repository, &FixedClock).await?)
            })
        })
        .await
}

fn command(
    command_key: &str,
    hold: &str,
    version: i64,
    action: CancellationAction,
    reason: &str,
) -> QualityCancellationCommand {
    QualityCancellationCommand {
        command_key: key(command_key),
        hold_id: RecordId::new(hold).unwrap(),
        expected_version: version,
        action,
        reason: reason.into(),
    }
}

#[tokio::test]
async fn workspace_and_detail_read_only_quality_owned_facts_in_current_scope() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        facts::hold(&pool, "hold-a").await;
        facts::hold_in_branch(&pool, "hold-other", "branch-other").await;
        facts::specification(&pool, "spec").await;
        facts::inspection(&pool, "hold-a", "inspection", "pass").await;
        facts::count(&pool, "hold-a", "count-a").await;
        sqlx::raw_sql(
            "UPDATE inventory_quality_holds SET status='awaiting_release',last_result='pass',last_inspector='inspector',version=2,spec_id='spec',spec_version=2 WHERE id='hold-a';\
             INSERT INTO inventory_quality_cancellations(id,hold_id,version,action,reason,actor_id,created_at)VALUES('cancel-1','hold-a',1,'request','Mistaken scope','maker',0);\
             INSERT INTO inventory_quality_disposition_events(id,hold_id,count_id,action,actor_id,reviewed_version,reason,created_at)VALUES('disposition-1','hold-a','count-a','request','manager',2,'Controlled scrap review',0);\
             INSERT INTO inventory_quality_alerts(id,hold_id,phase,source_version,due_date,detected_at)VALUES('alert-1','hold-a','due_soon',2,'2026-11-10',0);",
        )
        .execute(pool.raw())
        .await
        .unwrap();

        let reviewer = actor("manager", &["quality.read", "quality.inspect", "quality.release", "quality.hold"]);
        let result = read_workspace_view(&pool, reviewer.clone()).await.unwrap();
        assert_eq!(result.holds.len(), 1);
        assert_eq!(result.holds[0].hold.id, "hold-a");
        assert_eq!(result.holds[0].hold.status, HoldStatus::AwaitingRelease);
        assert_eq!(result.holds[0].hold.inspection_count, 1);
        assert!(result.holds[0].decision.can_release);

        let readback = read_detail_view(&pool, reviewer.clone(), "hold-a").await.unwrap();
        assert_eq!(readback.hold.hold.id, "hold-a");
        assert_eq!(readback.inspections.len(), 1);
        assert_eq!(readback.measurements.len(), 2);
        assert!(readback.measurements[0].passed);
        assert_eq!(readback.cancellations.len(), 1);
        assert_eq!(readback.dispositions.len(), 1);
        assert_eq!(readback.spec_metrics.len(), 2);
        assert_eq!(readback.alerts.len(), 1);
        assert!(readback.hold.decision.can_release);

        // The creator cannot release their own passing inspection.
        let creator = actor("maker", &["quality.read", "quality.inspect", "quality.release", "quality.hold"]);
        let result = read_detail_view(&pool, creator, "hold-a").await.unwrap();
        assert!(!result.hold.decision.can_release);

        // A branch-scoped read must not see another branch or a foreign Company.
        let other = actor("manager", &["quality.read"]);
        let missing = read_detail_view(&pool, other, "hold-other").await;
        denied(missing, "QUALITY_HOLD_NOT_FOUND");

        let foreign = erpx_server_quality::public::quality_read::QualityActor {
            company_id: CompanyId::new("foreign").unwrap(),
            ..actor("manager", &["quality.read"])
        };
        let missing = read_detail_view(&pool, foreign, "hold-a").await;
        denied(missing, "QUALITY_HOLD_NOT_FOUND");
    })
    .await;
}

#[tokio::test]
async fn current_workspace_snapshot_rejects_a_changed_hold_in_the_same_transaction() {
    fixture::with_database(1, |pool| async move {
        facts::setup(&pool).await;
        facts::hold(&pool, "hold-a").await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_quality_read_sources(c).await?;
                    let current = read_quality_workspace(c, &company(), Some(&branch())).await?;
                    sqlx::query("UPDATE inventory_quality_holds SET reason='Changed after the read' WHERE id='hold-a'")
                        .execute(&mut *c)
                        .await?;
                    denied(
                        current.require_current(c).await,
                        "QUALITY_READ_CHANGED",
                    );
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn cancellation_requests_and_approves_once_with_local_idempotency() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        facts::hold(&pool, "hold-a").await;
        let maker = actor("maker", &["quality.read", "quality.hold"]);
        let request = command(
            "quality-cancel-request",
            "hold-a",
            1,
            CancellationAction::Request,
            "Duplicate hold created for the wrong receiving review",
        );
        let receipt = cancel(&pool, maker.clone(), request.clone()).await.unwrap();
        assert_eq!(receipt.status, HoldStatus::CancelPending);
        assert_eq!(receipt.version, 2);
        assert_eq!(
            cancel(&pool, maker.clone(), request.clone()).await.unwrap(),
            receipt
        );
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM inventory_quality_cancellations WHERE hold_id='hold-a'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(count, 1);

        let conflict = command(
            "quality-cancel-request",
            "hold-a",
            1,
            CancellationAction::Request,
            "A different reason must not replay the saved receipt",
        );
        denied(cancel(&pool, maker.clone(), conflict).await, "QUALITY_CANCELLATION_CONFLICT");
        let self_manager = actor("maker", &["quality.read", "quality.release"]);
        denied(
            cancel(
                &pool,
                self_manager,
                command(
                    "quality-cancel-self",
                    "hold-a",
                    2,
                    CancellationAction::Approve,
                    "Self approval attempt",
                ),
            )
            .await,
            "INDEPENDENT_CANCELLATION_REQUIRED",
        );

        sqlx::raw_sql(
            "INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy)VALUES\
             ('company','maker','local:maker','person-1','active','local'),\
             ('company','alias','local:alias','person-1','active','local');",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let alias = actor("alias", &["quality.read", "quality.release"]);
        denied(
            cancel(
                &pool,
                alias,
                command(
                    "quality-cancel-alias",
                    "hold-a",
                    2,
                    CancellationAction::Reject,
                    "Merged identity must not review its own request",
                ),
            )
            .await,
            "INDEPENDENT_CANCELLATION_REQUIRED",
        );

        let manager = actor("manager", &["quality.read", "quality.release"]);
        let approved = cancel(
            &pool,
            manager.clone(),
            command(
                "quality-cancel-approve",
                "hold-a",
                2,
                CancellationAction::Approve,
                "Confirmed duplicate of the other active hold",
            ),
        )
        .await
        .unwrap();
        assert_eq!(approved.status, HoldStatus::Cancelled);
        assert_eq!(approved.version, 3);

        let readback = read_detail_view(&pool, manager, "hold-a").await.unwrap();
        assert_eq!(readback.hold.hold.status, HoldStatus::Cancelled);
        assert_eq!(readback.cancellations.len(), 2);
        assert!(!readback.hold.decision.can_inspect);
        assert!(!readback.hold.decision.can_cancel);

        let saved = sqlx::query(
            "SELECT status,cancelled_by,cancelled_at,version FROM inventory_quality_holds WHERE id='hold-a'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        use sqlx::Row;
        assert_eq!(saved.get::<String, _>("status"), "cancelled");
        assert_eq!(saved.get::<Option<String>, _>("cancelled_by"), Some("manager".into()));
        assert_eq!(saved.get::<Option<i64>, _>("cancelled_at"), Some(1_800_000_000_000));
        assert_eq!(saved.get::<i64, _>("version"), 3);
    })
    .await;
}

#[tokio::test]
async fn cancellation_rejects_inspected_holds_and_a_late_failure_rolls_back() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        facts::hold(&pool, "hold-inspected").await;
        facts::specification(&pool, "spec").await;
        facts::inspection(&pool, "hold-inspected", "inspection", "fail").await;
        sqlx::query("UPDATE inventory_quality_holds SET last_result='fail',last_inspector='inspector',version=2,spec_id='spec',spec_version=2 WHERE id='hold-inspected'")
            .execute(pool.raw())
            .await
            .unwrap();
        let maker = actor("maker", &["quality.read", "quality.hold"]);
        denied(
            cancel(
                &pool,
                maker,
                command(
                    "quality-cancel-inspected",
                    "hold-inspected",
                    2,
                    CancellationAction::Request,
                    "Try to bypass a failed inspection",
                ),
            )
            .await,
            "UNINSPECTED_HOLD_REQUIRED",
        );
        let version: i64 = sqlx::query_scalar("SELECT version FROM inventory_quality_holds WHERE id='hold-inspected'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(version, 2);

        facts::hold(&pool, "hold-rollback").await;
        let result = PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_quality_read_sources(c).await?;
                    let command = command(
                        "quality-cancel-rollback",
                        "hold-rollback",
                        1,
                        CancellationAction::Request,
                        "Rollback after a controlled late failure",
                    );
                    apply_quality_cancellation(
                        c,
                        &command,
                        &company(),
                        Some(&branch()),
                        &user("maker"),
                        1,
                    )
                    .await?;
                    Err(BusinessError::new(
                        FailureKind::Unavailable,
                        "QUALITY_TEST_LATE_FAILURE",
                        "Late failure after the Quality write.",
                    )
                    .into())
                })
            })
            .await;
        match result {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "QUALITY_TEST_LATE_FAILURE")
            }
            _ => panic!("expected the controlled late failure"),
        }
        let state = sqlx::query("SELECT status,version FROM inventory_quality_holds WHERE id='hold-rollback'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        use sqlx::Row;
        assert_eq!(state.get::<String, _>("status"), "held");
        assert_eq!(state.get::<i64, _>("version"), 1);
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM inventory_quality_cancellations WHERE hold_id='hold-rollback'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(count, 0);
    })
    .await;
}

#[tokio::test]
async fn cancellation_rejection_returns_to_held_and_keeps_the_hold_quarantined() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        facts::hold(&pool, "hold-a").await;
        let maker = actor("maker", &["quality.read", "quality.hold"]);
        cancel(
            &pool,
            maker,
            command(
                "quality-cancel-request",
                "hold-a",
                1,
                CancellationAction::Request,
                "Incorrect receiving hold",
            ),
        )
        .await
        .unwrap();
        let manager = actor(
            "manager",
            &["quality.read", "quality.inspect", "quality.release"],
        );
        let rejected = cancel(
            &pool,
            manager.clone(),
            command(
                "quality-cancel-reject",
                "hold-a",
                2,
                CancellationAction::Reject,
                "Verify supplier receipt before removing hold",
            ),
        )
        .await
        .unwrap();
        assert_eq!(rejected.status, HoldStatus::Held);
        assert_eq!(rejected.version, 3);
        let readback = read_detail_view(&pool, manager, "hold-a").await.unwrap();
        assert_eq!(readback.hold.hold.status, HoldStatus::Held);
        assert_eq!(readback.cancellations.len(), 2);
        assert!(readback.hold.decision.can_inspect);
    })
    .await;
}

#[tokio::test]
async fn quality_reads_require_an_owned_serializable_unit() {
    fixture::with_database(1, |pool| async move {
        facts::setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let error = read_quality_workspace(&mut connection, &company(), Some(&branch())).await;
        match error {
            Err(PgTransactionError::Persistence(error)) => {
                assert_eq!(error.code, "QUALITY_TRANSACTION_REQUIRED")
            }
            _ => panic!("expected the transaction requirement"),
        }
    })
    .await;
}

#[tokio::test]
async fn readback_uses_the_owner_transaction_and_persists_the_cancellation_event() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        facts::hold(&pool, "hold-a").await;
        let event = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_quality_read_sources(c).await?;
                    let command = command(
                        "quality-cancel-readback",
                        "hold-a",
                        1,
                        CancellationAction::Request,
                        "Read back the persisted cancellation",
                    );
                    let receipt = apply_quality_cancellation(
                        c,
                        &command,
                        &company(),
                        Some(&branch()),
                        &user("maker"),
                        7,
                    )
                    .await?;
                    let current = read_quality_detail(c, &company(), Some(&branch()), "hold-a")
                        .await?
                        .expect("hold detail");
                    assert_eq!(current.data().hold.status, HoldStatus::CancelPending);
                    assert_eq!(current.data().cancellations.len(), 1);
                    current.require_current(c).await?;
                    Ok(receipt)
                })
            })
            .await
            .unwrap();
        assert_eq!(event.status, HoldStatus::CancelPending);
        let row = sqlx::query("SELECT action,reason,actor_id,version FROM inventory_quality_cancellations WHERE id=$1")
            .bind(&event.cancellation_id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
        use sqlx::Row;
        assert_eq!(row.get::<String, _>("action"), "request");
        assert_eq!(row.get::<String, _>("reason"), "Read back the persisted cancellation");
        assert_eq!(row.get::<String, _>("actor_id"), "maker");
        assert_eq!(row.get::<i64, _>("version"), 2);
    })
    .await;
}
