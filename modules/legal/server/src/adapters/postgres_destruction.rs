//! Legal-owned phases of retained destruction. The caller must commit the
//! journal in its own transaction before calling execution in a new transaction.
//! Execution performs irreversible storage I/O: use a non-retrying transaction
//! (`PgUnitOfWork::run_once`) and retain the journal for crash recovery.
use std::future::Future;

use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};

use crate::application::destruction_sources::{
    CurrentDestructionSource, DestructionDocument, DestructionLedgerEntry, DestructionUpload,
    LegalDestructionManifest, LegalDestructionManifestFacts, LegalDestructionSourceFacts,
    LockedDestructionSource, legal_destruction_manifest, validate_legal_destruction_sources,
};

type Error = PgTransactionError<BusinessError>;

/// The composition owner supplies real, transaction-bound authority and Files
/// operations. Stored actor/manifest JSON alone is never an authority witness.
/// `prepare` acquires the Control and participating owner locks before reads;
/// `assert_current` revalidates the queued actor, exact source hash/version,
/// Legal visibility, independent reviewer and current purchase/history rights.
/// `destroy` must verify the same authorization immediately before deleting and
/// must be safe to replay after partial deletion of the same approved manifest.
pub trait PgLegalDestructionBridge: Send {
    fn prepare(
        &mut self,
        c: &mut PgConnection,
        company: &str,
        destruction: &str,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn assert_current(
        &mut self,
        c: &mut PgConnection,
        company: &str,
        destruction: &str,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn locked_source(
        &mut self,
        c: &mut PgConnection,
        company: &str,
        item: &str,
        source: &str,
    ) -> impl Future<Output = Result<Option<LockedDestructionSource>, Error>> + Send;
    fn link_current(
        &mut self,
        c: &mut PgConnection,
        company: &str,
        item: &str,
        source: &str,
        link: &str,
    ) -> impl Future<Output = Result<bool, Error>> + Send;
    fn source_shared(
        &mut self,
        c: &mut PgConnection,
        company: &str,
        item: &str,
        source: &str,
    ) -> impl Future<Output = Result<bool, Error>> + Send;
    fn retained_ledger(
        &mut self,
        c: &mut PgConnection,
        source: &str,
    ) -> impl Future<Output = Result<Vec<DestructionLedgerEntry>, Error>> + Send;
    fn object_shared(
        &mut self,
        c: &mut PgConnection,
        source: &str,
        key: &str,
    ) -> impl Future<Output = Result<bool, Error>> + Send;
    fn destroy(
        &mut self,
        c: &mut PgConnection,
        company: &str,
        destruction: &str,
        manifest: &LegalDestructionManifest,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn seal(
        &mut self,
        c: &mut PgConnection,
        company: &str,
        destruction: &str,
        manifest: &LegalDestructionManifest,
        now: i64,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}

/// Additional Files/authority operations needed while creating a request.
/// `prepare_manifest` must fence and validate the current actor, item visibility,
/// purchase history rights and all participating sources before the first owner
/// snapshot; it never grants byte deletion authority.
pub trait PgLegalDestructionManifestBridge: PgLegalDestructionBridge {
    fn prepare_manifest(
        &mut self,
        c: &mut PgConnection,
        company: &str,
        item: &str,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn canonical_source(
        &mut self,
        c: &mut PgConnection,
        company: &str,
        item: &str,
        source: &str,
    ) -> impl Future<Output = Result<Option<CurrentDestructionSource>, Error>> + Send;
    fn stored_ledger(
        &mut self,
        c: &mut PgConnection,
        source: &str,
    ) -> impl Future<Output = Result<Vec<DestructionLedgerEntry>, Error>> + Send;
}

async fn require_unit(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("LEGAL_OWNER_TRANSACTION_REQUIRED").into());
    }
    let level: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if level != "serializable" {
        return Err(PersistenceError::new("LEGAL_SERIALIZABLE_TRANSACTION_REQUIRED").into());
    }
    Ok(())
}
async fn lock_files(c: &mut PgConnection) -> Result<(), Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))")
        .execute(&mut *c)
        .await?;
    Ok(())
}
async fn lock_company(c: &mut PgConnection, company: &str) -> Result<(), Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("legal:{company}"))
        .execute(&mut *c)
        .await?;
    Ok(())
}

/// Select one company-scoped, retryable request. This is not an authority
/// witness and does not lease the row; the journal/execution phases lock it.
pub async fn legal_destruction_candidate(
    c: &mut PgConnection,
    company: &str,
) -> Result<Option<String>, Error> {
    require_unit(c).await?;
    Ok(sqlx::query_scalar("SELECT x.id FROM legal_destructions x JOIN legal_items i ON i.id=x.item_id WHERE i.company_id=$1 AND x.state IN ('approved','executing') AND x.attempts<6 ORDER BY x.created_at,x.id LIMIT 1")
        .bind(company).fetch_optional(&mut *c).await?)
}

/// After either phase has rolled back, persist the review state in a separate
/// transaction. An already-started journal remains executing and frozen.
pub async fn legal_destruction_require_review(
    c: &mut PgConnection,
    company: &str,
    destruction: &str,
) -> Result<(), Error> {
    require_unit(c).await?;
    sqlx::query("UPDATE legal_destructions x SET state=CASE WHEN x.started_at IS NULL THEN 'failed' ELSE 'executing' END,error='DESTRUCTION_REQUIRES_REVIEW',attempts=x.attempts+1 FROM legal_items i WHERE i.id=x.item_id AND i.company_id=$1 AND x.id=$2 AND x.state<>'completed'")
        .bind(company).bind(destruction).execute(&mut *c).await?;
    Ok(())
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum LegalDestructionJournal {
    NoChange,
    Blocked,
    Started,
}

/// Begin and commit this phase before physical deletion. The generated event ID
/// is owned by the caller and must be unique.
pub async fn journal_legal_destruction<B: PgLegalDestructionBridge>(
    c: &mut PgConnection,
    bridge: &mut B,
    company: &str,
    destruction: &str,
    event_id: &str,
    now: i64,
) -> Result<LegalDestructionJournal, Error> {
    require_unit(c).await?;
    bridge.prepare(c, company, destruction).await?;
    bridge.assert_current(c, company, destruction).await?;
    lock_files(c).await?;
    let row = sqlx::query("SELECT x.item_id,x.policy_id,x.policy_version::bigint FROM legal_destructions x JOIN legal_items i ON i.id=x.item_id WHERE x.id=$1 AND i.company_id=$2 AND x.state='approved' FOR UPDATE OF x,i")
        .bind(destruction).bind(company).fetch_optional(&mut *c).await?;
    let Some(row) = row else {
        return Ok(LegalDestructionJournal::NoChange);
    };
    lock_company(c, company).await?;
    let item: String = row.try_get("item_id")?;
    let policy: String = row.try_get("policy_id")?;
    let version: i64 = row.try_get("policy_version")?;
    let eligible: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM legal_retention_assignments a JOIN legal_retention_policies p ON p.id=a.policy_id JOIN legal_items i ON i.id=a.item_id WHERE a.item_id=$1 AND a.policy_id=$2 AND a.policy_version=$3 AND p.version=a.policy_version AND p.state='approved' AND a.eligible_at<=$4 AND i.state IN ('archived','closed','completed','expired','terminated') AND NOT EXISTS(SELECT 1 FROM legal_hold_targets t JOIN legal_holds h ON h.id=t.hold_id WHERE t.item_id=i.id AND h.state<>'released'))")
        .bind(&item).bind(&policy).bind(version).bind(now).fetch_one(&mut *c).await?;
    if !eligible {
        sqlx::query("UPDATE legal_destructions SET state='blocked',error='HOLD_OR_RETENTION_POLICY_CHANGED' WHERE id=$1").bind(destruction).execute(&mut *c).await?;
        bridge.assert_current(c, company, destruction).await?;
        return Ok(LegalDestructionJournal::Blocked);
    }
    sqlx::query("UPDATE legal_destructions SET state='executing',started_at=$2 WHERE id=$1")
        .bind(destruction)
        .bind(now)
        .execute(&mut *c)
        .await?;
    sqlx::query("INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) SELECT $1,id,'retention.started','system:legal',$2,version,$3 FROM legal_items WHERE id=$4")
        .bind(event_id).bind(format!("Execution journal {destruction}")).bind(now).bind(&item).execute(&mut *c).await?;
    bridge.assert_current(c, company, destruction).await?;
    Ok(LegalDestructionJournal::Started)
}

struct SourceFacts<'a, B> {
    c: &'a mut PgConnection,
    bridge: &'a mut B,
}
impl<B: PgLegalDestructionBridge> LegalDestructionSourceFacts for SourceFacts<'_, B> {
    type Error = Error;
    async fn document_current(
        &mut self,
        item: &str,
        d: &DestructionDocument,
    ) -> Result<bool, Error> {
        Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM legal_documents WHERE id=$1 AND item_id=$2 AND checksum=$3 AND object_key IS NOT DISTINCT FROM $4)").bind(&d.id).bind(item).bind(&d.checksum).bind(&d.object_key).fetch_one(&mut *self.c).await?)
    }
    async fn upload_current(&mut self, item: &str, u: &DestructionUpload) -> Result<bool, Error> {
        Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM legal_external_uploads u JOIN legal_shares s ON s.id=u.share_id WHERE u.id=$1 AND s.item_id=$2 AND u.checksum=$3 AND u.object_key=$4)").bind(&u.id).bind(item).bind(&u.checksum).bind(&u.object_key).fetch_one(&mut *self.c).await?)
    }
    async fn locked_source(
        &mut self,
        company: &str,
        item: &str,
        source: &str,
    ) -> Result<Option<LockedDestructionSource>, Error> {
        self.bridge
            .locked_source(self.c, company, item, source)
            .await
    }
    async fn link_current(
        &mut self,
        company: &str,
        item: &str,
        source: &str,
        link: &str,
    ) -> Result<bool, Error> {
        self.bridge
            .link_current(self.c, company, item, source, link)
            .await
    }
    async fn source_shared(
        &mut self,
        company: &str,
        item: &str,
        source: &str,
    ) -> Result<bool, Error> {
        self.bridge
            .source_shared(self.c, company, item, source)
            .await
    }
    async fn retained_ledger(
        &mut self,
        source: &str,
    ) -> Result<Vec<DestructionLedgerEntry>, Error> {
        self.bridge.retained_ledger(self.c, source).await
    }
    async fn object_shared(&mut self, source: &str, key: &str) -> Result<bool, Error> {
        self.bridge.object_shared(self.c, source, key).await
    }
}

impl<B: PgLegalDestructionManifestBridge> LegalDestructionManifestFacts for SourceFacts<'_, B> {
    async fn documents(&mut self, item: &str) -> Result<Vec<DestructionDocument>, Error> {
        let rows = sqlx::query("SELECT id,checksum,object_key,attachment_source_id,attachment_link_id FROM legal_documents WHERE item_id=$1 AND NOT EXISTS(SELECT 1 FROM legal_disposals disposed WHERE disposed.document_id=legal_documents.id) ORDER BY id")
            .bind(item).fetch_all(&mut *self.c).await?;
        rows.into_iter()
            .map(|row| {
                Ok(DestructionDocument {
                    id: row.try_get("id")?,
                    checksum: row.try_get("checksum")?,
                    object_key: row.try_get("object_key")?,
                    attachment_source_id: row.try_get("attachment_source_id")?,
                    attachment_link_id: row.try_get("attachment_link_id")?,
                })
            })
            .collect()
    }
    async fn uploads(&mut self, item: &str) -> Result<Vec<DestructionUpload>, Error> {
        let rows = sqlx::query("SELECT upload.id,upload.object_key,upload.checksum,upload.attachment_source_id,upload.attachment_link_id FROM legal_external_uploads upload JOIN legal_shares share ON share.id=upload.share_id WHERE share.item_id=$1 ORDER BY upload.id")
            .bind(item).fetch_all(&mut *self.c).await?;
        rows.into_iter()
            .map(|row| {
                Ok(DestructionUpload {
                    id: row.try_get("id")?,
                    checksum: row.try_get("checksum")?,
                    object_key: row.try_get("object_key")?,
                    attachment_source_id: row.try_get("attachment_source_id")?,
                    attachment_link_id: row.try_get("attachment_link_id")?,
                })
            })
            .collect()
    }
    async fn canonical_source(
        &mut self,
        company: &str,
        item: &str,
        source: &str,
    ) -> Result<Option<CurrentDestructionSource>, Error> {
        self.bridge
            .canonical_source(self.c, company, item, source)
            .await
    }
    async fn stored_ledger(&mut self, source: &str) -> Result<Vec<DestructionLedgerEntry>, Error> {
        self.bridge.stored_ledger(self.c, source).await
    }
}

/// Build the complete request-time manifest over the caller's serializable
/// owner transaction. Composition must establish all Control/Files fences before
/// the transaction's first snapshot and keep this unit through request commit.
pub async fn snapshot_legal_destruction_manifest<B: PgLegalDestructionManifestBridge>(
    c: &mut PgConnection,
    bridge: &mut B,
    company: &str,
    item: &str,
) -> Result<LegalDestructionManifest, Error> {
    require_unit(c).await?;
    bridge.prepare_manifest(c, company, item).await?;
    lock_files(c).await?;
    let exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM legal_items WHERE id=$1 AND company_id=$2 FOR UPDATE")
            .bind(item)
            .bind(company)
            .fetch_optional(&mut *c)
            .await?;
    if exists.is_none() {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "LEGAL_SOURCE_UNAVAILABLE",
            "The Legal source is unavailable.",
        )
        .into());
    }
    lock_company(c, company).await?;
    legal_destruction_manifest(&mut SourceFacts { c, bridge }, item, company).await
}

/// Opaque snapshot bound to the exact backend and transaction that fenced all
/// sources. It is consumed by the Legal retention request adapter; it is not a
/// stored actor or a reusable authorization grant.
pub struct PreparedLegalDestructionManifest {
    database: String,
    backend: i32,
    transaction: String,
    company: String,
    item: String,
    manifest: LegalDestructionManifest,
}

impl PreparedLegalDestructionManifest {
    pub(crate) async fn current(
        &self,
        c: &mut PgConnection,
        company: &str,
        item: &str,
    ) -> Result<LegalDestructionManifest, Error> {
        require_unit(c).await?;
        let row = sqlx::query("SELECT current_database() database,pg_backend_pid() backend,pg_current_xact_id()::text transaction_id")
            .fetch_one(&mut *c).await?;
        if self.database != row.try_get::<String, _>("database")?
            || self.backend != row.try_get::<i32, _>("backend")?
            || self.transaction != row.try_get::<String, _>("transaction_id")?
            || self.company != company
            || self.item != item
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "LEGAL_DESTRUCTION_MANIFEST_CHANGED",
                "The retained destruction source must be prepared in this transaction.",
            )
            .into());
        }
        Ok(self.manifest.clone())
    }
}

/// Mint once after real Files and authority fences. The wrapper verifies the
/// actual transaction again immediately before request persistence.
pub async fn prepare_legal_destruction_manifest<B: PgLegalDestructionManifestBridge>(
    c: &mut PgConnection,
    bridge: &mut B,
    company: &str,
    item: &str,
) -> Result<PreparedLegalDestructionManifest, Error> {
    let manifest = snapshot_legal_destruction_manifest(c, bridge, company, item).await?;
    let row = sqlx::query("SELECT current_database() database,pg_backend_pid() backend,pg_current_xact_id()::text transaction_id")
        .fetch_one(&mut *c).await?;
    Ok(PreparedLegalDestructionManifest {
        database: row.try_get("database")?,
        backend: row.try_get("backend")?,
        transaction: row.try_get("transaction_id")?,
        company: company.to_owned(),
        item: item.to_owned(),
        manifest,
    })
}

/// Execute against the already committed journal. An error after `destroy` does
/// not restore bytes: rollback leaves the durable executing journal for review.
pub async fn execute_legal_destruction<B: PgLegalDestructionBridge>(
    c: &mut PgConnection,
    bridge: &mut B,
    company: &str,
    destruction: &str,
    event_id: &str,
    now: i64,
) -> Result<bool, Error> {
    require_unit(c).await?;
    bridge.prepare(c, company, destruction).await?;
    bridge.assert_current(c, company, destruction).await?;
    lock_files(c).await?;
    let row = sqlx::query("SELECT x.item_id,x.manifest::text AS manifest,i.version::bigint item_version,(x.xmin::text::bigint = (pg_current_xact_id()::text::bigint % 4294967296)) same_transaction FROM legal_destructions x JOIN legal_items i ON i.id=x.item_id WHERE x.id=$1 AND i.company_id=$2 AND x.state='executing' AND x.started_at IS NOT NULL FOR UPDATE OF x,i")
        .bind(destruction).bind(company).fetch_optional(&mut *c).await?;
    let Some(row) = row else { return Ok(false) };
    lock_company(c, company).await?;
    if row.try_get::<bool, _>("same_transaction")? {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "LEGAL_DESTRUCTION_JOURNAL_NOT_COMMITTED",
            "The destruction journal must commit before deleting bytes.",
        )
        .into());
    }
    let item: String = row.try_get("item_id")?;
    let held: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM legal_hold_targets t JOIN legal_holds h ON h.id=t.hold_id WHERE t.item_id=$1 AND h.state<>'released')").bind(&item).fetch_one(&mut *c).await?;
    if held {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "LEGAL_HOLD",
            "A current Legal hold prevents deletion.",
        )
        .into());
    }
    let manifest_json: String = row.try_get("manifest")?;
    let manifest: LegalDestructionManifest =
        serde_json::from_str(&manifest_json).map_err(|_| {
            BusinessError::new(
                FailureKind::Conflict,
                "LEGAL_DESTRUCTION_MANIFEST",
                "The retained destruction manifest is invalid.",
            )
        })?;
    validate_legal_destruction_sources(
        &mut SourceFacts {
            c: &mut *c,
            bridge: &mut *bridge,
        },
        &item,
        company,
        &manifest,
    )
    .await?;
    sqlx::query("UPDATE legal_destructions SET attempts=attempts+1 WHERE id=$1")
        .bind(destruction)
        .execute(&mut *c)
        .await?;
    bridge.assert_current(c, company, destruction).await?;
    bridge.destroy(c, company, destruction, &manifest).await?;
    bridge.seal(c, company, destruction, &manifest, now).await?;
    for d in &manifest.documents {
        sqlx::query("INSERT INTO legal_disposals VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING")
            .bind(&d.id)
            .bind(destruction)
            .bind(&d.checksum)
            .bind(now)
            .execute(&mut *c)
            .await?;
        sqlx::query("UPDATE legal_documents SET body='',object_key=NULL WHERE id=$1")
            .bind(&d.id)
            .execute(&mut *c)
            .await?;
    }
    sqlx::query("UPDATE legal_processing_jobs SET state='failed',result_json=NULL,error='CONTENT_DESTROYED',lease_token=NULL,lease_until=NULL WHERE item_id=$1").bind(&item).execute(&mut *c).await?;
    sqlx::query("DELETE FROM legal_document_inspections WHERE document_id IN (SELECT id FROM legal_documents WHERE item_id=$1)").bind(&item).execute(&mut *c).await?;
    sqlx::query("UPDATE legal_ai_proposals SET proposal=NULL,question='[disposed under retention policy]',state='disposed' WHERE item_id=$1").bind(&item).execute(&mut *c).await?;
    sqlx::query("UPDATE legal_shares SET revoked_at=$2 WHERE item_id=$1 AND revoked_at IS NULL")
        .bind(&item)
        .bind(now)
        .execute(&mut *c)
        .await?;
    sqlx::query("UPDATE legal_destructions SET state='completed',completed_at=$2,error=NULL,version=version+1 WHERE id=$1").bind(destruction).bind(now).execute(&mut *c).await?;
    let item_version: i64 = row.try_get("item_version")?;
    sqlx::query("INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) VALUES($1,$2,'retention.destroyed','system:legal',$3,$4,$5)").bind(event_id).bind(&item).bind(format!("Destruction certificate {destruction}; checksums retained")).bind(item_version).bind(now).execute(&mut *c).await?;
    bridge.assert_current(c, company, destruction).await?;
    Ok(true)
}
