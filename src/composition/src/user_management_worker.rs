//! Native user-management reconciliation worker.
//!
//! One bounded cycle for every Holding database in the reviewed jobs manifest
//! that owns an `um_attachments` row for the requested Company. The native
//! owner takes the Company advisory lock, rejects revoked/reviewed candidates,
//! invalidates ineligible access and proposes individually reviewed grants.
//! The TypeScript job process only spawns this worker; there is no TS fallback.
use erpx_server_identity_access::public::user_management::reconcile_management;
use erpx_server_runtime::public::postgres::{
    DatabaseConfig, DatabasePool, PgTransactionError, PoolLimits, connect_pool,
};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

async fn open(url: &str) -> Result<DatabasePool, &'static str> {
    let config =
        DatabaseConfig::from_url(url).map_err(|_| "USER_MANAGEMENT_DATABASE_INVALID")?;
    connect_pool(
        &config,
        PoolLimits {
            max_connections: 1,
            acquire_timeout: Duration::from_secs(5),
        },
    )
    .await
    .map_err(|_| "USER_MANAGEMENT_DATABASE_UNAVAILABLE")
}

/// CLI entry: `user-management-worker --once --database-manifest <path>
/// --company <id>`. The manifest is the reviewed jobs-role manifest already
/// used by the TypeScript job process.
pub async fn run_once_file(path: &Path, company: &str) -> Result<String, &'static str> {
    let document =
        std::fs::read_to_string(path).map_err(|_| "USER_MANAGEMENT_MANIFEST_UNREADABLE")?;
    let manifest: DatabaseManifest =
        serde_json::from_str(&document).map_err(|_| "USER_MANAGEMENT_MANIFEST_INVALID")?;
    if manifest.version != 1 {
        return Err("USER_MANAGEMENT_MANIFEST_VERSION");
    }
    let writes: BTreeSet<&str> = manifest
        .slots
        .iter()
        .map(|slot| slot.write.as_str())
        .collect();
    let mut applied = Vec::new();
    for write in writes {
        let Some(url) = manifest.databases.get(write) else {
            return Err("USER_MANAGEMENT_DATABASE_NOT_CONFIGURED");
        };
        let pool = open(url).await?;
        let outcome = reconcile_company(&pool, company, now()).await;
        pool.close().await;
        match outcome {
            Ok(true) => applied.push(write.to_owned()),
            Ok(false) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(serde_json::json!({
        "companies": if applied.is_empty() {
            Vec::<serde_json::Value>::new()
        } else {
            vec![serde_json::json!({
                "companyId": company,
                "databases": applied,
            })]
        }
    })
    .to_string())
}

async fn reconcile_company(
    pool: &DatabasePool,
    company: &str,
    now: i64,
) -> Result<bool, &'static str> {
    let mut connection = pool
        .raw()
        .acquire()
        .await
        .map_err(|_| "USER_MANAGEMENT_DATABASE_UNAVAILABLE")?;
    let relevant: Option<i32> =
        sqlx::query_scalar("SELECT 1 FROM um_attachments WHERE company_id=$1 LIMIT 1")
            .bind(company)
            .fetch_optional(&mut *connection)
            .await
            .map_err(|_| "USER_MANAGEMENT_PERSISTENCE_UNAVAILABLE")?;
    if relevant.is_none() {
        return Ok(false);
    }
    sqlx::query("BEGIN ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *connection)
        .await
        .map_err(|_| "USER_MANAGEMENT_PERSISTENCE_UNAVAILABLE")?;
    match reconcile_management(&mut connection, company, now).await {
        Ok(()) => {
            sqlx::query("COMMIT")
                .execute(&mut *connection)
                .await
                .map_err(|_| "USER_MANAGEMENT_PERSISTENCE_UNAVAILABLE")?;
            Ok(true)
        }
        Err(error) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
            Err(match error {
                PgTransactionError::Business(error) => error.code,
                PgTransactionError::Persistence(_) => "USER_MANAGEMENT_PERSISTENCE_UNAVAILABLE",
            })
        }
    }
}
