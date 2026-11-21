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

struct Lease {
    name: String,
    lease: String,
    admin: PgConnection,
    admin_options: PgConnectOptions,
    pool: DatabasePool,
}

async fn create_lease(track: MigrationTrack, max_connections: u32) -> Lease {
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
            max_connections,
            acquire_timeout: Duration::from_secs(5),
        },
    )
    .await
    .unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(&root, track).await.unwrap();
    migrate_database(&pool, track, &migrations).await.unwrap();
    Lease {
        name,
        lease,
        admin,
        admin_options,
        pool,
    }
}

async fn release(lease: Lease) {
    let Lease {
        name,
        lease,
        mut admin,
        pool,
        ..
    } = lease;
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

async fn leased_database<F, Fut>(test: F, track: MigrationTrack)
where
    F: FnOnce(DatabasePool) -> Fut,
    Fut: Future<Output = ()>,
{
    let lease = create_lease(track, 2).await;
    test(lease.pool.clone()).await;
    release(lease).await;
}

async fn control_database<F, Fut>(test: F)
where
    F: FnOnce(DatabasePool) -> Fut,
    Fut: Future<Output = ()>,
{
    leased_database(test, MigrationTrack::Control).await
}

/// Control and Holding-write are separate databases in deployment. The
/// migration register keys receipts by file name, so one database cannot hold
/// both baseline tracks.
async fn onboarding_databases<F, Fut>(test: F)
where
    F: FnOnce(DatabasePool, DatabasePool) -> Fut,
    Fut: Future<Output = ()>,
{
    let control = create_lease(MigrationTrack::Control, 2).await;
    let holding = create_lease(MigrationTrack::HoldingWrite, 2).await;
    test(control.pool.clone(), holding.pool.clone()).await;
    release(holding).await;
    release(control).await;
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
    use erpx_server_manage_erpx::public::{
        lock_beta_policy_sources, require_beta_policy_sources_held,
    };
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
    use erpx_server_manage_erpx::public::{
        inspect_beta_policy_authority_boundary, lock_beta_policy_sources,
    };
    holding_fixture::with_database(2, |pool| async move {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let migrations = load_migrations(root, MigrationTrack::HoldingWrite)
            .await
            .unwrap();
        migrate_database(&pool, MigrationTrack::HoldingWrite, &migrations)
            .await
            .unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("SET LOCAL search_path TO pg_catalog, public, pg_temp")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(
            inspect_beta_policy_authority_boundary(&mut tx)
                .await
                .is_err()
        );
        sqlx::query("SAVEPOINT beta_authority")
            .execute(&mut *tx)
            .await
            .unwrap();
        lock_beta_policy_sources(&mut tx).await.unwrap();
        let released = inspect_beta_policy_authority_boundary(&mut tx)
            .await
            .unwrap();
        released.require_current(&mut tx).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT beta_authority")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(released.require_current(&mut tx).await.is_err());
        tx.rollback().await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("CREATE TEMP TABLE beta_policy(company_id text)")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("SET LOCAL search_path TO pg_catalog, public, pg_temp")
            .execute(&mut *tx)
            .await
            .unwrap();
        lock_beta_policy_sources(&mut tx).await.unwrap();
        let source = inspect_beta_policy_authority_boundary(&mut tx)
            .await
            .unwrap();
        source.require_current(&mut tx).await.unwrap();
        sqlx::query("SET LOCAL search_path TO pg_catalog, pg_temp, public")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(source.require_current(&mut tx).await.is_err());
        sqlx::query("SET LOCAL search_path TO pg_catalog, public, pg_temp")
            .execute(&mut *tx)
            .await
            .unwrap();
        source.require_current(&mut tx).await.unwrap();
        let mut other = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *other)
            .await
            .unwrap();
        assert!(source.require_current(&mut other).await.is_err());
        other.rollback().await.unwrap();
        tx.rollback().await.unwrap();
    })
    .await;
}

// --- Native onboarding command (POST) ---------------------------------------
//
// The command owns two caller transactions: the control plane and the selected
// Holding write database. These tests prove the actual writes, the single
// bootstrap receipt/audit/outbox, idempotent replay, waitlist/slug failures and
// the fail-closed transaction requirement.

use erpx_server_manage_erpx::public::onboarding_command::{
    OnboardingCommandActor, OnboardingCommandLimits, OnboardingCommandReceipt,
    OnboardingCommandResource, OnboardingCommandState, OnboardingReservation,
    PgOnboardingBootstrap, PgOnboardingControl, activate_onboarding, onboard_company,
    provision_onboarding, reserve_onboarding,
};

fn command_actor(verified: bool) -> OnboardingCommandActor {
    OnboardingCommandActor {
        user_id: UserId::new("owner").unwrap(),
        display_name: "Owner".into(),
        email: "owner@example.test".into(),
        email_verified: verified,
    }
}

fn command_resource() -> OnboardingCommandResource {
    OnboardingCommandResource {
        holding_id: "h001".into(),
        write_binding: "H001_DB".into(),
        read_binding: "H001_READ_DB".into(),
        files_binding: "H001_FILES".into(),
        beta: true,
    }
}

fn command_limits() -> OnboardingCommandLimits {
    OnboardingCommandLimits {
        members: 20,
        branches: 5,
        ai: 100,
        storage: 1_073_741_824,
    }
}

/// Mirror the retained commit order: commit the control reservation first so a
/// retry resumes the same slot, then commit the Holding bootstrap receipt, then
/// commit the idempotent control activation.
async fn command(
    control_pool: &DatabasePool,
    holding_pool: &DatabasePool,
    actor: &OnboardingCommandActor,
    company: &str,
    branch: &str,
    slug: &str,
) -> Result<OnboardingCommandReceipt, PgTransactionError<BusinessError>> {
    let mut control = control_pool.raw().begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *control)
        .await
        .unwrap();
    let reservation = {
        let mut repository = PgOnboardingControl::new(&mut control);
        reserve_onboarding(
            actor,
            true,
            company,
            branch,
            slug,
            &[command_resource()],
            command_limits(),
            1_700_000_000_000,
            &mut repository,
        )
        .await
    };
    let reservation = match reservation {
        Ok(reservation) => {
            control.commit().await.unwrap();
            reservation
        }
        Err(error) => {
            control.rollback().await.unwrap();
            return Err(error);
        }
    };
    let OnboardingReservation::Reserved { slot, resource } = reservation else {
        return Ok(OnboardingCommandReceipt::waitlisted());
    };

    let mut holding = holding_pool.raw().begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *holding)
        .await
        .unwrap();
    let receipt = {
        let mut repository = PgOnboardingBootstrap::new(&mut holding);
        provision_onboarding(
            actor,
            &slot,
            command_limits(),
            1_700_000_000_000,
            &mut repository,
        )
        .await
    };
    let receipt = match receipt {
        Ok(receipt) => {
            holding.commit().await.unwrap();
            receipt
        }
        Err(error) => {
            holding.rollback().await.unwrap();
            return Err(error);
        }
    };

    let mut control = control_pool.raw().begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *control)
        .await
        .unwrap();
    let activated = {
        let mut repository = PgOnboardingControl::new(&mut control);
        activate_onboarding(actor, &slot, &resource, 1_700_000_000_000, &mut repository).await
    };
    match activated {
        Ok(()) => {
            control.commit().await.unwrap();
            Ok(receipt)
        }
        Err(error) => {
            control.rollback().await.unwrap();
            Err(error)
        }
    }
}

async fn seed_command_owner(pool: &DatabasePool) {
    sqlx::query(
        "INSERT INTO \"user\"(id,name,email,\"emailVerified\",\"createdAt\",\"updatedAt\") \
VALUES('owner','Owner','owner@example.test',1,0,0)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn seed_available_slots(pool: &DatabasePool) {
    sqlx::query(
        "INSERT INTO beta_slots(holding_id,owner_user_id,slug,company_name,branch_name,state,\
created_at,updated_at) VALUES('h001',NULL,NULL,NULL,NULL,'available',0,0),\
('h002',NULL,NULL,NULL,NULL,'available',0,0)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn scalar_i64(pool: &DatabasePool, sql: &'static str) -> i64 {
    sqlx::query_scalar::<_, i64>(sql)
        .fetch_one(pool.raw())
        .await
        .unwrap()
}

#[test]
fn onboarding_command_provisions_once_and_replays_without_duplicate_evidence() {
    sqlx::test_block_on(onboarding_databases(|control, holding| async move {
        seed_command_owner(&control).await;
        seed_available_slots(&control).await;

        let receipt = command(
            &control,
            &holding,
            &command_actor(true),
            "Acme Company",
            "Main",
            "acme",
        )
        .await
        .unwrap();
        assert_eq!(receipt.state, OnboardingCommandState::Active);
        assert_eq!(receipt.href.as_deref(), Some("/holdings/acme/home"));

        let slot = sqlx::query(
            "SELECT owner_user_id,slug,company_name,branch_name,state FROM beta_slots \
WHERE holding_id='h001'",
        )
        .fetch_one(control.raw())
        .await
        .unwrap();
        assert_eq!(slot.get::<String, _>("owner_user_id"), "owner");
        assert_eq!(slot.get::<String, _>("slug"), "acme");
        assert_eq!(slot.get::<String, _>("company_name"), "Acme Company");
        assert_eq!(slot.get::<String, _>("branch_name"), "Main");
        assert_eq!(slot.get::<String, _>("state"), "active");

        // The unused reserved slot is untouched.
        let other =
            sqlx::query("SELECT owner_user_id,slug,state FROM beta_slots WHERE holding_id='h002'")
                .fetch_one(control.raw())
                .await
                .unwrap();
        assert!(other.get::<Option<String>, _>("owner_user_id").is_none());
        assert!(other.get::<Option<String>, _>("slug").is_none());
        assert_eq!(other.get::<String, _>("state"), "available");

        assert_eq!(
            scalar_i64(&control, "SELECT count(*) FROM beta_waitlist").await,
            0
        );
        assert_eq!(
            scalar_i64(
                &holding,
                "SELECT count(*) FROM beta_members WHERE user_id='owner'"
            )
            .await,
            1
        );

        let provisioning = sqlx::query(
            "SELECT status,version,owner_user_id FROM company_provisioning \
WHERE company_id='company-h001' AND holding_id='h001'",
        )
        .fetch_one(holding.raw())
        .await
        .unwrap();
        assert_eq!(provisioning.get::<String, _>("status"), "active");
        assert_eq!(provisioning.get::<i64, _>("version"), 2);
        assert_eq!(provisioning.get::<String, _>("owner_user_id"), "owner");
        assert_eq!(
            scalar_i64(
                &holding,
                "SELECT count(*) FROM company_provisioning_events WHERE company_id='company-h001'",
            )
            .await,
            2
        );

        assert_eq!(
            scalar_i64(
                &holding,
                "SELECT count(*) FROM beta_bootstrap WHERE holding_id='h001'"
            )
            .await,
            1
        );
        let policy = sqlx::query(
            "SELECT member_limit,branch_limit,ai_monthly_limit,storage_bytes FROM beta_policy \
WHERE company_id='company-h001'",
        )
        .fetch_one(holding.raw())
        .await
        .unwrap();
        assert_eq!(policy.get::<i64, _>("member_limit"), 20);
        assert_eq!(policy.get::<i64, _>("branch_limit"), 5);
        assert_eq!(policy.get::<i64, _>("ai_monthly_limit"), 100);
        assert_eq!(policy.get::<i64, _>("storage_bytes"), 1_073_741_824);

        assert_eq!(
            scalar_i64(
                &holding,
                "SELECT count(*) FROM organization_nodes WHERE company_id='company-h001'",
            )
            .await,
            3
        );
        assert_eq!(
            scalar_i64(
                &holding,
                "SELECT count(*) FROM staff_levels WHERE company_id='company-h001'",
            )
            .await,
            7
        );
        assert_eq!(
            scalar_i64(
                &holding,
                "SELECT count(*) FROM organization_positions WHERE company_id='company-h001' AND is_ceo=1",
            )
            .await,
            1
        );
        assert_eq!(
            scalar_i64(
                &holding,
                "SELECT count(*) FROM hr_people WHERE id='person-owner'"
            )
            .await,
            1
        );
        assert_eq!(
            scalar_i64(
                &holding,
                "SELECT count(*) FROM employees WHERE id='employee-owner'"
            )
            .await,
            1
        );
        assert_eq!(
            scalar_i64(
                &holding,
                "SELECT count(*) FROM hr_engagements WHERE id='engagement-owner'",
            )
            .await,
            1
        );
        assert_eq!(
            scalar_i64(
                &holding,
                "SELECT count(*) FROM assignments WHERE id='beta-assignment-owner'",
            )
            .await,
            1
        );

        assert_eq!(
            scalar_i64(
                &control,
                "SELECT count(*) FROM principals WHERE id='principal-owner'"
            )
            .await,
            1
        );
        let binding = sqlx::query(
            "SELECT slug,write_binding,read_binding,files_binding,status FROM holding_bindings \
WHERE holding_id='h001'",
        )
        .fetch_one(control.raw())
        .await
        .unwrap();
        assert_eq!(binding.get::<String, _>("slug"), "acme");
        assert_eq!(binding.get::<String, _>("write_binding"), "H001_DB");
        assert_eq!(binding.get::<String, _>("read_binding"), "H001_READ_DB");
        assert_eq!(binding.get::<String, _>("files_binding"), "H001_FILES");
        assert_eq!(binding.get::<String, _>("status"), "active");
        assert_eq!(
            scalar_i64(
                &control,
                "SELECT count(*) FROM beta_billing WHERE holding_id='h001'"
            )
            .await,
            1
        );

        assert_eq!(
            scalar_i64(&holding, "SELECT count(*) FROM command_assertions").await,
            0,
            "the transient bootstrap guard must be cleared"
        );
        assert_eq!(
            scalar_i64(&holding, "SELECT count(*) FROM command_receipts").await,
            1
        );
        assert_eq!(
            scalar_i64(&holding, "SELECT count(*) FROM audit_events").await,
            1
        );
        assert_eq!(
            scalar_i64(&holding, "SELECT count(*) FROM outbox_events").await,
            1
        );

        // Replay: the Holding receipt short-circuits the graph, and the control
        // activation remains idempotent. No duplicate events or evidence.
        let replay = command(
            &control,
            &holding,
            &command_actor(true),
            "Acme Company",
            "Main",
            "acme",
        )
        .await
        .unwrap();
        assert_eq!(replay, receipt);
        assert_eq!(
            scalar_i64(
                &holding,
                "SELECT count(*) FROM company_provisioning_events WHERE company_id='company-h001'",
            )
            .await,
            2
        );
        assert_eq!(
            scalar_i64(&holding, "SELECT count(*) FROM command_receipts").await,
            1
        );
        assert_eq!(
            scalar_i64(&holding, "SELECT count(*) FROM audit_events").await,
            1
        );
        assert_eq!(
            scalar_i64(&holding, "SELECT count(*) FROM outbox_events").await,
            1
        );
        assert_eq!(
            scalar_i64(
                &holding,
                "SELECT count(*) FROM organization_positions WHERE company_id='company-h001'",
            )
            .await,
            1
        );
    }));
}

#[test]
fn onboarding_command_waitlists_when_no_reserved_slot_is_available() {
    sqlx::test_block_on(onboarding_databases(|control, holding| async move {
        seed_command_owner(&control).await;
        let receipt = command(
            &control,
            &holding,
            &command_actor(true),
            "Acme Company",
            "Main",
            "acme",
        )
        .await
        .unwrap();
        assert_eq!(receipt, OnboardingCommandReceipt::waitlisted());
        assert_eq!(receipt.value()["href"], serde_json::Value::Null);
        assert_eq!(
            scalar_i64(
                &control,
                "SELECT count(*) FROM beta_waitlist WHERE user_id='owner'"
            )
            .await,
            1
        );
        assert_eq!(
            scalar_i64(&holding, "SELECT count(*) FROM company_provisioning").await,
            0
        );
        assert_eq!(
            scalar_i64(&holding, "SELECT count(*) FROM command_receipts").await,
            0
        );
    }));
}

#[test]
fn onboarding_command_rejects_a_bound_slug_and_a_mismatched_resume() {
    sqlx::test_block_on(onboarding_databases(|control, holding| async move {
        seed_command_owner(&control).await;
        seed_available_slots(&control).await;
        sqlx::query(
            "INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,\
files_binding,status) VALUES('h003','acme','Acme','H003_DB','H003_READ_DB','H003_FILES','active')",
        )
        .execute(control.raw())
        .await
        .unwrap();
        let error = command(
            &control,
            &holding,
            &command_actor(true),
            "Acme Company",
            "Main",
            "acme",
        )
        .await
        .unwrap_err();
        match error {
            PgTransactionError::Business(error) => {
                assert_eq!(error.kind, FailureKind::Conflict);
                assert_eq!(error.code, "SLUG_UNAVAILABLE");
            }
            _ => panic!("a bound slug must be a business conflict"),
        }
        assert_eq!(
            scalar_i64(
                &control,
                "SELECT count(*) FROM beta_slots WHERE owner_user_id IS NOT NULL",
            )
            .await,
            0
        );
        assert_eq!(
            scalar_i64(&control, "SELECT count(*) FROM beta_waitlist").await,
            0
        );

        sqlx::query(
            "UPDATE beta_slots SET owner_user_id='owner',slug='other',company_name='Other',\
branch_name='Other Main',state='provisioning' WHERE holding_id='h001'",
        )
        .execute(control.raw())
        .await
        .unwrap();
        let error = command(
            &control,
            &holding,
            &command_actor(true),
            "Acme Company",
            "Main",
            "acme",
        )
        .await
        .unwrap_err();
        match error {
            PgTransactionError::Business(error) => {
                assert_eq!(error.kind, FailureKind::Conflict);
                assert_eq!(error.code, "ONBOARDING_ALREADY_STARTED");
            }
            _ => panic!("different resume details must be a business conflict"),
        }
        assert_eq!(
            scalar_i64(&holding, "SELECT count(*) FROM command_receipts").await,
            0
        );
        assert_eq!(
            scalar_i64(&holding, "SELECT count(*) FROM company_provisioning").await,
            0
        );
    }));
}

#[test]
fn onboarding_command_requires_a_verified_actor_and_serializable_transactions() {
    sqlx::test_block_on(onboarding_databases(|control, holding| async move {
        seed_command_owner(&control).await;
        seed_available_slots(&control).await;

        let mut control_tx = control.raw().begin().await.unwrap();
        let mut holding_tx = holding.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *control_tx)
            .await
            .unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *holding_tx)
            .await
            .unwrap();
        let mut control_repo = PgOnboardingControl::new(&mut control_tx);
        let mut holding_repo = PgOnboardingBootstrap::new(&mut holding_tx);
        let denied = onboard_company(
            &command_actor(false),
            true,
            "Acme Company",
            "Main",
            "acme",
            &[command_resource()],
            command_limits(),
            1,
            &mut control_repo,
            &mut holding_repo,
        )
        .await
        .unwrap_err();
        match denied {
            PgTransactionError::Business(error) => {
                assert_eq!(error.kind, FailureKind::Forbidden);
                assert_eq!(error.code, "VERIFY_EMAIL");
            }
            _ => panic!("unverified actor must be a business denial"),
        }
        drop(control_repo);
        drop(holding_repo);
        holding_tx.rollback().await.unwrap();
        control_tx.rollback().await.unwrap();

        let mut control_conn = control.raw().acquire().await.unwrap();
        let mut holding_conn = holding.raw().acquire().await.unwrap();
        let mut control_repo = PgOnboardingControl::new(&mut control_conn);
        let mut holding_repo = PgOnboardingBootstrap::new(&mut holding_conn);
        let outside = onboard_company(
            &command_actor(true),
            true,
            "Acme Company",
            "Main",
            "acme",
            &[command_resource()],
            command_limits(),
            1,
            &mut control_repo,
            &mut holding_repo,
        )
        .await
        .unwrap_err();
        match outside {
            PgTransactionError::Persistence(error) => {
                assert_eq!(error.code, "ONBOARDING_CONTROL_TRANSACTION_REQUIRED")
            }
            _ => panic!("a missing transaction must fail closed"),
        }
        drop(control_repo);
        drop(holding_repo);
        drop(control_conn);
        drop(holding_conn);

        let mut holding_tx = holding.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *holding_tx)
            .await
            .unwrap();
        let mut control_tx = control.raw().begin().await.unwrap();
        let mut control_repo = PgOnboardingControl::new(&mut control_tx);
        let mut holding_repo = PgOnboardingBootstrap::new(&mut holding_tx);
        let weak = onboard_company(
            &command_actor(true),
            true,
            "Acme Company",
            "Main",
            "acme",
            &[command_resource()],
            command_limits(),
            1,
            &mut control_repo,
            &mut holding_repo,
        )
        .await
        .unwrap_err();
        match weak {
            PgTransactionError::Persistence(error) => {
                assert_eq!(error.code, "ONBOARDING_CONTROL_TRANSACTION_REQUIRED")
            }
            _ => panic!("read committed must fail closed"),
        }
        drop(control_repo);
        drop(holding_repo);
        control_tx.rollback().await.unwrap();
        holding_tx.rollback().await.unwrap();
        assert_eq!(
            scalar_i64(&holding, "SELECT count(*) FROM company_provisioning").await,
            0
        );
    }));
}
