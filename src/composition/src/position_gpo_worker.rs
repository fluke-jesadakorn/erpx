//! Native Position GPO delivery worker.
//!
//! One bounded cycle per exact Holding database. The worker takes the same
//! catalog and per-connection advisory locks the retained TypeScript drain took,
//! then runs the native owner drain inside one serializable Holding unit. The
//! retained TypeScript job process must invoke this worker and fails closed
//! without it; there is no TypeScript delivery fallback.
use erpx_server_identity_access::public::position_gpo::{
    PgPositionGpo, PgPositionGpoDirectory, ReqwestConnector, drain_position_gpo_locked,
};
use erpx_server_runtime::public::postgres::{
    DatabaseConfig, DatabasePool, PgTransactionError, PoolLimits, connect_pool,
};
use erpx_server_shared::public::BusinessError;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const CATALOG_LOCK: i64 = 821_039_320;
const MAX_CYCLE: i64 = 10;

#[derive(Deserialize)]
struct DatabaseManifest {
    version: u8,
    databases: BTreeMap<String, String>,
    slots: Vec<DatabaseManifestSlot>,
}

#[derive(Deserialize)]
struct DatabaseManifestSlot {
    id: String,
    write: String,
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn holding_url<'a>(manifest: &'a DatabaseManifest, holding: &str) -> Result<&'a str, &'static str> {
    let slot = manifest
        .slots
        .iter()
        .find(|slot| slot.id == holding)
        .ok_or("POSITION_GPO_HOLDING_NOT_FOUND")?;
    manifest
        .databases
        .get(&slot.write)
        .map(String::as_str)
        .ok_or("POSITION_GPO_DATABASE_NOT_CONFIGURED")
}

async fn open(url: &str) -> Result<DatabasePool, &'static str> {
    let config = DatabaseConfig::from_url(url).map_err(|_| "POSITION_GPO_DATABASE_INVALID")?;
    connect_pool(
        &config,
        PoolLimits {
            max_connections: 1,
            acquire_timeout: Duration::from_secs(5),
        },
    )
    .await
    .map_err(|_| "POSITION_GPO_DATABASE_UNAVAILABLE")
}

fn profile_path() -> Result<std::path::PathBuf, &'static str> {
    let value =
        std::env::var("ERPX_IAM_SECRET_PROFILES").map_err(|_| "POSITION_GPO_CONNECTOR_REQUIRED")?;
    let path = std::path::PathBuf::from(value);
    if !path.is_absolute() {
        return Err("POSITION_GPO_CONNECTOR_REQUIRED");
    }
    Ok(path)
}

/// CLI entry: `position-gpo-worker --once --database-manifest <path> --holding
/// <id>`. The manifest is the same reviewed jobs-role manifest the TypeScript
/// job process already uses.
pub async fn run_once_file(path: &Path, holding: &str) -> Result<String, &'static str> {
    let document = std::fs::read_to_string(path).map_err(|_| "POSITION_GPO_MANIFEST_UNREADABLE")?;
    let manifest: DatabaseManifest =
        serde_json::from_str(&document).map_err(|_| "POSITION_GPO_MANIFEST_INVALID")?;
    if manifest.version != 1 {
        return Err("POSITION_GPO_MANIFEST_VERSION");
    }
    let profile = profile_path()?;
    let url = holding_url(&manifest, holding)?;
    let pool = open(url).await?;
    let outcome = run_once(&pool, &profile, holding, now()).await;
    pool.close().await;
    outcome
}

async fn run_once(
    pool: &DatabasePool,
    profile: &Path,
    holding: &str,
    now: i64,
) -> Result<String, &'static str> {
    let mut connection = pool
        .raw()
        .acquire()
        .await
        .map_err(|_| "POSITION_GPO_DATABASE_UNAVAILABLE")?;
    let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(CATALOG_LOCK)
        .fetch_one(&mut *connection)
        .await
        .map_err(|_| "POSITION_GPO_PERSISTENCE_UNAVAILABLE")?;
    if !locked {
        return Ok(serde_json::json!({
            "holdings": [{"holdingId": holding, "applied": false, "reason": "catalog-lock"}]
        })
        .to_string());
    }
    let relevant: Vec<String> = sqlx::query_scalar(
        "SELECT connection_id FROM organization_gpo_rules WHERE active \
         UNION SELECT connection_id FROM position_gpo_config_ownership \
         UNION SELECT connection_id FROM organization_gpo_membership_claims \
         UNION SELECT connection_id FROM position_gpo_jobs \
           WHERE status IN ('queued','retry','running') \
         ORDER BY connection_id",
    )
    .fetch_all(&mut *connection)
    .await
    .map_err(|_| "POSITION_GPO_PERSISTENCE_UNAVAILABLE")?;
    let mut acquired: Vec<String> = Vec::new();
    for connection_id in relevant {
        let key = format!("people-directory:{connection_id}");
        let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtext($1))")
            .bind(&key)
            .fetch_one(&mut *connection)
            .await
            .map_err(|_| "POSITION_GPO_PERSISTENCE_UNAVAILABLE")?;
        if !locked {
            release(&mut connection, &acquired, true).await;
            return Ok(serde_json::json!({
                "holdings": [{"holdingId": holding, "applied": false, "reason": "provider-lock"}]
            })
            .to_string());
        }
        acquired.push(key);
    }
    if sqlx::query("BEGIN")
        .execute(&mut *connection)
        .await
        .is_err()
    {
        release(&mut connection, &acquired, true).await;
        return Err("POSITION_GPO_PERSISTENCE_UNAVAILABLE");
    }
    let result = drain(&mut connection, profile, now).await;
    let applied = match result {
        Ok(()) => {
            sqlx::query("COMMIT")
                .execute(&mut *connection)
                .await
                .map_err(|_| "POSITION_GPO_PERSISTENCE_UNAVAILABLE")?;
            true
        }
        Err(error) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
            release(&mut connection, &acquired, true).await;
            return Err(match error {
                PgTransactionError::Business(error) => error.code,
                PgTransactionError::Persistence(_) => "POSITION_GPO_PERSISTENCE_UNAVAILABLE",
            });
        }
    };
    release(&mut connection, &acquired, true).await;
    Ok(serde_json::json!({
        "holdings": [{"holdingId": holding, "applied": applied}]
    })
    .to_string())
}

async fn drain(
    connection: &mut sqlx::PgConnection,
    profile: &Path,
    now: i64,
) -> Result<(), PgTransactionError<BusinessError>> {
    let mut directory = PgPositionGpoDirectory::new(profile, ReqwestConnector);
    let mut repository = PgPositionGpo::new(&mut *connection);
    drain_position_gpo_locked(&mut repository, &mut directory, MAX_CYCLE, now).await
}

async fn release(connection: &mut sqlx::PgConnection, acquired: &[String], catalog: bool) {
    for key in acquired.iter().rev() {
        let _ = sqlx::query("SELECT pg_advisory_unlock(hashtext($1))")
            .bind(key)
            .execute(&mut *connection)
            .await;
    }
    if catalog {
        let _: Result<bool, _> = sqlx::query_scalar("SELECT pg_advisory_unlock($1)")
            .bind(CATALOG_LOCK)
            .fetch_one(&mut *connection)
            .await;
    }
}
