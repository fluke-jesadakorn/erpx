//! Native Sales customer-notice delivery worker.
//!
//! One bounded cycle per exact Holding database: the owner claims a due notice,
//! commits its durable attempt, dispatches one provider message and commits the
//! terminal state. The worker is the only delivery writer; the retained
//! TypeScript job process must invoke it and fails closed without it.
use erpx_server_commercial::public::purchases::PackageEnforcementMode;
use erpx_server_runtime::public::postgres::{
    DatabaseConfig, DatabasePool, PgTransactionError, PoolLimits, connect_pool,
};
use erpx_server_sales::public::sales_notification_execution::{
    DispatchOutcome, DispatchPlan, EmailDispatchPort, ExecutionFuture, OutboundEmail,
    PgNotificationExecution, ResendEmailDispatch, UnconfiguredEmailDispatch, drain_notifications,
};
use erpx_server_shared::public::BusinessError;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_CYCLE: usize = 10;

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

/// `NODE_ENV` selects the retained development `TRUE` release predicate.
pub fn production() -> bool {
    std::env::var("NODE_ENV").ok().as_deref() == Some("production")
}

/// Development-only package enforcement, independently reversible. Any other
/// environment refuses to enable it exactly like the retained boundary.
pub fn package_enforcement_mode() -> Result<PackageEnforcementMode, &'static str> {
    match std::env::var("ERPX_COMMERCE_PACKAGE_ENFORCEMENT")
        .unwrap_or_else(|_| "disabled".to_owned())
        .as_str()
    {
        "disabled" => Ok(PackageEnforcementMode::Disabled),
        "shadow" => Ok(PackageEnforcementMode::Shadow),
        "enforced" => {
            if std::env::var("ERPX_ENVIRONMENT").ok().as_deref() != Some("local") {
                return Err("PACKAGE_ENFORCEMENT_ENVIRONMENT");
            }
            Ok(PackageEnforcementMode::Enforced)
        }
        _ => Err("PACKAGE_ENFORCEMENT_CONFIGURATION"),
    }
}

/// One reviewed provider port. Missing credentials stay explicit.
pub enum CustomerEmailDispatch {
    Resend(ResendEmailDispatch),
    Unconfigured(UnconfiguredEmailDispatch),
}

impl CustomerEmailDispatch {
    pub fn from_environment() -> Self {
        match ResendEmailDispatch::from_environment() {
            Some(dispatch) => Self::Resend(dispatch),
            None => Self::Unconfigured(UnconfiguredEmailDispatch),
        }
    }

    pub fn provider(&self) -> &'static str {
        match self {
            Self::Resend(_) => "resend",
            Self::Unconfigured(_) => "unconfigured",
        }
    }
}

impl EmailDispatchPort for CustomerEmailDispatch {
    fn prepare(&self, plan: DispatchPlan) -> Result<(), BusinessError> {
        match self {
            Self::Resend(dispatch) => dispatch.prepare(plan),
            Self::Unconfigured(dispatch) => dispatch.prepare(plan),
        }
    }

    fn dispatch<'a>(
        &'a self,
        email: OutboundEmail<'a>,
    ) -> ExecutionFuture<'a, DispatchOutcome, BusinessError> {
        match self {
            Self::Resend(dispatch) => dispatch.dispatch(email),
            Self::Unconfigured(dispatch) => dispatch.dispatch(email),
        }
    }
}

/// One bounded drain for one already-opened Holding database.
pub async fn run_once(
    pool: DatabasePool,
    mode: PackageEnforcementMode,
    production: bool,
    dispatch: Arc<dyn EmailDispatchPort>,
    now: i64,
) -> Result<u32, PgTransactionError<BusinessError>> {
    let unit = PgNotificationExecution::new(pool, mode, production);
    drain_notifications(&unit, dispatch, now, MAX_CYCLE).await
}

/// CLI entry for one exact workflow notice: `--notification <id>` scans the
/// reviewed manifest for the Holding that owns this immutable notice. It opens
/// one Holding write binding at a time and never guesses between databases.
pub async fn run_once_notification_file(
    path: &Path,
    notification_id: &str,
) -> Result<String, &'static str> {
    if notification_id.trim().is_empty() || notification_id.len() > 256 {
        return Err("SALES_NOTIFICATION_ID_INVALID");
    }
    let document =
        std::fs::read_to_string(path).map_err(|_| "SALES_NOTIFICATION_MANIFEST_UNREADABLE")?;
    let manifest: DatabaseManifest =
        serde_json::from_str(&document).map_err(|_| "SALES_NOTIFICATION_MANIFEST_INVALID")?;
    if manifest.version != 1 {
        return Err("SALES_NOTIFICATION_MANIFEST_VERSION");
    }
    let mode = package_enforcement_mode()?;
    let dispatch: Arc<dyn EmailDispatchPort> = Arc::new(CustomerEmailDispatch::from_environment());
    let mut holdings = Vec::new();
    for slot in &manifest.slots {
        let Some(url) = manifest.databases.get(&slot.write) else {
            continue;
        };
        let config =
            DatabaseConfig::from_url(url).map_err(|_| "SALES_NOTIFICATION_DATABASE_INVALID")?;
        let pool = connect_pool(
            &config,
            PoolLimits {
                max_connections: 1,
                acquire_timeout: Duration::from_secs(5),
            },
        )
        .await
        .map_err(|_| "SALES_NOTIFICATION_DATABASE_UNAVAILABLE")?;
        let owner: Option<String> = sqlx::query_scalar(
            "SELECT company_id FROM commerce_customer_notifications WHERE id=$1",
        )
        .bind(notification_id)
        .fetch_optional(pool.raw())
        .await
        .unwrap_or(None);
        if owner.is_none() {
            pool.close().await;
            continue;
        }
        let result = run_once(
            pool.clone(),
            mode,
            production(),
            Arc::clone(&dispatch),
            now(),
        )
        .await;
        pool.close().await;
        match result {
            Ok(delivered) => holdings.push(serde_json::json!({
                "holdingId": slot.id,
                "delivered": delivered,
            })),
            Err(PgTransactionError::Business(error)) => return Err(error.code),
            Err(PgTransactionError::Persistence(_)) => {
                return Err("SALES_NOTIFICATION_PERSISTENCE_UNAVAILABLE");
            }
        }
    }
    Ok(serde_json::json!({ "holdings": holdings }).to_string())
}

fn holding_url<'a>(manifest: &'a DatabaseManifest, holding: &str) -> Result<&'a str, &'static str> {
    let slot = manifest
        .slots
        .iter()
        .find(|slot| slot.id == holding)
        .ok_or("SALES_NOTIFICATION_HOLDING_NOT_FOUND")?;
    manifest
        .databases
        .get(&slot.write)
        .map(String::as_str)
        .ok_or("SALES_NOTIFICATION_DATABASE_NOT_CONFIGURED")
}

/// CLI entry: `sales-notification-worker --once --database-manifest <path>
/// --holding <id>`. The manifest is the same reviewed jobs-role manifest the
/// TypeScript job process already uses; this worker opens only the exact
/// Holding write binding.
pub async fn run_once_file(path: &Path, holding: &str) -> Result<String, &'static str> {
    let document =
        std::fs::read_to_string(path).map_err(|_| "SALES_NOTIFICATION_MANIFEST_UNREADABLE")?;
    let manifest: DatabaseManifest =
        serde_json::from_str(&document).map_err(|_| "SALES_NOTIFICATION_MANIFEST_INVALID")?;
    if manifest.version != 1 {
        return Err("SALES_NOTIFICATION_MANIFEST_VERSION");
    }
    let url = holding_url(&manifest, holding)?;
    let config =
        DatabaseConfig::from_url(url).map_err(|_| "SALES_NOTIFICATION_DATABASE_INVALID")?;
    let pool = connect_pool(
        &config,
        PoolLimits {
            max_connections: 1,
            acquire_timeout: Duration::from_secs(5),
        },
    )
    .await
    .map_err(|_| "SALES_NOTIFICATION_DATABASE_UNAVAILABLE")?;
    let mode = package_enforcement_mode()?;
    let dispatch = CustomerEmailDispatch::from_environment();
    let provider = dispatch.provider();
    let result = run_once(pool.clone(), mode, production(), Arc::new(dispatch), now()).await;
    pool.close().await;
    match result {
        Ok(delivered) => Ok(serde_json::json!({
            "holdings": [{
                "holdingId": holding,
                "delivered": delivered,
                "provider": provider,
            }]
        })
        .to_string()),
        Err(PgTransactionError::Business(error)) => Err(error.code),
        Err(PgTransactionError::Persistence(_)) => {
            Err("SALES_NOTIFICATION_PERSISTENCE_UNAVAILABLE")
        }
    }
}
