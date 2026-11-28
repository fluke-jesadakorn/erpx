use crate::domain::company_plugins::{PluginManifest, VerifiedPluginVersion};
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind, UserId};
use serde::Serialize;
use sqlx::{PgConnection, Row, postgres::PgRow, types::Json};

type Error = PgTransactionError<BusinessError>;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanyInstallation {
    pub id: String,
    pub installation_version: i64,
    pub manifest: PluginManifest,
}

fn conflict() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "PLUGIN_INSTALLATION_CHANGED",
        "The plugin installation changed. Reload the catalogue before continuing.",
    )
    .into()
}

fn installed(row: &PgRow) -> Result<CompanyInstallation, Error> {
    Ok(CompanyInstallation {
        id: row.try_get("plugin_id")?,
        installation_version: row.try_get("installation_version")?,
        manifest: row.try_get::<Json<PluginManifest>, _>("manifest")?.0,
    })
}

async fn lock(
    connection: &mut PgConnection,
    company: &CompanyId,
    plugin: &str,
) -> Result<(), Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!(
            "commercial.company-plugin:{}:{plugin}",
            company.as_str()
        ))
        .execute(connection)
        .await?;
    Ok(())
}

/// Called inside the Company authorization transaction. The Command actor and
/// Company originate from that transaction's selected workspace, never the
/// request body. A repeated install of the same verified bytes returns the
/// current edition. Changing versions requires an explicit withdrawal.
pub async fn install_company_plugin(
    connection: &mut PgConnection,
    company: &CompanyId,
    actor: &UserId,
    verified: &VerifiedPluginVersion,
    now: i64,
) -> Result<CompanyInstallation, Error> {
    let pin = verified.pin();
    lock(connection, company, &pin.id).await?;
    if let Some(row) = sqlx::query(
        "SELECT plugin_id, installation_version, manifest, manifest_digest, repository_id, repository_commit
         FROM company_plugin_installations
         WHERE company_id = $1 AND plugin_id = $2 AND withdrawn_at IS NULL",
    )
    .bind(company.as_str())
    .bind(&pin.id)
    .fetch_optional(&mut *connection)
    .await?
    {
        let installed_repository = row.try_get::<i64, _>("repository_id")?;
        if u64::try_from(installed_repository).ok() != Some(pin.repository_id)
            || row.try_get::<String, _>("repository_commit")? != pin.commit
            || row.try_get::<String, _>("manifest_digest")? != pin.sha256
        {
            return Err(conflict());
        }
        return installed(&row);
    }
    let repository_id = i64::try_from(pin.repository_id).map_err(|_| {
        BusinessError::new(
            FailureKind::InvalidInput,
            "PLUGIN_CATALOGUE_INVALID",
            "The plugin repository ID is invalid.",
        )
    })?;
    let row = sqlx::query(
        "INSERT INTO company_plugin_installations
         (company_id, plugin_id, installation_version, plugin_version, repository_id,
          repository_commit, manifest_digest, manifest, installed_by, installed_at)
         SELECT $1, $2, COALESCE(MAX(installation_version), 0) + 1, $3, $4, $5, $6, $7, $8, $9
         FROM company_plugin_installations WHERE company_id = $1 AND plugin_id = $2
         RETURNING plugin_id, installation_version, manifest",
    )
    .bind(company.as_str())
    .bind(&pin.id)
    .bind(&pin.version)
    .bind(repository_id)
    .bind(&pin.commit)
    .bind(&pin.sha256)
    .bind(Json(verified.manifest()))
    .bind(actor.as_str())
    .bind(now)
    .fetch_one(connection)
    .await?;
    installed(&row)
}

pub async fn withdraw_company_plugin(
    connection: &mut PgConnection,
    company: &CompanyId,
    actor: &UserId,
    plugin_id: &str,
    installation_version: i64,
    now: i64,
) -> Result<(), Error> {
    lock(connection, company, plugin_id).await?;
    let row = sqlx::query(
        "UPDATE company_plugin_installations
         SET withdrawn_at = COALESCE(withdrawn_at, GREATEST(installed_at, $5)),
             withdrawn_by = COALESCE(withdrawn_by, $4)
         WHERE company_id = $1 AND plugin_id = $2 AND installation_version = $3
         RETURNING installation_version",
    )
    .bind(company.as_str())
    .bind(plugin_id)
    .bind(installation_version)
    .bind(actor.as_str())
    .bind(now)
    .fetch_optional(connection)
    .await?;
    row.map(|_| ()).ok_or_else(conflict)
}

pub async fn list_company_plugins(
    connection: &mut PgConnection,
    company: &CompanyId,
) -> Result<Vec<CompanyInstallation>, Error> {
    let rows = sqlx::query(
        "SELECT plugin_id, installation_version, manifest
         FROM company_plugin_installations
         WHERE company_id = $1 AND withdrawn_at IS NULL ORDER BY plugin_id",
    )
    .bind(company.as_str())
    .fetch_all(connection)
    .await?;
    rows.iter().map(installed).collect()
}
