#[path = "../../support/rust-postgres.rs"]
mod database;
use erpx_server_people_payroll::public::invitation_person_link::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, TimestampMillis, UserId};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn request(user: &str) -> InvitationPersonLinkRequest {
    InvitationPersonLinkRequest {
        company_id: CompanyId::new("company").unwrap(),
        person_id: RecordId::new("person").unwrap(),
        user_id: UserId::new(user).unwrap(),
        expected_person_version: 3,
        engagement_id: RecordId::new("engagement").unwrap(),
        expected_engagement_version: 7,
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: Some(RecordId::new("department").unwrap()),
        now: TimestampMillis::new(100),
    }
}
async fn setup(pool: &DatabasePool) {
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql("INSERT INTO hr_people(id,company_id,legal_name,country_code,version,created_by,created_at,updated_at) VALUES('person','company','Synthetic Invitee','TH',3,'fixture',0,0),('foreign','foreign','Synthetic Foreign','TH',1,'fixture',0,0);
      INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,version,created_by,created_at,updated_at) VALUES('engagement','person','company','branch','department','employee','salaried','active','2026-01-01','Synthetic Staff','THB',7,'fixture',0,0);
      INSERT INTO employees(id,company_id,branch_id,display_name,person_id,created_at) VALUES('employee','company','branch','Synthetic Employee','person',0)").execute(pool.raw()).await.unwrap();
}
async fn link(
    pool: &DatabasePool,
    input: InvitationPersonLinkRequest,
) -> Result<InvitationPersonLinkWitness, PgTransactionError<BusinessError>> {
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |connection| {
            let input = input.clone();
            Box::pin(async move {
                link_invited_person(&mut PgInvitationPersonLinker::new(connection), &input).await
            })
        })
        .await
}
async fn current(pool: &DatabasePool) -> (Option<String>, i64, Option<String>) {
    sqlx::query_as("SELECT p.user_id,p.version,e.user_id FROM hr_people p JOIN employees e ON e.person_id=p.id WHERE p.id='person'").fetch_one(pool.raw()).await.unwrap()
}
#[tokio::test]
async fn link_uses_caller_transaction_and_rolls_back_person_and_employee_effects_together() {
    database::with_database(1, |pool| async move {
        setup(&pool).await;
        let mut outside = pool.raw().acquire().await.unwrap();
        assert!(
            link_invited_person(
                &mut PgInvitationPersonLinker::new(&mut outside),
                &request("user")
            )
            .await
            .is_err()
        );
        drop(outside);
        let result = PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let witness = link_invited_person(
                        &mut PgInvitationPersonLinker::new(connection),
                        &request("user"),
                    )
                    .await?;
                    assert!(witness.changed);
                    assert_eq!(witness.person_version, 4);
                    Err(BusinessError::invalid(
                        "SYNTHETIC_ABORT",
                        "Abort the enclosing Identity command.",
                    )
                    .into())
                })
            })
            .await;
        assert!(result.is_err());
        assert_eq!(current(&pool).await, (None, 3, None));
        assert!(link(&pool, request("user")).await.unwrap().changed);
        assert!(!link(&pool, request("user")).await.unwrap().changed);
        assert_eq!(
            current(&pool).await,
            (Some("user".into()), 4, Some("user".into()))
        );
        assert!(link(&pool, request("steal")).await.is_err());
        assert_eq!(
            current(&pool).await,
            (Some("user".into()), 4, Some("user".into()))
        );
    })
    .await;
}
#[tokio::test]
async fn scoped_engagement_person_and_legacy_employee_conflicts_fail_without_partial_writes() {
    database::with_database(1, |pool| async move {
        setup(&pool).await;
        for case in 0..6 {
            let mut input = request("user");
            match case {
                0 => input.company_id = CompanyId::new("foreign").unwrap(),
                1 => input.expected_person_version = 2,
                2 => input.expected_engagement_version = 6,
                3 => input.branch_id = Some(RecordId::new("foreign").unwrap()),
                4 => input.department_id = Some(RecordId::new("foreign").unwrap()),
                5 => input.person_id = RecordId::new("foreign").unwrap(),
                _ => unreachable!(),
            }
            assert!(link(&pool, input).await.is_err());
            assert_eq!(current(&pool).await, (None, 3, None));
        }
        sqlx::query("UPDATE employees SET user_id='someone-else' WHERE id='employee'")
            .execute(pool.raw())
            .await
            .unwrap();
        assert!(link(&pool, request("user")).await.is_err());
        assert_eq!(current(&pool).await, (None, 3, Some("someone-else".into())));
    })
    .await;
}
#[tokio::test]
async fn competing_accounts_cannot_claim_the_same_person() {
    database::with_database(2, |pool| async move {
        setup(&pool).await;
        let (a, b) = tokio::join!(
            link(&pool, request("account-a")),
            link(&pool, request("account-b"))
        );
        assert_ne!(a.is_ok(), b.is_ok());
        let row = current(&pool).await;
        assert_eq!(row.1, 4);
        assert_eq!(row.0, row.2);
        assert!(matches!(row.0.as_deref(), Some("account-a" | "account-b")));
    })
    .await;
}
#[tokio::test]
async fn earlier_serializable_snapshot_retries_after_engagement_revocation_and_denies_the_link() {
    database::with_database(3, |pool| async move {
        setup(&pool).await;
        let ready = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let attempts = Arc::new(AtomicUsize::new(0));
        let reader_pool = pool.clone();
        let reader_ready = ready.clone();
        let reader_release = release.clone();
        let reader_attempts = attempts.clone();
        let reader = tokio::spawn(async move {
            PgUnitOfWork::new(reader_pool)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let ready = reader_ready.clone();
                    let release = reader_release.clone();
                    let attempts = reader_attempts.clone();
                    Box::pin(async move {
                        let attempt = attempts.fetch_add(1, Ordering::SeqCst);
                        if attempt == 0 {
                            sqlx::query("SELECT status FROM hr_engagements WHERE id='engagement'")
                                .fetch_one(&mut *connection)
                                .await?;
                            ready.notify_one();
                            release.notified().await;
                        }
                        link_invited_person(
                            &mut PgInvitationPersonLinker::new(connection),
                            &request("user"),
                        )
                        .await
                    })
                })
                .await
        });
        if tokio::time::timeout(std::time::Duration::from_secs(4), ready.notified())
            .await
            .is_err()
        {
            reader.abort();
            let _ = reader.await;
            panic!("reader did not establish its earlier source snapshot");
        }
        sqlx::query(
            "UPDATE hr_engagements SET status='suspended',version=version+1 WHERE id='engagement'",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        release.notify_one();
        assert!(reader.await.unwrap().is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert_eq!(current(&pool).await, (None, 3, None));
    })
    .await;
}
