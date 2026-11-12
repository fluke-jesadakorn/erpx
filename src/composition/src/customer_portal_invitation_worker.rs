//! Native Customer Portal invitation delivery worker.
//!
//! One bounded cycle per exact Holding database: the owner recovers interrupted
//! invitations, claims one due invitation under a 30 s lease, re-proves its
//! approved binding/order source and purchase entitlement, reproduces the
//! signed token against the stored digest and sends the private enrollment
//! link through the single reviewed email port. The retained TypeScript job
//! process must invoke this worker and fails closed without it.
use erpx_server_commercial::public::purchases::PackageEnforcementMode;
use erpx_server_customer_authority::public::portal_invitation_delivery::{
    InvitationClaims, PgPortalInvitationDelivery, PortalInvitationEmailPort,
    ResendPortalInvitationEmail, SandboxPortalInvitationEmail, UnconfiguredPortalInvitationEmail,
    deliver_one, require_invitation_token, resolve_account_origin,
};
use erpx_server_runtime::public::postgres::{
    DatabaseConfig, DatabasePool, PgTransactionError, PoolLimits, connect_pool,
};
use erpx_server_shared::public::BusinessError;
use serde::Deserialize;
use std::collections::BTreeMap;
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

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// `NODE_ENV` selects the retained development `TRUE` feature predicate.
pub fn production() -> bool {
    std::env::var("NODE_ENV").ok().as_deref() == Some("production")
}

/// Development-only package enforcement, independently reversible.
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

fn variable(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Exact Account origin for enrollment links, resolved from the reviewed
/// product-routing inputs only.
pub fn account_origin() -> Option<String> {
    let legacy = variable("BETTER_AUTH_URL");
    let holding_routing = variable("ERPX_HOLDING_ROUTING").as_deref() == Some("enabled")
        && variable("ERPX_ENVIRONMENT").as_deref() == Some("local")
        && variable("ERPX_PRODUCT_ENVIRONMENT").as_deref() == Some("development");
    let contributor = variable("NODE_ENV").as_deref() == Some("development")
        && variable("NEXT_PUBLIC_ERPX_DEV_ORIGIN_PROFILE").as_deref()
            == Some("container-localhost");
    resolve_account_origin(
        variable("ERPX_PRODUCT_ROUTING").as_deref() == Some("enabled"),
        variable("ERPX_PRODUCT_SSO").as_deref() == Some("enabled"),
        variable("ERPX_PRODUCT_ENVIRONMENT").as_deref() == Some("development"),
        variable("ERPX_ENVIRONMENT").as_deref() == Some("local"),
        holding_routing,
        contributor,
        legacy.as_deref(),
    )
}

/// One reviewed email port. Sandbox mode keeps the retained sensitive receipt
/// proof in the Control database; live mode requires the exact provider pair.
pub fn email_port(control: Option<DatabasePool>) -> Box<dyn PortalInvitationEmailPort> {
    if variable("ERPX_EMAIL_ADAPTER").as_deref() == Some("sandbox") {
        return Box::new(SandboxPortalInvitationEmail::new(
            control,
            variable("BETTER_AUTH_SECRET"),
            variable("ERPX_ENVIRONMENT").as_deref() == Some("production"),
        ));
    }
    match (variable("RESEND_FROM"), variable("RESEND_API_KEY")) {
        (Some(from), Some(key)) => match ResendPortalInvitationEmail::new(from, key) {
            Ok(port) => Box::new(port),
            Err(_) => Box::new(UnconfiguredPortalInvitationEmail),
        },
        _ => Box::new(UnconfiguredPortalInvitationEmail),
    }
}

fn signer(
    secret: Option<String>,
) -> impl Fn(&InvitationClaims, &str) -> Result<String, BusinessError> {
    move |claims, stored_hash| require_invitation_token(secret.as_deref(), claims, stored_hash)
}

/// One bounded cycle for one already-opened Holding database.
pub async fn run_once(
    pool: DatabasePool,
    control: Option<DatabasePool>,
    mode: PackageEnforcementMode,
    production: bool,
    account_origin: Option<String>,
    now: i64,
) -> Result<bool, PgTransactionError<BusinessError>> {
    let unit = PgPortalInvitationDelivery::new(pool, mode, production);
    let email = email_port(control);
    let sign = signer(variable("BETTER_AUTH_SECRET"));
    deliver_one(
        &unit,
        email.as_ref(),
        &sign,
        account_origin.as_deref(),
        &uuid::Uuid::new_v4().to_string(),
        now,
    )
    .await
}

fn holding_url<'a>(manifest: &'a DatabaseManifest, holding: &str) -> Result<&'a str, &'static str> {
    let slot = manifest
        .slots
        .iter()
        .find(|slot| slot.id == holding)
        .ok_or("CUSTOMER_PORTAL_INVITATION_HOLDING_NOT_FOUND")?;
    manifest
        .databases
        .get(&slot.write)
        .map(String::as_str)
        .ok_or("CUSTOMER_PORTAL_INVITATION_DATABASE_NOT_CONFIGURED")
}

async fn open(url: &str) -> Result<DatabasePool, &'static str> {
    let config =
        DatabaseConfig::from_url(url).map_err(|_| "CUSTOMER_PORTAL_INVITATION_DATABASE_INVALID")?;
    connect_pool(
        &config,
        PoolLimits {
            max_connections: 1,
            acquire_timeout: Duration::from_secs(5),
        },
    )
    .await
    .map_err(|_| "CUSTOMER_PORTAL_INVITATION_DATABASE_UNAVAILABLE")
}

/// CLI entry: `customer-portal-invitation-worker --once --database-manifest
/// <path> --holding <id>`. The manifest is the same reviewed jobs-role manifest
/// the TypeScript job process already uses.
pub async fn run_once_file(path: &Path, holding: &str) -> Result<String, &'static str> {
    let document = std::fs::read_to_string(path)
        .map_err(|_| "CUSTOMER_PORTAL_INVITATION_MANIFEST_UNREADABLE")?;
    let manifest: DatabaseManifest = serde_json::from_str(&document)
        .map_err(|_| "CUSTOMER_PORTAL_INVITATION_MANIFEST_INVALID")?;
    if manifest.version != 1 {
        return Err("CUSTOMER_PORTAL_INVITATION_MANIFEST_VERSION");
    }
    let url = holding_url(&manifest, holding)?;
    let pool = open(url).await?;
    let control = match manifest.databases.get("CONTROL_DB") {
        Some(url) => open(url).await.ok(),
        None => None,
    };
    let mode = package_enforcement_mode()?;
    let result = run_once(
        pool.clone(),
        control.clone(),
        mode,
        production(),
        account_origin(),
        now(),
    )
    .await;
    pool.close().await;
    if let Some(control) = control {
        control.close().await;
    }
    match result {
        Ok(delivered) => Ok(serde_json::json!({
            "holdings": [{
                "holdingId": holding,
                "delivered": delivered,
            }]
        })
        .to_string()),
        Err(PgTransactionError::Business(error)) => Err(error.code),
        Err(PgTransactionError::Persistence(_)) => {
            Err("CUSTOMER_PORTAL_INVITATION_PERSISTENCE_UNAVAILABLE")
        }
    }
}
