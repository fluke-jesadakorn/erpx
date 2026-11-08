//! Files-owned metadata for an already-attested set of inspection references.
//!
//! This port proves only the requested Files rows and revocation state. It
//! grants no record authority and cannot prove that the caller supplied every
//! Quality reference; composition must retain and revalidate that witness.
use std::collections::{HashMap, HashSet};
use futures_util::TryStreamExt;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
const MAX_IDS: usize = 5000;
const MAX_ID_BYTES: usize = 4096;
const MAX_METADATA_BYTES: usize = 4 * 1024 * 1024;
const MAX_SAFE: i64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualityInspectionDescriptor {
    pub id: String,
    pub original_name: String,
    pub media_type: String,
    pub bytes: i64,
    pub checksum_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Image { Missing, Revoked, Visible(QualityInspectionDescriptor) }

fn limit() -> Error { BusinessError::new(FailureKind::Unavailable, "FILES_INSPECTION_DESCRIPTORS_LIMIT", "Inspection evidence metadata exceeds the bounded read.").into() }
fn changed() -> Error { BusinessError::new(FailureKind::Conflict, "FILES_INSPECTION_DESCRIPTORS_CHANGED", "Inspection evidence metadata changed.").into() }

/// Utility-only lock. Composition must acquire this before the first snapshot
/// and in an order compatible with all participating owners and writers.
pub async fn lock_quality_inspection_descriptors(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() { return Err(PersistenceError::new("FILES_INSPECTION_DESCRIPTORS_TRANSACTION_REQUIRED").into()); }
    sqlx::query("LOCK TABLE attachments IN SHARE MODE").execute(c).await?;
    Ok(())
}

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() || sqlx::query_scalar::<_, String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" {
        return Err(PersistenceError::new("FILES_INSPECTION_DESCRIPTORS_TRANSACTION_REQUIRED").into());
    }
    let held: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND granted AND relation='attachments'::regclass AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))").fetch_one(&mut *c).await?;
    if !held { return Err(PersistenceError::new("FILES_INSPECTION_DESCRIPTORS_LOCK_REQUIRED").into()); }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}

async fn capture(c: &mut PgConnection, ids: &[String]) -> Result<Vec<Image>, Error> {
    if ids.len() > MAX_IDS || ids.iter().any(|id| id.is_empty() || id.len() > MAX_ID_BYTES) || ids.iter().collect::<HashSet<_>>().len() != ids.len() { return Err(limit()); }
    // Revoked rows never expose metadata in the legacy read; do not reject a
    // revoked row merely because its unused metadata is large.
    let sql = "SELECT id,(revoked_at IS NOT NULL) revoked, CASE WHEN revoked_at IS NOT NULL THEN NULL WHEN octet_length(original_name)::bigint+octet_length(media_type)::bigint+octet_length(checksum_sha256)::bigint<=$2 THEN original_name END original_name, CASE WHEN revoked_at IS NOT NULL THEN NULL WHEN octet_length(original_name)::bigint+octet_length(media_type)::bigint+octet_length(checksum_sha256)::bigint<=$2 THEN media_type END media_type, CASE WHEN revoked_at IS NOT NULL THEN NULL WHEN octet_length(original_name)::bigint+octet_length(media_type)::bigint+octet_length(checksum_sha256)::bigint<=$2 THEN checksum_sha256 END checksum_sha256, CASE WHEN revoked_at IS NULL AND bytes BETWEEN -9007199254740991 AND 9007199254740991 THEN bytes END bytes FROM attachments WHERE id=ANY($1::text[])";
    let mut stream = sqlx::query(sql).bind(ids).bind(MAX_METADATA_BYTES as i64).fetch(&mut *c);
    let mut images = HashMap::with_capacity(ids.len());
    let mut total = 0usize;
    while let Some(row) = stream.try_next().await? {
        let id: String = row.try_get("id")?;
        let revoked: bool = row.try_get("revoked")?;
        let image = if revoked { Image::Revoked } else {
            let original_name: String = row.try_get::<Option<String>, _>("original_name")?.ok_or_else(limit)?;
            let media_type: String = row.try_get::<Option<String>, _>("media_type")?.ok_or_else(limit)?;
            let checksum_sha256: String = row.try_get::<Option<String>, _>("checksum_sha256")?.ok_or_else(limit)?;
            let bytes = row.try_get::<Option<i64>, _>("bytes")?.ok_or_else(limit)?;
            if !(-MAX_SAFE..=MAX_SAFE).contains(&bytes) { return Err(limit()); }
            total = total.checked_add(id.len()).and_then(|n| n.checked_add(original_name.len())).and_then(|n| n.checked_add(media_type.len())).and_then(|n| n.checked_add(checksum_sha256.len())).ok_or_else(limit)?;
            if total > MAX_METADATA_BYTES { return Err(limit()); }
            Image::Visible(QualityInspectionDescriptor { id: id.clone(), original_name, media_type, bytes, checksum_sha256 })
        };
        images.insert(id, image);
    }
    drop(stream);
    Ok(ids.iter().map(|id| images.remove(id).unwrap_or(Image::Missing)).collect())
}

pub struct CurrentQualityInspectionDescriptors { unit: Unit, ids: Vec<String>, images: Vec<Image>, visible: Vec<QualityInspectionDescriptor> }
impl CurrentQualityInspectionDescriptors {
    pub fn descriptors(&self) -> &[QualityInspectionDescriptor] { &self.visible }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || capture(c, &self.ids).await? != self.images { return Err(changed()); }
        Ok(())
    }
}

/// The caller must supply only IDs from a retained, selected Quality source
/// witness in this same transaction; this function does not verify that claim.
pub async fn inspect_quality_inspection_descriptors(c: &mut PgConnection, ids: &[String]) -> Result<CurrentQualityInspectionDescriptors, Error> {
    let unit = unit(c).await?;
    let images = capture(c, ids).await?;
    let visible = images.iter().filter_map(|item| match item { Image::Visible(row) => Some(row.clone()), _ => None }).collect();
    Ok(CurrentQualityInspectionDescriptors { unit, ids: ids.to_vec(), images, visible })
}
