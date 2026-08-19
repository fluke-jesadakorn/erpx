//! Control-plane persistence for governed corporate identity.

use std::{collections::BTreeSet, sync::Arc};

use chrono::{DateTime, Utc};
use erpx_domain::corporate_identity::{
    BrandCategory, BrandLifecycle, BrandScope, CorporateIdentityError, CorporateIdentityProfile,
    ResolvedCorporateIdentity, resolve_corporate_identity,
};
use serde_json::json;
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{PersistenceError, SqlxControlPlane};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorporateIdentityHistoryEntry {
    pub profile: CorporateIdentityProfile,
    pub reason: String,
    pub correlation_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
    pub archived_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedBrandDomain {
    pub hostname: String,
    pub profile_id: Uuid,
    pub verified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorporateIdentityRollback {
    pub profile_id: Uuid,
    pub source_version: u64,
    pub expected_published_version: u64,
    pub actor_ref: String,
    pub reason: String,
    pub correlation_id: Uuid,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SqlxCorporateIdentityStore {
    control: Arc<SqlxControlPlane>,
}

impl SqlxCorporateIdentityStore {
    pub fn new(control: Arc<SqlxControlPlane>) -> Self {
        Self { control }
    }

    pub async fn save_draft(
        &self,
        profile: &CorporateIdentityProfile,
        expected_version: u64,
        reason: &str,
        correlation_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<CorporateIdentityProfile, PersistenceError> {
        profile.validate(false).map_err(contract_error)?;
        validate_reason(reason)?;
        validate_correlation(correlation_id)?;
        if profile.lifecycle != BrandLifecycle::Draft
            || profile.version != expected_version.saturating_add(1)
            || profile.profile_id.is_nil()
        {
            return Err(PersistenceError::InvalidData(
                "corporate identity draft must advance exactly one version".into(),
            ));
        }

        let mut tx = self.control.pools.write.begin().await?;
        let scope = scope_parts(&profile.scope);
        let existing = sqlx::query(
            "SELECT profile_id, current_draft_version, current_published_version
             FROM erpx_control.corporate_identity_profiles
             WHERE scope_key = $1
             FOR UPDATE",
        )
        .bind(&scope.scope_key)
        .fetch_optional(&mut *tx)
        .await?;

        match existing {
            Some(row) => {
                let persisted_profile_id: Uuid = row.try_get("profile_id")?;
                let current_draft = required_u64(&row, "current_draft_version")?;
                let current_published = optional_u64(&row, "current_published_version")?;
                let current_base = if current_draft > 0 {
                    current_draft
                } else {
                    current_published.unwrap_or(0)
                };
                if persisted_profile_id != profile.profile_id || current_base != expected_version {
                    return Err(PersistenceError::OptimisticConcurrency {
                        aggregate_type: "corporate_identity".into(),
                        aggregate_id: persisted_profile_id,
                        expected: Some(expected_version),
                        actual: Some(current_base),
                    });
                }
            }
            None if expected_version == 0 => {
                sqlx::query(
                    "INSERT INTO erpx_control.corporate_identity_profiles
                        (profile_id, scope_key, scope_kind, holding_id, canonical_holding_id,
                         business_unit, current_draft_version, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $5, $6, 0, $7, $7)",
                )
                .bind(profile.profile_id)
                .bind(&scope.scope_key)
                .bind(scope.scope_kind)
                .bind(scope.holding_id.map(|id| id.to_string()))
                .bind(scope.holding_id)
                .bind(scope.business_unit)
                .bind(at)
                .execute(&mut *tx)
                .await?;
            }
            None => {
                return Err(PersistenceError::OptimisticConcurrency {
                    aggregate_type: "corporate_identity".into(),
                    aggregate_id: profile.profile_id,
                    expected: Some(expected_version),
                    actual: None,
                });
            }
        }

        if !matches!(profile.scope, BrandScope::Platform) {
            let parent = load_parent_published(&mut tx, &profile.scope).await?;
            if let Some(parent) = parent.as_ref() {
                profile.assert_parent(parent).map_err(contract_error)?;
            } else {
                return Err(PersistenceError::InvalidData(
                    "corporate identity parent is not published".into(),
                ));
            }
        }

        let previous_draft_version = sqlx::query_scalar::<_, i64>(
            "SELECT current_draft_version FROM erpx_control.corporate_identity_profiles WHERE profile_id = $1",
        )
        .bind(profile.profile_id)
        .fetch_one(&mut *tx)
        .await?;
        if previous_draft_version > 0 {
            let archived = sqlx::query(
                "UPDATE erpx_control.corporate_identity_versions
                 SET lifecycle = 'archived', archived_at = $3
                 WHERE profile_id = $1 AND version = $2 AND lifecycle = 'draft'",
            )
            .bind(profile.profile_id)
            .bind(previous_draft_version)
            .bind(at)
            .execute(&mut *tx)
            .await?;
            if archived.rows_affected() != 1 {
                return Err(PersistenceError::OptimisticConcurrency {
                    aggregate_type: "corporate_identity".into(),
                    aggregate_id: profile.profile_id,
                    expected: u64::try_from(previous_draft_version).ok(),
                    actual: None,
                });
            }
        }

        insert_version(&mut tx, profile, reason, correlation_id, at, None).await?;
        sqlx::query(
            "UPDATE erpx_control.corporate_identity_profiles
             SET current_draft_version = $2, updated_at = $3
             WHERE profile_id = $1",
        )
        .bind(profile.profile_id)
        .bind(checked_i64(profile.version)?)
        .bind(at)
        .execute(&mut *tx)
        .await?;
        append_event(
            &mut tx,
            "corporate_identity.draft.saved",
            profile,
            reason,
            correlation_id,
            at,
        )
        .await?;
        tx.commit().await?;
        Ok(profile.clone())
    }

    pub async fn publish_draft(
        &self,
        profile_id: Uuid,
        expected_draft_version: u64,
        actor_ref: &str,
        reason: &str,
        correlation_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<CorporateIdentityProfile, PersistenceError> {
        validate_actor(actor_ref)?;
        validate_reason(reason)?;
        validate_correlation(correlation_id)?;
        if profile_id.is_nil() || expected_draft_version == 0 {
            return Err(PersistenceError::InvalidData(
                "corporate identity publication target is invalid".into(),
            ));
        }
        let mut tx = self.control.pools.write.begin().await?;
        let profile_row = sqlx::query(
            "SELECT scope_kind, holding_id, canonical_holding_id, business_unit,
                    current_draft_version, current_published_version, suspended_at
             FROM erpx_control.corporate_identity_profiles
             WHERE profile_id = $1
             FOR UPDATE",
        )
        .bind(profile_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "corporate identity profile",
            id: profile_id,
        })?;
        if profile_row
            .try_get::<Option<DateTime<Utc>>, _>("suspended_at")?
            .is_some()
        {
            return Err(PersistenceError::InvalidData(
                "suspended corporate identity cannot be published".into(),
            ));
        }
        let actual_draft = required_u64(&profile_row, "current_draft_version")?;
        if actual_draft != expected_draft_version {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: "corporate_identity".into(),
                aggregate_id: profile_id,
                expected: Some(expected_draft_version),
                actual: Some(actual_draft),
            });
        }
        let scope = decode_scope(&profile_row)?;
        let mut profile = load_version(&mut tx, profile_id, expected_draft_version).await?;
        profile.lifecycle = BrandLifecycle::Published;
        profile.updated_by = actor_ref.to_owned();
        profile.validate(true).map_err(contract_error)?;
        if !matches!(scope, BrandScope::Platform) {
            let parent = load_parent_published(&mut tx, &scope)
                .await?
                .ok_or_else(|| {
                    PersistenceError::InvalidData(
                        "corporate identity parent is not published".into(),
                    )
                })?;
            profile.assert_parent(&parent).map_err(contract_error)?;
        }

        if let Some(current_published) = optional_u64(&profile_row, "current_published_version")? {
            sqlx::query(
                "UPDATE erpx_control.corporate_identity_versions
                 SET lifecycle = 'archived', archived_at = $3
                 WHERE profile_id = $1 AND version = $2 AND lifecycle = 'published'",
            )
            .bind(profile_id)
            .bind(checked_i64(current_published)?)
            .bind(at)
            .execute(&mut *tx)
            .await?;
        }
        let published = sqlx::query(
            "UPDATE erpx_control.corporate_identity_versions
             SET lifecycle = 'published', published_at = $3,
                 updated_by = $4, reason = $5, correlation_id = $6
             WHERE profile_id = $1 AND version = $2 AND lifecycle = 'draft'",
        )
        .bind(profile_id)
        .bind(checked_i64(expected_draft_version)?)
        .bind(at)
        .bind(actor_ref)
        .bind(reason)
        .bind(correlation_id)
        .execute(&mut *tx)
        .await?;
        if published.rows_affected() != 1 {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: "corporate_identity".into(),
                aggregate_id: profile_id,
                expected: Some(expected_draft_version),
                actual: None,
            });
        }
        sqlx::query(
            "UPDATE erpx_control.corporate_identity_profiles
             SET current_published_version = $2, current_draft_version = 0,
                 updated_at = $3
             WHERE profile_id = $1",
        )
        .bind(profile_id)
        .bind(checked_i64(expected_draft_version)?)
        .bind(at)
        .execute(&mut *tx)
        .await?;
        append_event(
            &mut tx,
            "corporate_identity.published",
            &profile,
            reason,
            correlation_id,
            at,
        )
        .await?;
        tx.commit().await?;
        Ok(profile)
    }

    pub async fn rollback(
        &self,
        request: &CorporateIdentityRollback,
    ) -> Result<CorporateIdentityProfile, PersistenceError> {
        validate_actor(&request.actor_ref)?;
        validate_reason(&request.reason)?;
        validate_correlation(request.correlation_id)?;
        if request.source_version == 0 || request.expected_published_version == 0 {
            return Err(PersistenceError::InvalidData(
                "corporate identity rollback version is invalid".into(),
            ));
        }
        let mut tx = self.control.pools.write.begin().await?;
        let row = sqlx::query(
            "SELECT scope_kind, holding_id, canonical_holding_id, business_unit,
                    current_draft_version, current_published_version, suspended_at
             FROM erpx_control.corporate_identity_profiles
             WHERE profile_id = $1 FOR UPDATE",
        )
        .bind(request.profile_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "corporate identity profile",
            id: request.profile_id,
        })?;
        let actual = optional_u64(&row, "current_published_version")?;
        if actual != Some(request.expected_published_version)
            || required_u64(&row, "current_draft_version")? != 0
            || row
                .try_get::<Option<DateTime<Utc>>, _>("suspended_at")?
                .is_some()
        {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: "corporate_identity".into(),
                aggregate_id: request.profile_id,
                expected: Some(request.expected_published_version),
                actual,
            });
        }
        let mut restored =
            load_version(&mut tx, request.profile_id, request.source_version).await?;
        restored.version = request.expected_published_version.saturating_add(1);
        restored.lifecycle = BrandLifecycle::Published;
        restored.updated_by = request.actor_ref.clone();
        let scope = decode_scope(&row)?;
        if !matches!(scope, BrandScope::Platform) {
            let parent = load_parent_published(&mut tx, &scope)
                .await?
                .ok_or_else(|| PersistenceError::InvalidData("brand parent unavailable".into()))?;
            restored.parent_published_version = Some(parent.version);
            restored.assert_parent(&parent).map_err(contract_error)?;
        }
        restored.validate(true).map_err(contract_error)?;
        sqlx::query(
            "UPDATE erpx_control.corporate_identity_versions
             SET lifecycle = 'archived', archived_at = $3
             WHERE profile_id = $1 AND version = $2 AND lifecycle = 'published'",
        )
        .bind(request.profile_id)
        .bind(checked_i64(request.expected_published_version)?)
        .bind(request.at)
        .execute(&mut *tx)
        .await?;
        insert_version(
            &mut tx,
            &restored,
            &request.reason,
            request.correlation_id,
            request.at,
            Some(request.at),
        )
        .await?;
        sqlx::query(
            "UPDATE erpx_control.corporate_identity_profiles
             SET current_published_version = $2, updated_at = $3
             WHERE profile_id = $1",
        )
        .bind(request.profile_id)
        .bind(checked_i64(restored.version)?)
        .bind(request.at)
        .execute(&mut *tx)
        .await?;
        append_event(
            &mut tx,
            "corporate_identity.rolled_back",
            &restored,
            &request.reason,
            request.correlation_id,
            request.at,
        )
        .await?;
        tx.commit().await?;
        Ok(restored)
    }

    pub async fn draft(
        &self,
        scope: &BrandScope,
    ) -> Result<Option<CorporateIdentityProfile>, PersistenceError> {
        scope.validate().map_err(contract_error)?;
        let parts = scope_parts(scope);
        let row = sqlx::query(
            "SELECT profile_id, current_draft_version
             FROM erpx_control.corporate_identity_profiles
             WHERE scope_key = $1",
        )
        .bind(parts.scope_key)
        .fetch_optional(&self.control.pools.write)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let version = required_u64(&row, "current_draft_version")?;
        if version == 0 {
            return Ok(None);
        }
        load_version_from_pool(
            &self.control.pools.write,
            row.try_get("profile_id")?,
            version,
        )
        .await
        .map(Some)
    }

    pub async fn profile_scope(&self, profile_id: Uuid) -> Result<BrandScope, PersistenceError> {
        if profile_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "corporate identity profile ID is invalid".into(),
            ));
        }
        let row = sqlx::query(
            "SELECT scope_kind, holding_id, canonical_holding_id, business_unit
             FROM erpx_control.corporate_identity_profiles
             WHERE profile_id = $1",
        )
        .bind(profile_id)
        .fetch_optional(&self.control.pools.write)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "corporate identity profile",
            id: profile_id,
        })?;
        decode_scope(&row)
    }

    pub async fn resolved(
        &self,
        holding_id: Option<Uuid>,
        business_unit: Option<erpx_domain::data_plane::BusinessUnitId>,
    ) -> Result<ResolvedCorporateIdentity, PersistenceError> {
        let platform = self
            .published_for_scope(&BrandScope::Platform)
            .await?
            .unwrap_or_else(CorporateIdentityProfile::neutral_platform);
        let holding_scope = holding_id.map(|holding_id| BrandScope::Holding { holding_id });
        let holding = match holding_scope.as_ref() {
            Some(scope) => self.published_for_scope(scope).await?,
            None => None,
        };
        let unit_scope = holding_id
            .zip(business_unit)
            .map(|(holding_id, business_unit)| BrandScope::BusinessUnit {
                holding_id,
                business_unit,
            });
        let unit = match unit_scope.as_ref() {
            Some(scope) => self.published_for_scope(scope).await?,
            None => None,
        };
        resolve_corporate_identity(&platform, holding.as_ref(), unit.as_ref())
            .map_err(contract_error)
    }

    pub async fn resolved_for_verified_hostname(
        &self,
        hostname: &str,
    ) -> Result<Option<ResolvedCorporateIdentity>, PersistenceError> {
        let hostname = normalize_hostname(hostname)?;
        let row = sqlx::query(
            "SELECT profile.holding_id, profile.canonical_holding_id
             FROM erpx_control.corporate_identity_domains domain_binding
             JOIN erpx_control.corporate_identity_profiles profile
               ON profile.profile_id = domain_binding.profile_id
             WHERE domain_binding.hostname = $1
               AND domain_binding.verified_at IS NOT NULL
               AND domain_binding.revoked_at IS NULL
               AND profile.scope_kind = 'holding'
               AND profile.suspended_at IS NULL",
        )
        .bind(hostname)
        .fetch_optional(&self.control.pools.write)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let holding_id: Uuid = row.try_get("canonical_holding_id")?;
        self.resolved(Some(holding_id), None).await.map(Some)
    }

    pub async fn bind_verified_domain(
        &self,
        hostname: &str,
        profile_id: Uuid,
        verification_digest: [u8; 32],
        verified_at: DateTime<Utc>,
    ) -> Result<VerifiedBrandDomain, PersistenceError> {
        let hostname = normalize_hostname(hostname)?;
        if profile_id.is_nil() || verification_digest == [0; 32] {
            return Err(PersistenceError::InvalidData(
                "corporate identity domain binding is invalid".into(),
            ));
        }
        let result = sqlx::query(
            "INSERT INTO erpx_control.corporate_identity_domains
                (hostname, profile_id, verification_digest, verified_at)
             SELECT $1, profile_id, $3, $4
             FROM erpx_control.corporate_identity_profiles
             WHERE profile_id = $2 AND scope_kind = 'holding'
             ON CONFLICT (hostname) DO UPDATE SET
                 profile_id = EXCLUDED.profile_id,
                 verification_digest = EXCLUDED.verification_digest,
                 verified_at = EXCLUDED.verified_at,
                 revoked_at = NULL",
        )
        .bind(&hostname)
        .bind(profile_id)
        .bind(verification_digest.as_slice())
        .bind(verified_at)
        .execute(&self.control.pools.write)
        .await?;
        if result.rows_affected() != 1 {
            return Err(PersistenceError::NotFound {
                entity: "holding corporate identity",
                id: profile_id,
            });
        }
        Ok(VerifiedBrandDomain {
            hostname,
            profile_id,
            verified_at,
        })
    }

    pub async fn history(
        &self,
        scope: &BrandScope,
        limit: u32,
    ) -> Result<Vec<CorporateIdentityHistoryEntry>, PersistenceError> {
        scope.validate().map_err(contract_error)?;
        let limit = limit.clamp(1, 100);
        let parts = scope_parts(scope);
        let rows = sqlx::query(
            "SELECT version.profile_id, version.version, version.lifecycle,
                    version.parent_published_version, version.display_name,
                    version.short_name, version.tokens, version.assets,
                    version.overridden_categories, version.locked_categories,
                    version.updated_by, version.reason, version.correlation_id,
                    version.created_at, version.published_at, version.archived_at,
                    profile.scope_kind, profile.holding_id, profile.canonical_holding_id,
                    profile.business_unit
             FROM erpx_control.corporate_identity_profiles profile
             JOIN erpx_control.corporate_identity_versions version
               ON version.profile_id = profile.profile_id
             WHERE profile.scope_key = $1
             ORDER BY version.version DESC
             LIMIT $2",
        )
        .bind(parts.scope_key)
        .bind(i64::from(limit))
        .fetch_all(&self.control.pools.write)
        .await?;
        rows.iter().map(decode_history).collect()
    }

    async fn published_for_scope(
        &self,
        scope: &BrandScope,
    ) -> Result<Option<CorporateIdentityProfile>, PersistenceError> {
        scope.validate().map_err(contract_error)?;
        let parts = scope_parts(scope);
        let row = sqlx::query(
            "SELECT profile_id, current_published_version
             FROM erpx_control.corporate_identity_profiles
             WHERE scope_key = $1 AND suspended_at IS NULL",
        )
        .bind(parts.scope_key)
        .fetch_optional(&self.control.pools.write)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let Some(version) = optional_u64(&row, "current_published_version")? else {
            return Ok(None);
        };
        load_version_from_pool(
            &self.control.pools.write,
            row.try_get("profile_id")?,
            version,
        )
        .await
        .map(Some)
    }
}

struct ScopeParts {
    scope_key: String,
    scope_kind: &'static str,
    holding_id: Option<Uuid>,
    business_unit: Option<&'static str>,
}

fn scope_parts(scope: &BrandScope) -> ScopeParts {
    match scope {
        BrandScope::Platform => ScopeParts {
            scope_key: scope.canonical_key(),
            scope_kind: "platform",
            holding_id: None,
            business_unit: None,
        },
        BrandScope::Holding { holding_id } => ScopeParts {
            scope_key: scope.canonical_key(),
            scope_kind: "holding",
            holding_id: Some(*holding_id),
            business_unit: None,
        },
        BrandScope::BusinessUnit {
            holding_id,
            business_unit,
        } => ScopeParts {
            scope_key: scope.canonical_key(),
            scope_kind: "business_unit",
            holding_id: Some(*holding_id),
            business_unit: Some(business_unit.as_str()),
        },
    }
}

async fn insert_version(
    tx: &mut Transaction<'_, Postgres>,
    profile: &CorporateIdentityProfile,
    reason: &str,
    correlation_id: Uuid,
    at: DateTime<Utc>,
    published_at: Option<DateTime<Utc>>,
) -> Result<(), PersistenceError> {
    let overridden = categories(&profile.overridden_categories);
    let locked = categories(&profile.locked_categories);
    sqlx::query(
        "INSERT INTO erpx_control.corporate_identity_versions
            (profile_id, version, lifecycle, parent_published_version,
             display_name, short_name, tokens, assets, overridden_categories,
             locked_categories, updated_by, reason, correlation_id,
             created_at, published_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
    )
    .bind(profile.profile_id)
    .bind(checked_i64(profile.version)?)
    .bind(profile.lifecycle.as_str())
    .bind(
        profile
            .parent_published_version
            .map(checked_i64)
            .transpose()?,
    )
    .bind(&profile.display_name)
    .bind(&profile.short_name)
    .bind(serde_json::to_value(&profile.tokens)?)
    .bind(serde_json::to_value(&profile.assets)?)
    .bind(overridden)
    .bind(locked)
    .bind(&profile.updated_by)
    .bind(reason)
    .bind(correlation_id)
    .bind(at)
    .bind(published_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn load_parent_published(
    tx: &mut Transaction<'_, Postgres>,
    child_scope: &BrandScope,
) -> Result<Option<CorporateIdentityProfile>, PersistenceError> {
    let parent_scope_key = match child_scope {
        BrandScope::Platform => return Ok(None),
        BrandScope::Holding { .. } => "platform".to_owned(),
        BrandScope::BusinessUnit { holding_id, .. } => BrandScope::Holding {
            holding_id: *holding_id,
        }
        .canonical_key(),
    };
    let row = sqlx::query(
        "SELECT profile_id, current_published_version
         FROM erpx_control.corporate_identity_profiles
         WHERE scope_key = $1 AND suspended_at IS NULL",
    )
    .bind(parent_scope_key)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else { return Ok(None) };
    let Some(version) = optional_u64(&row, "current_published_version")? else {
        return Ok(None);
    };
    load_version(tx, row.try_get("profile_id")?, version)
        .await
        .map(Some)
}

async fn load_version(
    tx: &mut Transaction<'_, Postgres>,
    profile_id: Uuid,
    version: u64,
) -> Result<CorporateIdentityProfile, PersistenceError> {
    let row = sqlx::query(
        "SELECT version.profile_id, version.version, version.lifecycle,
                version.parent_published_version, version.display_name,
                version.short_name, version.tokens, version.assets,
                version.overridden_categories, version.locked_categories,
                version.updated_by, profile.scope_kind, profile.holding_id,
                profile.canonical_holding_id, profile.business_unit
         FROM erpx_control.corporate_identity_versions version
         JOIN erpx_control.corporate_identity_profiles profile
           ON profile.profile_id = version.profile_id
         WHERE version.profile_id = $1 AND version.version = $2",
    )
    .bind(profile_id)
    .bind(checked_i64(version)?)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(PersistenceError::NotFound {
        entity: "corporate identity version",
        id: profile_id,
    })?;
    decode_profile(&row)
}

async fn load_version_from_pool(
    pool: &sqlx::PgPool,
    profile_id: Uuid,
    version: u64,
) -> Result<CorporateIdentityProfile, PersistenceError> {
    let row = sqlx::query(
        "SELECT version.profile_id, version.version, version.lifecycle,
                version.parent_published_version, version.display_name,
                version.short_name, version.tokens, version.assets,
                version.overridden_categories, version.locked_categories,
                version.updated_by, profile.scope_kind, profile.holding_id,
                profile.canonical_holding_id, profile.business_unit
         FROM erpx_control.corporate_identity_versions version
         JOIN erpx_control.corporate_identity_profiles profile
           ON profile.profile_id = version.profile_id
         WHERE version.profile_id = $1 AND version.version = $2",
    )
    .bind(profile_id)
    .bind(checked_i64(version)?)
    .fetch_optional(pool)
    .await?
    .ok_or(PersistenceError::NotFound {
        entity: "corporate identity version",
        id: profile_id,
    })?;
    decode_profile(&row)
}

fn decode_profile(
    row: &sqlx::postgres::PgRow,
) -> Result<CorporateIdentityProfile, PersistenceError> {
    let lifecycle = match row.try_get::<String, _>("lifecycle")?.as_str() {
        "draft" => BrandLifecycle::Draft,
        "published" => BrandLifecycle::Published,
        "archived" => BrandLifecycle::Archived,
        "suspended" => BrandLifecycle::Suspended,
        _ => {
            return Err(PersistenceError::InvalidData(
                "unknown brand lifecycle".into(),
            ));
        }
    };
    let profile = CorporateIdentityProfile {
        profile_id: row.try_get("profile_id")?,
        scope: decode_scope(row)?,
        version: required_u64(row, "version")?,
        parent_published_version: optional_u64(row, "parent_published_version")?,
        lifecycle,
        display_name: row.try_get("display_name")?,
        short_name: row.try_get("short_name")?,
        tokens: serde_json::from_value(row.try_get("tokens")?)?,
        assets: serde_json::from_value(row.try_get("assets")?)?,
        overridden_categories: decode_categories(row.try_get("overridden_categories")?)?,
        locked_categories: decode_categories(row.try_get("locked_categories")?)?,
        updated_by: row.try_get("updated_by")?,
    };
    profile.validate(false).map_err(contract_error)?;
    Ok(profile)
}

fn decode_scope(row: &sqlx::postgres::PgRow) -> Result<BrandScope, PersistenceError> {
    match row.try_get::<String, _>("scope_kind")?.as_str() {
        "platform" => Ok(BrandScope::Platform),
        "holding" => Ok(BrandScope::Holding {
            holding_id: row.try_get("canonical_holding_id")?,
        }),
        "business_unit" => Ok(BrandScope::BusinessUnit {
            holding_id: row.try_get("canonical_holding_id")?,
            business_unit: erpx_domain::data_plane::BusinessUnitId::parse(
                &row.try_get::<String, _>("business_unit")?,
            )
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?,
        }),
        _ => Err(PersistenceError::InvalidData("unknown brand scope".into())),
    }
}

fn decode_history(
    row: &sqlx::postgres::PgRow,
) -> Result<CorporateIdentityHistoryEntry, PersistenceError> {
    Ok(CorporateIdentityHistoryEntry {
        profile: decode_profile(row)?,
        reason: row.try_get("reason")?,
        correlation_id: row.try_get("correlation_id")?,
        created_at: row.try_get("created_at")?,
        published_at: row.try_get("published_at")?,
        archived_at: row.try_get("archived_at")?,
    })
}

fn categories(values: &BTreeSet<BrandCategory>) -> Vec<&'static str> {
    values.iter().map(|category| category.as_str()).collect()
}

fn decode_categories(values: Vec<String>) -> Result<BTreeSet<BrandCategory>, PersistenceError> {
    values
        .iter()
        .map(|value| BrandCategory::parse(value).map_err(contract_error))
        .collect()
}

#[allow(clippy::too_many_arguments)]
async fn append_event(
    tx: &mut Transaction<'_, Postgres>,
    event_type: &str,
    profile: &CorporateIdentityProfile,
    reason: &str,
    correlation_id: Uuid,
    at: DateTime<Utc>,
) -> Result<(), PersistenceError> {
    let payload = json!({
        "scope": profile.scope.canonical_key(),
        "version": profile.version,
        "lifecycle": profile.lifecycle.as_str(),
        "overridden_categories": categories(&profile.overridden_categories),
        "locked_categories": categories(&profile.locked_categories),
        "contains_executable_css": false,
        "contains_secret_values": false,
    });
    let active_scope = profile.scope.holding_id().map(|holding_id| {
        json!({
            "holding_id": holding_id,
            "holding_id": profile.scope.holding_id(),
            "business_unit": profile.scope.business_unit().map(|unit| unit.as_str()),
        })
    });
    sqlx::query(
        "INSERT INTO erpx_control.outbox_events
            (id, event_type, event_version, aggregate_type, aggregate_id,
             aggregate_version, active_scope, actor, correlation_id,
             idempotency_key, payload, occurred_at)
         VALUES ($1, $2, 1, 'corporate_identity', $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(Uuid::new_v4())
    .bind(event_type)
    .bind(profile.profile_id)
    .bind(checked_i64(profile.version)?)
    .bind(active_scope)
    .bind(json!({"principal_ref": profile.updated_by}))
    .bind(correlation_id)
    .bind(format!(
        "brand:{event_type}:{}:{}",
        profile.profile_id, profile.version
    ))
    .bind(&payload)
    .bind(at)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO erpx_control.audit_events
            (id, actor_ref, customer_account_id, action, purpose,
             correlation_id, payload, recorded_at)
         VALUES ($1, $2, NULL, $3, $4, $5, $6, $7)",
    )
    .bind(Uuid::new_v4())
    .bind(&profile.updated_by)
    .bind(event_type)
    .bind(reason)
    .bind(correlation_id)
    .bind(payload)
    .bind(at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn normalize_hostname(value: &str) -> Result<String, PersistenceError> {
    let value = value.trim().trim_end_matches('.').to_ascii_lowercase();
    if value.is_empty()
        || value.len() > 253
        || value.contains('/')
        || value.contains('\\')
        || value.contains('?')
        || value.contains('#')
        || value.contains(':')
        || value.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    {
        Err(PersistenceError::InvalidData(
            "corporate identity hostname is invalid".into(),
        ))
    } else {
        Ok(value)
    }
}

fn validate_actor(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
        Err(PersistenceError::InvalidData(
            "brand actor is invalid".into(),
        ))
    } else {
        Ok(())
    }
}

fn validate_reason(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        Err(PersistenceError::InvalidData(
            "brand reason is invalid".into(),
        ))
    } else {
        Ok(())
    }
}

fn validate_correlation(value: Uuid) -> Result<(), PersistenceError> {
    if value.is_nil() {
        Err(PersistenceError::InvalidData(
            "brand correlation ID is invalid".into(),
        ))
    } else {
        Ok(())
    }
}

fn checked_i64(value: u64) -> Result<i64, PersistenceError> {
    i64::try_from(value)
        .map_err(|_| PersistenceError::InvalidData("brand version is invalid".into()))
}

fn required_u64(row: &sqlx::postgres::PgRow, field: &str) -> Result<u64, PersistenceError> {
    let value: i64 = row.try_get(field)?;
    u64::try_from(value).map_err(|_| PersistenceError::InvalidData(format!("{field} is invalid")))
}

fn optional_u64(row: &sqlx::postgres::PgRow, field: &str) -> Result<Option<u64>, PersistenceError> {
    row.try_get::<Option<i64>, _>(field)?
        .map(|value| {
            u64::try_from(value)
                .map_err(|_| PersistenceError::InvalidData(format!("{field} is invalid")))
        })
        .transpose()
}

fn contract_error(error: CorporateIdentityError) -> PersistenceError {
    PersistenceError::InvalidData(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostname_normalization_rejects_urls_and_ambiguous_hosts() {
        assert_eq!(
            normalize_hostname("Brand.Example.COM.").unwrap(),
            "brand.example.com"
        );
        assert!(normalize_hostname("https://brand.example.com").is_err());
        assert!(normalize_hostname("brand..example.com").is_err());
        assert!(normalize_hostname("-brand.example.com").is_err());
    }

    #[test]
    fn scope_parts_preserve_all_three_layers() {
        let holding_id = Uuid::new_v4();
        let platform = scope_parts(&BrandScope::Platform);
        assert_eq!(platform.scope_key, "platform");
        let unit_scope = BrandScope::BusinessUnit {
            holding_id,
            business_unit: erpx_domain::data_plane::BusinessUnitId::Finance,
        };
        let unit = scope_parts(&unit_scope);
        assert_eq!(unit.scope_kind, "business_unit");
        assert_eq!(unit.business_unit, Some("finance"));
    }
}
