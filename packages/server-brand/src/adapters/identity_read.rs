//! PostgreSQL Brand identity reads. Every read runs on the caller's retained
//! Holding transaction and never constructs an independent actor or grant.
use crate::adapters::identity::{BrandError, PgBrandIdentity, authorize};
use crate::adapters::identity_use_cases::BrandActor;
use crate::adapters::identity_read_use_cases::{
    BrandIdentityReadPorts, PublishedBrandIdentityPorts,
};
use crate::domain::identity::{
    ActiveBrandIdentity, ActiveBrandIdentityRow, BrandCategoryLock, BrandDraft,
    BrandIdentityEvent, BrandMediaOption, BrandPublished, LegacyBrand,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::PgConnection;

pub struct PgBrandIdentityRead<'c> {
    connection: &'c mut PgConnection,
    actor: &'c BrandActor,
    now: i64,
}

impl<'c> PgBrandIdentityRead<'c> {
    pub fn new(connection: &'c mut PgConnection, actor: &'c BrandActor, now: i64) -> Self {
        Self {
            connection,
            actor,
            now,
        }
    }

    async fn active_inner(&mut self) -> Result<Option<ActiveBrandIdentity>, BrandError> {
        PgBrandIdentity::new(self.connection, self.actor, self.now)
            .read_active()
            .await
    }

    async fn legacy_inner(&mut self) -> Result<LegacyBrand, BrandError> {
        let row = sqlx::query(
            "SELECT c.display_name,b.short_name,b.accent_color FROM organization_nodes c \
             LEFT JOIN storefront_app_brands b ON b.company_id=c.id \
             WHERE c.id=$1 AND c.kind='company' AND c.status='active'",
        )
        .bind(self.actor.company_id())
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "COMPANY_REQUIRED",
                "The Company storefront is unavailable.",
            )
            .into());
        };
        use sqlx::Row;
        let display_name: String = row.try_get("display_name")?;
        let short_name: Option<String> = row.try_get("short_name")?;
        let accent_color: Option<String> = row.try_get("accent_color")?;
        let short_name = short_name
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| truncate_units(display_name.trim(), 40));
        Ok(LegacyBrand {
            short_name,
            accent_color: accent_color.unwrap_or_else(|| "#3659c9".into()),
        })
    }
}

/// `Array.from(value).slice(0,limit).join("")` uses Unicode scalar values, not
/// UTF-16 code units.
fn truncate_units(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

impl BrandIdentityReadPorts for PgBrandIdentityRead<'_> {
    type Error = BrandError;

    fn actor(&self) -> &BrandActor {
        self.actor
    }

    fn authorize(
        &mut self,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send {
        authorize(self.connection, self.actor, self.now, false, false)
    }

    async fn drafts(&mut self, include_others: bool) -> Result<Vec<BrandDraft>, Self::Error> {
        Ok(sqlx::query_as::<_, BrandDraft>(
            "SELECT id,company_id,based_revision,identity_json,status,version,created_by,updated_by,created_at,updated_at \
             FROM brand_identity_drafts WHERE company_id=$1 AND ($2::boolean OR created_by=$3) \
             ORDER BY updated_at DESC",
        )
        .bind(self.actor.company_id())
        .bind(include_others)
        .bind(self.actor.user_id())
        .fetch_all(&mut *self.connection)
        .await?)
    }

    async fn versions(&mut self) -> Result<Vec<BrandPublished>, Self::Error> {
        Ok(sqlx::query_as::<_, BrandPublished>(
            "SELECT id,company_id,revision,identity_json,draft_id,draft_version,created_by,published_by,published_at \
             FROM brand_identity_versions WHERE company_id=$1 ORDER BY revision DESC",
        )
        .bind(self.actor.company_id())
        .fetch_all(&mut *self.connection)
        .await?)
    }

    async fn locks(&mut self) -> Result<Vec<BrandCategoryLock>, Self::Error> {
        Ok(sqlx::query_as::<_, BrandCategoryLock>(
            "SELECT company_id,category,locked,approved_snapshot_json,version,updated_by,reason,updated_at \
             FROM brand_category_locks WHERE company_id=$1 ORDER BY category",
        )
        .bind(self.actor.company_id())
        .fetch_all(&mut *self.connection)
        .await?)
    }

    async fn history(&mut self) -> Result<Vec<BrandIdentityEvent>, Self::Error> {
        Ok(sqlx::query_as::<_, BrandIdentityEvent>(
            "SELECT id,company_id,record_id,action,version,actor_id,reason,details_json,created_at \
             FROM brand_identity_events WHERE company_id=$1 ORDER BY created_at DESC,id LIMIT 200",
        )
        .bind(self.actor.company_id())
        .fetch_all(&mut *self.connection)
        .await?)
    }

    fn active(
        &mut self,
    ) -> impl std::future::Future<Output = Result<Option<ActiveBrandIdentity>, Self::Error>> + Send
    {
        self.active_inner()
    }

    fn legacy(
        &mut self,
    ) -> impl std::future::Future<Output = Result<LegacyBrand, Self::Error>> + Send {
        self.legacy_inner()
    }

    async fn media(&mut self) -> Result<Vec<BrandMediaOption>, Self::Error> {
        let rows = sqlx::query_as::<_, (String, Option<String>)>(
            "SELECT id,alt_text FROM attachment_publications WHERE company_id=$1 \
             AND target_type='company_storefront_brand' AND target_id=$2 AND state='approved' \
             ORDER BY approved_at DESC",
        )
        .bind(self.actor.company_id())
        .bind(self.actor.company_id())
        .fetch_all(&mut *self.connection)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, name)| BrandMediaOption { id, name })
            .collect())
    }
}

impl PublishedBrandIdentityPorts for PgBrandIdentityRead<'_> {
    type Error = BrandError;

    async fn available(&mut self) -> Result<bool, Self::Error> {
        Ok(
            sqlx::query_scalar("SELECT to_regclass('brand_identity_active') IS NOT NULL")
                .fetch_one(&mut *self.connection)
                .await?,
        )
    }

    async fn active_row(&mut self) -> Result<Option<ActiveBrandIdentityRow>, Self::Error> {
        Ok(sqlx::query_as::<_, ActiveBrandIdentityRow>(
            "SELECT v.id,v.company_id,v.revision,v.identity_json,v.draft_id,v.draft_version,\
             v.created_by,v.published_by,v.published_at,a.version active_version \
             FROM brand_identity_active a JOIN brand_identity_versions v \
             ON v.id=a.identity_version_id AND v.company_id=a.company_id WHERE a.company_id=$1",
        )
        .bind(self.actor.company_id())
        .fetch_optional(&mut *self.connection)
        .await?)
    }
}
