use super::{
    PersistenceError,
    pool::{DatabasePool, SessionLease},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{AssertSqlSafe, Connection};
use std::{collections::BTreeSet, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MigrationTrack {
    Control,
    ControlRead,
    HoldingWrite,
    HoldingRead,
}
impl MigrationTrack {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Control => "control",
            Self::ControlRead => "control-read",
            Self::HoldingWrite => "holding-write",
            Self::HoldingRead => "holding-read",
        }
    }
}
#[derive(Clone, Debug)]
pub struct MigrationFile {
    name: String,
    sql: String,
    checksum: String,
}
impl MigrationFile {
    /// Administrative trusted migration input, never request SQL. Raw UTF-8 is hashed unchanged.
    pub fn from_sql(
        name: impl Into<String>,
        sql: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let name = name.into();
        let sql = sql.into();
        if !name.ends_with(".sql")
            || name.len() > 240
            || name
                .bytes()
                .any(|b| !b.is_ascii_alphanumeric() && !matches!(b, b'_' | b'-' | b'.'))
            || name.starts_with('.')
        {
            return Err(PersistenceError::new("INVALID_MIGRATION_NAME"));
        }
        let checksum = hex::encode(Sha256::digest(sql.as_bytes()));
        Ok(Self {
            name,
            sql,
            checksum,
        })
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn checksum(&self) -> &str {
        &self.checksum
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationStatus {
    pub track: MigrationTrack,
    pub applied: usize,
    pub pending: Vec<String>,
    pub latest_applied: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationRun {
    pub applied: usize,
    pub skipped: usize,
}
pub async fn load_migrations(
    root: impl AsRef<Path>,
    track: MigrationTrack,
) -> Result<Vec<MigrationFile>, PersistenceError> {
    if !root.as_ref().is_absolute() {
        return Err(PersistenceError::new("ABSOLUTE_MIGRATION_ROOT_REQUIRED"));
    }
    let mut entries = tokio::fs::read_dir(root.as_ref().join(track.as_str()))
        .await
        .map_err(|_| PersistenceError::new("MIGRATION_ROOT_UNAVAILABLE"))?;
    let mut migrations = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|_| PersistenceError::new("MIGRATION_ROOT_UNAVAILABLE"))?
    {
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| PersistenceError::new("INVALID_MIGRATION_NAME"))?;
        if !name.ends_with(".sql") {
            continue;
        }
        if !entry
            .file_type()
            .await
            .map_err(|_| PersistenceError::new("MIGRATION_FILE_UNAVAILABLE"))?
            .is_file()
        {
            return Err(PersistenceError::new("INVALID_MIGRATION_FILE"));
        }
        let sql = tokio::fs::read_to_string(entry.path())
            .await
            .map_err(|_| PersistenceError::new("MIGRATION_FILE_UNAVAILABLE"))?;
        migrations.push(MigrationFile::from_sql(name, sql)?);
    }
    migrations.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(migrations)
}
fn allowed_predecessor(track: MigrationTrack, file: &MigrationFile, old: &str) -> bool {
    track == MigrationTrack::HoldingWrite
        && match file.name.as_str() {
            "0041_leave_self_service.sql" => {
                old == "fb926c55f087b296c11f0f3356a32c2473bd3c56ea13834f25f186564da9038a"
                    && file.checksum
                        == "0b964e468801e0d410cbcce5e1b8f5d3c4cc85d095d51ec38feaa0d6844e007e"
            }
            "0293_executive_evidence.sql" => {
                old == "250b2bbc409499b500d4dfc3b667c3e9bd849a15d04749cf3047c804563790e9"
                    && file.checksum
                        == "d713f4a01ef10cee409bc5a6cc40471154691a4d98903539cc698bf0bfd3964b"
            }
            _ => false,
        }
}
pub async fn migrate_database(
    pool: &DatabasePool,
    track: MigrationTrack,
    files: &[MigrationFile],
) -> Result<MigrationRun, PersistenceError> {
    let mut ordered: Vec<_> = files.iter().collect();
    ordered.sort_by(|a, b| a.name.cmp(&b.name));
    if ordered.windows(2).any(|pair| pair[0].name == pair[1].name) {
        return Err(PersistenceError::new("DUPLICATE_MIGRATION_NAME"));
    }
    let mut lease = SessionLease::acquire(pool).await?;
    sqlx::query("SELECT pg_advisory_lock(714928001)")
        .execute(lease.connection())
        .await?;
    let result=async {
        sqlx::query("CREATE TABLE IF NOT EXISTS erpx_migrations(name TEXT PRIMARY KEY,checksum TEXT NOT NULL,applied_at TIMESTAMPTZ NOT NULL DEFAULT now())").execute(lease.connection()).await?;
        let mut run=MigrationRun{applied:0,skipped:0};
        for file in ordered {
            let previous:Option<String>=sqlx::query_scalar("SELECT checksum FROM erpx_migrations WHERE name=$1").bind(&file.name).fetch_optional(lease.connection()).await?;
            if let Some(previous)=previous {
                if previous!=file.checksum && !allowed_predecessor(track,file,&previous) { return Err(PersistenceError::new("MIGRATION_CHANGED").with_context(format!("{}:{}",track.as_str(),file.name))); }
                run.skipped+=1; continue;
            }
            let mut tx=lease.connection().begin().await?;
            // Only reviewed repository migration bytes enter this audited unprepared path.
            // Preserve PL/pgSQL bodies and whole-file transaction atomicity; no semicolon splitting.
            sqlx::raw_sql(AssertSqlSafe(file.sql.as_str())).execute(&mut *tx).await?;
            sqlx::query("INSERT INTO erpx_migrations(name,checksum) VALUES($1,$2)").bind(&file.name).bind(&file.checksum).execute(&mut *tx).await?;
            tx.commit().await?;
            run.applied+=1;
        }
        if track==MigrationTrack::HoldingWrite && sqlx::query_scalar::<_,bool>("SELECT to_regprocedure('ensure_work_cache_coverage()') IS NOT NULL").fetch_one(lease.connection()).await? {
            sqlx::query("SELECT ensure_work_cache_coverage()").execute(lease.connection()).await?;
        }
        Ok(run)
    }.await;
    match sqlx::query_scalar::<_, bool>("SELECT pg_advisory_unlock(714928001)")
        .fetch_one(lease.connection())
        .await
    {
        Ok(true) => lease.dirty = false,
        _ => return Err(PersistenceError::new("MIGRATION_UNLOCK_FAILED")),
    }
    result
}
pub async fn inspect_migrations(
    pool: &DatabasePool,
    track: MigrationTrack,
    files: &[MigrationFile],
) -> Result<MigrationStatus, PersistenceError> {
    let names: Vec<String> =
        match sqlx::query_scalar("SELECT name FROM erpx_migration_receipts() ORDER BY name")
            .fetch_all(pool.raw())
            .await
        {
            Ok(names) => names,
            Err(error) => {
                let error = PersistenceError::from(error);
                if !matches!(error.sql_state(), Some("42P01" | "42883")) {
                    return Err(error);
                }
                if sqlx::query_scalar::<_, bool>(
                    "SELECT to_regclass('public.erpx_migrations') IS NOT NULL",
                )
                .fetch_one(pool.raw())
                .await?
                {
                    return Err(PersistenceError::new("MIGRATION_RECEIPTS_UNAVAILABLE"));
                }
                Vec::new()
            }
        };
    let applied: BTreeSet<_> = names.iter().collect();
    let mut pending: Vec<_> = files
        .iter()
        .filter(|f| !applied.contains(&f.name))
        .map(|f| f.name.clone())
        .collect();
    pending.sort();
    Ok(MigrationStatus {
        track,
        applied: names.len(),
        pending,
        latest_applied: names.last().cloned(),
    })
}
