//! Durable source facts on the caller's actual Holding transaction. These rows
//! contain keyed source data, never passwords or a replacement permission.
#[cfg(feature = "directory-ldaps")]
use crate::domain::directory::DirectoryConnectionDescriptor;
use crate::domain::directory::{DirectoryConnectionConfig, DirectoryMode};
use crate::domain::directory_provenance::*;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};
type Tx<T> = Result<T, PgTransactionError<BusinessError>>;

/// Borrowed current source data, never an approval or permission. Positive row
/// locks remain on this same caller transaction; no pool checkout or commit.
pub async fn read_current_directory_source(
    c: &mut PgConnection,
    id: &str,
    company: &str,
) -> Tx<SelectedSource> {
    if !c.is_in_transaction() {
        return Err(
            erpx_server_runtime::public::postgres::PersistenceError::new(
                "DIRECTORY_SOURCE_TRANSACTION_REQUIRED",
            )
            .into(),
        );
    }
    if id.is_empty() || id.len() > 200 || company.is_empty() || company.len() > 200 {
        return Err(Denial::SourceNotApproved.business().into());
    }
    current_selection(c, id, company).await
}

pub(crate) async fn lock_sources(c: &mut PgConnection) -> Tx<()> {
    sqlx::query("LOCK TABLE iam_directory_source_selections,iam_directory_observation_sources IN SHARE MODE").execute(c).await?;
    Ok(())
}
fn failure() -> BusinessError {
    Denial::SourceNotApproved.business()
}
fn from_row(row: &sqlx::postgres::PgRow) -> Tx<SelectedSource> {
    let config: Vec<u8> = row.try_get("config_fingerprint")?;
    let key_id: String = row.try_get("key_id")?;
    if row.try_get::<i32, _>("format_version")? != 1 || !valid_key_id(&key_id) {
        return Err(failure().into());
    }
    Ok(SelectedSource {
        revision: SourceRevision::checked(row.try_get("revision")?).map_err(Denial::business)?,
        key_id,
        profile_tag: ProfileTag::from_bytes(&row.try_get::<Vec<u8>, _>("profile_tag")?)
            .map_err(Denial::business)?,
        connection_version: row.try_get::<i32, _>("connection_version")?.into(),
        config_fingerprint: config.try_into().map_err(|_| failure())?,
    })
}
#[cfg(feature = "directory-ldaps")]
pub(super) async fn existing_selection(
    c: &mut PgConnection,
    id: &str,
    company: &str,
) -> Tx<Option<SelectedSource>> {
    let row=sqlx::query("SELECT revision,format_version,key_id,profile_tag,connection_version,config_fingerprint FROM iam_directory_source_selections WHERE connection_id=$1 AND company_id=$2 FOR SHARE")
        .bind(id).bind(company).fetch_optional(c).await?;
    row.map(|r| from_row(&r)).transpose()
}
#[cfg(feature = "directory-ldaps")]
pub(crate) fn config_bytes(d: &DirectoryConnectionDescriptor) -> Result<[u8; 32], BusinessError> {
    let mut bytes = [0; 32];
    hex::decode_to_slice(&d.config_fingerprint, &mut bytes).map_err(|_| failure())?;
    Ok(bytes)
}
/// Positive tuple fences reject an older SERIALIZABLE snapshot after a raw or
/// native source writer commits. Platform/Control has no mirrored schema yet.
pub(crate) async fn current_selection(
    c: &mut PgConnection,
    id: &str,
    company: &str,
) -> Tx<SelectedSource> {
    if company == "platform" {
        return Err(failure().into());
    }
    let row=sqlx::query("SELECT s.revision,s.format_version,s.key_id,s.profile_tag,s.connection_version,s.config_fingerprint,c.version::bigint actual_version,c.status,CASE WHEN octet_length(c.config::text)<=16384 THEN c.config::text END config FROM iam_directory_source_selections s JOIN iam_connections c ON c.id=s.connection_id AND c.company_id=s.company_id WHERE s.connection_id=$1 AND s.company_id=$2 FOR SHARE OF s,c")
        .bind(id).bind(company).fetch_optional(c).await?.ok_or_else(failure)?;
    let selected = from_row(&row)?;
    let config: String = row
        .try_get::<Option<String>, _>("config")?
        .ok_or_else(failure)?;
    let parsed: DirectoryConnectionConfig = serde_json::from_str(&config).map_err(|_| failure())?;
    if row.try_get::<&str, _>("status")? != "active"
        || parsed.mode != DirectoryMode::Ldaps
        || row.try_get::<i64, _>("actual_version")? != selected.connection_version
        || <[u8; 32]>::from(Sha256::digest(config.as_bytes())) != selected.config_fingerprint
    {
        return Err(Denial::ConfigurationNotSelected.business().into());
    }
    Ok(selected)
}
#[cfg(feature = "directory-ldaps")]
pub(crate) async fn selected_for_descriptor(
    c: &mut PgConnection,
    d: &DirectoryConnectionDescriptor,
) -> Tx<SelectedSource> {
    let source = current_selection(c, &d.connection_id, &d.company_id).await?;
    if source.connection_version != d.version || source.config_fingerprint != config_bytes(d)? {
        return Err(Denial::ConfigurationNotSelected.business().into());
    }
    Ok(source)
}
pub(crate) async fn observation(
    c: &mut PgConnection,
    id: &str,
    company: &str,
) -> Tx<Option<ObservationProvenance>> {
    let row=sqlx::query("SELECT source_revision revision,format_version,key_id,profile_tag,connection_version,config_fingerprint,observed_at FROM iam_directory_observation_sources WHERE connection_id=$1 AND company_id=$2 FOR SHARE")
        .bind(id).bind(company).fetch_optional(c).await?;
    row.map(|row| {
        Ok(ObservationProvenance {
            source: from_row(&row)?,
            observed_at: row.try_get("observed_at")?,
        })
    })
    .transpose()
}
#[cfg(feature = "directory-ldaps")]
pub(crate) async fn require_current_observation(
    c: &mut PgConnection,
    d: &DirectoryConnectionDescriptor,
    identity_observed_at: i64,
    now: i64,
) -> Tx<ObservationUse> {
    let current = selected_for_descriptor(c, d).await?;
    if d.source.as_ref() != Some(&current) {
        return Err(Denial::ConfigurationNotSelected.business().into());
    }
    let observed = observation(c, &d.connection_id, &d.company_id).await?;
    observation_status(&current, observed.as_ref(), identity_observed_at, now)
        .map_err(|e| e.business().into())
}
pub(crate) async fn require_member_observation(
    c: &mut PgConnection,
    id: &str,
    company: &str,
    identity_observed_at: i64,
    now: i64,
) -> Tx<bool> {
    let current = current_selection(c, id, company).await?;
    let observed = observation(c, id, company).await?;
    Ok(
        observation_status(&current, observed.as_ref(), identity_observed_at, now)
            .map_err(Denial::business)?
            == ObservationUse::Fresh,
    )
}
#[cfg(feature = "directory-ldaps")]
pub(crate) async fn stamp_complete(
    c: &mut PgConnection,
    d: &DirectoryConnectionDescriptor,
    observation_id: &str,
    observed_at: i64,
) -> Tx<()> {
    let current = selected_for_descriptor(c, d).await?;
    if d.source.as_ref() != Some(&current) {
        return Err(Denial::ConfigurationNotSelected.business().into());
    }
    // Identifier is generated before the retryable Holding unit. No pending
    // observation is inserted before all complete snapshot effects are ready.
    sqlx::query("INSERT INTO iam_directory_observation_sources(connection_id,company_id,source_revision,format_version,key_id,profile_tag,connection_version,config_fingerprint,observation_id,observed_at) VALUES($1,$2,$3,1,$4,$5,$6,$7,$8::text::uuid,$9) ON CONFLICT(connection_id) DO UPDATE SET company_id=excluded.company_id,source_revision=excluded.source_revision,format_version=excluded.format_version,key_id=excluded.key_id,profile_tag=excluded.profile_tag,connection_version=excluded.connection_version,config_fingerprint=excluded.config_fingerprint,observation_id=excluded.observation_id,observed_at=excluded.observed_at")
        .bind(&d.connection_id).bind(&d.company_id).bind(current.revision.get()).bind(&current.key_id).bind(current.profile_tag.database_bytes().as_slice()).bind(i32::try_from(current.connection_version).map_err(|_|failure())?).bind(current.config_fingerprint.as_slice()).bind(observation_id).bind(observed_at).execute(c).await?;
    Ok(())
}
#[cfg(feature = "directory-ldaps")]
pub(super) async fn require_stamp(
    c: &mut PgConnection,
    d: &DirectoryConnectionDescriptor,
    id: &str,
    observed_at: i64,
) -> Tx<()> {
    let stored = observation(c, &d.connection_id, &d.company_id)
        .await?
        .ok_or_else(failure)?;
    let stored_id:String=sqlx::query_scalar("SELECT observation_id::text FROM iam_directory_observation_sources WHERE connection_id=$1 AND company_id=$2 FOR SHARE")
        .bind(&d.connection_id).bind(&d.company_id).fetch_one(c).await?;
    if stored.observed_at != observed_at
        || d.source.as_ref() != Some(&stored.source)
        || stored_id != id
    {
        return Err(Denial::InconsistentObservation.business().into());
    }
    Ok(())
}
