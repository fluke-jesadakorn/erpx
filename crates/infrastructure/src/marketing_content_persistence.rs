//! Tenant-write persistence and replacement-read rebuilding for Marketing
//! content metadata. Object bytes and bearer tokens remain outside this
//! boundary; only validated opaque references are stored.

use chrono::{DateTime, Duration, Utc};
use erpx_domain::{
    contracts::{CONTRACT_VERSION, CommandContext, EventEnvelope, TenantScope},
    marketing_content::{
        ContentAsset, ContentForm, ContentFormRef, ContentTemplate, ContentTemplateRef,
        CustomDomain, CustomDomainBinding, CustomDomainStatus, FormStatus, MarketingContentError,
        PreviewGrant,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::{PersistenceError, finance_persistence::begin_scoped};

pub const MARKETING_CONTENT_TEMPLATE_AGGREGATE: &str = "marketing_content_template";
pub const MARKETING_CONTENT_ASSET_AGGREGATE: &str = "marketing_content_asset";
pub const MARKETING_CONTENT_TEMPLATE_SAVED_EVENT: &str = "marketing.content_template_saved.v1";
pub const MARKETING_CONTENT_ASSET_SAVED_EVENT: &str = "marketing.content_asset_saved.v1";
pub const MARKETING_CONTENT_FORM_AGGREGATE: &str = "marketing_content_form";
pub const MARKETING_CONTENT_FORM_SAVED_EVENT: &str = "marketing.content_form_saved.v1";
pub const MARKETING_CONTENT_DOMAIN_AGGREGATE: &str = "marketing_content_domain";
pub const MARKETING_CONTENT_DOMAIN_SAVED_EVENT: &str = "marketing.content_domain_saved.v1";
pub const MARKETING_CONTENT_PREVIEW_AGGREGATE: &str = "marketing_content_preview";
pub const MARKETING_CONTENT_PREVIEW_ISSUED_EVENT: &str = "marketing.content_preview_issued.v1";
pub const MARKETING_CONTENT_PREVIEW_REVOKED_EVENT: &str = "marketing.content_preview_revoked.v1";
pub const MARKETING_CONTENT_PREVIEW_EXPIRED_EVENT: &str = "marketing.content_preview_expired.v1";
pub const MARKETING_CONTENT_FORM_SUBMISSION_AGGREGATE: &str = "marketing_content_form_submission";
pub const MARKETING_CONTENT_FORM_SUBMITTED_EVENT: &str = "marketing.content_form_submitted.v1";
pub const MARKETING_CONTENT_FORM_SUBMISSION_REDACTED_EVENT: &str =
    "marketing.content_form_submission_redacted.v1";
pub const MARKETING_CONTENT_DOMAIN_VERIFICATION_AGGREGATE: &str =
    "marketing_content_domain_verification";
pub const MARKETING_CONTENT_DOMAIN_VERIFICATION_ISSUED_EVENT: &str =
    "marketing.content_domain_verification_issued.v1";
pub const MARKETING_CONTENT_DOMAIN_VERIFIED_EVENT: &str = "marketing.content_domain_verified.v1";
pub const MARKETING_CONTENT_DOMAIN_BINDING_AGGREGATE: &str = "marketing_content_domain_binding";
pub const MARKETING_CONTENT_DOMAIN_BOUND_EVENT: &str = "marketing.content_domain_bound.v1";
pub const MARKETING_CONTENT_PROJECTION: &str = "marketing_content";
pub const MARKETING_CONTENT_WORKFLOW_PROJECTION: &str = "marketing_content_workflow";
pub const MAX_MARKETING_CONTENT_READ_PAGE_SIZE: u32 = 100;
const MAX_MARKETING_CONTENT_READ_LABEL_BYTES: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingContentReadRow {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub scope: TenantScope,
    pub label: Option<String>,
    pub status: String,
    pub source_version: u64,
    pub source_event_id: Uuid,
    pub projected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingContentWriteReceipt {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub version: u64,
    pub idempotency_key: String,
    pub event_id: Uuid,
    pub replayed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingContentProjectionRebuildReport {
    pub projected_rows: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingContentWorkflowProjectionRebuildReport {
    pub previews: u64,
    pub submissions: u64,
    pub domains: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingContentPrivacySweepReport {
    pub scanned: u64,
    pub redacted: u64,
}

pub const MARKETING_CONTENT_FORM_SUBMISSION_RETENTION: Duration = Duration::days(365);
pub const MARKETING_CONTENT_PRIVACY_MAX_BATCH_SIZE: u32 = 100;

#[derive(Debug, Clone)]
pub struct SqlxMarketingContentStore {
    write_pool: PgPool,
    read_pool: PgPool,
    holding_ref: String,
}

impl SqlxMarketingContentStore {
    pub fn new(
        write_pool: PgPool,
        read_pool: PgPool,
        holding_ref: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let holding_ref = holding_ref.into();
        validate_holding_ref(&holding_ref)?;
        Ok(Self {
            write_pool,
            read_pool,
            holding_ref,
        })
    }

    pub fn holding_ref(&self) -> &str {
        &self.holding_ref
    }

    /// Persist a template snapshot, its immutable revision row, event, and
    /// idempotency receipt atomically. `None` expected version creates the
    /// aggregate at database version one; `Some` performs an optimistic update.
    pub async fn save_template(
        &self,
        template: &ContentTemplate,
        context: &CommandContext,
    ) -> Result<MarketingContentWriteReceipt, PersistenceError> {
        template.validate().map_err(content_error)?;
        let holding_ref = self.holding_ref.clone();
        let context_owned = context.clone();
        self.save_aggregate(
            MARKETING_CONTENT_TEMPLATE_AGGREGATE,
            MARKETING_CONTENT_TEMPLATE_SAVED_EVENT,
            template.id,
            template.scope.clone(),
            serde_json::to_value(template)?,
            template.slug.clone(),
            template_status_text(template.status),
            template.latest_version,
            context,
            |tx, version| {
                let template = template.clone();
                let holding_ref = holding_ref.clone();
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO erpx_tenant.marketing_content_templates
                            (holding_ref, template_id, scope, slug, status, latest_version,
                             approved_version, published_version, rolled_back_from, state, version,
                             last_command_id, last_idempotency_key, actor, correlation_id,
                             created_at, updated_at)
                         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)
                         ON CONFLICT (holding_ref, template_id) DO UPDATE SET
                            scope=EXCLUDED.scope, slug=EXCLUDED.slug, status=EXCLUDED.status,
                            latest_version=EXCLUDED.latest_version,
                            approved_version=EXCLUDED.approved_version,
                            published_version=EXCLUDED.published_version,
                            rolled_back_from=EXCLUDED.rolled_back_from, state=EXCLUDED.state,
                            version=EXCLUDED.version, last_command_id=EXCLUDED.last_command_id,
                            last_idempotency_key=EXCLUDED.last_idempotency_key, actor=EXCLUDED.actor,
                            correlation_id=EXCLUDED.correlation_id, updated_at=EXCLUDED.updated_at",
                    )
                    .bind(&holding_ref)
                    .bind(template.id)
                    .bind(serde_json::to_value(&template.scope)?)
                    .bind(&template.slug)
                    .bind(template_status_text(template.status))
                    .bind(as_i64(template.latest_version, "template latest version")?)
                    .bind(template.approved_version.map(|value| as_i64(value, "approved version")).transpose()?)
                    .bind(template.published_version.map(|value| as_i64(value, "published version")).transpose()?)
                    .bind(template.rolled_back_from.map(|value| as_i64(value, "rollback version")).transpose()?)
                    .bind(serde_json::to_value(&template)?)
                    .bind(as_i64(version, "template aggregate version")?)
                    .bind(context_owned.command_id)
                    .bind(&context_owned.idempotency_key)
                    .bind(serde_json::to_value(&context_owned.actor)?)
                    .bind(context_owned.correlation_id)
                    .bind(template_version_created_at(&template)?)
                    .bind(template_version_updated_at(&template)?)
                    .execute(&mut **tx)
                    .await?;
                    for revision in template.versions.values() {
                        let revision_scope = serde_json::to_value(&revision.scope)?;
                        let revision_state = serde_json::to_value(revision)?;
                        sqlx::query(
                            "INSERT INTO erpx_tenant.marketing_content_template_versions
                                (holding_ref, template_id, template_version_id, version, scope,
                                 state, created_by, created_at)
                             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
                             ON CONFLICT (holding_ref, template_id, version) DO NOTHING",
                        )
                        .bind(&holding_ref)
                        .bind(template.id)
                        .bind(revision.id)
                        .bind(as_i64(revision.version, "template revision")?)
                        .bind(&revision_scope)
                        .bind(&revision_state)
                        .bind(&revision.created_by)
                        .bind(revision.created_at)
                        .execute(&mut **tx)
                        .await?;
                        let stored = sqlx::query(
                            "SELECT template_version_id, scope, state, created_by, created_at
                               FROM erpx_tenant.marketing_content_template_versions
                              WHERE holding_ref=$1 AND template_id=$2 AND version=$3",
                        )
                        .bind(&holding_ref)
                        .bind(template.id)
                        .bind(as_i64(revision.version, "template revision")?)
                        .fetch_one(&mut **tx)
                        .await?;
                        if stored.get::<Uuid, _>("template_version_id") != revision.id
                            || stored.get::<Value, _>("scope") != revision_scope
                            || stored.get::<Value, _>("state") != revision_state
                            || stored.get::<String, _>("created_by") != revision.created_by
                            || stored.get::<chrono::DateTime<Utc>, _>("created_at")
                                != revision.created_at
                        {
                            return Err(invalid(
                                "marketing template revision is immutable and conflicts with the stored fact",
                            ));
                        }
                    }
                    Ok(())
                })
            },
        )
        .await
    }

    /// Persist a versioned asset snapshot and all immutable asset revisions.
    pub async fn save_asset(
        &self,
        asset: &ContentAsset,
        context: &CommandContext,
    ) -> Result<MarketingContentWriteReceipt, PersistenceError> {
        asset.validate().map_err(content_error)?;
        let holding_ref = self.holding_ref.clone();
        let context_owned = context.clone();
        self.save_aggregate(
            MARKETING_CONTENT_ASSET_AGGREGATE,
            MARKETING_CONTENT_ASSET_SAVED_EVENT,
            asset.id,
            asset.scope.clone(),
            serde_json::to_value(asset)?,
            asset.name.clone(),
            "active",
            asset.versions.len() as u64,
            context,
            |tx, version| {
                let asset = asset.clone();
                let holding_ref = holding_ref.clone();
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO erpx_tenant.marketing_content_assets
                            (holding_ref, asset_id, scope, name, latest_version, state, version,
                             last_command_id, last_idempotency_key, actor, correlation_id,
                             created_at, updated_at)
                         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
                         ON CONFLICT (holding_ref, asset_id) DO UPDATE SET
                            scope=EXCLUDED.scope, name=EXCLUDED.name,
                            latest_version=EXCLUDED.latest_version, state=EXCLUDED.state,
                            version=EXCLUDED.version, last_command_id=EXCLUDED.last_command_id,
                            last_idempotency_key=EXCLUDED.last_idempotency_key, actor=EXCLUDED.actor,
                            correlation_id=EXCLUDED.correlation_id, updated_at=EXCLUDED.updated_at",
                    )
                    .bind(&holding_ref)
                    .bind(asset.id)
                    .bind(serde_json::to_value(&asset.scope)?)
                    .bind(&asset.name)
                    .bind(as_i64(asset.versions.len() as u64, "asset latest version")?)
                    .bind(serde_json::to_value(&asset)?)
                    .bind(as_i64(version, "asset aggregate version")?)
                    .bind(context_owned.command_id)
                    .bind(&context_owned.idempotency_key)
                    .bind(serde_json::to_value(&context_owned.actor)?)
                    .bind(context_owned.correlation_id)
                    .bind(asset_created_at(&asset)?)
                    .bind(asset_updated_at(&asset)?)
                    .execute(&mut **tx)
                    .await?;
                    for revision in asset.versions.values() {
                        let revision_scope = serde_json::to_value(&revision.scope)?;
                        let revision_object_ref = serde_json::to_value(&revision.object_ref)?;
                        sqlx::query(
                            "INSERT INTO erpx_tenant.marketing_content_asset_versions
                                (holding_ref, asset_id, asset_version_id, version, scope,
                                 object_ref, content_sha256, byte_length, media_type, created_by, created_at)
                             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
                             ON CONFLICT (holding_ref, asset_id, version) DO NOTHING",
                        )
                        .bind(&holding_ref)
                        .bind(asset.id)
                        .bind(revision.id)
                        .bind(as_i64(revision.version, "asset revision")?)
                        .bind(&revision_scope)
                        .bind(&revision_object_ref)
                        .bind(&revision.content_sha256)
                        .bind(i64::try_from(revision.byte_length).map_err(|_| invalid("asset byte length exceeds BIGINT"))?)
                        .bind(&revision.media_type)
                        .bind(&revision.created_by)
                        .bind(revision.created_at)
                        .execute(&mut **tx)
                        .await?;
                        let stored = sqlx::query(
                            "SELECT asset_version_id, scope, object_ref, content_sha256,
                                    byte_length, media_type, created_by, created_at
                               FROM erpx_tenant.marketing_content_asset_versions
                              WHERE holding_ref=$1 AND asset_id=$2 AND version=$3",
                        )
                        .bind(&holding_ref)
                        .bind(asset.id)
                        .bind(as_i64(revision.version, "asset revision")?)
                        .fetch_one(&mut **tx)
                        .await?;
                        if stored.get::<Uuid, _>("asset_version_id") != revision.id
                            || stored.get::<Value, _>("scope") != revision_scope
                            || stored.get::<Value, _>("object_ref") != revision_object_ref
                            || stored.get::<String, _>("content_sha256")
                                != revision.content_sha256
                            || stored.get::<i64, _>("byte_length")
                                != i64::try_from(revision.byte_length).map_err(|_| {
                                    invalid("asset byte length exceeds BIGINT")
                                })?
                            || stored.get::<String, _>("media_type") != revision.media_type
                            || stored.get::<String, _>("created_by") != revision.created_by
                            || stored.get::<chrono::DateTime<Utc>, _>("created_at")
                                != revision.created_at
                        {
                            return Err(invalid(
                                "marketing asset revision is immutable and conflicts with the stored fact",
                            ));
                        }
                    }
                    Ok(())
                })
            },
        )
        .await
    }

    pub async fn save_form(
        &self,
        form: &ContentForm,
        context: &CommandContext,
    ) -> Result<MarketingContentWriteReceipt, PersistenceError> {
        form.validate().map_err(content_error)?;
        let holding_ref = self.holding_ref.clone();
        let context_owned = context.clone();
        self.save_aggregate(
            MARKETING_CONTENT_FORM_AGGREGATE,
            MARKETING_CONTENT_FORM_SAVED_EVENT,
            form.id,
            form.scope.clone(),
            serde_json::to_value(form)?,
            form.slug.clone(),
            form_status_text(form.status),
            0,
            context,
            |tx, version| {
                let form = form.clone();
                let holding_ref = holding_ref.clone();
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO erpx_tenant.marketing_content_forms
                            (holding_ref, form_id, scope, slug, status, state, version,
                             last_command_id, last_idempotency_key, actor, correlation_id,
                             created_at, updated_at)
                         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,now(),now())
                         ON CONFLICT (holding_ref, form_id) DO UPDATE SET
                            scope=EXCLUDED.scope, slug=EXCLUDED.slug, status=EXCLUDED.status,
                            state=EXCLUDED.state, version=EXCLUDED.version,
                            last_command_id=EXCLUDED.last_command_id,
                            last_idempotency_key=EXCLUDED.last_idempotency_key,
                            actor=EXCLUDED.actor, correlation_id=EXCLUDED.correlation_id,
                            updated_at=now()",
                    )
                    .bind(&holding_ref)
                    .bind(form.id)
                    .bind(serde_json::to_value(&form.scope)?)
                    .bind(&form.slug)
                    .bind(form_status_text(form.status))
                    .bind(serde_json::to_value(&form)?)
                    .bind(as_i64(version, "form aggregate version")?)
                    .bind(context_owned.command_id)
                    .bind(&context_owned.idempotency_key)
                    .bind(serde_json::to_value(&context_owned.actor)?)
                    .bind(context_owned.correlation_id)
                    .execute(&mut **tx)
                    .await?;
                    Ok(())
                })
            },
        )
        .await
    }

    pub async fn save_domain(
        &self,
        domain: &CustomDomain,
        context: &CommandContext,
    ) -> Result<MarketingContentWriteReceipt, PersistenceError> {
        domain.validate().map_err(content_error)?;
        let holding_ref = self.holding_ref.clone();
        let context_owned = context.clone();
        self.save_aggregate(
            MARKETING_CONTENT_DOMAIN_AGGREGATE,
            MARKETING_CONTENT_DOMAIN_SAVED_EVENT,
            domain.id,
            domain.scope.clone(),
            serde_json::to_value(domain)?,
            domain.hostname.clone(),
            domain_status_text(domain.status),
            0,
            context,
            |tx, version| {
                let domain = domain.clone();
                let holding_ref = holding_ref.clone();
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO erpx_tenant.marketing_content_domains
                            (holding_ref, domain_id, scope, hostname, status, verification_hash,
                             state, version, last_command_id, last_idempotency_key, actor,
                             correlation_id, created_at, updated_at)
                         VALUES ($1,$2,$3,$4,$5,NULL,$6,$7,$8,$9,$10,$11,now(),now())
                         ON CONFLICT (holding_ref, domain_id) DO UPDATE SET
                            scope=EXCLUDED.scope, hostname=EXCLUDED.hostname,
                            status=EXCLUDED.status, state=EXCLUDED.state,
                            version=EXCLUDED.version, last_command_id=EXCLUDED.last_command_id,
                            last_idempotency_key=EXCLUDED.last_idempotency_key,
                            actor=EXCLUDED.actor, correlation_id=EXCLUDED.correlation_id,
                            updated_at=now()",
                    )
                    .bind(&holding_ref)
                    .bind(domain.id)
                    .bind(serde_json::to_value(&domain.scope)?)
                    .bind(&domain.hostname)
                    .bind(domain_status_text(domain.status))
                    .bind(serde_json::to_value(&domain)?)
                    .bind(as_i64(version, "domain aggregate version")?)
                    .bind(context_owned.command_id)
                    .bind(&context_owned.idempotency_key)
                    .bind(serde_json::to_value(&context_owned.actor)?)
                    .bind(context_owned.correlation_id)
                    .execute(&mut **tx)
                    .await?;
                    Ok(())
                })
            },
        )
        .await
    }

    /// Persist a preview grant without ever storing its bearer token. The
    /// caller supplies only a fixed-width hash produced by the auth boundary.
    pub async fn issue_preview(
        &self,
        grant: &PreviewGrant,
        token_hash: &str,
        context: &CommandContext,
    ) -> Result<MarketingContentWriteReceipt, PersistenceError> {
        grant.validate().map_err(content_error)?;
        validate_sha256(token_hash, "preview token hash")?;
        validate_context(context, &grant.template.scope)?;
        let payload = serde_json::json!({ "preview": grant });
        let request_material = serde_json::json!({
            "preview": grant,
            "token_hash": token_hash,
        });
        let holding_ref = self.holding_ref.clone();
        let grant = grant.clone();
        let token_hash = token_hash.to_ascii_lowercase();
        self.save_fact(
            MARKETING_CONTENT_PREVIEW_AGGREGATE,
            MARKETING_CONTENT_PREVIEW_ISSUED_EVENT,
            grant.id,
            grant.template.scope.clone(),
            payload,
            request_material,
            context,
            move |tx, _version| {
                let grant = grant.clone();
                let holding_ref = holding_ref.clone();
                let token_hash = token_hash.clone();
                Box::pin(async move {
                    let template = sqlx::query(
                        "SELECT v.scope, t.published_version
                           FROM erpx_tenant.marketing_content_template_versions v
                           JOIN erpx_tenant.marketing_content_templates t
                             ON t.holding_ref=v.holding_ref AND t.template_id=v.template_id
                          WHERE v.holding_ref=$1 AND v.template_id=$2 AND v.version=$3",
                    )
                    .bind(&holding_ref)
                    .bind(grant.template.template_id)
                    .bind(as_i64(grant.template.version, "preview template version")?)
                    .fetch_optional(&mut **tx)
                    .await?
                    .ok_or_else(|| invalid("preview template version does not exist"))?;
                    let template_scope: TenantScope =
                        serde_json::from_value(template.get("scope"))?;
                    if template_scope != grant.template.scope
                        || template.get::<Option<i64>, _>("published_version")
                            != Some(as_i64(grant.template.version, "published preview version")?)
                    {
                        return Err(invalid("preview requires the published template version"));
                    }
                    let already_exists = sqlx::query(
                        "SELECT 1 FROM erpx_tenant.marketing_content_previews
                          WHERE holding_ref=$1 AND preview_id=$2",
                    )
                    .bind(&holding_ref)
                    .bind(grant.id)
                    .fetch_optional(&mut **tx)
                    .await?
                    .is_some();
                    if already_exists {
                        return Err(invalid("preview grant already exists"));
                    }
                    sqlx::query(
                        "INSERT INTO erpx_tenant.marketing_content_previews
                            (holding_ref, preview_id, template_id, template_version, scope,
                             token_hash, issued_at, expires_at, revoked_at)
                         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
                    )
                    .bind(&holding_ref)
                    .bind(grant.id)
                    .bind(grant.template.template_id)
                    .bind(as_i64(grant.template.version, "preview template version")?)
                    .bind(serde_json::to_value(&grant.template.scope)?)
                    .bind(token_hash)
                    .bind(grant.issued_at)
                    .bind(grant.expires_at)
                    .bind(grant.revoked_at)
                    .execute(&mut **tx)
                    .await?;
                    Ok(())
                })
            },
        )
        .await
    }

    /// Revoke a preview grant atomically and append an audit-capable event.
    pub async fn revoke_preview(
        &self,
        preview_id: Uuid,
        scope: TenantScope,
        revoked_at: chrono::DateTime<chrono::Utc>,
        context: &CommandContext,
    ) -> Result<MarketingContentWriteReceipt, PersistenceError> {
        if preview_id.is_nil() {
            return Err(invalid("preview ID cannot be nil"));
        }
        validate_context(context, &scope)?;
        let payload = serde_json::json!({
            "preview_id": preview_id,
            "revoked_at": revoked_at,
        });
        let holding_ref = self.holding_ref.clone();
        self.save_fact(
            MARKETING_CONTENT_PREVIEW_AGGREGATE,
            MARKETING_CONTENT_PREVIEW_REVOKED_EVENT,
            preview_id,
            scope.clone(),
            payload.clone(),
            payload,
            context,
            move |tx, _version| {
                let holding_ref = holding_ref.clone();
                let scope = scope.clone();
                Box::pin(async move {
                    let row = sqlx::query(
                        "SELECT scope, issued_at, revoked_at
                           FROM erpx_tenant.marketing_content_previews
                          WHERE holding_ref=$1 AND preview_id=$2 FOR UPDATE",
                    )
                    .bind(&holding_ref)
                    .bind(preview_id)
                    .fetch_optional(&mut **tx)
                    .await?
                    .ok_or_else(|| invalid("preview grant does not exist"))?;
                    let stored_scope: TenantScope = serde_json::from_value(row.get("scope"))?;
                    if stored_scope != scope {
                        return Err(content_error(MarketingContentError::ScopeMismatch));
                    }
                    let issued_at: chrono::DateTime<chrono::Utc> = row.get("issued_at");
                    if revoked_at < issued_at {
                        return Err(content_error(MarketingContentError::InvalidPreviewExpiry));
                    }
                    if row
                        .get::<Option<chrono::DateTime<chrono::Utc>>, _>("revoked_at")
                        .is_some()
                    {
                        return Err(content_error(MarketingContentError::PreviewAlreadyRevoked));
                    }
                    sqlx::query(
                        "UPDATE erpx_tenant.marketing_content_previews
                            SET revoked_at=$3
                          WHERE holding_ref=$1 AND preview_id=$2",
                    )
                    .bind(&holding_ref)
                    .bind(preview_id)
                    .bind(revoked_at)
                    .execute(&mut **tx)
                    .await?;
                    Ok(())
                })
            },
        )
        .await
    }

    /// Mark an elapsed preview as expired at its authoritative expiry
    /// instant. This is the write-side operation used by the worker; it does
    /// not accept a caller-supplied token or extend the grant lifetime.
    pub async fn expire_preview(
        &self,
        preview_id: Uuid,
        scope: TenantScope,
        expired_at: chrono::DateTime<chrono::Utc>,
        context: &CommandContext,
    ) -> Result<MarketingContentWriteReceipt, PersistenceError> {
        if preview_id.is_nil() {
            return Err(invalid("preview ID cannot be nil"));
        }
        validate_context(context, &scope)?;
        let payload = serde_json::json!({
            "preview_id": preview_id,
            "expired_at": expired_at,
        });
        // The worker's deterministic idempotency key identifies the expiry
        // operation for this preview. Its scheduling clock is deliberately
        // excluded from the request hash so a retry after a lease timeout
        // replays the committed expiry instead of becoming a false conflict.
        let request_material = serde_json::json!({
            "preview_id": preview_id,
            "scope": scope,
        });
        let holding_ref = self.holding_ref.clone();
        self.save_fact(
            MARKETING_CONTENT_PREVIEW_AGGREGATE,
            MARKETING_CONTENT_PREVIEW_EXPIRED_EVENT,
            preview_id,
            scope.clone(),
            payload.clone(),
            request_material,
            context,
            move |tx, _version| {
                let holding_ref = holding_ref.clone();
                let scope = scope.clone();
                Box::pin(async move {
                    let row = sqlx::query(
                        "SELECT scope, expires_at, revoked_at
                           FROM erpx_tenant.marketing_content_previews
                          WHERE holding_ref=$1 AND preview_id=$2 FOR UPDATE",
                    )
                    .bind(&holding_ref)
                    .bind(preview_id)
                    .fetch_optional(&mut **tx)
                    .await?
                    .ok_or_else(|| invalid("preview grant does not exist"))?;
                    let stored_scope: TenantScope = serde_json::from_value(row.get("scope"))?;
                    if stored_scope != scope {
                        return Err(content_error(MarketingContentError::ScopeMismatch));
                    }
                    let expires_at: chrono::DateTime<chrono::Utc> = row.get("expires_at");
                    if expired_at < expires_at {
                        return Err(invalid("preview has not expired"));
                    }
                    if row
                        .get::<Option<chrono::DateTime<chrono::Utc>>, _>("revoked_at")
                        .is_some()
                    {
                        return Err(content_error(MarketingContentError::PreviewAlreadyRevoked));
                    }
                    sqlx::query(
                        "UPDATE erpx_tenant.marketing_content_previews
                            SET revoked_at=$3
                          WHERE holding_ref=$1 AND preview_id=$2",
                    )
                    .bind(&holding_ref)
                    .bind(preview_id)
                    .bind(expires_at)
                    .execute(&mut **tx)
                    .await?;
                    Ok(())
                })
            },
        )
        .await
    }

    /// Return one bounded, holding-scoped batch of due preview IDs. The
    /// caller still performs each expiry under its own idempotent command;
    /// this selection is only scheduling input.
    pub async fn list_due_preview_ids(
        &self,
        scope: &TenantScope,
        now: chrono::DateTime<chrono::Utc>,
        limit: u32,
    ) -> Result<Vec<Uuid>, PersistenceError> {
        if limit == 0 || limit > 100 {
            return Err(invalid("preview expiry batch must be 1..=100"));
        }
        if matches!(scope, TenantScope::Platform) {
            return Err(invalid("preview expiry selection requires tenant scope"));
        }
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let rows = sqlx::query(
            "SELECT preview_id
               FROM erpx_tenant.marketing_content_previews
              WHERE holding_ref=$1 AND scope=$2 AND expires_at <= $3
                AND revoked_at IS NULL
              ORDER BY expires_at, preview_id
              LIMIT $4",
        )
        .bind(&self.holding_ref)
        .bind(serde_json::to_value(scope)?)
        .bind(now)
        .bind(i64::from(limit))
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(rows.into_iter().map(|row| row.get("preview_id")).collect())
    }

    /// Accept one published form submission. Values are persisted in the
    /// tenant write database, while the outbox event intentionally carries
    /// only bounded identifiers and timestamps rather than submitted PII.
    pub async fn submit_form(
        &self,
        form: &ContentFormRef,
        submission_id: Uuid,
        values: &BTreeMap<String, String>,
        received_at: chrono::DateTime<chrono::Utc>,
        context: &CommandContext,
    ) -> Result<MarketingContentWriteReceipt, PersistenceError> {
        if submission_id.is_nil() {
            return Err(invalid("form submission ID cannot be nil"));
        }
        let form = ContentFormRef::new(form.form_id, form.scope.clone()).map_err(content_error)?;
        validate_context(context, &form.scope)?;
        let payload = serde_json::json!({
            "submission_id": submission_id,
            "form_id": form.form_id,
            "received_at": received_at,
        });
        let request_material = serde_json::json!({
            "submission": payload,
            "values": values,
        });
        let holding_ref = self.holding_ref.clone();
        let idempotency_key = context.idempotency_key.clone();
        let form = form.clone();
        let values = values.clone();
        self.save_fact(
            MARKETING_CONTENT_FORM_SUBMISSION_AGGREGATE,
            MARKETING_CONTENT_FORM_SUBMITTED_EVENT,
            submission_id,
            form.scope.clone(),
            payload,
            request_material,
            context,
            move |tx, _version| {
                let holding_ref = holding_ref.clone();
                let form = form.clone();
                let values = values.clone();
                let idempotency_key = idempotency_key.clone();
                Box::pin(async move {
                    let row = sqlx::query(
                        "SELECT state FROM erpx_tenant.marketing_content_forms
                          WHERE holding_ref=$1 AND form_id=$2 FOR SHARE",
                    )
                    .bind(&holding_ref)
                    .bind(form.form_id)
                    .fetch_optional(&mut **tx)
                    .await?
                    .ok_or_else(|| invalid("form does not exist"))?;
                    let stored: ContentForm = serde_json::from_value(row.get("state"))?;
                    let stored_ref = stored.reference().map_err(content_error)?;
                    if stored_ref != form {
                        return Err(content_error(MarketingContentError::ScopeMismatch));
                    }
                    stored
                        .validate_submission(&form, &values)
                        .map_err(content_error)?;
                    let duplicate = sqlx::query(
                        "SELECT 1 FROM erpx_tenant.marketing_content_form_submissions
                          WHERE holding_ref=$1 AND submission_id=$2",
                    )
                    .bind(&holding_ref)
                    .bind(submission_id)
                    .fetch_optional(&mut **tx)
                    .await?
                    .is_some();
                    if duplicate {
                        return Err(invalid("form submission ID already exists"));
                    }
                    sqlx::query(
                        "INSERT INTO erpx_tenant.marketing_content_form_submissions
                            (holding_ref, submission_id, form_id, scope, idempotency_key,
                             values_json, received_at, retention_until)
                         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
                    )
                    .bind(&holding_ref)
                    .bind(submission_id)
                    .bind(form.form_id)
                    .bind(serde_json::to_value(&form.scope)?)
                    .bind(idempotency_key)
                    .bind(serde_json::to_value(&values)?)
                    .bind(received_at)
                    .bind(
                        received_at
                            .checked_add_signed(MARKETING_CONTENT_FORM_SUBMISSION_RETENTION)
                            .ok_or_else(|| {
                                invalid("form submission retention deadline overflow")
                            })?,
                    )
                    .execute(&mut **tx)
                    .await?;
                    Ok(())
                })
            },
        )
        .await
    }

    /// Irreversibly redact expired form values in one tenant-write
    /// transaction. Submission identity, retention metadata, and the
    /// append-only redaction event remain available for audit and replay, but
    /// the submitted values are replaced with `{}` and can never be restored
    /// by a read-model rebuild.
    pub async fn redact_expired_form_submissions(
        &self,
        now: chrono::DateTime<chrono::Utc>,
        limit: u32,
        context: &CommandContext,
    ) -> Result<MarketingContentPrivacySweepReport, PersistenceError> {
        if limit == 0 || limit > MARKETING_CONTENT_PRIVACY_MAX_BATCH_SIZE {
            return Err(invalid(format!(
                "form submission privacy batch must be 1..={MARKETING_CONTENT_PRIVACY_MAX_BATCH_SIZE}"
            )));
        }
        let holding_scope = match &context.tenant_scope {
            TenantScope::Holding { .. } => context.tenant_scope.clone(),
            _ => {
                return Err(invalid(
                    "form submission privacy sweep requires holding scope",
                ));
            }
        };
        validate_context(context, &holding_scope)?;

        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let rows = sqlx::query(
            "SELECT submission_id, scope, received_at, retention_until
               FROM erpx_tenant.marketing_content_form_submissions
              WHERE holding_ref=$1 AND retention_until <= $2 AND redacted_at IS NULL
              ORDER BY retention_until, submission_id
              LIMIT $3
              FOR UPDATE SKIP LOCKED",
        )
        .bind(&self.holding_ref)
        .bind(now)
        .bind(i64::from(limit))
        .fetch_all(&mut *tx)
        .await?;

        let scanned = rows.len() as u64;
        let mut redacted = 0_u64;
        for row in rows {
            let submission_id: Uuid = row.get("submission_id");
            let scope: TenantScope = serde_json::from_value(row.get("scope"))?;
            validate_context(context, &scope)?;
            let received_at: chrono::DateTime<chrono::Utc> = row.get("received_at");
            let retention_until: chrono::DateTime<chrono::Utc> = row.get("retention_until");
            if retention_until < received_at || retention_until > now {
                return Err(invalid("form submission retention metadata is invalid"));
            }

            let next_version = next_fact_version(
                &mut tx,
                &self.holding_ref,
                MARKETING_CONTENT_FORM_SUBMISSION_AGGREGATE,
                submission_id,
            )
            .await?;
            let redacted_at = now;
            sqlx::query(
                "UPDATE erpx_tenant.marketing_content_form_submissions
                    SET values_json='{}'::jsonb, redacted_at=$3
                  WHERE holding_ref=$1 AND submission_id=$2 AND redacted_at IS NULL",
            )
            .bind(&self.holding_ref)
            .bind(submission_id)
            .bind(redacted_at)
            .execute(&mut *tx)
            .await?;

            let mut redaction_context = context.clone();
            redaction_context.command_id = Uuid::new_v4();
            redaction_context.idempotency_key =
                format!("marketing-content-privacy-redact:{submission_id}");
            let payload = serde_json::json!({
                "submission_id": submission_id,
                "redacted_at": redacted_at,
                "reason": "retention_expired",
            });
            let event = build_event(
                &redaction_context,
                MARKETING_CONTENT_FORM_SUBMISSION_AGGREGATE,
                submission_id,
                next_version,
                scope,
                MARKETING_CONTENT_FORM_SUBMISSION_REDACTED_EVENT,
                payload.clone(),
            )?;
            append_event(&mut tx, &self.holding_ref, &event).await?;
            let request_hash = request_hash(
                MARKETING_CONTENT_FORM_SUBMISSION_AGGREGATE,
                &payload,
                &redaction_context,
            )?;
            let receipt = MarketingContentWriteReceipt {
                aggregate_type: MARKETING_CONTENT_FORM_SUBMISSION_AGGREGATE.into(),
                aggregate_id: submission_id,
                version: next_version,
                idempotency_key: redaction_context.idempotency_key.clone(),
                event_id: event.event_id,
                replayed: false,
            };
            sqlx::query(
                "INSERT INTO erpx_tenant.marketing_content_command_idempotency
                    (holding_ref, idempotency_key, operation, request_hash, aggregate_type,
                     aggregate_id, version, event_id, response)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
            )
            .bind(&self.holding_ref)
            .bind(&redaction_context.idempotency_key)
            .bind(MARKETING_CONTENT_FORM_SUBMISSION_REDACTED_EVENT)
            .bind(request_hash)
            .bind(MARKETING_CONTENT_FORM_SUBMISSION_AGGREGATE)
            .bind(submission_id)
            .bind(as_i64(next_version, "form submission privacy version")?)
            .bind(event.event_id)
            .bind(serde_json::to_value(receipt)?)
            .execute(&mut *tx)
            .await?;
            redacted = redacted.saturating_add(1);
        }
        tx.commit().await?;
        Ok(MarketingContentPrivacySweepReport { scanned, redacted })
    }

    /// Resolve a published form on the authoritative write side. Public
    /// submission transport uses this to ensure a retired or unpublished
    /// form cannot receive traffic after a read-model lag.
    pub async fn load_published_form(
        &self,
        form_id: Uuid,
    ) -> Result<Option<ContentForm>, PersistenceError> {
        if form_id.is_nil() {
            return Err(invalid("Marketing content form ID cannot be nil"));
        }
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let row = sqlx::query(
            "SELECT state FROM erpx_tenant.marketing_content_forms
              WHERE holding_ref=$1 AND form_id=$2 AND status='published'",
        )
        .bind(&self.holding_ref)
        .bind(form_id)
        .fetch_optional(&mut *tx)
        .await?;
        let form = row
            .map(|row| serde_json::from_value(row.get("state")))
            .transpose()?;
        tx.commit().await?;
        Ok(form)
    }

    /// Authorize a preview bearer token without returning or persisting the
    /// raw secret. The returned template reference is authoritative write
    /// state and is safe to use for a read-model content lookup.
    pub async fn authorize_preview(
        &self,
        preview_id: Uuid,
        raw_token: &str,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<ContentTemplateRef>, PersistenceError> {
        if preview_id.is_nil() {
            return Err(invalid("Marketing preview ID cannot be nil"));
        }
        if raw_token.trim().is_empty()
            || raw_token.len() > 512
            || raw_token.chars().any(char::is_control)
        {
            return Err(invalid("Marketing preview token is invalid"));
        }
        let token_hash = hex::encode(Sha256::digest(raw_token.as_bytes()));
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let row = sqlx::query(
            "SELECT template_id, template_version, scope
               FROM erpx_tenant.marketing_content_previews
              WHERE holding_ref=$1 AND preview_id=$2 AND token_hash=$3
                AND issued_at <= $4 AND expires_at > $4 AND revoked_at IS NULL",
        )
        .bind(&self.holding_ref)
        .bind(preview_id)
        .bind(token_hash)
        .bind(at)
        .fetch_optional(&mut *tx)
        .await?;
        let result = row
            .map(|row| {
                let template_id: Uuid = row.get("template_id");
                let version: i64 = row.get("template_version");
                let scope: TenantScope = serde_json::from_value(row.get("scope"))?;
                ContentTemplateRef::new(
                    template_id,
                    as_u64(version, "preview template version")?,
                    scope,
                )
                .map_err(content_error)
            })
            .transpose()?;
        tx.commit().await?;
        Ok(result)
    }

    /// Resolve an active custom-domain binding on the write side. The
    /// hostname is normalized only for lookup; tenant routing remains the
    /// caller's server-side registry decision.
    pub async fn load_active_domain_binding(
        &self,
        hostname: &str,
    ) -> Result<Option<CustomDomainBinding>, PersistenceError> {
        let hostname = hostname.trim().trim_end_matches('.').to_ascii_lowercase();
        if hostname.is_empty()
            || hostname.len() > 253
            || hostname
                .chars()
                .any(|ch| ch.is_control() || ch.is_whitespace())
        {
            return Err(invalid("Marketing hostname is invalid"));
        }
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let row = sqlx::query(
            "SELECT d.domain_id, d.scope AS domain_scope, d.hostname,
                    b.template_id, b.template_version, b.scope AS template_scope,
                    b.bound_at
               FROM erpx_tenant.marketing_content_domains d
               JOIN erpx_tenant.marketing_content_domain_bindings b
                 ON b.holding_ref=d.holding_ref AND b.domain_id=d.domain_id
              WHERE d.holding_ref=$1 AND d.hostname=$2 AND d.status='active'",
        )
        .bind(&self.holding_ref)
        .bind(&hostname)
        .fetch_optional(&mut *tx)
        .await?;
        let result = row
            .map(|row| {
                let domain_id: Uuid = row.get("domain_id");
                let domain_scope: TenantScope = serde_json::from_value(row.get("domain_scope"))?;
                let domain = CustomDomain {
                    id: domain_id,
                    scope: domain_scope,
                    hostname: row.get("hostname"),
                    status: CustomDomainStatus::Active,
                };
                let template = ContentTemplateRef::new(
                    row.get("template_id"),
                    as_u64(row.get("template_version"), "bound template version")?,
                    serde_json::from_value(row.get("template_scope"))?,
                )
                .map_err(content_error)?;
                domain
                    .bind_template(&template, row.get("bound_at"))
                    .map_err(content_error)
            })
            .transpose()?;
        tx.commit().await?;
        Ok(result)
    }

    /// Read published template content from the replacement database. This
    /// keeps anonymous preview serving off the authoritative write pool after
    /// the preview token has been checked.
    pub async fn load_published_template_read(
        &self,
        template_id: Uuid,
    ) -> Result<Option<ContentTemplate>, PersistenceError> {
        if template_id.is_nil() {
            return Err(invalid("Marketing content template ID cannot be nil"));
        }
        let mut tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let row = sqlx::query(
            "SELECT state FROM erpx_tenant.marketing_content_read_rows
              WHERE holding_ref=$1 AND aggregate_type='template'
                AND aggregate_id=$2 AND status='published'",
        )
        .bind(&self.holding_ref)
        .bind(template_id)
        .fetch_optional(&mut *tx)
        .await?;
        let template = row
            .map(|row| serde_json::from_value(row.get("state")))
            .transpose()?;
        tx.commit().await?;
        Ok(template)
    }

    /// List only bounded, non-secret metadata from the replacement read
    /// model. The keyset is stable across pages and the checkpoint/event
    /// joins make a stale or partially rebuilt row fail closed instead of
    /// returning metadata that cannot be reconciled to its source fact.
    pub async fn list_read_rows(
        &self,
        aggregate_type: Option<&str>,
        page_size: u32,
        after_id: Option<Uuid>,
    ) -> Result<(Vec<MarketingContentReadRow>, Option<Uuid>), PersistenceError> {
        if !(1..=MAX_MARKETING_CONTENT_READ_PAGE_SIZE).contains(&page_size) {
            return Err(invalid(format!(
                "Marketing content read page size must be 1..={MAX_MARKETING_CONTENT_READ_PAGE_SIZE}"
            )));
        }
        if let Some(aggregate_type) = aggregate_type {
            validate_marketing_content_aggregate_type(aggregate_type)?;
        }
        if after_id.is_some_and(|value| value.is_nil()) {
            return Err(invalid(
                "Marketing content read cursor aggregate ID cannot be nil",
            ));
        }

        let mut tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        if aggregate_type.is_none() {
            let duplicate_id: Option<Uuid> = sqlx::query_scalar(
                "SELECT aggregate_id
                   FROM erpx_tenant.marketing_content_read_rows
                  WHERE holding_ref=$1
                  GROUP BY aggregate_id
                 HAVING count(*) > 1
                  LIMIT 1",
            )
            .bind(&self.holding_ref)
            .fetch_optional(&mut *tx)
            .await?;
            if duplicate_id.is_some() {
                return Err(invalid(
                    "Marketing content read UUID cursor is ambiguous across aggregate types",
                ));
            }
        }
        if let Some(after_id) = after_id {
            validate_read_cursor_continuity(&mut tx, &self.holding_ref, aggregate_type, after_id)
                .await?;
        }
        let rows = sqlx::query(
            "SELECT r.aggregate_type, r.aggregate_id, r.scope, r.slug, r.status,
                    r.source_version, r.source_event_id, r.projected_at,
                    c.source_version AS checkpoint_source_version,
                    c.last_event_id AS checkpoint_event_id,
                    e.aggregate_version AS receipt_source_version
               FROM erpx_tenant.marketing_content_read_rows r
               LEFT JOIN erpx_tenant.marketing_content_projection_checkpoints c
                 ON c.holding_ref=r.holding_ref
                AND c.projection_name='marketing_content'
                AND c.aggregate_type=r.aggregate_type
                AND c.aggregate_id=r.aggregate_id
               LEFT JOIN erpx_tenant.marketing_content_projection_event_receipts e
                 ON e.holding_ref=r.holding_ref
                AND e.projection_name='marketing_content'
                AND e.event_id=r.source_event_id
                AND e.aggregate_type=r.aggregate_type
                AND e.aggregate_id=r.aggregate_id
              WHERE r.holding_ref=$1
                AND ($2::text IS NULL OR r.aggregate_type=$2)
                AND ($3::uuid IS NULL OR r.aggregate_id > $3)
              ORDER BY r.aggregate_id, r.aggregate_type
              LIMIT $4",
        )
        .bind(&self.holding_ref)
        .bind(aggregate_type)
        .bind(after_id)
        .bind(i64::from(page_size) + 1)
        .fetch_all(&mut *tx)
        .await?;

        let has_more = rows.len() > page_size as usize;
        let rows = rows
            .into_iter()
            .take(page_size as usize)
            .map(read_row_from_sql)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more
            .then(|| rows.last())
            .flatten()
            .map(|row| row.aggregate_id);
        tx.commit().await?;
        Ok((rows, next_cursor))
    }

    /// Store only a SHA-256 verification challenge for a pending domain.
    pub async fn issue_domain_verification(
        &self,
        domain_id: Uuid,
        scope: TenantScope,
        verification_hash: &str,
        context: &CommandContext,
    ) -> Result<MarketingContentWriteReceipt, PersistenceError> {
        if domain_id.is_nil() {
            return Err(invalid("domain ID cannot be nil"));
        }
        validate_sha256(verification_hash, "domain verification hash")?;
        validate_context(context, &scope)?;
        let payload = serde_json::json!({ "domain_id": domain_id });
        let request_material = serde_json::json!({
            "domain_id": domain_id,
            "verification_hash": verification_hash,
        });
        let holding_ref = self.holding_ref.clone();
        let verification_hash = verification_hash.to_ascii_lowercase();
        self.save_fact(
            MARKETING_CONTENT_DOMAIN_VERIFICATION_AGGREGATE,
            MARKETING_CONTENT_DOMAIN_VERIFICATION_ISSUED_EVENT,
            domain_id,
            scope.clone(),
            payload,
            request_material,
            context,
            move |tx, _version| {
                let holding_ref = holding_ref.clone();
                let scope = scope.clone();
                let verification_hash = verification_hash.clone();
                Box::pin(async move {
                    let row = sqlx::query(
                        "SELECT scope, status FROM erpx_tenant.marketing_content_domains
                          WHERE holding_ref=$1 AND domain_id=$2 FOR UPDATE",
                    )
                    .bind(&holding_ref)
                    .bind(domain_id)
                    .fetch_optional(&mut **tx)
                    .await?
                    .ok_or_else(|| invalid("custom domain does not exist"))?;
                    let stored_scope: TenantScope = serde_json::from_value(row.get("scope"))?;
                    if stored_scope != scope {
                        return Err(content_error(MarketingContentError::ScopeMismatch));
                    }
                    if row.get::<String, _>("status") != "pending_verification" {
                        return Err(invalid("custom domain is not pending verification"));
                    }
                    sqlx::query(
                        "UPDATE erpx_tenant.marketing_content_domains
                            SET verification_hash=$3, updated_at=now()
                          WHERE holding_ref=$1 AND domain_id=$2",
                    )
                    .bind(&holding_ref)
                    .bind(domain_id)
                    .bind(verification_hash)
                    .execute(&mut **tx)
                    .await?;
                    Ok(())
                })
            },
        )
        .await
    }

    /// Verify a domain using a pre-hashed proof. Raw DNS/HTTP challenge
    /// material never crosses this persistence boundary.
    pub async fn verify_domain(
        &self,
        domain_id: Uuid,
        scope: TenantScope,
        proof_hash: &str,
        context: &CommandContext,
    ) -> Result<MarketingContentWriteReceipt, PersistenceError> {
        if domain_id.is_nil() {
            return Err(invalid("domain ID cannot be nil"));
        }
        validate_sha256(proof_hash, "domain verification proof hash")?;
        validate_context(context, &scope)?;
        let payload = serde_json::json!({ "domain_id": domain_id });
        let request_material = serde_json::json!({
            "domain_id": domain_id,
            "proof_hash": proof_hash,
        });
        let holding_ref = self.holding_ref.clone();
        let proof_hash = proof_hash.to_ascii_lowercase();
        self.save_fact(
            MARKETING_CONTENT_DOMAIN_VERIFICATION_AGGREGATE,
            MARKETING_CONTENT_DOMAIN_VERIFIED_EVENT,
            domain_id,
            scope.clone(),
            payload,
            request_material,
            context,
            move |tx, _version| {
                let holding_ref = holding_ref.clone();
                let scope = scope.clone();
                let proof_hash = proof_hash.clone();
                Box::pin(async move {
                    let row = sqlx::query(
                        "SELECT scope, status, verification_hash
                           FROM erpx_tenant.marketing_content_domains
                          WHERE holding_ref=$1 AND domain_id=$2 FOR UPDATE",
                    )
                    .bind(&holding_ref)
                    .bind(domain_id)
                    .fetch_optional(&mut **tx)
                    .await?
                    .ok_or_else(|| invalid("custom domain does not exist"))?;
                    let stored_scope: TenantScope = serde_json::from_value(row.get("scope"))?;
                    if stored_scope != scope {
                        return Err(content_error(MarketingContentError::ScopeMismatch));
                    }
                    if row.get::<String, _>("status") != "pending_verification"
                        || row.get::<Option<String>, _>("verification_hash").as_deref()
                            != Some(proof_hash.as_str())
                    {
                        return Err(invalid("custom domain verification proof is invalid"));
                    }
                    sqlx::query(
                        "UPDATE erpx_tenant.marketing_content_domains
                            SET status='active', verification_hash=NULL,
                                state=jsonb_set(state, '{status}', to_jsonb('active'::text), true),
                                updated_at=now()
                          WHERE holding_ref=$1 AND domain_id=$2",
                    )
                    .bind(&holding_ref)
                    .bind(domain_id)
                    .execute(&mut **tx)
                    .await?;
                    Ok(())
                })
            },
        )
        .await
    }

    /// Bind only an active, verified domain to a published template revision.
    pub async fn bind_domain(
        &self,
        binding: &CustomDomainBinding,
        context: &CommandContext,
    ) -> Result<MarketingContentWriteReceipt, PersistenceError> {
        if binding.domain_id.is_nil() {
            return Err(invalid("domain ID cannot be nil"));
        }
        binding.template.validate().map_err(content_error)?;
        validate_context(context, &binding.domain_scope)?;
        let payload = serde_json::json!({ "binding": binding });
        let holding_ref = self.holding_ref.clone();
        let binding = binding.clone();
        self.save_fact(
            MARKETING_CONTENT_DOMAIN_BINDING_AGGREGATE,
            MARKETING_CONTENT_DOMAIN_BOUND_EVENT,
            binding.domain_id,
            binding.domain_scope.clone(),
            payload.clone(),
            payload,
            context,
            move |tx, _version| {
                let holding_ref = holding_ref.clone();
                let binding = binding.clone();
                Box::pin(async move {
                    let domain = sqlx::query(
                        "SELECT scope, status FROM erpx_tenant.marketing_content_domains
                          WHERE holding_ref=$1 AND domain_id=$2 FOR UPDATE",
                    )
                    .bind(&holding_ref)
                    .bind(binding.domain_id)
                    .fetch_optional(&mut **tx)
                    .await?
                    .ok_or_else(|| invalid("custom domain does not exist"))?;
                    let domain_scope: TenantScope = serde_json::from_value(domain.get("scope"))?;
                    if domain_scope != binding.domain_scope {
                        return Err(content_error(MarketingContentError::ScopeMismatch));
                    }
                    if domain.get::<String, _>("status") != "active" {
                        return Err(invalid("custom domain must be verified and active"));
                    }
                    let template = sqlx::query(
                        "SELECT scope, published_version
                           FROM erpx_tenant.marketing_content_templates
                          WHERE holding_ref=$1 AND template_id=$2",
                    )
                    .bind(&holding_ref)
                    .bind(binding.template.template_id)
                    .fetch_optional(&mut **tx)
                    .await?
                    .ok_or_else(|| invalid("template does not exist"))?;
                    let template_scope: TenantScope =
                        serde_json::from_value(template.get("scope"))?;
                    if !binding.domain_scope.contains(&template_scope)
                        || template_scope != binding.template.scope
                        || template.get::<Option<i64>, _>("published_version")
                            != Some(as_i64(binding.template.version, "published version")?)
                    {
                        return Err(content_error(MarketingContentError::DomainScopeMismatch));
                    }
                    sqlx::query(
                        "INSERT INTO erpx_tenant.marketing_content_domain_bindings
                            (holding_ref, domain_id, template_id, template_version, scope, bound_at)
                         VALUES ($1,$2,$3,$4,$5,$6)
                         ON CONFLICT (holding_ref, domain_id) DO UPDATE SET
                            template_id=EXCLUDED.template_id,
                            template_version=EXCLUDED.template_version,
                            scope=EXCLUDED.scope, bound_at=EXCLUDED.bound_at",
                    )
                    .bind(&holding_ref)
                    .bind(binding.domain_id)
                    .bind(binding.template.template_id)
                    .bind(as_i64(binding.template.version, "bound template version")?)
                    .bind(serde_json::to_value(&binding.template.scope)?)
                    .bind(binding.bound_at)
                    .execute(&mut **tx)
                    .await?;
                    Ok(())
                })
            },
        )
        .await
    }

    pub async fn load_template(
        &self,
        template_id: Uuid,
    ) -> Result<Option<ContentTemplate>, PersistenceError> {
        if template_id.is_nil() {
            return Err(invalid("Marketing content template ID cannot be nil"));
        }
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let row = sqlx::query(
            "SELECT state FROM erpx_tenant.marketing_content_templates
              WHERE holding_ref=$1 AND template_id=$2",
        )
        .bind(&self.holding_ref)
        .bind(template_id)
        .fetch_optional(&mut *tx)
        .await?;
        let result = row
            .map(|row| serde_json::from_value(row.get("state")))
            .transpose()?;
        tx.commit().await?;
        Ok(result)
    }

    /// Rebuild the replacement read database from current write snapshots.
    /// The target is cleared only after the source snapshot is read under its
    /// holding scope; a failed rebuild leaves the prior read model intact.
    pub async fn rebuild_read_projection(
        &self,
    ) -> Result<MarketingContentProjectionRebuildReport, PersistenceError> {
        let mut source = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        lock_content_rebuild_source(&mut source, false).await?;
        let templates = sqlx::query(
            "SELECT template_id, slug, status, state, version, actor
               FROM erpx_tenant.marketing_content_templates
              WHERE holding_ref=$1 ORDER BY template_id",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *source)
        .await?;
        let assets = sqlx::query(
            "SELECT asset_id, name, state, version, actor
               FROM erpx_tenant.marketing_content_assets
              WHERE holding_ref=$1 ORDER BY asset_id",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *source)
        .await?;
        let forms = sqlx::query(
            "SELECT form_id, slug, status, state, version, actor
               FROM erpx_tenant.marketing_content_forms
              WHERE holding_ref=$1 ORDER BY form_id",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *source)
        .await?;
        let domains = sqlx::query(
            "SELECT domain_id, hostname, status, state, version, actor
               FROM erpx_tenant.marketing_content_domains
              WHERE holding_ref=$1 ORDER BY domain_id",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *source)
        .await?;
        let mut target = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        sqlx::query("DELETE FROM erpx_tenant.marketing_content_projection_event_receipts WHERE holding_ref=$1")
            .bind(&self.holding_ref)
            .execute(&mut *target)
            .await?;
        sqlx::query(
            "DELETE FROM erpx_tenant.marketing_content_projection_checkpoints WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .execute(&mut *target)
        .await?;
        sqlx::query("DELETE FROM erpx_tenant.marketing_content_read_rows WHERE holding_ref=$1")
            .bind(&self.holding_ref)
            .execute(&mut *target)
            .await?;
        let mut projected_rows = 0_u64;
        for row in templates {
            let id: Uuid = row.get("template_id");
            let version: i64 = row.get("version");
            let (event_id, actor) = require_latest_event_for_version(
                latest_event(
                    &mut source,
                    &self.holding_ref,
                    MARKETING_CONTENT_TEMPLATE_AGGREGATE,
                    id,
                    MARKETING_CONTENT_TEMPLATE_SAVED_EVENT,
                )
                .await?,
                version,
                "template",
            )?;
            insert_read_row(
                &mut target,
                &self.holding_ref,
                "template",
                id,
                row.get("state"),
                Some(row.get("slug")),
                row.get("status"),
                version,
                event_id,
                actor,
            )
            .await?;
            projected_rows = projected_rows.saturating_add(1);
        }
        for row in assets {
            let id: Uuid = row.get("asset_id");
            let version: i64 = row.get("version");
            let (event_id, actor) = require_latest_event_for_version(
                latest_event(
                    &mut source,
                    &self.holding_ref,
                    MARKETING_CONTENT_ASSET_AGGREGATE,
                    id,
                    MARKETING_CONTENT_ASSET_SAVED_EVENT,
                )
                .await?,
                version,
                "asset",
            )?;
            insert_read_row(
                &mut target,
                &self.holding_ref,
                "asset",
                id,
                row.get("state"),
                Some(row.get("name")),
                "active".into(),
                version,
                event_id,
                actor,
            )
            .await?;
            projected_rows = projected_rows.saturating_add(1);
        }
        for row in forms {
            let id: Uuid = row.get("form_id");
            let version: i64 = row.get("version");
            let (event_id, actor) = require_latest_event_for_version(
                latest_event(
                    &mut source,
                    &self.holding_ref,
                    MARKETING_CONTENT_FORM_AGGREGATE,
                    id,
                    MARKETING_CONTENT_FORM_SAVED_EVENT,
                )
                .await?,
                version,
                "form",
            )?;
            insert_read_row(
                &mut target,
                &self.holding_ref,
                "form",
                id,
                row.get("state"),
                Some(row.get("slug")),
                row.get("status"),
                version,
                event_id,
                actor,
            )
            .await?;
            projected_rows = projected_rows.saturating_add(1);
        }
        for row in domains {
            let id: Uuid = row.get("domain_id");
            let version: i64 = row.get("version");
            let (event_id, actor) = require_latest_event_for_version(
                latest_event(
                    &mut source,
                    &self.holding_ref,
                    MARKETING_CONTENT_DOMAIN_AGGREGATE,
                    id,
                    MARKETING_CONTENT_DOMAIN_SAVED_EVENT,
                )
                .await?,
                version,
                "domain",
            )?;
            insert_read_row(
                &mut target,
                &self.holding_ref,
                "domain",
                id,
                row.get("state"),
                Some(row.get("hostname")),
                row.get("status"),
                version,
                event_id,
                actor,
            )
            .await?;
            projected_rows = projected_rows.saturating_add(1);
        }
        target.commit().await?;
        source.commit().await?;
        Ok(MarketingContentProjectionRebuildReport { projected_rows })
    }

    /// Rebuild workflow read rows from tenant-write facts. Secret hashes are
    /// selected only from the write side when commands run and are never
    /// copied into the read database.
    pub async fn rebuild_workflow_read_projection(
        &self,
    ) -> Result<MarketingContentWorkflowProjectionRebuildReport, PersistenceError> {
        let mut source = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        lock_content_rebuild_source(&mut source, true).await?;
        let previews = sqlx::query(
            "SELECT p.preview_id, p.template_id, p.template_version, p.scope,
                    p.issued_at, p.expires_at, p.revoked_at,
                    e.aggregate_version, e.event_id, e.actor, e.event_type, e.payload,
                    (SELECT array_agg(h.aggregate_version ORDER BY h.aggregate_version, h.event_id)
                       FROM erpx_tenant.outbox_events h
                      WHERE h.holding_ref=p.holding_ref
                        AND h.aggregate_type=e.aggregate_type
                        AND h.aggregate_id=p.preview_id) AS source_versions
               FROM erpx_tenant.marketing_content_previews p
               JOIN LATERAL (
                    SELECT aggregate_type, aggregate_version, event_id, actor, event_type, payload
                      FROM erpx_tenant.outbox_events
                     WHERE holding_ref=p.holding_ref
                       AND aggregate_type=$2 AND aggregate_id=p.preview_id
                     ORDER BY aggregate_version DESC, event_id DESC
                     LIMIT 1
               ) e ON true
              WHERE p.holding_ref=$1 ORDER BY p.preview_id",
        )
        .bind(&self.holding_ref)
        .bind(MARKETING_CONTENT_PREVIEW_AGGREGATE)
        .fetch_all(&mut *source)
        .await?;
        let submissions = sqlx::query(
            "SELECT s.submission_id, s.form_id, s.scope, s.values_json, s.received_at,
                    s.retention_until, s.redacted_at,
                    e.aggregate_version, e.event_id, e.actor, e.event_type, e.payload,
                    (SELECT array_agg(h.aggregate_version ORDER BY h.aggregate_version, h.event_id)
                       FROM erpx_tenant.outbox_events h
                      WHERE h.holding_ref=s.holding_ref
                        AND h.aggregate_type=e.aggregate_type
                        AND h.aggregate_id=s.submission_id) AS source_versions
               FROM erpx_tenant.marketing_content_form_submissions s
               JOIN LATERAL (
                    SELECT aggregate_type, aggregate_version, event_id, actor, event_type, payload
                      FROM erpx_tenant.outbox_events
                     WHERE holding_ref=s.holding_ref
                       AND aggregate_type=$2 AND aggregate_id=s.submission_id
                     ORDER BY aggregate_version DESC, event_id DESC
                     LIMIT 1
               ) e ON true
              WHERE s.holding_ref=$1 ORDER BY s.received_at, s.submission_id",
        )
        .bind(&self.holding_ref)
        .bind(MARKETING_CONTENT_FORM_SUBMISSION_AGGREGATE)
        .fetch_all(&mut *source)
        .await?;
        let domains = sqlx::query(
            "SELECT d.domain_id, d.scope, d.hostname, d.status,
                    d.state, d.version AS domain_version,
                    b.template_id AS binding_template_id,
                    b.template_version AS binding_template_version,
                    b.bound_at,
                    e.aggregate_version, e.event_id, e.actor, e.event_type, e.payload,
                    (SELECT array_agg(h.aggregate_version ORDER BY h.aggregate_version, h.event_id)
                       FROM erpx_tenant.outbox_events h
                      WHERE h.holding_ref=d.holding_ref
                        AND h.aggregate_type=e.aggregate_type
                        AND h.aggregate_id=d.domain_id) AS source_versions
               FROM erpx_tenant.marketing_content_domains d
               LEFT JOIN erpx_tenant.marketing_content_domain_bindings b
                 ON b.holding_ref=d.holding_ref AND b.domain_id=d.domain_id
               JOIN LATERAL (
                    SELECT aggregate_type, aggregate_version, event_id, actor, event_type, payload
                      FROM erpx_tenant.outbox_events
                     WHERE holding_ref=d.holding_ref
                       AND aggregate_id=d.domain_id
                       AND aggregate_type IN ($2, $3, $4)
                     ORDER BY aggregate_version DESC, event_id DESC
                     LIMIT 1
               ) e ON true
              WHERE d.holding_ref=$1 ORDER BY d.hostname, d.domain_id",
        )
        .bind(&self.holding_ref)
        .bind(MARKETING_CONTENT_DOMAIN_AGGREGATE)
        .bind(MARKETING_CONTENT_DOMAIN_VERIFICATION_AGGREGATE)
        .bind(MARKETING_CONTENT_DOMAIN_BINDING_AGGREGATE)
        .fetch_all(&mut *source)
        .await?;
        let mut target = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        for table in [
            "marketing_content_workflow_projection_event_receipts",
            "marketing_content_workflow_projection_checkpoints",
            "marketing_content_preview_read_rows",
            "marketing_content_submission_read_rows",
            "marketing_content_domain_read_rows",
        ] {
            let query = format!("DELETE FROM erpx_tenant.{table} WHERE holding_ref=$1");
            sqlx::query(&query)
                .bind(&self.holding_ref)
                .execute(&mut *target)
                .await?;
        }
        for row in &previews {
            let version: i64 = row.get("aggregate_version");
            let event_id: Uuid = row.get("event_id");
            let event_type: String = row.get("event_type");
            let payload: Value = row.get("payload");
            let source_versions: Option<Vec<i64>> = row.get("source_versions");
            validate_workflow_source_event(
                WorkflowEventKind::Preview,
                event_id,
                event_type,
                row.get("preview_id"),
                version,
                payload,
                source_versions,
            )?;
            sqlx::query(
                "INSERT INTO erpx_tenant.marketing_content_preview_read_rows
                    (holding_ref, preview_id, template_id, template_version, scope,
                     issued_at, expires_at, revoked_at, source_version, source_event_id,
                     source_actor)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
            )
            .bind(&self.holding_ref)
            .bind(row.get::<Uuid, _>("preview_id"))
            .bind(row.get::<Uuid, _>("template_id"))
            .bind(row.get::<i64, _>("template_version"))
            .bind(row.get::<Value, _>("scope"))
            .bind(row.get::<chrono::DateTime<chrono::Utc>, _>("issued_at"))
            .bind(row.get::<chrono::DateTime<chrono::Utc>, _>("expires_at"))
            .bind(row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("revoked_at"))
            .bind(version)
            .bind(event_id)
            .bind(row.get::<Value, _>("actor"))
            .execute(&mut *target)
            .await?;
            insert_workflow_projection_facts(
                &mut target,
                &self.holding_ref,
                "preview",
                row.get("preview_id"),
                version,
                event_id,
                row.get("actor"),
            )
            .await?;
        }
        for row in &submissions {
            let version: i64 = row.get("aggregate_version");
            let event_id: Uuid = row.get("event_id");
            let event_type: String = row.get("event_type");
            let payload: Value = row.get("payload");
            let source_versions: Option<Vec<i64>> = row.get("source_versions");
            validate_workflow_source_event(
                WorkflowEventKind::FormSubmission,
                event_id,
                event_type,
                row.get("submission_id"),
                version,
                payload,
                source_versions,
            )?;
            sqlx::query(
                "INSERT INTO erpx_tenant.marketing_content_submission_read_rows
                    (holding_ref, submission_id, form_id, scope, values_json, received_at,
                     retention_until, redacted_at, source_version, source_event_id, source_actor)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
            )
            .bind(&self.holding_ref)
            .bind(row.get::<Uuid, _>("submission_id"))
            .bind(row.get::<Uuid, _>("form_id"))
            .bind(row.get::<Value, _>("scope"))
            .bind(row.get::<Value, _>("values_json"))
            .bind(row.get::<chrono::DateTime<chrono::Utc>, _>("received_at"))
            .bind(row.get::<chrono::DateTime<chrono::Utc>, _>("retention_until"))
            .bind(row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("redacted_at"))
            .bind(version)
            .bind(event_id)
            .bind(row.get::<Value, _>("actor"))
            .execute(&mut *target)
            .await?;
            insert_workflow_projection_facts(
                &mut target,
                &self.holding_ref,
                "form_submission",
                row.get("submission_id"),
                version,
                event_id,
                row.get("actor"),
            )
            .await?;
        }
        for row in &domains {
            let version: i64 = row.get("aggregate_version");
            let event_id: Uuid = row.get("event_id");
            let event_type: String = row.get("event_type");
            let payload: Value = row.get("payload");
            let source_versions: Option<Vec<i64>> = row.get("source_versions");
            validate_workflow_source_event(
                WorkflowEventKind::Domain,
                event_id,
                event_type,
                row.get("domain_id"),
                version,
                payload,
                source_versions,
            )?;
            sqlx::query(
                "INSERT INTO erpx_tenant.marketing_content_domain_read_rows
                    (holding_ref, domain_id, scope, hostname, status,
                     binding_template_id, binding_template_version, bound_at,
                     source_version, source_event_id, source_actor)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
            )
            .bind(&self.holding_ref)
            .bind(row.get::<Uuid, _>("domain_id"))
            .bind(row.get::<Value, _>("scope"))
            .bind(row.get::<String, _>("hostname"))
            .bind(row.get::<String, _>("status"))
            .bind(row.get::<Option<Uuid>, _>("binding_template_id"))
            .bind(row.get::<Option<i64>, _>("binding_template_version"))
            .bind(row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("bound_at"))
            .bind(version)
            .bind(event_id)
            .bind(row.get::<Value, _>("actor"))
            .execute(&mut *target)
            .await?;
            insert_workflow_projection_facts(
                &mut target,
                &self.holding_ref,
                "domain",
                row.get("domain_id"),
                version,
                event_id,
                row.get("actor"),
            )
            .await?;
        }
        target.commit().await?;
        source.commit().await?;
        Ok(MarketingContentWorkflowProjectionRebuildReport {
            previews: previews.len() as u64,
            submissions: submissions.len() as u64,
            domains: domains.len() as u64,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn save_fact<F>(
        &self,
        aggregate_type: &str,
        event_type: &str,
        aggregate_id: Uuid,
        scope: TenantScope,
        event_payload: Value,
        request_material: Value,
        context: &CommandContext,
        write_fact: F,
    ) -> Result<MarketingContentWriteReceipt, PersistenceError>
    where
        F: for<'a> FnOnce(
                &'a mut Transaction<'_, Postgres>,
                u64,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<(), PersistenceError>> + Send + 'a>,
            > + Send,
    {
        validate_context(context, &scope)?;
        if aggregate_id.is_nil() {
            return Err(invalid("Marketing content fact ID cannot be nil"));
        }
        let request_hash = request_hash(aggregate_type, &request_material, context)?;
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!(
                "erpx.{aggregate_type}:{}:{}",
                self.holding_ref, aggregate_id
            ))
            .execute(&mut *tx)
            .await?;
        if let Some(receipt) = find_replay(
            &mut tx,
            &self.holding_ref,
            aggregate_type,
            aggregate_id,
            &context.idempotency_key,
            &request_hash,
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt);
        }
        let next_version =
            next_fact_version(&mut tx, &self.holding_ref, aggregate_type, aggregate_id).await?;
        write_fact(&mut tx, next_version).await?;
        let event = build_event(
            context,
            aggregate_type,
            aggregate_id,
            next_version,
            scope,
            event_type,
            event_payload,
        )?;
        append_event(&mut tx, &self.holding_ref, &event).await?;
        let receipt = MarketingContentWriteReceipt {
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            version: next_version,
            idempotency_key: context.idempotency_key.clone(),
            event_id: event.event_id,
            replayed: false,
        };
        sqlx::query(
            "INSERT INTO erpx_tenant.marketing_content_command_idempotency
                (holding_ref, idempotency_key, operation, request_hash, aggregate_type,
                 aggregate_id, version, event_id, response)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
        )
        .bind(&self.holding_ref)
        .bind(&context.idempotency_key)
        .bind(event_type)
        .bind(&request_hash)
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(as_i64(next_version, "content fact version")?)
        .bind(event.event_id)
        .bind(serde_json::to_value(&receipt)?)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    #[allow(clippy::too_many_arguments)]
    async fn save_aggregate<F>(
        &self,
        aggregate_type: &str,
        event_type: &str,
        aggregate_id: Uuid,
        scope: TenantScope,
        state: Value,
        _display_ref: String,
        _status: &str,
        _content_version: u64,
        context: &CommandContext,
        write_state: F,
    ) -> Result<MarketingContentWriteReceipt, PersistenceError>
    where
        F: for<'a> FnOnce(
                &'a mut Transaction<'_, Postgres>,
                u64,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<(), PersistenceError>> + Send + 'a>,
            > + Send,
    {
        validate_context(context, &scope)?;
        if aggregate_id.is_nil() {
            return Err(invalid("Marketing content aggregate ID cannot be nil"));
        }
        let request_hash = request_hash(aggregate_type, &state, context)?;
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!(
                "erpx.{aggregate_type}:{}:{}",
                self.holding_ref, aggregate_id
            ))
            .execute(&mut *tx)
            .await?;
        if let Some(receipt) = find_replay(
            &mut tx,
            &self.holding_ref,
            aggregate_type,
            aggregate_id,
            &context.idempotency_key,
            &request_hash,
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt);
        }
        let current =
            current_version(&mut tx, &self.holding_ref, aggregate_type, aggregate_id).await?;
        let expected = context.expected_aggregate_version;
        if expected.is_none() && current.is_some() {
            return Err(invalid("Marketing content aggregate already exists"));
        }
        if expected.is_some() && current != expected {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: aggregate_type.into(),
                aggregate_id,
                expected,
                actual: current,
            });
        }
        let next_version = expected
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| invalid("Marketing content version overflow"))?;
        write_state(&mut tx, next_version).await?;
        sqlx::query(
            "INSERT INTO erpx_tenant.aggregate_snapshots
                (aggregate_type, aggregate_id, holding_ref, version, state)
             VALUES ($1,$2,$3,$4,$5)
             ON CONFLICT (aggregate_type, aggregate_id) DO UPDATE SET
                holding_ref=EXCLUDED.holding_ref, version=EXCLUDED.version,
                state=EXCLUDED.state, updated_at=now()",
        )
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(&self.holding_ref)
        .bind(as_i64(next_version, "content aggregate version")?)
        .bind(&state)
        .execute(&mut *tx)
        .await?;
        let event = build_event(
            context,
            aggregate_type,
            aggregate_id,
            next_version,
            scope,
            event_type,
            state,
        )?;
        append_event(&mut tx, &self.holding_ref, &event).await?;
        let receipt = MarketingContentWriteReceipt {
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            version: next_version,
            idempotency_key: context.idempotency_key.clone(),
            event_id: event.event_id,
            replayed: false,
        };
        sqlx::query(
            "INSERT INTO erpx_tenant.marketing_content_command_idempotency
                (holding_ref, idempotency_key, operation, request_hash, aggregate_type,
                 aggregate_id, version, event_id, response)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
        )
        .bind(&self.holding_ref)
        .bind(&context.idempotency_key)
        .bind("save")
        .bind(&request_hash)
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(as_i64(next_version, "content receipt version")?)
        .bind(event.event_id)
        .bind(serde_json::to_value(&receipt)?)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }
}

async fn current_version(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
) -> Result<Option<u64>, PersistenceError> {
    let table = match aggregate_type {
        MARKETING_CONTENT_TEMPLATE_AGGREGATE => "marketing_content_templates",
        MARKETING_CONTENT_ASSET_AGGREGATE => "marketing_content_assets",
        MARKETING_CONTENT_FORM_AGGREGATE => "marketing_content_forms",
        MARKETING_CONTENT_DOMAIN_AGGREGATE => "marketing_content_domains",
        _ => return Err(invalid("unsupported Marketing content aggregate")),
    };
    let query = format!(
        "SELECT version FROM erpx_tenant.{table} WHERE holding_ref=$1 AND {}=$2 FOR UPDATE",
        match aggregate_type {
            MARKETING_CONTENT_TEMPLATE_AGGREGATE => "template_id",
            MARKETING_CONTENT_ASSET_AGGREGATE => "asset_id",
            MARKETING_CONTENT_FORM_AGGREGATE => "form_id",
            _ => "domain_id",
        }
    );
    let row = sqlx::query(&query)
        .bind(holding_ref)
        .bind(aggregate_id)
        .fetch_optional(&mut **tx)
        .await?;
    row.map(|row| as_u64(row.get::<i64, _>("version"), "content aggregate version"))
        .transpose()
}

async fn next_fact_version(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
) -> Result<u64, PersistenceError> {
    let row = sqlx::query(
        "SELECT COALESCE(MAX(aggregate_version), 0) + 1 AS next_version
           FROM erpx_tenant.outbox_events
          WHERE holding_ref=$1 AND aggregate_type=$2 AND aggregate_id=$3",
    )
    .bind(holding_ref)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .fetch_one(&mut **tx)
    .await?;
    as_u64(row.get("next_version"), "content fact version")
}

async fn find_replay(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    idempotency_key: &str,
    request_hash: &str,
) -> Result<Option<MarketingContentWriteReceipt>, PersistenceError> {
    let row = sqlx::query(
        "SELECT request_hash, aggregate_type, aggregate_id, response
           FROM erpx_tenant.marketing_content_command_idempotency
          WHERE holding_ref=$1 AND idempotency_key=$2 FOR UPDATE",
    )
    .bind(holding_ref)
    .bind(idempotency_key)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else { return Ok(None) };
    if row.get::<String, _>("request_hash") != request_hash
        || row.get::<String, _>("aggregate_type") != aggregate_type
        || row.get::<Uuid, _>("aggregate_id") != aggregate_id
    {
        return Err(PersistenceError::IdempotencyConflict {
            key: idempotency_key.into(),
        });
    }
    let mut receipt: MarketingContentWriteReceipt = serde_json::from_value(row.get("response"))?;
    receipt.replayed = true;
    Ok(Some(receipt))
}

async fn latest_event(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    event_type: &str,
) -> Result<Option<(Uuid, i64, Value)>, PersistenceError> {
    let row = sqlx::query(
        "SELECT event_id, aggregate_version, actor FROM erpx_tenant.outbox_events
          WHERE holding_ref=$1 AND aggregate_type=$2 AND aggregate_id=$3
            AND event_type=$4
          ORDER BY aggregate_version DESC, event_id DESC LIMIT 1",
    )
    .bind(holding_ref)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(event_type)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(|row| {
        (
            row.get("event_id"),
            row.get("aggregate_version"),
            row.get("actor"),
        )
    }))
}

/// Keep the source snapshot and its latest outbox evidence stable until the
/// replacement read transaction commits.  A source transaction alone is not
/// enough here: PostgreSQL's default READ COMMITTED isolation gives each
/// statement a fresh snapshot, so a writer could otherwise append an event
/// between the snapshot reads and the per-aggregate event lookup.
async fn lock_content_rebuild_source(
    tx: &mut Transaction<'_, Postgres>,
    include_workflow_tables: bool,
) -> Result<(), PersistenceError> {
    let mut tables = vec![
        "marketing_content_templates",
        "marketing_content_assets",
        "marketing_content_forms",
        "marketing_content_domains",
    ];
    if include_workflow_tables {
        tables.extend([
            "marketing_content_previews",
            "marketing_content_form_submissions",
            "marketing_content_domain_bindings",
        ]);
    }
    tables.push("outbox_events");
    for table in tables {
        let statement = format!("LOCK TABLE erpx_tenant.{table} IN SHARE MODE");
        sqlx::query(&statement).execute(&mut **tx).await?;
    }
    Ok(())
}

fn require_latest_event_for_version(
    event: Option<(Uuid, i64, Value)>,
    expected_version: i64,
    aggregate_label: &str,
) -> Result<(Uuid, Value), PersistenceError> {
    let Some((event_id, event_version, actor)) = event else {
        return Err(invalid(format!(
            "Marketing content {aggregate_label} snapshot has no saved source event"
        )));
    };
    if event_id.is_nil() || event_version <= 0 || event_version != expected_version {
        return Err(invalid(format!(
            "Marketing content {aggregate_label} snapshot and source event versions diverge"
        )));
    }
    Ok((event_id, actor))
}

#[allow(clippy::too_many_arguments)]
async fn insert_read_row(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    state: Value,
    slug: Option<String>,
    status: String,
    source_version: i64,
    source_event_id: Uuid,
    source_actor: Value,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_tenant.marketing_content_read_rows
            (holding_ref, aggregate_type, aggregate_id, scope, slug, status, state,
             source_version, source_event_id, source_actor)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
    )
    .bind(holding_ref)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(state.get("scope").cloned().unwrap_or(Value::Null))
    .bind(slug)
    .bind(status)
    .bind(&state)
    .bind(source_version)
    .bind(source_event_id)
    .bind(source_actor)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO erpx_tenant.marketing_content_projection_checkpoints
            (holding_ref, projection_name, aggregate_type, aggregate_id,
             source_version, last_event_id)
         VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(holding_ref)
    .bind(MARKETING_CONTENT_PROJECTION)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(source_version)
    .bind(source_event_id)
    .execute(&mut **tx)
    .await?;
    if !source_event_id.is_nil() {
        sqlx::query(
            "INSERT INTO erpx_tenant.marketing_content_projection_event_receipts
                (holding_ref, projection_name, event_id, aggregate_type, aggregate_id,
                 aggregate_version)
             VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .bind(holding_ref)
        .bind(MARKETING_CONTENT_PROJECTION)
        .bind(source_event_id)
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(source_version)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn insert_workflow_projection_facts(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    source_version: i64,
    source_event_id: Uuid,
    source_actor: Value,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_tenant.marketing_content_workflow_projection_checkpoints
            (holding_ref, projection_name, aggregate_type, aggregate_id,
             source_version, last_event_id)
         VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(holding_ref)
    .bind(MARKETING_CONTENT_WORKFLOW_PROJECTION)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(source_version)
    .bind(source_event_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO erpx_tenant.marketing_content_workflow_projection_event_receipts
            (holding_ref, projection_name, event_id, aggregate_type, aggregate_id,
             aggregate_version)
         VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(holding_ref)
    .bind(MARKETING_CONTENT_WORKFLOW_PROJECTION)
    .bind(source_event_id)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(source_version)
    .execute(&mut **tx)
    .await?;
    let _ = source_actor;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum WorkflowEventKind {
    Preview,
    FormSubmission,
    Domain,
}

/// Validate the event lineage used to rebuild a workflow read row.  The
/// source tables are authoritative, but a rebuild must not silently turn an
/// incomplete or identity-drifting outbox history into a readable row.  The
/// version array is fetched in the same source transaction as the snapshot;
/// requiring 1..=latest rejects gaps and duplicate versions before the target
/// transaction is allowed to replace any rows.
fn validate_workflow_source_event(
    kind: WorkflowEventKind,
    event_id: Uuid,
    event_type: String,
    aggregate_id: Uuid,
    source_version: i64,
    payload: Value,
    source_versions: Option<Vec<i64>>,
) -> Result<(), PersistenceError> {
    if event_id.is_nil() {
        return Err(invalid(
            "Marketing content workflow source event ID cannot be nil",
        ));
    }
    if aggregate_id.is_nil() || source_version <= 0 {
        return Err(invalid(
            "Marketing content workflow source identity is invalid",
        ));
    }
    let versions = source_versions
        .ok_or_else(|| invalid("Marketing content workflow source event history is missing"))?;
    let expected_len = usize::try_from(source_version).map_err(|_| {
        invalid("Marketing content workflow source version exceeds platform limits")
    })?;
    if versions.len() != expected_len
        || versions
            .iter()
            .enumerate()
            .any(|(index, actual)| *actual != i64::try_from(index + 1).unwrap_or(i64::MAX))
    {
        return Err(invalid(
            "Marketing content workflow source event history is not contiguous",
        ));
    }

    let allowed = match kind {
        WorkflowEventKind::Preview => matches!(
            event_type.as_str(),
            MARKETING_CONTENT_PREVIEW_ISSUED_EVENT
                | MARKETING_CONTENT_PREVIEW_REVOKED_EVENT
                | MARKETING_CONTENT_PREVIEW_EXPIRED_EVENT
        ),
        WorkflowEventKind::FormSubmission => matches!(
            event_type.as_str(),
            MARKETING_CONTENT_FORM_SUBMITTED_EVENT
                | MARKETING_CONTENT_FORM_SUBMISSION_REDACTED_EVENT
        ),
        WorkflowEventKind::Domain => matches!(
            event_type.as_str(),
            MARKETING_CONTENT_DOMAIN_SAVED_EVENT
                | MARKETING_CONTENT_DOMAIN_VERIFICATION_ISSUED_EVENT
                | MARKETING_CONTENT_DOMAIN_VERIFIED_EVENT
                | MARKETING_CONTENT_DOMAIN_BOUND_EVENT
        ),
    };
    if !allowed {
        return Err(invalid(
            "Marketing content workflow source event type is not allowed",
        ));
    }

    match kind {
        WorkflowEventKind::Preview => {
            if event_type == MARKETING_CONTENT_PREVIEW_ISSUED_EVENT {
                if source_version != 1 {
                    return Err(invalid(
                        "Marketing content preview issue event must start the history",
                    ));
                }
                let preview: PreviewGrant =
                    serde_json::from_value(payload.get("preview").cloned().ok_or_else(|| {
                        invalid("Marketing content preview aggregate is missing")
                    })?)?;
                if preview.id != aggregate_id {
                    return Err(invalid(
                        "Marketing content preview event identity does not match its row",
                    ));
                }
            } else if source_version <= 1 || payload_uuid(&payload, "preview_id")? != aggregate_id {
                return Err(invalid(
                    "Marketing content preview lifecycle event identity is invalid",
                ));
            }
        }
        WorkflowEventKind::FormSubmission => {
            if event_type == MARKETING_CONTENT_FORM_SUBMITTED_EVENT {
                if source_version != 1 || payload_uuid(&payload, "submission_id")? != aggregate_id {
                    return Err(invalid(
                        "Marketing content form submission event identity is invalid",
                    ));
                }
            } else if source_version <= 1
                || payload_uuid(&payload, "submission_id")? != aggregate_id
            {
                return Err(invalid(
                    "Marketing content form submission redaction event identity is invalid",
                ));
            }
        }
        WorkflowEventKind::Domain => match event_type.as_str() {
            MARKETING_CONTENT_DOMAIN_SAVED_EVENT => {
                let domain: CustomDomain = serde_json::from_value(payload)?;
                if domain.id != aggregate_id {
                    return Err(invalid(
                        "Marketing content domain event identity does not match its row",
                    ));
                }
            }
            MARKETING_CONTENT_DOMAIN_VERIFICATION_ISSUED_EVENT
            | MARKETING_CONTENT_DOMAIN_VERIFIED_EVENT
            | MARKETING_CONTENT_DOMAIN_BOUND_EVENT => {
                let event_domain_id = if event_type == MARKETING_CONTENT_DOMAIN_BOUND_EVENT {
                    let binding: CustomDomainBinding =
                        serde_json::from_value(payload.get("binding").cloned().ok_or_else(
                            || invalid("Marketing content domain binding is missing"),
                        )?)?;
                    binding.domain_id
                } else {
                    payload_uuid(&payload, "domain_id")?
                };
                if event_domain_id != aggregate_id {
                    return Err(invalid(
                        "Marketing content domain event identity does not match its row",
                    ));
                }
            }
            _ => unreachable!("domain event type was allow-listed above"),
        },
    }
    Ok(())
}

fn payload_uuid(payload: &Value, field: &str) -> Result<Uuid, PersistenceError> {
    let value = payload.get(field).and_then(Value::as_str).ok_or_else(|| {
        invalid(format!(
            "Marketing content workflow payload {field} is missing"
        ))
    })?;
    Uuid::parse_str(value).map_err(|_| {
        invalid(format!(
            "Marketing content workflow payload {field} is invalid"
        ))
    })
}

fn build_event(
    context: &CommandContext,
    aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    scope: TenantScope,
    event_type: &str,
    payload: Value,
) -> Result<EventEnvelope, PersistenceError> {
    Ok(EventEnvelope {
        contract_version: context.contract_version,
        event_id: Uuid::new_v4(),
        event_type: event_type.into(),
        event_version: 1,
        occurred_at: Utc::now(),
        aggregate_type: aggregate_type.into(),
        aggregate_id,
        aggregate_version,
        tenant_scope: scope,
        actor: context.actor.clone(),
        correlation_id: context.correlation_id,
        causation_id: context.causation_id,
        idempotency_key: Some(context.idempotency_key.clone()),
        payload,
    })
}

async fn append_event(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    event: &EventEnvelope,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_tenant.outbox_events
            (event_id, aggregate_type, aggregate_id, holding_ref,
             aggregate_version, event_type, payload, occurred_at,
             contract_version, event_version, tenant_scope, actor,
             correlation_id, causation_id, idempotency_key)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
    )
    .bind(event.event_id)
    .bind(&event.aggregate_type)
    .bind(event.aggregate_id)
    .bind(holding_ref)
    .bind(as_i64(event.aggregate_version, "content event version")?)
    .bind(&event.event_type)
    .bind(&event.payload)
    .bind(event.occurred_at)
    .bind(i32::from(event.contract_version))
    .bind(i32::from(event.event_version))
    .bind(serde_json::to_value(&event.tenant_scope)?)
    .bind(serde_json::to_value(&event.actor)?)
    .bind(event.correlation_id)
    .bind(event.causation_id)
    .bind(&event.idempotency_key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn validate_context(context: &CommandContext, scope: &TenantScope) -> Result<(), PersistenceError> {
    context.validate().map_err(|error| {
        invalid(format!(
            "invalid Marketing content command context: {error}"
        ))
    })?;
    if context.contract_version != CONTRACT_VERSION
        || matches!(context.tenant_scope, TenantScope::Platform)
        || !context.tenant_scope.contains(scope)
    {
        return Err(invalid(
            "Marketing content command scope or contract is invalid",
        ));
    }
    if context.actor.principal_ref.trim().is_empty()
        || context.actor.principal_ref.len() > 255
        || context.actor.principal_ref.chars().any(char::is_control)
    {
        return Err(invalid("Marketing content actor reference is invalid"));
    }
    Ok(())
}

fn request_hash(
    aggregate_type: &str,
    state: &Value,
    context: &CommandContext,
) -> Result<String, PersistenceError> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&(
        aggregate_type,
        state,
        context,
    ))?)))
}

fn validate_holding_ref(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
        return Err(PersistenceError::Configuration(
            "holding reference must be bounded and non-empty",
        ));
    }
    Ok(())
}

fn validate_sha256(value: &str, field: &str) -> Result<(), PersistenceError> {
    if value.len() != 64 || !value.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(invalid(format!("{field} must be a SHA-256 hex digest")));
    }
    Ok(())
}

fn content_error(error: MarketingContentError) -> PersistenceError {
    invalid(error.to_string())
}

fn invalid(message: impl Into<String>) -> PersistenceError {
    PersistenceError::InvalidData(message.into())
}

fn as_i64(value: u64, field: &str) -> Result<i64, PersistenceError> {
    i64::try_from(value).map_err(|_| invalid(format!("{field} exceeds BIGINT")))
}

fn as_u64(value: i64, field: &str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| invalid(format!("{field} is negative")))
}

fn template_version_created_at(
    template: &ContentTemplate,
) -> Result<chrono::DateTime<Utc>, PersistenceError> {
    template
        .versions
        .values()
        .next()
        .map(|version| version.created_at)
        .ok_or_else(|| invalid("Marketing content template has no revision"))
}

fn template_version_updated_at(
    template: &ContentTemplate,
) -> Result<chrono::DateTime<Utc>, PersistenceError> {
    template
        .versions
        .values()
        .next_back()
        .map(|version| version.created_at)
        .ok_or_else(|| invalid("Marketing content template has no revision"))
}

fn asset_created_at(asset: &ContentAsset) -> Result<chrono::DateTime<Utc>, PersistenceError> {
    asset
        .versions
        .values()
        .next()
        .map(|version| version.created_at)
        .ok_or_else(|| invalid("Marketing content asset has no revision"))
}

fn asset_updated_at(asset: &ContentAsset) -> Result<chrono::DateTime<Utc>, PersistenceError> {
    asset
        .versions
        .values()
        .next_back()
        .map(|version| version.created_at)
        .ok_or_else(|| invalid("Marketing content asset has no revision"))
}

fn template_status_text(status: erpx_domain::marketing_content::PublicationStatus) -> &'static str {
    use erpx_domain::marketing_content::PublicationStatus;
    match status {
        PublicationStatus::Draft => "draft",
        PublicationStatus::InReview => "in_review",
        PublicationStatus::Approved => "approved",
        PublicationStatus::Scheduled => "scheduled",
        PublicationStatus::Published => "published",
        PublicationStatus::RolledBack => "rolled_back",
        PublicationStatus::Archived => "archived",
    }
}

fn form_status_text(status: FormStatus) -> &'static str {
    match status {
        FormStatus::Draft => "draft",
        FormStatus::InReview => "in_review",
        FormStatus::Published => "published",
        FormStatus::Archived => "archived",
    }
}

fn domain_status_text(status: CustomDomainStatus) -> &'static str {
    match status {
        CustomDomainStatus::PendingVerification => "pending_verification",
        CustomDomainStatus::Active => "active",
        CustomDomainStatus::Suspended => "suspended",
    }
}

fn validate_marketing_content_aggregate_type(value: &str) -> Result<(), PersistenceError> {
    if matches!(value, "asset" | "template" | "form" | "domain") {
        Ok(())
    } else {
        Err(invalid(
            "Marketing content read aggregate type must be asset, template, form, or domain",
        ))
    }
}

fn validate_read_scope(scope: &TenantScope) -> Result<(), PersistenceError> {
    let valid = match scope {
        TenantScope::Platform => false,
        TenantScope::Holding { holding_id } => !holding_id.is_nil(),
        TenantScope::Company {
            holding_id,
            company_id,
        } => !holding_id.is_nil() && !company_id.is_nil(),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => !holding_id.is_nil() && !company_id.is_nil() && !branch_id.is_nil(),
        TenantScope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        } => {
            !holding_id.is_nil()
                && company_ids.iter().all(|id| !id.is_nil())
                && branch_ids.iter().all(|id| !id.is_nil())
        }
    };
    if valid {
        Ok(())
    } else {
        Err(invalid(
            "Marketing content read row scope is invalid or platform-wide",
        ))
    }
}

fn validate_read_label(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty()
        || value.len() > MAX_MARKETING_CONTENT_READ_LABEL_BYTES
        || value.chars().any(char::is_control)
        || contains_marketing_content_sensitive_marker(value)
    {
        return Err(invalid(
            "Marketing content read label is empty, oversized, unsafe, or sensitive",
        ));
    }
    Ok(())
}

fn validate_read_status(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > 64 || value.chars().any(char::is_control) {
        return Err(invalid("Marketing content read status is invalid"));
    }
    Ok(())
}

fn contains_marketing_content_sensitive_marker(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "access_token",
        "refresh_token",
        "authorization",
        "password",
        "secret",
        "api_key",
        "private_key",
        "credential",
        "database_url",
        "connection_string",
        "bearer ",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn read_row_from_sql(
    row: sqlx::postgres::PgRow,
) -> Result<MarketingContentReadRow, PersistenceError> {
    let aggregate_type: String = row.try_get("aggregate_type")?;
    validate_marketing_content_aggregate_type(&aggregate_type)?;
    let aggregate_id: Uuid = row.try_get("aggregate_id")?;
    if aggregate_id.is_nil() {
        return Err(invalid("Marketing content read aggregate ID cannot be nil"));
    }
    let scope: TenantScope = serde_json::from_value(row.try_get("scope")?)?;
    validate_read_scope(&scope)?;
    let label: Option<String> = row.try_get("slug")?;
    if let Some(label) = &label {
        validate_read_label(label)?;
    }
    let status: String = row.try_get("status")?;
    validate_read_status(&status)?;
    let source_version: i64 = row.try_get("source_version")?;
    if source_version <= 0 {
        return Err(invalid(
            "Marketing content read source version must be positive",
        ));
    }
    let source_event_id: Uuid = row.try_get("source_event_id")?;
    if source_event_id.is_nil() {
        return Err(invalid(
            "Marketing content read source event ID cannot be nil",
        ));
    }
    let checkpoint_source_version: Option<i64> = row.try_get("checkpoint_source_version")?;
    let checkpoint_event_id: Option<Uuid> = row.try_get("checkpoint_event_id")?;
    let receipt_source_version: Option<i64> = row.try_get("receipt_source_version")?;
    if checkpoint_source_version != Some(source_version)
        || checkpoint_event_id != Some(source_event_id)
        || receipt_source_version != Some(source_version)
    {
        return Err(invalid(
            "Marketing content read row checkpoint or source receipt is discontinuous",
        ));
    }
    Ok(MarketingContentReadRow {
        aggregate_type,
        aggregate_id,
        scope,
        label,
        status,
        source_version: as_u64(source_version, "Marketing content read source version")?,
        source_event_id,
        projected_at: row.try_get("projected_at")?,
    })
}

async fn validate_read_cursor_continuity(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    aggregate_type: Option<&str>,
    aggregate_id: Uuid,
) -> Result<(), PersistenceError> {
    let row = sqlx::query(
        "SELECT r.aggregate_type, r.aggregate_id, r.scope, r.slug, r.status,
                r.source_version, r.source_event_id, r.projected_at,
                c.source_version AS checkpoint_source_version,
                c.last_event_id AS checkpoint_event_id,
                e.aggregate_version AS receipt_source_version
           FROM erpx_tenant.marketing_content_read_rows r
           LEFT JOIN erpx_tenant.marketing_content_projection_checkpoints c
             ON c.holding_ref=r.holding_ref
            AND c.projection_name='marketing_content'
            AND c.aggregate_type=r.aggregate_type
            AND c.aggregate_id=r.aggregate_id
           LEFT JOIN erpx_tenant.marketing_content_projection_event_receipts e
             ON e.holding_ref=r.holding_ref
            AND e.projection_name='marketing_content'
            AND e.event_id=r.source_event_id
            AND e.aggregate_type=r.aggregate_type
            AND e.aggregate_id=r.aggregate_id
          WHERE r.holding_ref=$1 AND r.aggregate_id=$2
            AND ($3::text IS NULL OR r.aggregate_type=$3)",
    )
    .bind(holding_ref)
    .bind(aggregate_id)
    .bind(aggregate_type)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| invalid("Marketing content read cursor is not in this holding projection"))?;
    read_row_from_sql(row).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuild_requires_expected_saved_source_event() {
        let event_id = Uuid::from_u128(1);
        let actor = serde_json::json!({"principal_ref": "worker"});

        assert!(require_latest_event_for_version(None, 1, "template").is_err());
        assert!(
            require_latest_event_for_version(Some((event_id, 2, actor.clone())), 1, "template")
                .is_err()
        );
        assert!(
            require_latest_event_for_version(Some((Uuid::nil(), 1, actor.clone())), 1, "template")
                .is_err()
        );

        let (actual_id, actual_actor) =
            require_latest_event_for_version(Some((event_id, 1, actor.clone())), 1, "template")
                .expect("matching saved source event");
        assert_eq!(actual_id, event_id);
        assert_eq!(actual_actor, actor);
    }

    #[test]
    fn constants_and_hashes_are_bounded() {
        assert_eq!(MARKETING_CONTENT_PROJECTION, "marketing_content");
        let hash = request_hash(
            "template",
            &serde_json::json!({"state": "bounded"}),
            &test_context(),
        )
        .expect("hash");
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn workflow_rebuild_rejects_gapped_or_identity_drifting_history() {
        let preview_id = Uuid::from_u128(41);
        let valid_payload = serde_json::json!({
            "preview_id": preview_id,
            "revoked_at": Utc::now(),
        });
        assert!(
            validate_workflow_source_event(
                WorkflowEventKind::Preview,
                Uuid::from_u128(42),
                MARKETING_CONTENT_PREVIEW_REVOKED_EVENT.into(),
                preview_id,
                2,
                valid_payload.clone(),
                Some(vec![1, 2]),
            )
            .is_ok()
        );
        assert!(
            validate_workflow_source_event(
                WorkflowEventKind::Preview,
                Uuid::from_u128(42),
                MARKETING_CONTENT_PREVIEW_REVOKED_EVENT.into(),
                preview_id,
                2,
                valid_payload.clone(),
                Some(vec![1, 3]),
            )
            .is_err()
        );
        assert!(
            validate_workflow_source_event(
                WorkflowEventKind::Preview,
                Uuid::from_u128(42),
                MARKETING_CONTENT_PREVIEW_REVOKED_EVENT.into(),
                Uuid::from_u128(43),
                2,
                valid_payload,
                Some(vec![1, 2]),
            )
            .is_err()
        );
    }

    fn test_context() -> CommandContext {
        CommandContext::try_new(
            "marketing-content-test",
            erpx_domain::contracts::ActorAttribution {
                principal_ref: "test-user".into(),
                kind: erpx_domain::contracts::ActorKind::TenantUser,
                session_id: Some(Uuid::new_v4()),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            TenantScope::Holding {
                holding_id: Uuid::new_v4(),
            },
            "marketing.content.test",
        )
        .expect("context")
    }
}
