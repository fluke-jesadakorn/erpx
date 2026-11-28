//! Strict startup diagnostic for reviewed fresh schemas. Never migrates, repairs,
//! accepts historical checksum predecessors or turns an owner into HTTP-ready.
use super::{
    PersistenceError,
    migrations::{MIGRATION_LOCK_KEY, MigrationFile, MigrationTrack},
    pool::{DatabasePool, SessionLease},
};
use sha2::{Digest, Sha256};
use sqlx::{Connection, Row};
use std::collections::BTreeMap;
/// An observation, never actor/business authority. No public constructor,
/// Deserialize, Serialize or ready flag can promote this into a permission.
pub struct StartupMigrationInventory {
    track: MigrationTrack,
    count: usize,
    digest: String,
}
impl StartupMigrationInventory {
    pub fn track(&self) -> MigrationTrack {
        self.track
    }
    pub fn count(&self) -> usize {
        self.count
    }
    pub fn sha256(&self) -> &str {
        &self.digest
    }
}
fn invalid() -> PersistenceError {
    PersistenceError::new("CURRENT_MIGRATION_INVENTORY_MISMATCH")
}
/// Caller supplies only source-owned reviewed files for this explicit track.
/// The shared lock is the SAME canonical key held exclusively by migrate_database;
/// READ COMMITTED takes the receipt snapshot after a waiting migration finishes.
/// The entire exact-set query is read-only and bounded, including one extra row.
pub async fn verify_current_migration_inventory(
    pool: &DatabasePool,
    track: MigrationTrack,
    files: &[MigrationFile],
) -> Result<StartupMigrationInventory, PersistenceError> {
    if files.is_empty() || files.len() > 2048 {
        return Err(invalid());
    }
    let expected: BTreeMap<_, _> = files.iter().map(|f| (f.name(), f.checksum())).collect();
    if expected.len() != files.len() {
        return Err(invalid());
    }
    let mut lease = SessionLease::acquire(pool).await?;
    let mut tx = lease
        .connection()
        .begin_with("BEGIN ISOLATION LEVEL READ COMMITTED READ ONLY")
        .await?;
    let result=async {
        sqlx::query("SELECT pg_advisory_xact_lock_shared($1::bigint)").bind(MIGRATION_LOCK_KEY).execute(&mut *tx).await?;
        let limit=i64::try_from(files.len()+1).map_err(|_|invalid())?;
        let rows=sqlx::query("SELECT CASE WHEN octet_length(name)<=240 THEN name END name,CASE WHEN octet_length(checksum)=64 THEN checksum END checksum FROM public.erpx_migration_receipts() ORDER BY name LIMIT $1")
            .bind(limit).fetch_all(&mut *tx).await?;
        if rows.len()!=expected.len(){return Err(invalid())}
        let mut actual=BTreeMap::new();
        for row in rows {
            let name:String=row.try_get::<Option<String>,_>("name")?.ok_or_else(invalid)?;
            let checksum:String=row.try_get::<Option<String>,_>("checksum")?.ok_or_else(invalid)?;
            if actual.insert(name,checksum).is_some(){return Err(invalid())}
        }
        if expected.iter().any(|(name,checksum)|actual.get(*name).map(String::as_str)!=Some(*checksum)){return Err(invalid())}
        let mut digest=Sha256::new();digest.update(b"erpx-startup-schema-v1\0");digest.update(track.as_str().as_bytes());
        for (name,checksum)in expected{digest.update((name.len()as u64).to_be_bytes());digest.update(name.as_bytes());digest.update(checksum.as_bytes());}
        Ok(StartupMigrationInventory{track,count:files.len(),digest:hex::encode(digest.finalize())})
    }.await;
    match result {
        Ok(inventory) => {
            tx.commit().await?;
            lease.dirty = false;
            Ok(inventory)
        }
        Err(error) => {
            tx.rollback().await?;
            lease.dirty = false;
            Err(error)
        }
    }
}
