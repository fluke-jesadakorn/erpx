#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_platform_administration::public::legal_release::EngineeringReadConfiguration;
use erpx_server_pms::public::initial_count_modules::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::CompanyId;
use sha2::{Digest, Sha256};
use std::{fs, time::Duration};
fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}
async fn setup(p: &DatabasePool) {
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(p, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('company','company','company','Company',NULL,'company',NULL,'active'); INSERT INTO company_feature_rollouts(company_id,module_id,rollout_state,reason,changed_by,changed_at) VALUES('company','inventory','pilot','fixture','user',1),('company','finance','pilot','fixture','user',1),('company','field_service','available','fixture','user',1)").execute(p.raw()).await.unwrap();
}
fn sealed_attestation(
    run: &str,
) -> (
    tempfile::TempDir,
    EngineeringReadConfiguration,
    String,
    String,
    String,
) {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join(".runtime/browser").join(run);
    fs::create_dir_all(dir.join("release")).unwrap();
    let fingerprint = "a".repeat(64);
    let artifact = format!("release-{fingerprint}");
    let bytes = b"[]";
    let manifest = hex::encode(Sha256::digest(bytes));
    fs::write(dir.join("release/release-files.json"), bytes).unwrap();
    fs::write(dir.join("release/release.json"),serde_json::to_vec(&serde_json::json!({"sourceFingerprint":fingerprint,"softwareVersion":artifact,"fileManifestSha256":manifest})).unwrap()).unwrap();
    fs::write(dir.join("engineering-attestation.json"),serde_json::to_vec(&serde_json::json!({"scope":"isolated-engineering","run":run,"sourceFingerprint":fingerprint,"softwareVersion":artifact,"fileManifestSha256":manifest})).unwrap()).unwrap();
    let config = EngineeringReadConfiguration {
        run: run.into(),
        environment: "local".into(),
        port: "3104".into(),
        browser_simulation_port: "3104".into(),
        data_directory: dir,
    };
    (root, config, artifact, fingerprint, manifest)
}
#[tokio::test]
async fn production_fails_closed_without_engineering_and_detects_rollout_drift() {
    fixture::with_database(2,|p|async move{setup(&p).await;let mut tx=p.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_initial_count_module_sources(&mut tx,InitialCountModuleMode::Production).await.unwrap();let witness=read_initial_count_modules(&mut tx,&company(),InitialCountModuleMode::Production,"unbuilt",None).await.unwrap();assert!(!witness.allowed(),"even available field_service is outside the production release scope");witness.require_current(&mut tx).await.unwrap();sqlx::query("UPDATE company_feature_rollouts SET rollout_state='off' WHERE company_id='company' AND module_id='inventory'").execute(&mut *tx).await.unwrap();assert!(matches!(witness.require_current(&mut tx).await,Err(PgTransactionError::Business(ref e)) if e.code=="INITIAL_COUNT_MODULE_SOURCE_CHANGED"));tx.rollback().await.unwrap();}).await;
}
#[tokio::test]
async fn real_engineering_row_and_clock_drift_are_witnessed_but_wrong_database_never_grants() {
    fixture::with_database(2,|p|async move{setup(&p).await;let (_tmp,config,artifact,fingerprint,manifest)=sealed_attestation("0123456789ab");let mut tx=p.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_initial_count_module_sources(&mut tx,InitialCountModuleMode::Production).await.unwrap();sqlx::query("INSERT INTO engineering_verification_contexts(company_id,run_id,source_fingerprint,software_version,release_manifest_sha256,enabled,expires_at,reason,created_at) VALUES('company',$1,$2,$3,$4,true,floor(extract(epoch FROM clock_timestamp())*1000)::bigint+10000,'synthetic fixture',1)").bind(&config.run).bind(&fingerprint).bind(&artifact).bind(&manifest).execute(&mut *tx).await.unwrap();let witness=read_initial_count_modules(&mut tx,&company(),InitialCountModuleMode::Production,&artifact,Some(&config)).await.unwrap();assert!(!witness.allowed(),"a leased non-browser DB cannot borrow engineering authority");witness.require_current(&mut tx).await.unwrap();sqlx::query("UPDATE engineering_verification_contexts SET enabled=false WHERE company_id='company'").execute(&mut *tx).await.unwrap();assert!(matches!(witness.require_current(&mut tx).await,Err(PgTransactionError::Business(ref e)) if e.code=="INITIAL_COUNT_MODULE_SOURCE_CHANGED"));sqlx::query("UPDATE engineering_verification_contexts SET enabled=true,expires_at=floor(extract(epoch FROM clock_timestamp())*1000)::bigint+1500 WHERE company_id='company'").execute(&mut *tx).await.unwrap();let soon=read_initial_count_modules(&mut tx,&company(),InitialCountModuleMode::Production,&artifact,Some(&config)).await.unwrap();tokio::time::sleep(Duration::from_millis(1800)).await;assert!(matches!(soon.require_current(&mut tx).await,Err(PgTransactionError::Business(ref e)) if e.code=="INITIAL_COUNT_MODULE_SOURCE_CHANGED"));let expired=read_initial_count_modules(&mut tx,&company(),InitialCountModuleMode::Production,&artifact,Some(&config)).await.unwrap();assert!(!expired.allowed());fs::write(config.data_directory.join("release/release-files.json"),b"tampered").unwrap();assert!(matches!(expired.require_current(&mut tx).await,Err(PgTransactionError::Business(ref e)) if e.code=="INITIAL_COUNT_MODULE_SOURCE_CHANGED"));tx.rollback().await.unwrap();}).await;
}
#[tokio::test]
async fn development_works_without_scheduled_schema_production_does_not() {
    fixture::with_database(2, |p| async move {
        setup(&p).await;
        let mut tx = p.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("DROP TABLE company_feature_rollouts")
            .execute(&mut *tx)
            .await
            .unwrap();
        lock_initial_count_module_sources(&mut tx, InitialCountModuleMode::Development)
            .await
            .unwrap();
        let source = read_initial_count_modules(
            &mut tx,
            &company(),
            InitialCountModuleMode::Development,
            "",
            None,
        )
        .await
        .unwrap();
        assert!(source.allowed());
        source.require_current(&mut tx).await.unwrap();
        assert!(
            lock_initial_count_module_sources(&mut tx, InitialCountModuleMode::Production)
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
    })
    .await;
}
#[test]
fn pure_triple_policy_development_and_production() {
    assert!(initial_count_modules_allowed(
        InitialCountModuleMode::Development,
        false
    ));
    assert!(!initial_count_modules_allowed(
        InitialCountModuleMode::Production,
        false
    ));
    assert!(initial_count_modules_allowed(
        InitialCountModuleMode::Production,
        true
    ));
}

// Only a freshly-created, commented lease bearing an exact browser database
// identity is touched. Never adopts or terminates another process's database.
#[derive(Clone, Copy)]
enum InjectFailure {
    None,
    BeforeLease,
    AfterLease,
    Body,
}
async fn with_browser_database<F, Fut>(failure: InjectFailure, test: F) -> bool
where
    F: FnOnce(DatabasePool, String) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    use erpx_server_runtime::public::postgres::{DatabaseConfig, PoolLimits, connect_pool};
    use sqlx::{AssertSqlSafe, Connection, PgConnection, Row, postgres::PgConnectOptions};
    use std::str::FromStr;
    let options = PgConnectOptions::from_str(
        &std::env::var("ERPX_TEST_ADMIN_URL").expect("test admin required"),
    )
    .unwrap();
    assert_eq!(options.get_host(), "127.0.0.1");
    assert_eq!(options.get_port(), 5432);
    assert_eq!(options.get_database(), Some("postgres"));
    assert_eq!(options.get_username(), "erpx_test_admin");
    let mut admin = PgConnection::connect_with(&options).await.unwrap();
    let role = sqlx::query(
        "SELECT rolsuper,rolcreatedb,rolcreaterole FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut admin)
    .await
    .unwrap();
    assert!(!role.get::<bool, _>("rolsuper"));
    assert!(role.get::<bool, _>("rolcreatedb"));
    assert!(!role.get::<bool, _>("rolcreaterole"));
    let token = uuid::Uuid::new_v4().simple().to_string();
    let run = token[..12].to_string();
    let name = format!("erpx_browser_{run}_h001_db");
    let lease = format!("erpx-pms-count-test:{token}");
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname=$1)")
            .bind(&name)
            .fetch_one(&mut admin)
            .await
            .unwrap();
    assert!(!exists, "never adopt an existing database");
    sqlx::raw_sql(AssertSqlSafe(format!(
        "CREATE DATABASE \"{name}\" TEMPLATE template0"
    )))
    .execute(&mut admin)
    .await
    .unwrap();
    // Every operation after successful CREATE, including lease setup and pool
    // construction, is caught before the single guarded cleanup path.
    let task_name = name.clone();
    let task_lease = lease.clone();
    let task_options = options.clone();
    let task=tokio::spawn(async move {
        if matches!(failure,InjectFailure::BeforeLease) {panic!("injected before lease");}
        let mut init=PgConnection::connect_with(&task_options).await.unwrap();
        sqlx::raw_sql(AssertSqlSafe(format!("COMMENT ON DATABASE \"{task_name}\" IS '{task_lease}'; REVOKE CONNECT ON DATABASE \"{task_name}\" FROM PUBLIC"))).execute(&mut init).await.unwrap();
        if matches!(failure,InjectFailure::AfterLease) {panic!("injected initialization failure");}
        let config=DatabaseConfig::from_options(task_options.database(&task_name)).unwrap();
        let pool=connect_pool(&config,PoolLimits {max_connections:3,acquire_timeout:Duration::from_secs(5)}).await.unwrap();
        let task_pool=pool.clone();
        let body=tokio::spawn(async move {if matches!(failure,InjectFailure::Body) {panic!("injected test failure");} test(task_pool,run).await}).await;
        pool.close().await; body.unwrap();
    }).await;
    let owned=sqlx::query("SELECT pg_get_userbyid(datdba)::text owner,shobj_description(oid,'pg_database') lease FROM pg_database WHERE datname=$1").bind(&name).fetch_one(&mut admin).await.unwrap();
    assert_eq!(owned.get::<String, _>("owner"), "erpx_test_admin");
    assert_eq!(name, format!("erpx_browser_{}_h001_db", &token[..12]));
    let present: Option<String> = owned.try_get("lease").unwrap();
    if present.is_none() {
        // CREATE succeeded here and the randomly generated database has no
        // other lease; install ours before the ordinary exact-token cleanup.
        sqlx::raw_sql(AssertSqlSafe(format!(
            "COMMENT ON DATABASE \"{name}\" IS '{lease}'"
        )))
        .execute(&mut admin)
        .await
        .unwrap();
    } else {
        assert_eq!(present.as_deref(), Some(lease.as_str()));
    }
    let current: Option<String> = sqlx::query_scalar(
        "SELECT shobj_description(oid,'pg_database') FROM pg_database WHERE datname=$1",
    )
    .bind(&name)
    .fetch_one(&mut admin)
    .await
    .unwrap();
    assert_eq!(current.as_deref(), Some(lease.as_str()));
    for attempt in 0..30 {
        match sqlx::raw_sql(AssertSqlSafe(format!("DROP DATABASE \"{name}\"")))
            .execute(&mut admin)
            .await
        {
            Ok(_) => break,
            Err(e)
                if e.as_database_error().and_then(|e| e.code()).as_deref() == Some("55006")
                    && attempt < 29 =>
            {
                tokio::time::sleep(Duration::from_millis(100)).await
            }
            Err(e) => panic!("leased test database cleanup failed: {e}"),
        }
    }
    let remains: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname=$1)")
            .bind(&name)
            .fetch_one(&mut admin)
            .await
            .unwrap();
    assert!(!remains, "owned database must be removed");
    task.is_ok()
}
#[tokio::test]
async fn browser_database_cleanup_covers_initialization_and_body_panics() {
    for failure in [
        InjectFailure::BeforeLease,
        InjectFailure::AfterLease,
        InjectFailure::Body,
    ] {
        assert!(!with_browser_database(failure, |_, _| async {}).await);
    }
}
#[tokio::test]
async fn real_browser_identity_and_attestation_admit_only_current_enabled_engineering() {
    assert!(with_browser_database(InjectFailure::None,|p,run|async move{
    setup(&p).await;let (_tmp,config,artifact,fingerprint,manifest)=sealed_attestation(&run);
    assert!(erpx_server_platform_administration::public::legal_release::engineering_attestation_matches(&config,&artifact));
    let mut tx=p.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_initial_count_module_sources(&mut tx,InitialCountModuleMode::Production).await.unwrap();
    let missing=read_initial_count_modules(&mut tx,&company(),InitialCountModuleMode::Production,&artifact,Some(&config)).await.unwrap();assert!(!missing.allowed());
    sqlx::query("INSERT INTO engineering_verification_contexts(company_id,run_id,source_fingerprint,software_version,release_manifest_sha256,enabled,expires_at,reason,created_at) VALUES('company',$1,$2,$3,$4,true,floor(extract(epoch FROM clock_timestamp())*1000)::bigint+10000,'synthetic verified fixture',4000000000000000)").bind(&run).bind(&fingerprint).bind(&artifact).bind(&manifest).execute(&mut *tx).await.unwrap();
    let active=read_initial_count_modules(&mut tx,&company(),InitialCountModuleMode::Production,&artifact,Some(&config)).await.unwrap();// The retained predicate does not treat created_at as a start boundary.
        assert!(active.allowed());active.require_current(&mut tx).await.unwrap();
    let legacy:bool=sqlx::query_scalar("SELECT current_database() IN ($1,$2,$3) AND EXISTS(SELECT 1 FROM engineering_verification_contexts WHERE company_id='company' AND run_id=$4 AND source_fingerprint=$5 AND software_version=$6 AND release_manifest_sha256=$7 AND enabled AND expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)").bind(format!("erpx_browser_{run}_h001_db")).bind(format!("erpx_browser_{run}_h002_db")).bind(format!("erpx_browser_{run}_h003_db")).bind(&run).bind(&fingerprint).bind(&artifact).bind(&manifest).fetch_one(&mut *tx).await.unwrap();assert_eq!(active.allowed(),legacy);
    sqlx::query("UPDATE engineering_verification_contexts SET enabled=false WHERE company_id='company'").execute(&mut *tx).await.unwrap();assert!(matches!(active.require_current(&mut tx).await,Err(PgTransactionError::Business(ref e)) if e.code=="INITIAL_COUNT_MODULE_SOURCE_CHANGED"));assert!(!read_initial_count_modules(&mut tx,&company(),InitialCountModuleMode::Production,&artifact,Some(&config)).await.unwrap().allowed());
    sqlx::query("UPDATE engineering_verification_contexts SET enabled=true,expires_at=floor(extract(epoch FROM clock_timestamp())*1000)::bigint+1500 WHERE company_id='company'").execute(&mut *tx).await.unwrap();let soon=read_initial_count_modules(&mut tx,&company(),InitialCountModuleMode::Production,&artifact,Some(&config)).await.unwrap();assert!(soon.allowed());tokio::time::sleep(Duration::from_millis(1800)).await;assert!(matches!(soon.require_current(&mut tx).await,Err(PgTransactionError::Business(ref e)) if e.code=="INITIAL_COUNT_MODULE_SOURCE_CHANGED"));assert!(!read_initial_count_modules(&mut tx,&company(),InitialCountModuleMode::Production,&artifact,Some(&config)).await.unwrap().allowed());
    tx.rollback().await.unwrap();
 }).await);
}
