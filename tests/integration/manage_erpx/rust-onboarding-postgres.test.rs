//! Actual Manage ERPX onboarding-view reads on a leased control database.
//!
//! The test proves owner-user isolation, strict source validation, the retained
//! serializable caller transaction and persisted readback. It does not prove a
//! browser render or the onboarding command.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod holding_fixture;
use erpx_server_manage_erpx::public::onboarding::{
    OnboardingActor, OnboardingView, read_onboarding_view,
};
use erpx_server_runtime::public::postgres::{
    DatabaseConfig, DatabasePool, MigrationTrack, PgTransactionError, PoolLimits, connect_pool,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, FailureKind, UserId};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row, postgres::PgConnectOptions};
use std::{
    future::Future,
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn token() -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!(
        "{:016x}{:016x}",
        nanos,
        ((std::process::id() as u64) << 32) ^ sequence
    )
}

async fn control_database<F, Fut>(test: F)
where
    F: FnOnce(DatabasePool) -> Fut,
    Fut: Future<Output = ()>,
{
    let admin_url = std::env::var("ERPX_TEST_ADMIN_URL")
        .expect("ERPX_TEST_ADMIN_URL required for isolated native Postgres tests");
    let admin_options = PgConnectOptions::from_str(&admin_url).expect("invalid test admin URL");
    assert_eq!(admin_options.get_host(), "127.0.0.1");
    assert_eq!(admin_options.get_port(), 5432);
    assert_eq!(admin_options.get_database(), Some("postgres"));
    assert_eq!(admin_options.get_username(), "erpx_test_admin");
    let mut admin = PgConnection::connect_with(&admin_options)
        .await
        .expect("test administrator unavailable");
    let role = sqlx::query(
        "SELECT current_user::text AS role,rolsuper,rolcreatedb,rolcreaterole FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut admin)
    .await
    .unwrap();
    assert_eq!(role.get::<String, _>("role"), "erpx_test_admin");
    assert!(!role.get::<bool, _>("rolsuper"));
    assert!(role.get::<bool, _>("rolcreatedb"));
    assert!(!role.get::<bool, _>("rolcreaterole"));

    let token = token();
    let name = format!("erpx_dev_rust_{token}");
    let lease = format!("erpx-rust-test:{token}");
    sqlx::query(AssertSqlSafe(format!(
        "CREATE DATABASE \"{name}\" TEMPLATE template0"
    )))
    .execute(&mut admin)
    .await
    .unwrap();
    sqlx::query(AssertSqlSafe(format!(
        "REVOKE CONNECT ON DATABASE \"{name}\" FROM PUBLIC"
    )))
    .execute(&mut admin)
    .await
    .unwrap();
    sqlx::query(AssertSqlSafe(format!(
        "COMMENT ON DATABASE \"{name}\" IS '{lease}'"
    )))
    .execute(&mut admin)
    .await
    .unwrap();
    let config = DatabaseConfig::from_options(admin_options.clone().database(&name)).unwrap();
    let pool = connect_pool(
        &config,
        PoolLimits {
            max_connections: 2,
            acquire_timeout: Duration::from_secs(5),
        },
    )
    .await
    .unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(&root, MigrationTrack::Control)
        .await
        .unwrap();
    migrate_database(&pool, MigrationTrack::Control, &migrations)
        .await
        .unwrap();

    test(pool.clone()).await;
    pool.close().await;

    let ownership = sqlx::query(
        "SELECT pg_get_userbyid(datdba)::text owner,shobj_description(oid,'pg_database') lease FROM pg_database WHERE datname=$1",
    )
    .bind(&name)
    .fetch_one(&mut admin)
    .await
    .unwrap();
    assert!(name.starts_with("erpx_dev_rust_"));
    assert_eq!(ownership.get::<String, _>("owner"), "erpx_test_admin");
    assert_eq!(ownership.get::<String, _>("lease"), lease);
    for attempt in 0..30 {
        match sqlx::query(AssertSqlSafe(format!("DROP DATABASE \"{name}\"")))
            .execute(&mut admin)
            .await
        {
            Ok(_) => break,
            Err(error)
                if error.as_database_error().and_then(|e| e.code()).as_deref() == Some("55006")
                    && attempt < 29 =>
            {
                std::thread::sleep(Duration::from_millis(100))
            }
            Err(error) => panic!(
                "owned test database cleanup failed (SQLSTATE {:?})",
                error.as_database_error().and_then(|e| e.code())
            ),
        }
    }
}

fn actor(user_id: &str) -> OnboardingActor {
    OnboardingActor {
        user_id: UserId::new(user_id).unwrap(),
        email_verified: true,
    }
}

async fn read(
    pool: &DatabasePool,
    actor: &OnboardingActor,
    enabled: bool,
) -> Result<OnboardingView, PgTransactionError<BusinessError>> {
    let mut transaction = pool.raw().begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *transaction)
        .await
        .unwrap();
    let result = read_onboarding_view(&mut transaction, actor, enabled).await;
    transaction.rollback().await.unwrap();
    result
}

async fn seed(pool: &DatabasePool) {
    sqlx::query(
        "INSERT INTO \"user\"(id,name,email,\"emailVerified\",\"createdAt\",\"updatedAt\") VALUES('owner','Owner','owner@example.test',1,0,0),('other','Other','other@example.test',1,0,0),('long','Long','long@example.test',1,0,0)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO beta_slots(holding_id,owner_user_id,slug,company_name,branch_name,state,created_at,updated_at) VALUES('h001','owner','acme','Acme Company','Main','provisioning',0,0),('h002','other','other','Other Company','Other Main','active',0,0)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[test]
fn onboarding_view_is_owner_scoped_strictly_validated_and_persisted() {
    sqlx::test_block_on(control_database(|pool| async move {
        seed(&pool).await;
        let view = read(&pool, &actor("owner"), true).await.unwrap();
        assert!(view.enabled);
        let slot = view.slot.expect("owner slot");
        assert_eq!(slot.slug, "acme");
        assert_eq!(slot.company_name, "Acme Company");
        assert_eq!(slot.branch_name, "Main");
        assert_eq!(
            slot.state,
            erpx_server_manage_erpx::public::onboarding::CompanySlotState::Provisioning
        );

        let closed = read(&pool, &actor("owner"), false).await.unwrap();
        assert!(!closed.enabled);
        assert_eq!(
            closed.slot.unwrap().slug,
            "acme",
            "registration flag does not hide the owner slot"
        );

        let other = read(&pool, &actor("other"), true).await.unwrap();
        assert_eq!(other.slot.unwrap().slug, "other");
        assert!(
            read(&pool, &actor("unknown"), true)
                .await
                .unwrap()
                .slot
                .is_none()
        );

        sqlx::query(
            "UPDATE beta_slots SET state='active',updated_at=1 WHERE owner_user_id='owner'",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let active = read(&pool, &actor("owner"), true).await.unwrap();
        assert_eq!(
            active.slot.unwrap().state,
            erpx_server_manage_erpx::public::onboarding::CompanySlotState::Active
        );

        sqlx::query(
            "INSERT INTO beta_slots(holding_id,owner_user_id,slug,company_name,branch_name,state,created_at,updated_at) VALUES('h003','long','long',$1,'Main','provisioning',0,0)",
        )
        .bind("X".repeat(201))
        .execute(pool.raw())
        .await
        .unwrap();
        let error = read(&pool, &actor("long"), true).await.unwrap_err();
        match error {
            PgTransactionError::Business(error) => {
                assert_eq!(error.kind, FailureKind::Unavailable);
                assert_eq!(error.code, "ONBOARDING_SOURCE_INVALID");
            }
            _ => panic!("malformed source must be an owner business failure"),
        }

        let mut connection = pool.raw().acquire().await.unwrap();
        let error = read_onboarding_view(&mut connection, &actor("owner"), true)
            .await
            .unwrap_err();
        match error {
            PgTransactionError::Persistence(error) => {
                assert_eq!(error.code, "ONBOARDING_READ_TRANSACTION_REQUIRED")
            }
            _ => panic!("missing transaction must be a persistence failure"),
        }
        drop(connection);

        let mut read_committed = pool.raw().begin().await.unwrap();
        let error = read_onboarding_view(&mut read_committed, &actor("owner"), true)
            .await
            .unwrap_err();
        match error {
            PgTransactionError::Persistence(error) => {
                assert_eq!(error.code, "ONBOARDING_READ_SERIALIZABLE_REQUIRED")
            }
            _ => panic!("read committed must fail closed"),
        }
        read_committed.rollback().await.unwrap();

        let denied = read(
            &pool,
            &OnboardingActor {
                user_id: UserId::new("owner").unwrap(),
                email_verified: false,
            },
            true,
        )
        .await
        .unwrap_err();
        match denied {
            PgTransactionError::Business(error) => {
                assert_eq!(error.kind, FailureKind::Forbidden);
                assert_eq!(error.code, "VERIFY_EMAIL");
            }
            _ => panic!("unverified owner must be denied"),
        }
    }));
}

#[tokio::test]
async fn beta_policy_held_lock_rejects_weak_and_released_fences() {
    use erpx_server_manage_erpx::public::{lock_beta_policy_sources,require_beta_policy_sources_held};
    holding_fixture::with_database(2, |pool| async move {
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let migrations=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&migrations).await.unwrap();
        let mut outside=pool.raw().acquire().await.unwrap();assert!(require_beta_policy_sources_held(&mut outside).await.is_err());drop(outside);
        let mut weak=pool.raw().begin().await.unwrap();sqlx::query("LOCK TABLE beta_policy IN ROW SHARE MODE").execute(&mut *weak).await.unwrap();assert!(matches!(require_beta_policy_sources_held(&mut weak).await,Err(PgTransactionError::Persistence(ref e)) if e.code=="BETA_POLICY_LOCK_REQUIRED"));weak.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();sqlx::query("SELECT pg_current_xact_id()").execute(&mut *tx).await.unwrap();sqlx::query("SAVEPOINT before_beta_lock").execute(&mut *tx).await.unwrap();
        lock_beta_policy_sources(&mut tx).await.unwrap();require_beta_policy_sources_held(&mut tx).await.unwrap();let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND relation='beta_policy'::regclass AND mode='ShareLock' AND granted)").fetch_one(&mut *tx).await.unwrap();assert!(held);
        sqlx::query("ROLLBACK TO SAVEPOINT before_beta_lock").execute(&mut *tx).await.unwrap();assert!(matches!(require_beta_policy_sources_held(&mut tx).await,Err(PgTransactionError::Persistence(ref e)) if e.code=="BETA_POLICY_LOCK_REQUIRED"));
        sqlx::query("LOCK TABLE beta_policy IN SHARE ROW EXCLUSIVE MODE").execute(&mut *tx).await.unwrap();require_beta_policy_sources_held(&mut tx).await.unwrap();tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn beta_policy_authority_boundary_checks_canonical_source_and_unit() {
    use erpx_server_manage_erpx::public::{lock_beta_policy_sources,inspect_beta_policy_authority_boundary};
    holding_fixture::with_database(2,|pool|async move {
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let migrations=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool,MigrationTrack::HoldingWrite,&migrations).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::query("SET LOCAL search_path TO pg_catalog, public, pg_temp").execute(&mut *tx).await.unwrap();
        assert!(inspect_beta_policy_authority_boundary(&mut tx).await.is_err());
        sqlx::query("SAVEPOINT beta_authority").execute(&mut *tx).await.unwrap();
        lock_beta_policy_sources(&mut tx).await.unwrap();
        let released=inspect_beta_policy_authority_boundary(&mut tx).await.unwrap();
        released.require_current(&mut tx).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT beta_authority").execute(&mut *tx).await.unwrap();
        assert!(released.require_current(&mut tx).await.is_err());tx.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::query("CREATE TEMP TABLE beta_policy(company_id text)").execute(&mut *tx).await.unwrap();
        sqlx::query("SET LOCAL search_path TO pg_catalog, public, pg_temp").execute(&mut *tx).await.unwrap();
        lock_beta_policy_sources(&mut tx).await.unwrap();
        let source=inspect_beta_policy_authority_boundary(&mut tx).await.unwrap();
        source.require_current(&mut tx).await.unwrap();
        sqlx::query("SET LOCAL search_path TO pg_catalog, pg_temp, public").execute(&mut *tx).await.unwrap();
        assert!(source.require_current(&mut tx).await.is_err());
        sqlx::query("SET LOCAL search_path TO pg_catalog, public, pg_temp").execute(&mut *tx).await.unwrap();
        source.require_current(&mut tx).await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        assert!(source.require_current(&mut other).await.is_err());
        other.rollback().await.unwrap();tx.rollback().await.unwrap();
    }).await;
}
