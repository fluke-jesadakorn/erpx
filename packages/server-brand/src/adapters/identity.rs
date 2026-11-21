//! PostgreSQL Brand identity commands. Every method runs on the caller's
//! serializable Holding transaction and keeps the retained guard, receipt,
//! audit and outbox evidence in that same unit.
use crate::adapters::identity_use_cases::{
    BrandActor, BrandIdentityPorts, BrandMutation, BrandMutationAction,
};
use crate::domain::identity::{
    ActiveBrandIdentity, ActiveBrandIdentityRow, BrandCategoryLock, BrandDraft, BrandIdentityEvent,
    BrandPublished,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandEventScope, CommandJson, command_fingerprint,
};
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};

pub type BrandError = PgTransactionError<BusinessError>;

fn conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CONFLICT",
        "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
    )
}

fn authority_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "BRAND_AUTHORITY_CHANGED",
        "Your current Brand authority is unavailable.",
    )
}

/// Current IAM, module and Company scope authority. The retained TypeScript
/// owner repeated this IAM projection after the selected-context check; the
/// same exact source is re-read here on the caller's transaction.
pub(crate) async fn authorize(
    connection: &mut PgConnection,
    actor: &BrandActor,
    now: i64,
    write: bool,
    owner: bool,
) -> Result<(), BrandError> {
    let permission = if write {
        "organization.manage"
    } else {
        "records.read"
    };
    actor.require_permission(permission)?;
    actor.require_brand_module()?;
    if write {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM organization_nodes WHERE id=$1 AND kind='company' AND status='active')",
        )
        .bind(actor.company_id())
        .fetch_one(&mut *connection)
        .await?;
        if !exists {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "BRAND_MANAGEMENT_FORBIDDEN",
                "A Holding owner or authorized organization administrator is required.",
            )
            .into());
        }
    }
    let current: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM assignments g WHERE g.id=$1 AND g.user_id=$2 AND g.role=$3 \
         AND g.company_id=$4 AND iam_assignment_active(g.id,$5) \
         AND iam_principal_active(g.user_id,g.company_id,$5) \
         AND iam_role_allows(g.role,g.company_id,$6))",
    )
    .bind(actor.assignment_id())
    .bind(actor.user_id())
    .bind(actor.role())
    .bind(actor.company_id())
    .bind(now)
    .bind(permission)
    .fetch_one(&mut *connection)
    .await?;
    if !current {
        return Err(authority_changed().into());
    }
    if owner {
        actor.require_permission("company.owner")?;
    }
    Ok(())
}

/// Constraint failures are the retained guard outcome; serialization and
/// deadlock states stay persistence failures so the owning unit can retry.
fn statement_error(error: sqlx::Error) -> BrandError {
    match error.as_database_error().and_then(|error| error.code()).as_deref() {
        Some("23502" | "23503" | "23505" | "23514") => conflict().into(),
        _ => PgTransactionError::Persistence(PersistenceError::from(error)),
    }
}

/// The active Company identity for consuming owners (for example the
/// storefront projection). This read carries no authority; callers retain their
/// own current scope and source checks.
pub async fn read_active_brand_identity(
    connection: &mut PgConnection,
    company: &str,
) -> Result<Option<ActiveBrandIdentity>, BrandError> {
    let available: bool =
        sqlx::query_scalar("SELECT to_regclass('brand_identity_active') IS NOT NULL")
            .fetch_one(&mut *connection)
            .await?;
    if !available {
        return Ok(None);
    }
    let row = sqlx::query_as::<_, ActiveBrandIdentityRow>(
        "SELECT v.id,v.company_id,v.revision,v.identity_json,v.draft_id,v.draft_version,\
         v.created_by,v.published_by,v.published_at,a.version active_version \
         FROM brand_identity_active a JOIN brand_identity_versions v \
         ON v.id=a.identity_version_id AND v.company_id=a.company_id WHERE a.company_id=$1",
    )
    .bind(company)
    .fetch_optional(&mut *connection)
    .await?;
    Ok(row.map(ActiveBrandIdentityRow::into_identity).transpose()?)
}

pub struct PgBrandIdentity<'c> {
    connection: &'c mut PgConnection,
    actor: &'c BrandActor,
    now: i64,
}

impl<'c> PgBrandIdentity<'c> {
    pub fn new(connection: &'c mut PgConnection, actor: &'c BrandActor, now: i64) -> Self {
        Self {
            connection,
            actor,
            now,
        }
    }

    async fn require_transaction(connection: &mut PgConnection) -> Result<(), BrandError> {
        if !connection.is_in_transaction() {
            return Err(PersistenceError::new("BRAND_COMMAND_TRANSACTION_REQUIRED").into());
        }
        Ok(())
    }

    async fn authorize_inner(&mut self, write: bool, owner: bool) -> Result<(), BrandError> {
        authorize(self.connection, self.actor, self.now, write, owner).await
    }

    async fn replay_inner(
        &mut self,
        key: &str,
        operation: &str,
        record_id: &str,
        input: &CommandJson,
    ) -> Result<Option<Value>, BrandError> {
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
                "BRAND_COMMAND_RESULT_UNAVAILABLE",
                "The retained Brand command result is unavailable.",
            )
        })?;
        Ok(Some(value))
    }

    /// The active identity for the Brand storefront projection. Callers use a
    /// dedicated read adapter; this method does not authorize.
    pub async fn read_active(mut self) -> Result<Option<ActiveBrandIdentity>, BrandError> {
        self.active_inner().await
    }

    async fn active_inner(&mut self) -> Result<Option<ActiveBrandIdentity>, BrandError> {
        let available: bool = sqlx::query_scalar(
            "SELECT to_regclass('brand_identity_active') IS NOT NULL",
        )
        .fetch_one(&mut *self.connection)
        .await?;
        if !available {
            return Ok(None);
        }
        let row = sqlx::query_as::<_, ActiveBrandIdentityRow>(
            "SELECT v.id,v.company_id,v.revision,v.identity_json,v.draft_id,v.draft_version,\
             v.created_by,v.published_by,v.published_at,a.version active_version \
             FROM brand_identity_active a JOIN brand_identity_versions v \
             ON v.id=a.identity_version_id AND v.company_id=a.company_id WHERE a.company_id=$1",
        )
        .bind(self.actor.company_id())
        .fetch_optional(&mut *self.connection)
        .await?;
        match row {
            Some(row) => Ok(Some(row.into_identity()?)),
            None => Ok(None),
        }
    }

    async fn locks_inner(&mut self) -> Result<Vec<BrandCategoryLock>, BrandError> {
        Ok(sqlx::query_as::<_, BrandCategoryLock>(
            "SELECT company_id,category,locked,approved_snapshot_json,version,updated_by,reason,updated_at \
             FROM brand_category_locks WHERE company_id=$1 ORDER BY category",
        )
        .bind(self.actor.company_id())
        .fetch_all(&mut *self.connection)
        .await?)
    }

    async fn draft_inner(&mut self, id: &str) -> Result<Option<BrandDraft>, BrandError> {
        Ok(sqlx::query_as::<_, BrandDraft>(
            "SELECT id,company_id,based_revision,identity_json,status,version,created_by,updated_by,created_at,updated_at \
             FROM brand_identity_drafts WHERE id=$1 AND company_id=$2",
        )
        .bind(id)
        .bind(self.actor.company_id())
        .fetch_optional(&mut *self.connection)
        .await?)
    }

    async fn published_inner(&mut self, id: &str) -> Result<Option<BrandPublished>, BrandError> {
        Ok(sqlx::query_as::<_, BrandPublished>(
            "SELECT id,company_id,revision,identity_json,draft_id,draft_version,created_by,published_by,published_at \
             FROM brand_identity_versions WHERE id=$1 AND company_id=$2",
        )
        .bind(id)
        .bind(self.actor.company_id())
        .fetch_optional(&mut *self.connection)
        .await?)
    }

    async fn next_revision_inner(&mut self) -> Result<i64, BrandError> {
        Ok(sqlx::query_scalar(
            "SELECT COALESCE(max(revision),0)+1 FROM brand_identity_versions WHERE company_id=$1",
        )
        .bind(self.actor.company_id())
        .fetch_one(&mut *self.connection)
        .await?)
    }

    async fn same_person_inner(&mut self, user_id: &str) -> Result<bool, BrandError> {
        Ok(sqlx::query_scalar("SELECT iam_same_person($1,$2,$3)")
            .bind(self.actor.user_id())
            .bind(user_id)
            .bind(self.actor.company_id())
            .fetch_one(&mut *self.connection)
            .await?)
    }

    async fn commit_inner(
        &mut self,
        mutation: &BrandMutation,
        result: &Value,
    ) -> Result<(), BrandError> {
        let actor = self.actor;
        let company = actor.company_id();
        let now = mutation.common.now;
        let version = mutation.common.version;
        let record_id = mutation.common.record_id.as_str();
        let key = mutation.common.key.as_str();
        let connection = &mut *self.connection;
        Self::require_transaction(&mut *connection).await?;
        // A Company write is anchored to the active Company row and its lock.
        sqlx::query(
            "SELECT id FROM organization_nodes WHERE id=$1 AND kind='company' AND status='active' FOR UPDATE",
        )
        .bind(company)
        .execute(&mut *connection)
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
        .execute(&mut *connection)
        .await
        .map_err(statement_error)?;
        match &mutation.action {
            BrandMutationAction::Save {
                identity,
                locks,
                existing,
                active,
                reason,
                expected_published_version,
                expected_version,
            } => {
                guard_authority(&mut *connection, key, &actor.command, false).await?;
                guard_locks(&mut *connection, key, company, locks).await?;
                guard_media(&mut *connection, key, company, identity).await?;
                guard_published_version(&mut *connection, key, company, *expected_published_version)
                    .await?;
                let identity_json = identity.canonical_json()?;
                match existing {
                    Some(_) => {
                        guard_draft_version(
                            &mut *connection,
                            key,
                            record_id,
                            company,
                            (*expected_version).unwrap_or(0),
                        )
                        .await?;
                        sqlx::query(
                            "UPDATE brand_identity_drafts SET identity_json=$1,based_revision=$2,version=version+1,updated_by=$3,updated_at=$4 WHERE id=$5",
                        )
                        .bind(&identity_json)
                        .bind(active.as_ref().map(|row| row.revision).unwrap_or(0))
                        .bind(actor.user_id())
                        .bind(now)
                        .bind(record_id)
                        .execute(&mut *connection)
                        .await
                        .map_err(statement_error)?;
                    }
                    None => {
                        sqlx::query(
                            "INSERT INTO brand_identity_drafts(id,company_id,based_revision,identity_json,created_by,updated_by,created_at,updated_at) \
                             VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
                        )
                        .bind(record_id)
                        .bind(company)
                        .bind(active.as_ref().map(|row| row.revision).unwrap_or(0))
                        .bind(&identity_json)
                        .bind(actor.user_id())
                        .bind(actor.user_id())
                        .bind(now)
                        .bind(now)
                        .execute(&mut *connection)
                        .await
                        .map_err(statement_error)?;
                    }
                }
                let action = if existing.is_some() { "save" } else { "draft" };
                insert_event(
                    &mut *connection,
                    key,
                    company,
                    record_id,
                    action,
                    version,
                    actor.user_id(),
                    reason,
                    &serde_json::json!({
                        "identity": identity,
                        "basedRevision": active.as_ref().map(|row| row.revision).unwrap_or(0),
                    }),
                    now,
                )
                .await?;
            }
            BrandMutationAction::Publish {
                identity,
                locks,
                draft,
                published_id,
                revision,
                expected_published_version,
                expected_version,
                reason,
            } => {
                guard_authority(&mut *connection, key, &actor.command, false).await?;
                guard_locks(&mut *connection, key, company, locks).await?;
                guard_media(&mut *connection, key, company, identity).await?;
                guard_published_version(&mut *connection, key, company, *expected_published_version)
                    .await?;
                guard_independent_draft(&mut *connection, key, record_id, company, *expected_version, actor)
                    .await?;
                sqlx::query(
                    "INSERT INTO brand_identity_versions VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
                )
                .bind(published_id)
                .bind(company)
                .bind(revision)
                .bind(&draft.identity_json)
                .bind(record_id)
                .bind(expected_version)
                .bind(&draft.created_by)
                .bind(actor.user_id())
                .bind(now)
                .execute(&mut *connection)
                .await
                .map_err(statement_error)?;
                sqlx::query(
                    "UPDATE brand_identity_drafts SET status='published',version=version+1,updated_by=$1,updated_at=$2 WHERE id=$3",
                )
                .bind(actor.user_id())
                .bind(now)
                .bind(record_id)
                .execute(&mut *connection)
                .await
                .map_err(statement_error)?;
                activate(
                    &mut *connection,
                    company,
                    published_id,
                    *expected_published_version + 1,
                    identity,
                    actor.user_id(),
                    now,
                )
                .await?;
                insert_event(
                    &mut *connection,
                    key,
                    company,
                    published_id,
                    "publish",
                    *expected_published_version + 1,
                    actor.user_id(),
                    reason,
                    &serde_json::json!({
                        "draftId": record_id,
                        "draftVersion": expected_version,
                        "revision": revision,
                        "identity": identity,
                    }),
                    now,
                )
                .await?;
            }
            BrandMutationAction::Rollback {
                identity,
                locks,
                published,
                expected_published_version,
                reason,
            } => {
                guard_authority(&mut *connection, key, &actor.command, false).await?;
                guard_locks(&mut *connection, key, company, locks).await?;
                guard_media(&mut *connection, key, company, identity).await?;
                guard_active_version(&mut *connection, key, company, *expected_published_version).await?;
                activate(
                    &mut *connection,
                    company,
                    &published.id,
                    version,
                    identity,
                    actor.user_id(),
                    now,
                )
                .await?;
                insert_event(
                    &mut *connection,
                    key,
                    company,
                    &published.id,
                    "rollback",
                    version,
                    actor.user_id(),
                    reason,
                    &serde_json::json!({
                        "revision": published.revision,
                        "identity": identity,
                    }),
                    now,
                )
                .await?;
            }
            BrandMutationAction::Lock {
                category,
                locked,
                active,
                expected_version,
                expected_published_version,
                reason,
            } => {
                guard_authority(&mut *connection, key, &actor.command, true).await?;
                guard_lock_version(
                    &mut *connection,
                    key,
                    company,
                    category.as_str(),
                    *expected_version,
                )
                .await?;
                guard_active_version(&mut *connection, key, company, *expected_published_version).await?;
                sqlx::query(
                    "INSERT INTO brand_category_locks VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
                     ON CONFLICT(company_id,category) DO UPDATE SET locked=excluded.locked,\
                     approved_snapshot_json=excluded.approved_snapshot_json,version=excluded.version,\
                     updated_by=excluded.updated_by,reason=excluded.reason,updated_at=excluded.updated_at",
                )
                .bind(company)
                .bind(category.as_str())
                .bind(locked)
                .bind(active.identity.category(*category).to_string())
                .bind(version)
                .bind(actor.user_id())
                .bind(reason)
                .bind(now)
                .execute(&mut *connection)
                .await
                .map_err(statement_error)?;
                insert_event(
                    &mut *connection,
                    key,
                    company,
                    record_id,
                    if *locked { "lock" } else { "unlock" },
                    version,
                    actor.user_id(),
                    reason,
                    &serde_json::json!({
                        "category": category.as_str(),
                        "publishedId": active.id,
                        "locked": locked,
                    }),
                    now,
                )
                .await?;
            }
            BrandMutationAction::Cancel {
                expected_version,
                reason,
            } => {
                guard_authority(&mut *connection, key, &actor.command, false).await?;
                guard_draft_version(&mut *connection, key, record_id, company, *expected_version).await?;
                sqlx::query(
                    "UPDATE brand_identity_drafts SET status='cancelled',version=version+1,updated_by=$1,updated_at=$2 WHERE id=$3",
                )
                .bind(actor.user_id())
                .bind(now)
                .bind(record_id)
                .execute(&mut *connection)
                .await
                .map_err(statement_error)?;
                insert_event(
                    &mut *connection,
                    key,
                    company,
                    record_id,
                    "cancel",
                    version,
                    actor.user_id(),
                    reason,
                    &serde_json::json!({}),
                    now,
                )
                .await?;
            }
        }
        insert_audit_and_outbox(&mut *connection, &actor.command, mutation, result, now).await?;
        sqlx::query("DELETE FROM command_assertions WHERE command_id=$1")
            .bind(key)
            .execute(&mut *connection)
            .await?;
        Ok(())
    }
}

impl BrandIdentityPorts for PgBrandIdentity<'_> {
    type Error = BrandError;

    fn actor(&self) -> &BrandActor {
        self.actor
    }

    fn now(&self) -> i64 {
        self.now
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

    fn active(
        &mut self,
    ) -> impl std::future::Future<Output = Result<Option<ActiveBrandIdentity>, Self::Error>> + Send
    {
        self.active_inner()
    }

    fn locks(
        &mut self,
    ) -> impl std::future::Future<Output = Result<Vec<BrandCategoryLock>, Self::Error>> + Send {
        self.locks_inner()
    }

    fn draft(
        &mut self,
        id: &str,
    ) -> impl std::future::Future<Output = Result<Option<BrandDraft>, Self::Error>> + Send {
        self.draft_inner(id)
    }

    fn published(
        &mut self,
        id: &str,
    ) -> impl std::future::Future<Output = Result<Option<BrandPublished>, Self::Error>> + Send {
        self.published_inner(id)
    }

    fn next_revision(
        &mut self,
    ) -> impl std::future::Future<Output = Result<i64, Self::Error>> + Send {
        self.next_revision_inner()
    }

    fn is_same_person(
        &mut self,
        user_id: &str,
    ) -> impl std::future::Future<Output = Result<bool, Self::Error>> + Send {
        self.same_person_inner(user_id)
    }

    fn commit(
        &mut self,
        mutation: &BrandMutation,
        result: &Value,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send {
        self.commit_inner(mutation, result)
    }
}

async fn activate(
    connection: &mut PgConnection,
    company: &str,
    published_id: &str,
    version: i64,
    identity: &crate::domain::identity::BrandIdentity,
    actor: &str,
    now: i64,
) -> Result<(), BrandError> {
    sqlx::query(
        "INSERT INTO brand_identity_active VALUES($1,$2,$3,$4,$5) ON CONFLICT(company_id) DO UPDATE SET \
         identity_version_id=excluded.identity_version_id,version=excluded.version,\
         updated_by=excluded.updated_by,updated_at=excluded.updated_at",
    )
    .bind(company)
    .bind(published_id)
    .bind(version)
    .bind(actor)
    .bind(now)
    .execute(&mut *connection)
    .await
    .map_err(statement_error)?;
    // The storefront row is owned exclusively by the native Branding command.
    // Published identity remains authoritative through brand_identity_active,
    // so identity publication never becomes a second storefront writer.
    let _ = (identity, now);
    Ok(())
}

async fn guard_authority(
    connection: &mut PgConnection,
    key: &str,
    actor: &CommandActor,
    owner: bool,
) -> Result<(), BrandError> {
    let permission = if owner {
        "company.owner"
    } else {
        "organization.manage"
    };
    sqlx::query(
        "INSERT INTO command_assertions(command_id,step,ok) VALUES($1,'brand-authority',CASE WHEN EXISTS(\
         SELECT 1 FROM assignments g WHERE g.id=$2 AND g.user_id=$3 AND g.role=$4 AND g.company_id=$5 \
         AND g.scope_kind='holding' \
         AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
         AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
         AND iam_role_allows(g.role,g.company_id,$6)) THEN 1 ELSE 0 END)",
    )
    .bind(key)
    .bind(actor.assignment_id.as_str())
    .bind(actor.user_id.as_str())
    .bind(&actor.role)
    .bind(actor.company_id.as_str())
    .bind(permission)
    .execute(&mut *connection)
    .await
    .map_err(statement_error)?;
    Ok(())
}

async fn guard_locks(
    connection: &mut PgConnection,
    key: &str,
    company: &str,
    locks: &[BrandCategoryLock],
) -> Result<(), BrandError> {
    let expected: i64 = locks.iter().map(|lock| lock.version).sum();
    sqlx::query(
        "INSERT INTO command_assertions(command_id,step,ok) VALUES($1,'brand-lock-state',CASE WHEN \
         COALESCE((SELECT sum(version) FROM brand_category_locks WHERE company_id=$2),0)=$3 THEN 1 ELSE 0 END)",
    )
    .bind(key)
    .bind(company)
    .bind(expected)
    .execute(&mut *connection)
    .await
    .map_err(statement_error)?;
    Ok(())
}

async fn guard_media(
    connection: &mut PgConnection,
    key: &str,
    company: &str,
    identity: &crate::domain::identity::BrandIdentity,
) -> Result<(), BrandError> {
    let Some(publication) = identity.visual.logo_publication_id.as_deref() else {
        return Ok(());
    };
    sqlx::query("SELECT id FROM attachment_publications WHERE id=$1 FOR SHARE")
        .bind(publication)
        .execute(&mut *connection)
        .await
        .map_err(statement_error)?;
    sqlx::query(
        "INSERT INTO command_assertions(command_id,step,ok) VALUES($1,'brand-approved-media',CASE WHEN EXISTS(\
         SELECT 1 FROM attachment_publications WHERE id=$2 AND company_id=$3 \
         AND target_type='company_storefront_brand' AND target_id=$4 AND state='approved') THEN 1 ELSE 0 END)",
    )
    .bind(key)
    .bind(publication)
    .bind(company)
    .bind(company)
    .execute(&mut *connection)
    .await
    .map_err(statement_error)?;
    Ok(())
}

async fn guard_published_version(
    connection: &mut PgConnection,
    key: &str,
    company: &str,
    expected: i64,
) -> Result<(), BrandError> {
    sqlx::query(
        "INSERT INTO command_assertions(command_id,step,ok) VALUES($1,'published-version',CASE WHEN \
         COALESCE((SELECT version FROM brand_identity_active WHERE company_id=$2),0)=$3 THEN 1 ELSE 0 END)",
    )
    .bind(key)
    .bind(company)
    .bind(expected)
    .execute(&mut *connection)
    .await
    .map_err(statement_error)?;
    Ok(())
}

async fn guard_active_version(
    connection: &mut PgConnection,
    key: &str,
    company: &str,
    expected: i64,
) -> Result<(), BrandError> {
    sqlx::query(
        "INSERT INTO command_assertions(command_id,step,ok) VALUES($1,'published-version',CASE WHEN \
         EXISTS(SELECT 1 FROM brand_identity_active WHERE company_id=$2 AND version=$3) THEN 1 ELSE 0 END)",
    )
    .bind(key)
    .bind(company)
    .bind(expected)
    .execute(&mut *connection)
    .await
    .map_err(statement_error)?;
    Ok(())
}

async fn guard_draft_version(
    connection: &mut PgConnection,
    key: &str,
    draft: &str,
    company: &str,
    expected: i64,
) -> Result<(), BrandError> {
    sqlx::query(
        "INSERT INTO command_assertions(command_id,step,ok) VALUES($1,'draft-version',CASE WHEN EXISTS(\
         SELECT 1 FROM brand_identity_drafts WHERE id=$2 AND company_id=$3 AND version=$4 AND status='draft') THEN 1 ELSE 0 END)",
    )
    .bind(key)
    .bind(draft)
    .bind(company)
    .bind(expected)
    .execute(&mut *connection)
    .await
    .map_err(statement_error)?;
    Ok(())
}

async fn guard_independent_draft(
    connection: &mut PgConnection,
    key: &str,
    draft: &str,
    company: &str,
    expected: i64,
    actor: &BrandActor,
) -> Result<(), BrandError> {
    sqlx::query(
        "INSERT INTO command_assertions(command_id,step,ok) VALUES($1,'independent-draft',CASE WHEN EXISTS(\
         SELECT 1 FROM brand_identity_drafts WHERE id=$2 AND company_id=$3 AND version=$4 AND status='draft' \
         AND NOT iam_same_person(created_by,$5,company_id) AND NOT iam_same_person(updated_by,$6,company_id)) THEN 1 ELSE 0 END)",
    )
    .bind(key)
    .bind(draft)
    .bind(company)
    .bind(expected)
    .bind(actor.user_id())
    .bind(actor.user_id())
    .execute(&mut *connection)
    .await
    .map_err(statement_error)?;
    Ok(())
}

async fn guard_lock_version(
    connection: &mut PgConnection,
    key: &str,
    company: &str,
    category: &str,
    expected: i64,
) -> Result<(), BrandError> {
    sqlx::query(
        "INSERT INTO command_assertions(command_id,step,ok) VALUES($1,'lock-version',CASE WHEN \
         COALESCE((SELECT version FROM brand_category_locks WHERE company_id=$2 AND category=$3),0)=$4 THEN 1 ELSE 0 END)",
    )
    .bind(key)
    .bind(company)
    .bind(category)
    .bind(expected)
    .execute(&mut *connection)
    .await
    .map_err(statement_error)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_event(
    connection: &mut PgConnection,
    key: &str,
    company: &str,
    record_id: &str,
    action: &str,
    version: i64,
    actor: &str,
    reason: &str,
    details: &Value,
    now: i64,
) -> Result<(), BrandError> {
    sqlx::query("INSERT INTO brand_identity_events VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
        .bind(format!("brand-event-{key}"))
        .bind(company)
        .bind(record_id)
        .bind(action)
        .bind(version)
        .bind(actor)
        .bind(reason)
        .bind(details.to_string())
        .bind(now)
        .execute(&mut *connection)
        .await
        .map_err(statement_error)?;
    Ok(())
}

async fn insert_audit_and_outbox(
    connection: &mut PgConnection,
    actor: &CommandActor,
    mutation: &BrandMutation,
    result: &Value,
    now: i64,
) -> Result<(), BrandError> {
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

/// Explicit row decoding; the workspace sqlx build does not enable derive macros.
macro_rules! from_row {
    ($type:ty { $($field:ident : $column:literal),+ $(,)? }) => {
        impl<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> for $type {
            fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
                use sqlx::Row;
                Ok(Self { $($field: row.try_get($column)?),+ })
            }
        }
    };
}

from_row!(BrandDraft {
    id: "id",
    company_id: "company_id",
    based_revision: "based_revision",
    identity_json: "identity_json",
    status: "status",
    version: "version",
    created_by: "created_by",
    updated_by: "updated_by",
    created_at: "created_at",
    updated_at: "updated_at",
});

from_row!(BrandPublished {
    id: "id",
    company_id: "company_id",
    revision: "revision",
    identity_json: "identity_json",
    draft_id: "draft_id",
    draft_version: "draft_version",
    created_by: "created_by",
    published_by: "published_by",
    published_at: "published_at",
});

from_row!(ActiveBrandIdentityRow {
    id: "id",
    company_id: "company_id",
    revision: "revision",
    identity_json: "identity_json",
    draft_id: "draft_id",
    draft_version: "draft_version",
    created_by: "created_by",
    published_by: "published_by",
    published_at: "published_at",
    active_version: "active_version",
});

from_row!(BrandCategoryLock {
    company_id: "company_id",
    category: "category",
    locked: "locked",
    approved_snapshot_json: "approved_snapshot_json",
    version: "version",
    updated_by: "updated_by",
    reason: "reason",
    updated_at: "updated_at",
});

from_row!(BrandIdentityEvent {
    id: "id",
    company_id: "company_id",
    record_id: "record_id",
    action: "action",
    version: "version",
    actor_id: "actor_id",
    reason: "reason",
    details_json: "details_json",
    created_at: "created_at",
});
