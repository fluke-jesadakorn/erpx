//! Test-only adoption of the eight fresh leases owned by the engineering driver.
use super::facts::account;
use erpx_server_identity_access::public::UnavailableInvitationCredentialAuthority;
use erpx_server_runtime::public::postgres::{
    DatabasePool, LeasedDevelopmentDatabase, MigrationTrack, PoolLimits, load_migrations,
    migrate_database,
};
use sha2::{Digest, Sha256};
use sqlx::{ConnectOptions, Row, postgres::PgConnectOptions};
use std::{
    collections::HashSet,
    os::unix::fs::MetadataExt,
    str::FromStr,
    sync::{Arc, atomic::AtomicI64},
    time::Duration,
};

pub async fn open() -> (account::System, Vec<DatabasePool>) {
    let path = std::env::var("ERPX_FIELD_ENGINEERING_ROSTER").expect("engineering roster required");
    let metadata = std::fs::symlink_metadata(&path).expect("private roster required");
    let parent = std::path::Path::new(&path).parent().unwrap();
    let directory = std::fs::symlink_metadata(parent).unwrap();
    assert!(
        directory.is_dir() && directory.mode() & 0o077 == 0 && directory.uid() == metadata.uid()
    );
    assert_eq!(
        std::fs::canonicalize(&path).unwrap(),
        std::path::Path::new(&path)
    );
    assert!(metadata.is_file() && metadata.mode() & 0o777 == 0o600 && metadata.len() <= 16384);
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        std::env::var("ERPX_FIELD_ENGINEERING_ROSTER_SHA256").unwrap()
    );
    let roster: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for (field, variable) in [
        ("run", "ERPX_FIELD_ENGINEERING_RUN"),
        ("sourceFingerprint", "ERPX_FIELD_ENGINEERING_SOURCE"),
        ("fileManifestSha256", "ERPX_FIELD_ENGINEERING_MANIFEST"),
    ] {
        assert_eq!(
            roster[field].as_str(),
            Some(std::env::var(variable).unwrap().as_str())
        );
    }
    let entries = roster["entries"].as_array().expect("roster entries");
    let bindings = [
        "CONTROL_DB",
        "CONTROL_READ_DB",
        "H001_DB",
        "H001_READ_DB",
        "H002_DB",
        "H002_READ_DB",
        "H003_DB",
        "H003_READ_DB",
    ];
    assert_eq!(entries.len(), bindings.len());
    assert_eq!(roster["version"], 1);
    let start = roster["postmasterStartedMs"].as_i64().unwrap();
    let admin = PgConnectOptions::from_str(&std::env::var("ERPX_TEST_ADMIN_URL").unwrap()).unwrap();
    let mut pools = Vec::new();
    let mut tokens = HashSet::new();
    let mut oids = HashSet::new();
    for (entry, binding) in entries.iter().zip(bindings) {
        assert_eq!(entry["binding"], binding);
        let token = entry["token"].as_str().unwrap();
        let oid = entry["oid"].as_i64().unwrap();
        assert!(tokens.insert(token.to_owned()) && oids.insert(oid));
        let name = format!("erpx_dev_rust_{token}");
        let mut url = admin.clone().database(&name).to_url_lossy();
        url.set_query(None);
        let lease = LeasedDevelopmentDatabase::parse(url.as_str(), token).unwrap();
        let pool = lease
            .connect(PoolLimits {
                max_connections: 3,
                acquire_timeout: Duration::from_secs(5),
            })
            .await
            .unwrap();
        let row=sqlx::query("SELECT d.oid::bigint oid,floor(extract(epoch FROM pg_postmaster_start_time())*1000)::bigint started FROM pg_database d WHERE d.datname=current_database()").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(row.get::<i64, _>("oid"), oid);
        assert_eq!(row.get::<i64, _>("started"), start);
        pools.push(pool);
    }
    for (index, track) in [
        (1, MigrationTrack::ControlRead),
        (2, MigrationTrack::HoldingWrite),
        (3, MigrationTrack::HoldingRead),
        (4, MigrationTrack::HoldingWrite),
        (5, MigrationTrack::HoldingRead),
        (6, MigrationTrack::HoldingWrite),
        (7, MigrationTrack::HoldingRead),
    ] {
        let files = load_migrations(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
            track,
        )
        .await
        .unwrap();
        migrate_database(&pools[index], track, &files)
            .await
            .unwrap();
    }
    let holdings = [pools[2].clone(), pools[4].clone()];
    let clock = Arc::new(account::fixture::MutableClock(AtomicI64::new(
        account::fixture::account::NOW,
    )));
    let scenario = account::fixture::setup_with_clock_and_invalidation(
        &pools[0],
        Arc::new(UnavailableInvitationCredentialAuthority),
        clock.clone(),
        account::invalidator(&holdings, clock),
    )
    .await;
    sqlx::query("INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding) VALUES('h002','northstar','Synthetic Northstar','HOLDING_H002_DB','HOLDING_H002_READ_DB','HOLDING_H002_FILES')").execute(pools[0].raw()).await.unwrap();
    (
        account::System {
            control: pools[0].clone(),
            holdings,
            scenario,
        },
        pools,
    )
}
