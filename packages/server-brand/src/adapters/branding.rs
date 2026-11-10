//! PostgreSQL Branding commands and reads. Every method runs on the caller's
//! serializable Holding transaction and keeps the retained guard, receipt,
//! audit and outbox evidence in that same unit.
use crate::adapters::identity::{authorize, read_active_brand_identity};
use crate::adapters::branding_use_cases::{
    BrandingMutation, BrandingMutationAction, BrandingPorts,
};
use crate::adapters::identity_use_cases::BrandActor;
use crate::domain::branding::{
    BrandScopeKind, BrandVariant, BrandingAttachment, BrandingBranch, BrandingProfile,
    BrandingPublication, BrandingStorefront,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_workflow::public::commands::{
    CommandEventScope, CommandJson, command_fingerprint,
};
use serde_json::Value;
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

pub type BrandingError = PgTransactionError<BusinessError>;

fn conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CONFLICT",
        "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
    )
}

fn statement_error(error: sqlx::Error) -> BrandingError {
    match error.as_database_error().and_then(|error| error.code()).as_deref() {
        Some("23502" | "23503" | "23505" | "23514") => conflict().into(),
        _ => PgTransactionError::Persistence(PersistenceError::from(error)),
    }
}

pub struct PgBranding<'c> {
    connection: &'c mut PgConnection,
    actor: &'c BrandActor,
    now: i64,
}

impl<'c> PgBranding<'c> {
    pub fn new(connection: &'c mut PgConnection, actor: &'c BrandActor, now: i64) -> Self {
        Self {
            connection,
            actor,
            now,
        }
    }

    async fn require_transaction(connection: &mut PgConnection) -> Result<(), BrandingError> {
        if !connection.is_in_transaction() {
            return Err(PersistenceError::new("BRANDING_COMMAND_TRANSACTION_REQUIRED").into());
        }
        Ok(())
    }

    async fn authorize_inner(&mut self, write: bool, owner: bool) -> Result<(), BrandingError> {
        authorize(self.connection, self.actor, self.now, write, owner).await
    }

    async fn replay_inner(
        &mut self,
        key: &str,
        operation: &str,
        record_id: &str,
        input: &CommandJson,
    ) -> Result<Option<Value>, BrandingError> {
        let row = sqlx::query(
            "SELECT operation,request_hash,result_json FROM command_receipts WHERE command_id=$1",
        )
        .bind(key)
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let saved_operation: String = row.try_get("operation")?;
        let saved_hash: String = row.try_get("request_hash")?;
        let digest = command_fingerprint(&self.actor.command, operation, record_id, input)?;
        if saved_operation != operation || saved_hash != digest {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different request.",
            )
            .into());
        }
        let result: String = row.try_get("result_json")?;
        let value = serde_json::from_str::<Value>(&result).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "BRANDING_COMMAND_RESULT_UNAVAILABLE",
                "The retained Branding command result is unavailable.",
            )
        })?;
        Ok(Some(value))
    }

    async fn profile_inner(
        &mut self,
        scope_kind: BrandScopeKind,
        scope_id: &str,
    ) -> Result<Option<BrandingProfile>, BrandingError> {
        let row = sqlx::query(
            "SELECT scope_kind,scope_id,company_id,primary_attachment_id,inverse_attachment_id,\
             primary_publication_id,inverse_publication_id,document_mode,updated_at \
             FROM organization_brand_profiles WHERE scope_kind=$1 AND scope_id=$2",
        )
        .bind(scope_kind.as_str())
        .bind(scope_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        Ok(row.map(|row| BrandingProfile {
            scope_kind: row.get("scope_kind"),
            scope_id: row.get("scope_id"),
            company_id: row.get("company_id"),
            primary_attachment_id: row.get("primary_attachment_id"),
            inverse_attachment_id: row.get("inverse_attachment_id"),
            primary_publication_id: row.get("primary_publication_id"),
            inverse_publication_id: row.get("inverse_publication_id"),
            document_mode: row.get("document_mode"),
            updated_at: row.get("updated_at"),
        }))
    }

    async fn branch_inner(&mut self, id: &str) -> Result<Option<BrandingBranch>, BrandingError> {
        let row = sqlx::query(
            "SELECT id,display_name FROM organization_nodes WHERE id=$1 AND company_id=$2 \
             AND kind='branch' AND status='active'",
        )
        .bind(id)
        .bind(self.actor.company_id())
        .fetch_optional(&mut *self.connection)
        .await?;
        Ok(row.map(|row| BrandingBranch {
            id: row.get("id"),
            display_name: row.get("display_name"),
        }))
    }

    async fn branches_inner(&mut self) -> Result<Vec<BrandingBranch>, BrandingError> {
        let rows = sqlx::query(
            "SELECT id,display_name FROM organization_nodes WHERE company_id=$1 AND kind='branch' \
             AND status='active' ORDER BY display_name,id",
        )
        .bind(self.actor.company_id())
        .fetch_all(&mut *self.connection)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| BrandingBranch {
                id: row.get("id"),
                display_name: row.get("display_name"),
            })
            .collect())
    }

    async fn storefront_inner(&mut self) -> Result<Option<BrandingStorefront>, BrandingError> {
        let row = sqlx::query(
            "SELECT c.id company_id,c.slug company_slug,c.display_name company_name,\
             b.short_name,b.accent_color,b.logo_attachment_id,b.logo_publication_id,\
             COALESCE(b.updated_at,0) updated_at FROM organization_nodes c \
             LEFT JOIN storefront_app_brands b ON b.company_id=c.id \
             WHERE c.id=$1 AND c.kind='company' AND c.status='active'",
        )
        .bind(self.actor.company_id())
        .fetch_optional(&mut *self.connection)
        .await?;
        Ok(row.map(|row| BrandingStorefront {
            company_id: row.get("company_id"),
            company_slug: row.get("company_slug"),
            company_name: row.get("company_name"),
            short_name: row.get("short_name"),
            accent_color: row.get("accent_color"),
            logo_attachment_id: row.get("logo_attachment_id"),
            logo_publication_id: row.get("logo_publication_id"),
            updated_at: row.get("updated_at"),
        }))
    }

    async fn published_inner(&mut self) -> Result<Option<Value>, BrandingError> {
        let active = read_active_brand_identity(&mut *self.connection, self.actor.company_id()).await?;
        match active {
            Some(active) => Ok(Some(serde_json::to_value(active).map_err(|_| {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "BRAND_IDENTITY_UNAVAILABLE",
                    "The published Brand identity is unavailable.",
                )
            })?)),
            None => Ok(None),
        }
    }

    async fn attachment_inner(
        &mut self,
        id: &str,
    ) -> Result<Option<BrandingAttachment>, BrandingError> {
        let row = sqlx::query(
            "SELECT id,company_id,owner_type,owner_id,purpose,revoked_at,object_key \
             FROM attachments WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&mut *self.connection)
        .await?;
        Ok(row.map(|row| BrandingAttachment {
            id: row.get("id"),
            company_id: row.get("company_id"),
            owner_type: row.get("owner_type"),
            owner_id: row.get("owner_id"),
            purpose: row.get("purpose"),
            revoked_at: row.get("revoked_at"),
            object_key: row.get("object_key"),
        }))
    }

    async fn publication_inner(
        &mut self,
        id: &str,
    ) -> Result<Option<BrandingPublication>, BrandingError> {
        let row = sqlx::query(
            "SELECT p.id,p.company_id,p.target_type,p.target_id,p.state,l.attachment_id source_attachment_id \
             FROM attachment_publications p LEFT JOIN attachment_links l ON l.id=p.source_link_id \
             WHERE p.id=$1",
        )
        .bind(id)
        .fetch_optional(&mut *self.connection)
        .await?;
        Ok(row.map(|row| BrandingPublication {
            id: row.get("id"),
            company_id: row.get("company_id"),
            target_type: row.get("target_type"),
            target_id: row.get("target_id"),
            state: row.get("state"),
            source_attachment_id: row.get("source_attachment_id"),
        }))
    }

    async fn guard_expected(
        &mut self,
        table: &'static str,
        scope_kind: Option<&str>,
        scope_id: Option<&str>,
        expected: Option<i64>,
    ) -> Result<(), BrandingError> {
        let Some(expected) = expected else {
            return Ok(());
        };
        let actual: i64 = match (scope_kind, scope_id) {
            (Some(kind), Some(id)) => sqlx::query_scalar(
                "SELECT COALESCE((SELECT updated_at FROM organization_brand_profiles WHERE scope_kind=$1 AND scope_id=$2),0)",
            )
            .bind(kind)
            .bind(id)
            .fetch_one(&mut *self.connection)
            .await?,
            _ => sqlx::query_scalar(
                "SELECT COALESCE((SELECT updated_at FROM storefront_app_brands WHERE company_id=$1),0)",
            )
            .bind(self.actor.company_id())
            .fetch_one(&mut *self.connection)
            .await?,
        };
        if actual != expected {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                if table == "storefront" {
                    "STOREFRONT_CHANGED"
                } else {
                    "BRAND_PROFILE_CHANGED"
                },
                "The Branding record changed. Refresh and review again.",
            )
            .into());
        }
        Ok(())
    }

    async fn commit_inner(
        &mut self,
        mutation: &BrandingMutation,
        result: &Value,
    ) -> Result<(), BrandingError> {
        let actor = self.actor.clone();
        let company = actor.company_id().to_owned();
        let now = mutation.common.now;
        let key = mutation.common.key.as_str();
        let record_id = mutation.common.record_id.as_str();
        Self::require_transaction(&mut *self.connection).await?;
        let owner = match &mutation.action {
            BrandingMutationAction::Profile { input, .. } => {
                input.scope_kind == BrandScopeKind::Holding
            }
            BrandingMutationAction::Media { input, .. } => {
                input.scope_kind == BrandScopeKind::Holding
            }
            BrandingMutationAction::Storefront { .. } => false,
        };
        authorize(&mut *self.connection, &actor, now, true, owner).await?;
        sqlx::query(
            "SELECT id FROM organization_nodes WHERE id=$1 AND kind='company' AND status='active' FOR UPDATE",
        )
        .bind(&company)
        .execute(&mut *self.connection)
        .await?;
        let digest = command_fingerprint(
            &actor.command,
            mutation.common.operation,
            record_id,
            &mutation.common.input,
        )?;
        sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(key)
        .bind(mutation.common.operation)
        .bind(actor.user_id())
        .bind(digest)
        .bind(record_id)
        .bind(result.to_string())
        .bind(now)
        .bind(actor.command.receipt_authority()?.as_str())
        .execute(&mut *self.connection)
        .await
        .map_err(statement_error)?;
        match &mutation.action {
            BrandingMutationAction::Profile {
                input,
                current,
                previous_attachment_id,
                previous_publication_id,
                ..
            } => {
                self.guard_expected(
                    "profile",
                    Some(input.scope_kind.as_str()),
                    Some(&input.scope_id),
                    input.expected_updated_at,
                )
                .await?;
                let document_mode = if input.scope_kind == BrandScopeKind::Holding {
                    "holding"
                } else {
                    input
                        .document_mode
                        .map(|mode| mode.as_str())
                        .or_else(|| current.as_ref().map(|row| row.document_mode.as_str()))
                        .unwrap_or("branch")
                };
                let remove_column = match input.remove {
                    Some(BrandVariant::Primary) => Some("primary_attachment_id"),
                    Some(BrandVariant::Inverse) => Some("inverse_attachment_id"),
                    None => None,
                };
                let remove_publication_column = match input.remove {
                    Some(BrandVariant::Primary) => Some("primary_publication_id"),
                    Some(BrandVariant::Inverse) => Some("inverse_publication_id"),
                    None => None,
                };
                let mut columns = String::from(
                    "scope_kind,scope_id,company_id,document_mode,updated_by,updated_at",
                );
                let mut values = String::from("$1,$2,$3,$4,$5,$6");
                let mut updates = String::from(
                    "document_mode=excluded.document_mode,updated_by=excluded.updated_by,updated_at=excluded.updated_at",
                );
                if let Some(column) = remove_column {
                    columns.push(',');
                    columns.push_str(column);
                    values.push_str(",NULL");
                    updates.push(',');
                    updates.push_str(column);
                    updates.push_str("=NULL");
                }
                if let Some(column) = remove_publication_column {
                    columns.push(',');
                    columns.push_str(column);
                    values.push_str(",NULL");
                    updates.push(',');
                    updates.push_str(column);
                    updates.push_str("=NULL");
                }
                let sql = format!(
                    "INSERT INTO organization_brand_profiles({columns}) VALUES({values}) \
                     ON CONFLICT(scope_kind,scope_id) DO UPDATE SET {updates}"
                );
                sqlx::query(AssertSqlSafe(sql))
                    .bind(input.scope_kind.as_str())
                    .bind(&input.scope_id)
                    .bind(if input.scope_kind == BrandScopeKind::Branch { Some(company.as_str()) } else { None })
                    .bind(document_mode)
                    .bind(actor.user_id())
                    .bind(now)
                    .execute(&mut *self.connection)
                    .await
                    .map_err(statement_error)?;
                revoke_media(
                    &mut *self.connection,
                    actor.user_id(),
                    now,
                    previous_attachment_id.as_deref(),
                    previous_publication_id.as_deref(),
                    "Removed from the active workspace brand",
                )
                .await?;
            }
            BrandingMutationAction::Storefront {
                input,
                current,
                previous_attachment_id,
                previous_publication_id,
                ..
            } => {
                if read_active_brand_identity(&mut *self.connection, &company).await?.is_some() {
                    return Err(BusinessError::new(
                        FailureKind::Conflict,
                        "BRAND_PUBLISHED_IDENTITY_CONTROLLED",
                        "Save and independently publish a Brand identity draft to change the current storefront identity.",
                    )
                    .into());
                }
                self.guard_expected("storefront", None, None, input.expected_updated_at)
                    .await?;
                let short_name = input
                    .short_name
                    .clone()
                    .or_else(|| current.short_name.clone())
                    .unwrap_or_else(|| current.company_name.chars().take(40).collect());
                let accent = input
                    .accent_color
                    .clone()
                    .or_else(|| current.accent_color.clone())
                    .unwrap_or_else(|| "#3659c9".into());
                sqlx::query(
                    "INSERT INTO storefront_app_brands(company_id,short_name,accent_color,logo_attachment_id,logo_publication_id,updated_by,updated_at) \
                     VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(company_id) DO UPDATE SET \
                     short_name=excluded.short_name,accent_color=excluded.accent_color,\
                     logo_attachment_id=CASE WHEN $8::boolean THEN NULL ELSE COALESCE(storefront_app_brands.logo_attachment_id,excluded.logo_attachment_id) END,\
                     logo_publication_id=CASE WHEN $8::boolean THEN NULL ELSE COALESCE(storefront_app_brands.logo_publication_id,excluded.logo_publication_id) END,\
                     updated_by=excluded.updated_by,updated_at=excluded.updated_at",
                )
                .bind(&company)
                .bind(short_name)
                .bind(accent)
                .bind(current.logo_attachment_id.as_deref())
                .bind(current.logo_publication_id.as_deref())
                .bind(actor.user_id())
                .bind(now)
                .bind(input.remove_logo.unwrap_or(false))
                .execute(&mut *self.connection)
                .await
                .map_err(statement_error)?;
                if input.remove_logo.unwrap_or(false) {
                    revoke_media(
                        &mut *self.connection,
                        actor.user_id(),
                        now,
                        previous_attachment_id.as_deref(),
                        previous_publication_id.as_deref(),
                        "Removed from the active storefront brand",
                    )
                    .await?;
                }
            }
            BrandingMutationAction::Media {
                input,
                current_profile,
                current_storefront,
                previous_attachment_id,
                previous_publication_id,
                ..
            } => {
                let publication_attachment = match input.publication_id.as_deref() {
                    Some(id) => self
                        .publication_inner(id)
                        .await?
                        .and_then(|row| row.source_attachment_id),
                    None => None,
                };
                let attachment_id = input.attachment_id.clone().or(publication_attachment);
                if input.variant.is_some() {
                    self.guard_expected(
                        "profile",
                        Some(input.scope_kind.as_str()),
                        Some(&input.scope_id),
                        input.expected_updated_at,
                    )
                    .await?;
                } else {
                    self.guard_expected("storefront", None, None, input.expected_updated_at)
                        .await?;
                }
                match input.variant {
                    Some(BrandVariant::Primary) | Some(BrandVariant::Inverse) => {
                        let column = if input.variant == Some(BrandVariant::Primary) {
                            "primary_attachment_id"
                        } else {
                            "inverse_attachment_id"
                        };
                        let publication_column = if input.variant == Some(BrandVariant::Primary) {
                            "primary_publication_id"
                        } else {
                            "inverse_publication_id"
                        };
                        let document_mode = if input.scope_kind == BrandScopeKind::Holding {
                            "holding"
                        } else {
                            current_profile
                                .as_ref()
                                .map(|row| row.document_mode.as_str())
                                .unwrap_or("branch")
                        };
                        let sql = format!(
                            "INSERT INTO organization_brand_profiles(scope_kind,scope_id,company_id,document_mode,{column},{publication_column},updated_by,updated_at) \
                             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(scope_kind,scope_id) DO UPDATE SET \
                             {column}=excluded.{column},{publication_column}=excluded.{publication_column},\
                             updated_by=excluded.updated_by,updated_at=excluded.updated_at"
                        );
                        sqlx::query(AssertSqlSafe(sql))
                            .bind(input.scope_kind.as_str())
                            .bind(&input.scope_id)
                            .bind(if input.scope_kind == BrandScopeKind::Branch { Some(company.as_str()) } else { None })
                            .bind(document_mode)
                            .bind(attachment_id.as_deref())
                            .bind(input.publication_id.as_deref())
                            .bind(actor.user_id())
                            .bind(now)
                            .execute(&mut *self.connection)
                            .await
                            .map_err(statement_error)?;
                    }
                    None => {
                        sqlx::query(
                            "INSERT INTO storefront_app_brands(company_id,short_name,accent_color,logo_attachment_id,logo_publication_id,updated_by,updated_at) \
                             SELECT $1,left(display_name,40),'#3659c9',$2,$3,$4,$5 FROM organization_nodes \
                             WHERE id=$1 AND kind='company' AND status='active' \
                             ON CONFLICT(company_id) DO UPDATE SET logo_attachment_id=excluded.logo_attachment_id,\
                             logo_publication_id=excluded.logo_publication_id,updated_by=excluded.updated_by,updated_at=excluded.updated_at",
                        )
                        .bind(&company)
                        .bind(attachment_id.as_deref())
                        .bind(input.publication_id.as_deref())
                        .bind(actor.user_id())
                        .bind(now)
                        .execute(&mut *self.connection)
                        .await
                        .map_err(statement_error)?;
                    }
                }
                revoke_media(
                    &mut *self.connection,
                    actor.user_id(),
                    now,
                    previous_attachment_id.as_deref(),
                    previous_publication_id.as_deref(),
                    if input.variant.is_some() {
                        "Removed from the active workspace brand"
                    } else {
                        "Removed from the active storefront brand"
                    },
                )
                .await?;
                let _ = (current_profile, current_storefront);
            }
        }
        insert_audit_and_outbox(&mut *self.connection, &actor.command, mutation, result, now).await?;
        sqlx::query("DELETE FROM command_assertions WHERE command_id=$1")
            .bind(key)
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }
}

async fn revoke_media(
    connection: &mut PgConnection,
    actor: &str,
    now: i64,
    attachment_id: Option<&str>,
    publication_id: Option<&str>,
    reason: &str,
) -> Result<(), BrandingError> {
    if let Some(publication_id) = publication_id {
        sqlx::query(
            "UPDATE attachment_publications SET state='revoked',revoked_by=$1,revoked_at=$2,revocation_reason=$3 \
             WHERE id=$4 AND state='approved'",
        )
        .bind(actor)
        .bind(now)
        .bind(reason)
        .bind(publication_id)
        .execute(&mut *connection)
        .await
        .map_err(statement_error)?;
        sqlx::query(
            "INSERT INTO attachment_events(id,company_id,link_id,actor_id,action,details_json,created_at) \
             SELECT gen_random_uuid()::text,company_id,source_link_id,$1,'publication.revoked',$2,$3 \
             FROM attachment_publications WHERE id=$4 AND state='revoked'",
        )
        .bind(actor)
        .bind(serde_json::json!({"publicationId":publication_id,"reason":reason}).to_string())
        .bind(now)
        .bind(publication_id)
        .execute(&mut *connection)
        .await
        .map_err(statement_error)?;
    }
    if let Some(attachment_id) = attachment_id {
        sqlx::query("UPDATE attachments SET revoked_at=$1 WHERE id=$2 AND revoked_at IS NULL")
            .bind(now)
            .bind(attachment_id)
            .execute(&mut *connection)
            .await
            .map_err(statement_error)?;
    }
    Ok(())
}

async fn insert_audit_and_outbox(
    connection: &mut PgConnection,
    actor: &erpx_server_workflow::public::commands::CommandActor,
    mutation: &BrandingMutation,
    result: &Value,
    now: i64,
) -> Result<(), BrandingError> {
    let operation = mutation.common.operation;
    let record_type = operation.split('.').take(2).collect::<Vec<_>>().join("_");
    sqlx::query(
        "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
         VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7,$4,$8,(SELECT authority_json FROM command_receipts WHERE command_id=$1))",
    )
    .bind(&mutation.common.key)
    .bind(actor.user_id.as_str())
    .bind(actor.assignment_id.as_str())
    .bind(operation)
    .bind(&record_type)
    .bind(&mutation.common.record_id)
    .bind(mutation.common.version)
    .bind(now)
    .execute(&mut *connection)
    .await
    .map_err(statement_error)?;
    let state = result
        .get("state")
        .and_then(Value::as_str)
        .or_else(|| result.get("status").and_then(Value::as_str));
    let scope = CommandEventScope::from(actor);
    let payload = actor.outbox_payload(
        &mutation.common.record_id,
        mutation.common.version,
        state,
        &scope,
    )?;
    sqlx::query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
         VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(&mutation.common.key)
    .bind(operation)
    .bind(&record_type)
    .bind(&mutation.common.record_id)
    .bind(mutation.common.version)
    .bind(payload.as_str())
    .bind(now)
    .execute(&mut *connection)
    .await
    .map_err(statement_error)?;
    Ok(())
}

impl BrandingPorts for PgBranding<'_> {
    type Error = BrandingError;

    fn actor(&self) -> &BrandActor {
        self.actor
    }

    fn now(&self) -> i64 {
        self.now
    }

    fn assert_media_writable(&self) -> Result<(), BusinessError> {
        if std::env::var("ERPX_ATTACHMENTS_READ_ONLY").ok().as_deref() == Some("true") {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "ATTACHMENTS_READ_ONLY",
                "Brand media changes are disabled during attachment rollback. Existing approved media remains readable.",
            ));
        }
        Ok(())
    }

    fn authorize(
        &mut self,
        write: bool,
        owner: bool,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send {
        self.authorize_inner(write, owner)
    }

    fn replay(
        &mut self,
        key: &str,
        operation: &str,
        record_id: &str,
        input: &CommandJson,
    ) -> impl std::future::Future<Output = Result<Option<Value>, Self::Error>> + Send {
        self.replay_inner(key, operation, record_id, input)
    }

    fn profile(
        &mut self,
        scope_kind: BrandScopeKind,
        scope_id: &str,
    ) -> impl std::future::Future<Output = Result<Option<BrandingProfile>, Self::Error>> + Send {
        self.profile_inner(scope_kind, scope_id)
    }

    fn branch(
        &mut self,
        id: &str,
    ) -> impl std::future::Future<Output = Result<Option<BrandingBranch>, Self::Error>> + Send {
        self.branch_inner(id)
    }

    fn branches(
        &mut self,
    ) -> impl std::future::Future<Output = Result<Vec<BrandingBranch>, Self::Error>> + Send {
        self.branches_inner()
    }

    fn storefront(
        &mut self,
    ) -> impl std::future::Future<Output = Result<Option<BrandingStorefront>, Self::Error>> + Send {
        self.storefront_inner()
    }

    fn published(
        &mut self,
    ) -> impl std::future::Future<Output = Result<Option<Value>, Self::Error>> + Send {
        self.published_inner()
    }

    fn attachment(
        &mut self,
        id: &str,
    ) -> impl std::future::Future<Output = Result<Option<BrandingAttachment>, Self::Error>> + Send {
        self.attachment_inner(id)
    }

    fn publication(
        &mut self,
        id: &str,
    ) -> impl std::future::Future<Output = Result<Option<BrandingPublication>, Self::Error>> + Send {
        self.publication_inner(id)
    }

    fn commit(
        &mut self,
        mutation: &BrandingMutation,
        result: &Value,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send {
        self.commit_inner(mutation, result)
    }
}
