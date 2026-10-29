use super::postgres_purchases::{PgCompanyPurchaseReader, lock_commercial_authority_sources};
use crate::application::purchases::CompanyPurchaseReadPort;
use crate::{
    application::grant_commands::{
        GrantChange, GrantPreview, GrantReceipt, PackageGrantUnit, apply_package_grant,
        preview_package_grant,
    },
    domain::packages::{CommercialCatalogue, CompanyPackageGrant},
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, HoldingId, UserId};
use sqlx::{Connection, PgConnection, Row};
use std::future::Future;
type Error = PgTransactionError<BusinessError>;

/// Trusted composition supplies real permission/review and settlement-owner
/// sources. prepare must take their complete leases before the first snapshot;
/// an arbitrary previously-read transaction cannot inherit that guarantee.
pub trait PgPackageGrantPolicy: Send {
    fn prepare_sources(
        &mut self,
        connection: &mut PgConnection,
        input: &GrantChange,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn authorize(
        &mut self,
        connection: &mut PgConnection,
        holding: &HoldingId,
        company: &CompanyId,
    ) -> impl Future<Output = Result<UserId, Error>> + Send;
    fn obligations(
        &mut self,
        connection: &mut PgConnection,
        company: &CompanyId,
        package: &str,
    ) -> impl Future<Output = Result<i64, Error>> + Send;
}

/// Public facade exposes the application, never its private raw save operation.
/// No pool, independent commit/retry or default authority/backlog is supplied.
pub struct PgPackageGrantCommands<'c, Policy, ClockSource> {
    unit: PgGrantUnit<'c, Policy, ClockSource>,
}
impl<'c, Policy: PgPackageGrantPolicy, ClockSource: Clock + Sync>
    PgPackageGrantCommands<'c, Policy, ClockSource>
{
    pub fn new(
        holding: HoldingId,
        company: CompanyId,
        connection: &'c mut PgConnection,
        catalogue: &'c CommercialCatalogue,
        policy: &'c mut Policy,
        clock: &'c ClockSource,
    ) -> Self {
        Self {
            unit: PgGrantUnit {
                holding,
                company,
                connection,
                catalogue,
                policy,
                clock,
                writing: false,
            },
        }
    }
    pub async fn apply(&mut self, input: &GrantChange) -> Result<CompanyPackageGrant, Error> {
        self.unit.writing = true;
        let catalogue = self.unit.catalogue;
        apply_package_grant(&mut self.unit, catalogue, input).await
    }
    pub async fn preview(&mut self, input: &GrantChange) -> Result<GrantPreview, Error> {
        self.unit.writing = false;
        let catalogue = self.unit.catalogue;
        preview_package_grant(&mut self.unit, catalogue, input).await
    }
}
struct PgGrantUnit<'c, Policy, ClockSource> {
    holding: HoldingId,
    company: CompanyId,
    connection: &'c mut PgConnection,
    catalogue: &'c CommercialCatalogue,
    policy: &'c mut Policy,
    clock: &'c ClockSource,
    writing: bool,
}
impl<Policy: PgPackageGrantPolicy, ClockSource: Clock + Sync> PgGrantUnit<'_, Policy, ClockSource> {
    fn scope(&self, company: &CompanyId) -> Result<(), Error> {
        if company != &self.company {
            return Err(BusinessError::invalid(
                "COMMERCIAL_COMPANY_MISMATCH",
                "The command belongs to another Company.",
            )
            .into());
        }
        Ok(())
    }
    async fn require_transaction(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("COMMERCIAL_OWNER_TRANSACTION_REQUIRED").into());
        }
        let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?;
        if isolation != "serializable" {
            return Err(PersistenceError::new("COMMERCIAL_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }
}
impl<Policy: PgPackageGrantPolicy, ClockSource: Clock + Sync> PackageGrantUnit
    for PgGrantUnit<'_, Policy, ClockSource>
{
    type Error = Error;
    async fn prepare_sources(&mut self, input: &GrantChange) -> Result<(), Error> {
        self.require_transaction().await?;
        self.scope(&CompanyId::new(&input.grant.company_id)?)?;
        lock_commercial_authority_sources(self.connection).await?;
        self.policy.prepare_sources(self.connection, input).await
    }
    async fn lock(&mut self, key: &str) -> Result<(), Error> {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("commercial.package:{key}"))
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }
    async fn authorize(&mut self, company: &CompanyId) -> Result<UserId, Error> {
        self.scope(company)?;
        self.policy
            .authorize(self.connection, &self.holding, company)
            .await
    }
    async fn current(
        &mut self,
        company: &CompanyId,
        package: &str,
    ) -> Result<Option<CompanyPackageGrant>, Error> {
        self.scope(company)?;
        let row=sqlx::query("SELECT company_id,package_id,package_version::bigint AS package_version,state,starts_at,ends_at,version::bigint AS version,reason,evidence FROM company_package_grants WHERE company_id=$1 AND package_id=$2 FOR UPDATE").bind(company.as_str()).bind(package).fetch_optional(&mut *self.connection).await?;
        row.map(|r| {
            Ok(CompanyPackageGrant {
                company_id: r.try_get("company_id")?,
                package_id: r.try_get("package_id")?,
                package_version: r.try_get("package_version")?,
                state: r.try_get("state")?,
                starts_at: r.try_get("starts_at")?,
                ends_at: r.try_get("ends_at")?,
                version: r.try_get("version")?,
                reason: r.try_get("reason")?,
                evidence: r.try_get("evidence")?,
            })
        })
        .transpose()
    }
    async fn receipt(&mut self, key: &str) -> Result<Option<GrantReceipt>, Error> {
        let batch: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM company_package_change_sets WHERE id=$1)",
        )
        .bind(key)
        .fetch_one(&mut *self.connection)
        .await?;
        if batch {
            return Err(BusinessError::invalid(
                "IDEMPOTENCY_CONFLICT",
                "The key belongs to another Commercial command.",
            )
            .into());
        }
        let row = sqlx::query(
            "SELECT request_hash,after_json FROM company_package_grant_events WHERE id=$1",
        )
        .bind(key)
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|r| {
            let grant = serde_json::from_value::<CompanyPackageGrant>(r.try_get("after_json")?)
                .map_err(|_| PersistenceError::new("COMMERCIAL_RECEIPT_INVALID"))?;
            Ok(GrantReceipt {
                hash: r.try_get("request_hash")?,
                grant,
            })
        })
        .transpose()
    }
    async fn obligations(&mut self, company: &CompanyId, package: &str) -> Result<i64, Error> {
        self.scope(company)?;
        self.policy
            .obligations(self.connection, company, package)
            .await
    }
    async fn dependencies(&mut self, grant: &CompanyPackageGrant) -> Result<(), Error> {
        // The original revision trigger also covers compatible grant writers.
        // This anchor catches unseen inserts into an earlier SERIALIZABLE view.
        // Previews are observations: they must not create a revision anchor.
        // Applying always obtains its own anchor and reruns every predicate.
        if self.writing {
            sqlx::query("INSERT INTO company_package_revisions(company_id,version) VALUES($1,0) ON CONFLICT DO NOTHING").bind(self.company.as_str()).execute(&mut *self.connection).await?;
            sqlx::query(
                "SELECT version FROM company_package_revisions WHERE company_id=$1 FOR UPDATE",
            )
            .bind(self.company.as_str())
            .fetch_one(&mut *self.connection)
            .await?;
        }
        let facts = PgCompanyPurchaseReader::new(
            self.holding.clone(),
            self.company.clone(),
            self.connection,
            self.catalogue,
        )
        .read(&self.company)
        .await?;
        self.catalogue
            .validate_dependencies(grant, &facts.grants, self.clock.now().get())?;
        Ok(())
    }
    async fn save(
        &mut self,
        input: &GrantChange,
        current: Option<&CompanyPackageGrant>,
        actor: &UserId,
        hash: &str,
    ) -> Result<(), Error> {
        let g = &input.grant;
        let instant = self.clock.now().get();
        let written=sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(company_id,package_id) DO UPDATE SET package_version=excluded.package_version,state=excluded.state,starts_at=excluded.starts_at,ends_at=excluded.ends_at,version=excluded.version,reason=excluded.reason,evidence=excluded.evidence,updated_at=excluded.updated_at WHERE company_package_grants.version=$11")
            .bind(&g.company_id).bind(&g.package_id).bind(g.package_version).bind(&g.state).bind(g.starts_at).bind(g.ends_at).bind(g.version).bind(&g.reason).bind(&g.evidence).bind(instant).bind(input.expected_version).execute(&mut *self.connection).await?;
        if written.rows_affected() != 1 {
            return Err(BusinessError::invalid(
                "STALE_PACKAGE_GRANT",
                "The current package grant changed.",
            )
            .into());
        }
        let before = current
            .map(serde_json::to_value)
            .transpose()
            .map_err(|_| PersistenceError::new("COMMERCIAL_AUDIT_ENCODING"))?;
        let after = serde_json::to_value(g)
            .map_err(|_| PersistenceError::new("COMMERCIAL_AUDIT_ENCODING"))?;
        sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(&input.idempotency_key).bind(&g.company_id).bind(&g.package_id).bind(actor.as_str()).bind(hash).bind(before).bind(after).bind(instant).execute(&mut *self.connection).await?;
        Ok(())
    }
}

/// The reviewed source must be leased before the first Holding snapshot. The
/// final callback validates the exact prospective set; it cannot be omitted.
pub trait PgPackageBatchReview: Send {
    fn prepare_sources(
        &mut self,
        connection: &mut PgConnection,
        input: &GrantChange,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn verify(
        &mut self,
        connection: &mut PgConnection,
        preview: &crate::application::grant_batches::GrantBatchPreview,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    /// After grant/audit/batch writes, finish required deferred owner effects
    /// and recheck current review freshness. Receipt replays skip this hook.
    fn retain(
        &mut self,
        connection: &mut PgConnection,
        preview: &crate::application::grant_batches::GrantBatchPreview,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
impl<Policy: PgPackageGrantPolicy, ClockSource: Clock + Sync>
    PgPackageGrantCommands<'_, Policy, ClockSource>
{
    pub async fn preview_batch<Review: PgPackageBatchReview>(
        &mut self,
        input: &crate::application::grant_batches::GrantBatch,
        review: &mut Review,
    ) -> Result<crate::application::grant_batches::GrantBatchPreview, Error> {
        self.unit.writing = false;
        let catalogue = self.unit.catalogue;
        crate::application::grant_batches::preview_package_batch(
            &mut PgBatchUnit {
                inner: &mut self.unit,
                review,
            },
            catalogue,
            input,
        )
        .await
    }
    pub async fn apply_batch<Review: PgPackageBatchReview>(
        &mut self,
        input: &crate::application::grant_batches::GrantBatch,
        review: &mut Review,
    ) -> Result<crate::application::grant_batches::GrantBatchPreview, Error> {
        self.unit.writing = true;
        let catalogue = self.unit.catalogue;
        crate::application::grant_batches::apply_package_batch(
            &mut PgBatchUnit {
                inner: &mut self.unit,
                review,
            },
            catalogue,
            input,
        )
        .await
    }
}
struct PgBatchUnit<'a, 'c, P, C, R> {
    inner: &'a mut PgGrantUnit<'c, P, C>,
    review: &'a mut R,
}
impl<P: PgPackageGrantPolicy, C: Clock + Sync, R: PgPackageBatchReview> PackageGrantUnit
    for PgBatchUnit<'_, '_, P, C, R>
{
    type Error = Error;
    async fn prepare_sources(&mut self, i: &GrantChange) -> Result<(), Error> {
        self.inner.prepare_sources(i).await?;
        self.review.prepare_sources(self.inner.connection, i).await
    }
    async fn lock(&mut self, k: &str) -> Result<(), Error> {
        self.inner.lock(k).await
    }
    async fn authorize(&mut self, c: &CompanyId) -> Result<UserId, Error> {
        self.inner.authorize(c).await
    }
    async fn current(
        &mut self,
        c: &CompanyId,
        p: &str,
    ) -> Result<Option<CompanyPackageGrant>, Error> {
        self.inner.current(c, p).await
    }
    async fn receipt(&mut self, k: &str) -> Result<Option<GrantReceipt>, Error> {
        self.inner.receipt(k).await
    }
    async fn obligations(&mut self, c: &CompanyId, p: &str) -> Result<i64, Error> {
        self.inner.obligations(c, p).await
    }
    async fn dependencies(&mut self, g: &CompanyPackageGrant) -> Result<(), Error> {
        self.inner.dependencies(g).await
    }
    async fn save(
        &mut self,
        i: &GrantChange,
        c: Option<&CompanyPackageGrant>,
        a: &UserId,
        h: &str,
    ) -> Result<(), Error> {
        self.inner.save(i, c, a, h).await
    }
}
impl<P: PgPackageGrantPolicy, C: Clock + Sync, R: PgPackageBatchReview>
    crate::application::grant_batches::PackageBatchUnit for PgBatchUnit<'_, '_, P, C, R>
{
    async fn all(
        &mut self,
        company: &CompanyId,
        writing: bool,
    ) -> Result<Vec<CompanyPackageGrant>, Error> {
        self.inner.scope(company)?;
        if writing {
            sqlx::query("INSERT INTO company_package_revisions(company_id,version) VALUES($1,0) ON CONFLICT DO NOTHING").bind(company.as_str()).execute(&mut *self.inner.connection).await?;
            sqlx::query(
                "SELECT version FROM company_package_revisions WHERE company_id=$1 FOR UPDATE",
            )
            .bind(company.as_str())
            .fetch_one(&mut *self.inner.connection)
            .await?;
        }
        Ok(PgCompanyPurchaseReader::new(
            self.inner.holding.clone(),
            company.clone(),
            self.inner.connection,
            self.inner.catalogue,
        )
        .read(company)
        .await?
        .grants)
    }
    async fn batch_receipt(
        &mut self,
        key: &str,
    ) -> Result<Option<crate::application::grant_batches::BatchReceipt>, Error> {
        let single: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM company_package_grant_events WHERE id=$1)",
        )
        .bind(key)
        .fetch_one(&mut *self.inner.connection)
        .await?;
        if single {
            return Err(BusinessError::invalid(
                "IDEMPOTENCY_CONFLICT",
                "The key belongs to another Commercial command.",
            )
            .into());
        }
        let row = sqlx::query(
            "SELECT request_hash,result_json FROM company_package_change_sets WHERE id=$1",
        )
        .bind(key)
        .fetch_optional(&mut *self.inner.connection)
        .await?;
        row.map(|row| {
            Ok(crate::application::grant_batches::BatchReceipt {
                hash: row.try_get("request_hash")?,
                result: serde_json::from_value(row.try_get("result_json")?)
                    .map_err(|_| PersistenceError::new("COMMERCIAL_RECEIPT_INVALID"))?,
            })
        })
        .transpose()
    }
    async fn review(
        &mut self,
        preview: &crate::application::grant_batches::GrantBatchPreview,
    ) -> Result<(), Error> {
        self.review.verify(self.inner.connection, preview).await
    }
    async fn retain_review(
        &mut self,
        preview: &crate::application::grant_batches::GrantBatchPreview,
    ) -> Result<(), Error> {
        self.review.retain(self.inner.connection, preview).await
    }
    async fn prospective_dependencies(
        &mut self,
        grant: &CompanyPackageGrant,
        prospective: &[CompanyPackageGrant],
    ) -> Result<(), Error> {
        self.inner.catalogue.validate_dependencies(
            grant,
            prospective,
            self.inner.clock.now().get(),
        )?;
        Ok(())
    }
    async fn save_batch_receipt(
        &mut self,
        key: &str,
        hash: &str,
        actor: &UserId,
        result: &crate::application::grant_batches::GrantBatchPreview,
    ) -> Result<(), Error> {
        let value = serde_json::to_value(result)
            .map_err(|_| PersistenceError::new("COMMERCIAL_AUDIT_ENCODING"))?;
        sqlx::query("INSERT INTO company_package_change_sets(id,company_id,actor_id,request_hash,result_json,created_at) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(key).bind(&result.company_id).bind(actor.as_str()).bind(hash).bind(value).bind(self.inner.clock.now().get()).execute(&mut *self.inner.connection).await?;
        Ok(())
    }
}
