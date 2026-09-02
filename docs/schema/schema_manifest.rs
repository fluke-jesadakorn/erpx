use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row, migrate::MigrateError};
use thiserror::Error;

use crate::{
    CONTROL_PLANE_MIGRATOR, CONTROL_READ_MIGRATOR, HOLDING_READ_MIGRATOR, HOLDING_WRITE_MIGRATOR,
};

/// Clean-rebuild schema revision accepted by ERPX runtime composition.
pub const SCHEMA_REVISION: i16 = 2;
pub const SCHEMA_MANIFEST_VERSION: i16 = 1;

/// Physical database roles are closed so a write migration can never be
/// applied to a disposable read database (or vice versa) by configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseRole {
    ControlWrite,
    ControlRead,
    HoldingWrite,
    HoldingRead,
}

impl DatabaseRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ControlWrite => "control_write",
            Self::ControlRead => "control_read",
            Self::HoldingWrite => "holding_write",
            Self::HoldingRead => "holding_read",
        }
    }

    pub const fn migration_set(self) -> &'static str {
        match self {
            Self::ControlWrite => "control-write",
            Self::ControlRead => "control-read",
            Self::HoldingWrite => "holding-write",
            Self::HoldingRead => "holding-read",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaManifest {
    pub schema_revision: i16,
    pub manifest_version: i16,
    pub database_role: String,
    pub migration_set: String,
    pub installed_at: DateTime<Utc>,
}

/// Fail-closed schema errors deliberately omit database names and connection
/// details. The caller can report the expected role without leaking topology.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SchemaManifestViolation {
    #[error("database contains schema state without an ERPX generation manifest")]
    UnversionedSchema,
    #[error("ERPX schema revision is unsupported")]
    UnsupportedGeneration,
    #[error("ERPX schema manifest version is unsupported")]
    UnsupportedManifestVersion,
    #[error("ERPX database role does not match the requested migration set")]
    RoleMismatch,
    #[error("ERPX migration-set identity does not match the requested database role")]
    MigrationSetMismatch,
    #[error("ERPX schema revision manifest is missing after migration")]
    MissingAfterMigration,
}

/// Run exactly one role-specific migration set. Existing unversioned schemas
/// are rejected before SQLx can append migration history, preserving the
/// clean-rebuild boundary and every retained pre-cutover database.
pub async fn migrate_database_role(pool: &PgPool, role: DatabaseRole) -> Result<(), MigrateError> {
    match load_schema_manifest(pool).await? {
        Some(manifest) => validate_manifest(&manifest, role).map_err(as_migrate_error)?,
        None if database_has_user_relations(pool).await? => {
            return Err(as_migrate_error(SchemaManifestViolation::UnversionedSchema));
        }
        None => {}
    }

    migrator(role).run(pool).await?;
    verify_schema_manifest(pool, role).await.map(|_| ())
}

/// Verify the generation and physical role before a runtime accepts a pool.
pub async fn verify_schema_manifest(
    pool: &PgPool,
    role: DatabaseRole,
) -> Result<SchemaManifest, MigrateError> {
    let manifest = load_schema_manifest(pool)
        .await?
        .ok_or_else(|| as_migrate_error(SchemaManifestViolation::MissingAfterMigration))?;
    validate_manifest(&manifest, role).map_err(as_migrate_error)?;
    Ok(manifest)
}

fn validate_manifest(
    manifest: &SchemaManifest,
    role: DatabaseRole,
) -> Result<(), SchemaManifestViolation> {
    if manifest.schema_revision != SCHEMA_REVISION {
        return Err(SchemaManifestViolation::UnsupportedGeneration);
    }
    if manifest.manifest_version != SCHEMA_MANIFEST_VERSION {
        return Err(SchemaManifestViolation::UnsupportedManifestVersion);
    }
    if manifest.database_role != role.as_str() {
        return Err(SchemaManifestViolation::RoleMismatch);
    }
    if manifest.migration_set != role.migration_set() {
        return Err(SchemaManifestViolation::MigrationSetMismatch);
    }
    Ok(())
}

async fn load_schema_manifest(pool: &PgPool) -> Result<Option<SchemaManifest>, MigrateError> {
    let exists: bool =
        sqlx::query_scalar("SELECT to_regclass('erpx_meta.schema_manifest') IS NOT NULL")
            .fetch_one(pool)
            .await
            .map_err(MigrateError::Execute)?;
    if !exists {
        return Ok(None);
    }

    let row = sqlx::query(
        "SELECT schema_revision, manifest_version, database_role, migration_set, installed_at
         FROM erpx_meta.schema_manifest WHERE singleton = TRUE",
    )
    .fetch_optional(pool)
    .await
    .map_err(MigrateError::Execute)?;
    row.map(|row| {
        Ok(SchemaManifest {
            schema_revision: row.try_get("schema_revision")?,
            manifest_version: row.try_get("manifest_version")?,
            database_role: row.try_get("database_role")?,
            migration_set: row.try_get("migration_set")?,
            installed_at: row.try_get("installed_at")?,
        })
    })
    .transpose()
    .map_err(MigrateError::Execute)
}

async fn database_has_user_relations(pool: &PgPool) -> Result<bool, MigrateError> {
    sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1
             FROM pg_class relation
             JOIN pg_namespace namespace ON namespace.oid = relation.relnamespace
             WHERE namespace.nspname NOT IN ('pg_catalog', 'information_schema')
               AND namespace.nspname NOT LIKE 'pg_toast%'
               AND relation.relkind IN ('r', 'p', 'v', 'm', 'S')
         )",
    )
    .fetch_one(pool)
    .await
    .map_err(MigrateError::Execute)
}

const fn migrator(role: DatabaseRole) -> &'static sqlx::migrate::Migrator {
    match role {
        DatabaseRole::ControlWrite => &CONTROL_PLANE_MIGRATOR,
        DatabaseRole::ControlRead => &CONTROL_READ_MIGRATOR,
        DatabaseRole::HoldingWrite => &HOLDING_WRITE_MIGRATOR,
        DatabaseRole::HoldingRead => &HOLDING_READ_MIGRATOR,
    }
}

fn as_migrate_error(error: SchemaManifestViolation) -> MigrateError {
    MigrateError::Source(Box::new(error))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(role: DatabaseRole) -> SchemaManifest {
        SchemaManifest {
            schema_revision: SCHEMA_REVISION,
            manifest_version: SCHEMA_MANIFEST_VERSION,
            database_role: role.as_str().into(),
            migration_set: role.migration_set().into(),
            installed_at: Utc::now(),
        }
    }

    #[test]
    fn every_database_role_has_a_distinct_manifest_identity() {
        let roles = [
            DatabaseRole::ControlWrite,
            DatabaseRole::ControlRead,
            DatabaseRole::HoldingWrite,
            DatabaseRole::HoldingRead,
        ];
        let mut role_names = roles.map(DatabaseRole::as_str).to_vec();
        role_names.sort_unstable();
        role_names.dedup();
        assert_eq!(role_names.len(), roles.len());

        let mut migration_sets = roles.map(DatabaseRole::migration_set).to_vec();
        migration_sets.sort_unstable();
        migration_sets.dedup();
        assert_eq!(migration_sets.len(), roles.len());
    }

    #[test]
    fn manifest_validation_rejects_generation_role_and_set_mismatches() {
        let mut value = manifest(DatabaseRole::HoldingWrite);
        assert!(validate_manifest(&value, DatabaseRole::HoldingWrite).is_ok());
        value.schema_revision = 1;
        assert_eq!(
            validate_manifest(&value, DatabaseRole::HoldingWrite),
            Err(SchemaManifestViolation::UnsupportedGeneration)
        );
        value = manifest(DatabaseRole::HoldingWrite);
        assert_eq!(
            validate_manifest(&value, DatabaseRole::ControlWrite),
            Err(SchemaManifestViolation::RoleMismatch)
        );
        value.migration_set = "holding-read".into();
        assert_eq!(
            validate_manifest(&value, DatabaseRole::HoldingWrite),
            Err(SchemaManifestViolation::MigrationSetMismatch)
        );
    }

    #[test]
    fn every_embedded_migration_set_ends_with_its_current_role_marker() {
        for role in [
            DatabaseRole::ControlWrite,
            DatabaseRole::ControlRead,
            DatabaseRole::HoldingWrite,
            DatabaseRole::HoldingRead,
        ] {
            let marker = migrator(role)
                .iter()
                .last()
                .expect("schema marker migration");
            assert!(marker.sql.contains("erpx_meta.schema_manifest"));
            assert!(marker.sql.contains("schema_revision"));
            assert!(marker.sql.contains(role.as_str()));
            assert!(marker.sql.contains(role.migration_set()));
        }
    }
}
