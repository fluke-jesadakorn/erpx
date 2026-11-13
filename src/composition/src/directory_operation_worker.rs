//! Native Directory operation drain worker.
//!
//! One bounded cycle for every Holding write database in the reviewed jobs
//! manifest that owns Directory work for the requested Company. The native
//! owner takes the connection advisory lock, rechecks current authority and
//! connector state, performs the installed-connector effect and verifies the
//! readback. The TypeScript job process only spawns this worker.
use erpx_server_identity_access::public::account_security::MFA_FRESH_MS;
use erpx_server_identity_access::public::directory_operation::{
    DirectoryDirectFreshness, DirectoryOperationFuture, drain_operations,
};
use erpx_server_identity_access::public::position_gpo::{
    PgPositionGpoDirectory, ReqwestConnector,
};
use erpx_server_runtime::public::postgres::{
    DatabaseConfig, DatabasePool, PoolLimits, connect_pool,
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

struct ControlFreshness {
    control: Option<DatabasePool>,
}

impl DirectoryDirectFreshness for ControlFreshness {
    fn fresh<'a>(
        &'a self,
        user_id: &'a str,
        session_id: &'a str,
        now: i64,
    ) -> DirectoryOperationFuture<'a, bool> {
        Box::pin(async move {
            let Some(control) = &self.control else {
                return Ok(false);
            };
            let mut connection = control
                .raw()
                .acquire()
                .await
                .map_err(|_| unavailable())?;
            let fresh: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM iam_mfa_sessions f \
                 JOIN iam_mfa m ON m.user_id=f.user_id AND m.enabled=true \
                 JOIN session s ON s.id=f.session_id AND s.\"userId\"=f.user_id \
                 WHERE f.user_id=$1 AND f.session_id=$2 AND f.verified_at>$3 \
                 AND s.\"expiresAt\"::timestamptz>to_timestamp($4::double precision/1000.0))",
            )
            .bind(user_id)
            .bind(session_id)
            .bind(now - MFA_FRESH_MS)
            .bind(now as f64)
            .fetch_one(&mut *connection)
            .await
            .unwrap_or(false);
            Ok(fresh)
        })
    }
}

fn unavailable() -> erpx_server_shared::public::BusinessError {
    erpx_server_shared::public::BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "DIRECTORY_UNAVAILABLE",
        "The approved directory endpoint is unavailable.",
    )
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

async fn open(url: &str) -> Result<DatabasePool, &'static str> {
    let config = DatabaseConfig::from_url(url)
        .map_err(|_| "DIRECTORY_OPERATION_DATABASE_INVALID")?;
    connect_pool(
        &config,
        PoolLimits {
            max_connections: 1,
            acquire_timeout: Duration::from_secs(5),
        },
    )
    .await
    .map_err(|_| "DIRECTORY_OPERATION_DATABASE_UNAVAILABLE")
}

/// CLI entry: `directory-operation-worker --once --database-manifest <path>
/// --company <id>`. The manifest is the reviewed jobs-role manifest already
/// used by the TypeScript job process.
pub async fn run_once_file(path: &Path, company: &str) -> Result<String, &'static str> {
    let document = std::fs::read_to_string(path)
        .map_err(|_| "DIRECTORY_OPERATION_MANIFEST_UNREADABLE")?;
    let manifest: DatabaseManifest = serde_json::from_str(&document)
        .map_err(|_| "DIRECTORY_OPERATION_MANIFEST_INVALID")?;
    if manifest.version != 1 {
        return Err("DIRECTORY_OPERATION_MANIFEST_VERSION");
    }
    let profile = std::env::var("ERPX_IAM_SECRET_PROFILES")
        .map(std::path::PathBuf::from)
        .map_err(|_| "DIRECTORY_OPERATION_PROFILE_REQUIRED")?;
    let control_url = manifest
        .slots
        .iter()
        .find(|slot| slot.id.eq_ignore_ascii_case("CONTROL_DB"))
        .and_then(|slot| manifest.databases.get(&slot.write))
        .or_else(|| {
            manifest
                .databases
                .iter()
                .find(|(id, _)| id.to_ascii_lowercase().contains("control"))
                .map(|(_, url)| url)
        });
    let control = match control_url {
        Some(url) => Some(open(url).await?),
        None => None,
    };
    let freshness = ControlFreshness {
        control: control.clone(),
    };
    let writes: BTreeSet<&str> = manifest
        .slots
        .iter()
        .map(|slot| slot.write.as_str())
        .collect();
    let mut applied = Vec::new();
    for write in writes {
        let Some(url) = manifest.databases.get(write) else {
            return Err("DIRECTORY_OPERATION_DATABASE_NOT_CONFIGURED");
        };
        let pool = open(url).await?;
        let outcome = drain_company(&pool, &profile, &freshness, company, now()).await;
        pool.close().await;
        match outcome {
            Ok(Some(completed)) => applied.push((write.to_owned(), completed)),
            Ok(None) => {}
            Err(error) => return Err(error),
        }
    }
    if let Some(control) = control {
        control.close().await;
    }
    Ok(serde_json::json!({
        "companies": if applied.is_empty() {
            Vec::<serde_json::Value>::new()
        } else {
            vec![serde_json::json!({
                "companyId": company,
                "databases": applied.iter().map(|(database, _)| database).collect::<Vec<_>>(),
                "completed": applied.iter().map(|(_, completed)| *completed).sum::<usize>(),
            })]
        }
    })
    .to_string())
}

async fn drain_company(
    pool: &DatabasePool,
    profile: &Path,
    freshness: &ControlFreshness,
    company: &str,
    now: i64,
) -> Result<Option<usize>, &'static str> {
    let mut connection = pool
        .raw()
        .acquire()
        .await
        .map_err(|_| "DIRECTORY_OPERATION_DATABASE_UNAVAILABLE")?;
    let relevant: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM people_directory_operations WHERE company_id=$1 LIMIT 1",
    )
    .bind(company)
    .fetch_optional(&mut *connection)
    .await
    .map_err(|_| "DIRECTORY_OPERATION_PERSISTENCE_UNAVAILABLE")?;
    if relevant.is_none() {
        return Ok(None);
    }
    sqlx::query("BEGIN ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *connection)
        .await
        .map_err(|_| "DIRECTORY_OPERATION_PERSISTENCE_UNAVAILABLE")?;
    let connector = PgPositionGpoDirectory::new(profile, ReqwestConnector);
    match drain_operations(
        &mut connection,
        &connector,
        freshness,
        Some(company),
        now,
        25,
    )
    .await
    {
        Ok(completed) => {
            sqlx::query("COMMIT")
                .execute(&mut *connection)
                .await
                .map_err(|_| "DIRECTORY_OPERATION_PERSISTENCE_UNAVAILABLE")?;
            Ok(Some(completed))
        }
        Err(error) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
            Err(match error {
                erpx_server_runtime::public::postgres::PgTransactionError::Business(error) => {
                    error.code
                }
                erpx_server_runtime::public::postgres::PgTransactionError::Persistence(_) => {
                    "DIRECTORY_OPERATION_PERSISTENCE_UNAVAILABLE"
                }
            })
        }
    }
}
