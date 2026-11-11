#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_commercial::public::subscriptions::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgUnitOfWork, TransactionOptions, load_migrations,
    migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use sqlx::Connection;

fn company(value: &str) -> CompanyId {
    CompanyId::new(value).unwrap()
}

fn now(value: i64) -> TimestampMillis {
    TimestampMillis::new(value)
}

async fn insert_grant(
    connection: &mut sqlx::PgConnection,
    company: &str,
    state: &str,
    starts_at: i64,
    ends_at: Option<i64>,
    version: i64,
) {
    sqlx::query(
        "INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES($1,'legal',1,$2,$3,$4,$5,'Synthetic approval','PRIVATE_GRANT_EVIDENCE',1000)",
    )
    .bind(company)
    .bind(state)
    .bind(starts_at)
    .bind(ends_at)
    .bind(version)
    .execute(&mut *connection)
    .await
    .unwrap();
}

async fn update_grant(
    connection: &mut sqlx::PgConnection,
    company: &str,
    state: &str,
    version: i64,
) {
    sqlx::query(
        "UPDATE company_package_grants SET state=$2,version=$3,updated_at=1000 WHERE company_id=$1 AND package_id='legal'",
    )
    .bind(company)
    .bind(state)
    .bind(version)
    .execute(&mut *connection)
    .await
    .unwrap();
}

async fn insert_event(
    connection: &mut sqlx::PgConnection,
    company: &str,
    state: &str,
    starts_at: i64,
    ends_at: Option<i64>,
    version: i64,
    recorded_at: i64,
) {
    let after = serde_json::json!({
        "companyId": company,
        "packageId": "legal",
        "packageVersion": 1,
        "state": state,
        "startsAt": starts_at,
        "endsAt": ends_at,
        "version": version,
        "reason": "Synthetic approval",
        "evidence": "PRIVATE_GRANT_EVIDENCE",
    });
    sqlx::query(
        "INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) VALUES($1,$2,'legal','synthetic-actor','synthetic-hash',NULL,$3,$4)",
    )
    .bind(format!("event-{company}-{version}"))
    .bind(company)
    .bind(after)
    .bind(recorded_at)
    .execute(&mut *connection)
    .await
    .unwrap();
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
    let mut tx = pool.raw().begin().await.unwrap();
    insert_grant(&mut tx, "active", "active", 0, Some(3000), 1).await;
    insert_event(&mut tx, "active", "active", 0, Some(3000), 1, 100).await;
    insert_grant(&mut tx, "grace", "active", 0, Some(3000), 1).await;
    insert_event(&mut tx, "grace", "active", 0, Some(3000), 1, 100).await;
    // The retained audit trigger requires each event to match the current
    // grant at insert time, so build the transitions in version order.
    insert_grant(&mut tx, "expired", "draining", 0, Some(3000), 1).await;
    insert_event(&mut tx, "expired", "draining", 0, Some(3000), 1, 100).await;
    update_grant(&mut tx, "expired", "inactive", 2).await;
    insert_event(&mut tx, "expired", "inactive", 0, Some(3000), 2, 200).await;
    insert_grant(&mut tx, "recovered", "draining", 0, Some(3000), 1).await;
    insert_event(&mut tx, "recovered", "draining", 0, Some(3000), 1, 100).await;
    update_grant(&mut tx, "recovered", "active", 2).await;
    insert_event(&mut tx, "recovered", "active", 0, Some(3000), 2, 200).await;
    sqlx::query(
        "INSERT INTO company_package_history_rights(company_id,package_id,package_version,source_event_id,source_image,source_grant_version,retained_at) VALUES('active','legal',1,'event-active-1','after',1,50)",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

async fn read(pool: &DatabasePool, value: &str, at: i64) -> CompanySubscriptionRead {
    let mut connection = pool.raw().acquire().await.unwrap();
    let mut reader = PgCompanySubscriptionReader::new(company(value), &mut connection);
    reader.inspect(now(at)).await.unwrap()
}

#[tokio::test]
async fn postgres_reports_active_grace_expired_and_recovered_states() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;

        let active = read(&pool, "active", 1000).await;
        assert_eq!(active.company_id, "active");
        assert_eq!(active.revision.len(), 64);
        assert_eq!(active.observed_at, 1000);
        let subscription = &active.subscriptions[0];
        assert_eq!(subscription.state, SubscriptionState::Active);
        assert_eq!(subscription.renewal_at, Some(3000));
        assert!(subscription.grace.is_none());
        assert!(subscription.next_action.is_none());
        assert_eq!(active.history.len(), 1);
        assert!(!active.transitions.is_empty());

        let grace = read(&pool, "grace", 3000).await;
        let subscription = &grace.subscriptions[0];
        assert_eq!(subscription.state, SubscriptionState::Grace);
        assert_eq!(
            subscription.grace,
            Some(GraceWindow {
                starts_at: 3000,
                ends_at: None,
                remaining_ms: None,
            })
        );
        assert_eq!(subscription.next_action, Some(SubscriptionAction::Restore));

        let expired = read(&pool, "expired", 4000).await;
        assert_eq!(expired.subscriptions[0].state, SubscriptionState::Expired);
        assert!(expired.subscriptions[0].grace.is_none());
        assert_eq!(
            expired.subscriptions[0].next_action,
            Some(SubscriptionAction::Restore)
        );

        let recovered = read(&pool, "recovered", 1000).await;
        assert_eq!(
            recovered.subscriptions[0].state,
            SubscriptionState::Recovered
        );
        assert!(recovered.subscriptions[0].next_action.is_none());
        assert_eq!(recovered.transitions.len(), 2);
    })
    .await;
}

#[tokio::test]
async fn postgres_reader_is_company_scoped_and_uses_the_callers_connection() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        assert!(!connection.is_in_transaction());
        let mut reader = PgCompanySubscriptionReader::new(company("active"), &mut connection);
        let read = reader.inspect(now(1000)).await.unwrap();
        assert_eq!(read.company_id, "active");
        assert!(read.history.iter().all(|row| row.company_id == "active"));
        assert!(!connection.is_in_transaction());

        let mut foreign = PgCompanySubscriptionReader::new(company("none"), &mut connection);
        let read = foreign.inspect(now(1000)).await.unwrap();
        assert_eq!(read.company_id, "none");
        assert!(read.subscriptions.is_empty());
        assert!(read.history.is_empty());
        assert!(read.transitions.is_empty());
    })
    .await;
}

#[tokio::test]
async fn postgres_read_runs_inside_the_callers_owner_unit() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let read = PgUnitOfWork::new(pool.clone())
            .run::<CompanySubscriptionRead, BusinessError, _>(
                TransactionOptions::default(),
                |connection| {
                    Box::pin(async move {
                        let mut reader =
                            PgCompanySubscriptionReader::new(company("active"), connection);
                        reader.inspect(now(1000)).await
                    })
                },
            )
            .await
            .unwrap();
        assert_eq!(read.company_id, "active");
        assert_eq!(read.subscriptions[0].state, SubscriptionState::Active);
    })
    .await;
}
