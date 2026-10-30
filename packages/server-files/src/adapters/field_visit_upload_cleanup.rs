//! Files-owned cleanup of expired unreferenced Field upload objects. No Person
//! endpoint or caller-provided key can authorize deletion.
use super::local_field_quarantine_writer::{FieldCleanupObject, LocalFieldQuarantineWriter};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_runtime::public::postgres::PgWorkFuture;
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind};
use sqlx::{PgConnection, Row};
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;
const LEASE_MS: i64 = 30_000;
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "ATTACHMENT_CLEANUP_CHANGED",
        "File cleanup authority changed; retry later.",
    )
    .into()
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldCleanupOutcome {
    NotEligible,
    Busy,
    Cleaned,
}
async fn advisory(c: &mut sqlx::PgConnection, company: &str, key: &str) -> Result<(), Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("attachment-object:{company}:{key}"))
        .execute(c)
        .await?;
    Ok(())
}
/// Composition supplies current registered Holding authority for every phase.
/// The physical callback is non-retrying and holds reference-table SHARE locks
/// in a READ COMMITTED transaction through the filesystem operation.
pub trait FieldCleanupTransactions: Send + Sync {
    fn metadata<T, F>(&self, work: F) -> impl std::future::Future<Output = Result<T, Error>> + Send
    where
        T: Send + 'static,
        F: for<'a> Fn(&'a mut PgConnection, &'a CompanyId) -> PgWorkFuture<'a, T, BusinessError>
            + Clone
            + Send
            + Sync
            + 'static;
    fn physical<T, F>(&self, work: F) -> impl std::future::Future<Output = Result<T, Error>> + Send
    where
        T: Send + 'static,
        F: for<'a> FnOnce(
                &'a mut PgConnection,
                &'a CompanyId,
            ) -> PgWorkFuture<'a, T, BusinessError>
            + Send
            + 'static;
}

/// Source history and retained/bound files are outside this operational cleanup.
/// The OS object lock remains held throughout all verified phases.
pub async fn reconcile_field_upload_object<T, C>(
    writer: &LocalFieldQuarantineWriter,
    transactions: &T,
    upload_id: &str,
    clock: Arc<C>,
) -> Result<FieldCleanupOutcome, Error>
where
    T: FieldCleanupTransactions,
    C: Clock + Send + Sync + 'static,
{
    let id = upload_id.to_owned();
    let lookup_id = id.clone();
    let lookup_clock = clock.clone();
    let key: Option<String> = transactions.metadata(move |c, company| {
        let id = lookup_id.clone(); let clock = lookup_clock.clone();
        Box::pin(async move {
            Ok(sqlx::query_scalar(r#"SELECT COALESCE(u.quarantine_key,u.staging_key) FROM upload_intents u JOIN attachment_sets s ON s.id=u.attachment_set_id AND s.company_id=u.company_id JOIN attachment_links l ON l.id=u.attachment_link_id AND l.set_id=s.id AND l.upload_intent_id=u.id WHERE u.id=$1 AND u.company_id=$2 AND u.owner_type='field_service_visit' AND s.target_type=u.owner_type AND s.target_id=u.owner_id AND s.purpose_key=u.purpose AND s.component_kind='record' AND s.purpose_key IN ('site_photo','technician_report','signed_job_sheet','exception_evidence') AND (u.quarantine_key IS NULL OR u.quarantine_key=u.staging_key) AND u.status IN ('pending','failed','expired') AND u.expires_at<=$3"#).bind(id).bind(company.as_str()).bind(clock.now().get()).fetch_optional(c).await?)
        })
    }).await?;
    let Some(key) = key else {
        return Ok(FieldCleanupOutcome::NotEligible);
    };
    let object_lock = writer.lock_object(&key).await?;
    let token = uuid::Uuid::new_v4();
    let claim_id = id.clone();
    let claim_key = key.clone();
    let claim_clock = clock.clone();
    let claimed = transactions.metadata(move |c, company| {
        let id=claim_id.clone(); let key=claim_key.clone(); let clock=claim_clock.clone();
        Box::pin(async move {
            advisory(c, company.as_str(), &key).await?;
            let now=clock.now().get(); let until=now.checked_add(LEASE_MS).ok_or_else(changed)?;
            let eligible: bool=sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM upload_intents u JOIN attachment_sets s ON s.id=u.attachment_set_id AND s.company_id=u.company_id JOIN attachment_links l ON l.id=u.attachment_link_id AND l.set_id=s.id AND l.upload_intent_id=u.id WHERE u.id=$1 AND u.company_id=$2 AND u.owner_type='field_service_visit' AND s.target_type=u.owner_type AND s.target_id=u.owner_id AND s.purpose_key=u.purpose AND s.component_kind='record' AND s.purpose_key IN ('site_photo','technician_report','signed_job_sheet','exception_evidence') AND (u.quarantine_key IS NULL OR u.quarantine_key=u.staging_key) AND COALESCE(u.quarantine_key,u.staging_key)=$3 AND u.status IN ('pending','failed','expired') AND u.expires_at<=$4) AND NOT EXISTS(SELECT 1 FROM attachments WHERE (object_key=$3 OR preview_object_key=$3)) AND NOT EXISTS(SELECT 1 FROM attachment_storage_ledger WHERE object_key=$3) AND NOT EXISTS(SELECT 1 FROM upload_intents WHERE id<>$1 AND $3 IN (staging_key,quarantine_key,final_key)) AND NOT EXISTS(SELECT 1 FROM field_upload_publication_pins WHERE object_key=$3 AND state='active' AND expires_at>$4)"#).bind(&id).bind(company.as_str()).bind(&key).bind(now).fetch_one(&mut *c).await?;
            if !eligible { return Ok(Err(FieldCleanupOutcome::NotEligible)); }
            let row=sqlx::query(r#"INSERT INTO field_upload_object_cleanup_claims(company_id,object_key,upload_intent_id,token,claimed_at,claim_until) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(company_id,object_key) DO UPDATE SET token=EXCLUDED.token,claimed_at=EXCLUDED.claimed_at,claim_until=EXCLUDED.claim_until WHERE field_upload_object_cleanup_claims.claim_until<=$5 AND field_upload_object_cleanup_claims.upload_intent_id=$3 RETURNING object_version,checksum_sha256,bytes"#).bind(company.as_str()).bind(&key).bind(&id).bind(token).bind(now).bind(until).fetch_optional(&mut *c).await?;
            let Some(row)=row else { return Ok(Err(FieldCleanupOutcome::Busy)) };
            let version:Option<String>=row.try_get("object_version")?;
            let checksum:Option<String>=row.try_get("checksum_sha256")?;
            let size:Option<i64>=row.try_get("bytes")?;
            let staged=match (version,checksum,size) {
                (None,None,None)=>None,
                (Some(version),Some(checksum),Some(bytes)) if (1..=10*1024*1024).contains(&bytes) && uuid::Uuid::parse_str(&version).is_ok_and(|v| v.to_string()==version) && crate::domain::attachment_scan::valid_sha(&checksum) => Some(FieldCleanupObject{version,checksum,bytes:bytes as usize}),
                _=>return Err(changed()),
            };
            Ok(Ok(staged))
        })
    }).await?;
    let staged = match claimed {
        Ok(value) => value,
        Err(outcome) => return Ok(outcome),
    };
    let (object_lock, current) = writer
        .inspect_locked(key.clone(), object_lock, staged.clone())
        .await?;
    if staged.is_some() && current.is_some() && staged != current {
        return Err(changed());
    }
    let expected = staged.or(current);
    let stage_id = id.clone();
    let stage_key = key.clone();
    let stage_object = expected.clone();
    let stage_clock = clock.clone();
    transactions.metadata(move |c, company| {
        let id=stage_id.clone(); let key=stage_key.clone(); let object=stage_object.clone(); let clock=stage_clock.clone();
        Box::pin(async move {
            advisory(c,company.as_str(),&key).await?;
            let now=clock.now().get();
            let valid:bool=sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM field_upload_object_cleanup_claims WHERE company_id=$1 AND object_key=$2 AND upload_intent_id=$3 AND token=$4 AND claim_until>$5) AND NOT EXISTS(SELECT 1 FROM attachments WHERE (object_key=$2 OR preview_object_key=$2)) AND NOT EXISTS(SELECT 1 FROM attachment_storage_ledger WHERE object_key=$2) AND NOT EXISTS(SELECT 1 FROM upload_intents WHERE id<>$3 AND $2 IN (staging_key,quarantine_key,final_key)) AND NOT EXISTS(SELECT 1 FROM field_upload_publication_pins WHERE object_key=$2 AND state='active' AND expires_at>$5)"#).bind(company.as_str()).bind(&key).bind(&id).bind(token).bind(now).fetch_one(&mut *c).await?;
            if !valid { return Err(changed()) }
            if let Some(object)=object {
                let rows=sqlx::query(r#"UPDATE field_upload_object_cleanup_claims SET object_version=$5,checksum_sha256=$6,bytes=$7 WHERE company_id=$1 AND object_key=$2 AND upload_intent_id=$3 AND token=$4 AND (object_version IS NULL OR (object_version=$5 AND checksum_sha256=$6 AND bytes=$7))"#).bind(company.as_str()).bind(&key).bind(&id).bind(token).bind(&object.version).bind(&object.checksum).bind(object.bytes as i64).execute(&mut *c).await?.rows_affected();
                if rows!=1 { return Err(changed()) }
            }
            Ok(())
        })
    }).await?;
    let physical_id = id.clone();
    let physical_key = key.clone();
    let physical_clock = clock.clone();
    let physical_writer = writer.clone();
    let object_lock=transactions.physical(move |c, company| Box::pin(async move {
        advisory(c,company.as_str(),&physical_key).await?;
        let now=physical_clock.now().get();
        let valid:bool=sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM field_upload_object_cleanup_claims WHERE company_id=$1 AND object_key=$2 AND upload_intent_id=$3 AND token=$4 AND claim_until>$5) AND NOT EXISTS(SELECT 1 FROM attachments WHERE (object_key=$2 OR preview_object_key=$2)) AND NOT EXISTS(SELECT 1 FROM attachment_storage_ledger WHERE object_key=$2) AND NOT EXISTS(SELECT 1 FROM upload_intents WHERE id<>$3 AND $2 IN (staging_key,quarantine_key,final_key)) AND NOT EXISTS(SELECT 1 FROM field_upload_publication_pins WHERE object_key=$2 AND state='active' AND expires_at>$5)"#).bind(company.as_str()).bind(&physical_key).bind(&physical_id).bind(token).bind(now).fetch_one(&mut *c).await?;
        if !valid { return Err(changed()) }
        // The persisted claim fences new pins. The OS flock excludes an
        // in-flight publisher, and table locks protect this fresh-time release.
        // Release only this verified Company; other Companies' pins still block.
        sqlx::query("UPDATE field_upload_publication_pins SET state='released',closed_at=$3 WHERE company_id=$1 AND object_key=$2 AND state='active' AND expires_at<=$3")
            .bind(company.as_str()).bind(&physical_key).bind(now).execute(&mut *c).await?;
        let verified_now=physical_clock.now().get();
        if verified_now < now { return Err(changed()) }
        let eligible:bool=sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM upload_intents u JOIN attachment_sets s ON s.id=u.attachment_set_id AND s.company_id=u.company_id JOIN attachment_links l ON l.id=u.attachment_link_id AND l.set_id=s.id AND l.upload_intent_id=u.id WHERE u.id=$1 AND u.company_id=$2 AND u.owner_type='field_service_visit' AND s.target_type=u.owner_type AND s.target_id=u.owner_id AND s.purpose_key=u.purpose AND s.component_kind='record' AND s.purpose_key IN ('site_photo','technician_report','signed_job_sheet','exception_evidence') AND (u.quarantine_key IS NULL OR u.quarantine_key=u.staging_key) AND COALESCE(u.quarantine_key,u.staging_key)=$3 AND u.status IN ('pending','failed','expired') AND u.expires_at<=$4) AND NOT EXISTS(SELECT 1 FROM attachments WHERE (object_key=$3 OR preview_object_key=$3)) AND NOT EXISTS(SELECT 1 FROM attachment_storage_ledger WHERE object_key=$3) AND NOT EXISTS(SELECT 1 FROM upload_intents WHERE id<>$1 AND $3 IN (staging_key,quarantine_key,final_key)) AND NOT EXISTS(SELECT 1 FROM field_upload_publication_pins WHERE object_key=$3 AND state='active')"#).bind(&physical_id).bind(company.as_str()).bind(&physical_key).bind(verified_now).fetch_one(&mut *c).await?;
        if !eligible { return Err(changed()) }
        let live_claim: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM field_upload_object_cleanup_claims WHERE company_id=$1 AND object_key=$2 AND upload_intent_id=$3 AND token=$4 AND claim_until>$5)")
            .bind(company.as_str()).bind(&physical_key).bind(&physical_id).bind(token).bind(verified_now).fetch_one(&mut *c).await?;
        if !live_claim { return Err(changed()) }
        Ok(physical_writer.delete_locked(physical_key,expected,object_lock).await?)
    })).await?;
    let clear_id = id.clone();
    let clear_key = key.clone();
    transactions.metadata(move |c, company| {
        let id=clear_id.clone(); let key=clear_key.clone();
        Box::pin(async move {
            advisory(c,company.as_str(),&key).await?;
            let rows=sqlx::query(r#"DELETE FROM field_upload_object_cleanup_claims WHERE company_id=$1 AND object_key=$2 AND upload_intent_id=$3 AND token=$4 AND NOT EXISTS(SELECT 1 FROM attachments WHERE (object_key=$2 OR preview_object_key=$2)) AND NOT EXISTS(SELECT 1 FROM attachment_storage_ledger WHERE object_key=$2) AND NOT EXISTS(SELECT 1 FROM upload_intents WHERE id<>$3 AND $2 IN (staging_key,quarantine_key,final_key)) AND NOT EXISTS(SELECT 1 FROM field_upload_publication_pins WHERE object_key=$2 AND state='active')"#).bind(company.as_str()).bind(&key).bind(&id).bind(token).execute(&mut *c).await?.rows_affected();
            if rows!=1 { return Err(changed()) }
            Ok(())
        })
    }).await?;
    drop(object_lock);
    Ok(FieldCleanupOutcome::Cleaned)
}

async fn require_serializable(c: &mut sqlx::PgConnection) -> Result<(), Error> {
    use sqlx::Connection;
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(
            erpx_server_runtime::public::postgres::PersistenceError::new(
                "FILES_SERIALIZABLE_REQUIRED",
            )
            .into(),
        );
    }
    Ok(())
}

/// Expire a bounded set of coherent Field drafts in one SERIALIZABLE unit.
/// Retains all rows, source objects, receipts and events. Ambiguous lineage is
/// skipped for investigation; no job may reinterpret it as another owner.
pub async fn expire_field_drafts(
    c: &mut sqlx::PgConnection,
    company: &CompanyId,
    clock: &(dyn Clock + Sync),
) -> Result<usize, Error> {
    let now = clock.now().get();
    let oldest = now.checked_sub(7 * 24 * 60 * 60_000).ok_or_else(changed)?;
    require_serializable(c).await?;
    let sets:Vec<String>=sqlx::query_scalar("SELECT s.id FROM attachment_sets s WHERE s.company_id=$1 AND s.target_type='field_service_visit' AND s.component_kind='record' AND s.purpose_key IN ('site_photo','technician_report','signed_job_sheet','exception_evidence') AND s.state='draft' AND (s.created_at<=$2 OR EXISTS(SELECT 1 FROM attachment_links l WHERE l.set_id=s.id AND l.state='draft' AND l.expires_at<=$3) OR EXISTS(SELECT 1 FROM upload_intents u WHERE u.attachment_set_id=s.id AND u.status='pending' AND u.expires_at<=$3)) AND NOT EXISTS(SELECT 1 FROM upload_intents u WHERE u.attachment_set_id=s.id AND (u.company_id<>s.company_id OR u.owner_type<>s.target_type OR u.owner_id<>s.target_id OR u.purpose<>s.purpose_key)) AND NOT EXISTS(SELECT 1 FROM attachment_links l LEFT JOIN upload_intents u ON u.id=l.upload_intent_id LEFT JOIN attachments a ON a.id=l.attachment_id WHERE l.set_id=s.id AND ((l.upload_intent_id IS NOT NULL AND (u.id IS NULL OR u.company_id<>s.company_id OR u.attachment_set_id IS DISTINCT FROM s.id OR u.attachment_link_id IS DISTINCT FROM l.id OR u.owner_type<>s.target_type OR u.owner_id<>s.target_id OR u.purpose<>s.purpose_key)) OR (l.attachment_id IS NOT NULL AND (a.id IS NULL OR a.company_id<>s.company_id OR a.owner_type<>s.target_type OR a.owner_id<>s.target_id OR a.purpose<>s.purpose_key)))) ORDER BY s.id LIMIT 50 FOR UPDATE OF s SKIP LOCKED").bind(company.as_str()).bind(oldest).bind(now).fetch_all(&mut *c).await?;
    let mut changed_count = 0;
    for set in sets {
        let links:Vec<(String,Option<String>)>=sqlx::query_as("UPDATE attachment_links SET state='expired' WHERE set_id=$1 AND state='draft' AND expires_at<=$2 RETURNING id,attachment_id").bind(&set).bind(now).fetch_all(&mut *c).await?;
        for (id, source) in links {
            sqlx::query("INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,$5,'system:jobs','link.expired','{}',$6)").bind(format!("attachment-event-{}",uuid::Uuid::new_v4())).bind(company.as_str()).bind(source).bind(id).bind(&set).bind(now).execute(&mut *c).await?;
            changed_count += 1;
        }
        changed_count+=sqlx::query("UPDATE upload_intents SET status='expired',updated_at=$2 WHERE attachment_set_id=$1 AND status='pending' AND expires_at<=$2").bind(&set).bind(now).execute(&mut *c).await?.rows_affected() as usize;
        let expired:Option<String>=sqlx::query_scalar("UPDATE attachment_sets s SET state='expired',updated_at=$2 WHERE s.id=$1 AND s.state='draft' AND NOT EXISTS(SELECT 1 FROM attachment_links l WHERE l.set_id=s.id AND l.state IN ('draft','bound')) AND ((EXISTS(SELECT 1 FROM attachment_links l WHERE l.set_id=s.id) AND NOT EXISTS(SELECT 1 FROM attachment_links l WHERE l.set_id=s.id AND l.expires_at>$2)) OR (NOT EXISTS(SELECT 1 FROM attachment_links l WHERE l.set_id=s.id) AND s.created_at<=$3)) RETURNING id").bind(&set).bind(now).bind(oldest).fetch_optional(&mut *c).await?;
        if expired.is_some() {
            sqlx::query("INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,'system:jobs','set.expired','{}',$4)").bind(format!("attachment-event-{}",uuid::Uuid::new_v4())).bind(company.as_str()).bind(&set).bind(now).execute(&mut *c).await?;
            changed_count += 1;
        }
    }
    Ok(changed_count)
}

/// Bounded cursor; failures do not prevent later candidates from being visited.
pub async fn next_field_cleanup_candidate(
    c: &mut sqlx::PgConnection,
    company: &CompanyId,
    after: Option<&str>,
    now: i64,
) -> Result<Option<String>, Error> {
    require_serializable(c).await?;
    Ok(sqlx::query_scalar("SELECT u.id FROM upload_intents u JOIN attachment_sets s ON s.id=u.attachment_set_id AND s.company_id=u.company_id JOIN attachment_links l ON l.id=u.attachment_link_id AND l.set_id=s.id AND l.upload_intent_id=u.id WHERE u.company_id=$1 AND u.owner_type='field_service_visit' AND s.target_type=u.owner_type AND s.target_id=u.owner_id AND s.purpose_key=u.purpose AND s.component_kind='record' AND s.purpose_key IN ('site_photo','technician_report','signed_job_sheet','exception_evidence') AND u.status IN ('pending','failed','expired') AND u.expires_at<=$2 AND ($3::text IS NULL OR u.id>$3) ORDER BY u.id LIMIT 1").bind(company.as_str()).bind(now).bind(after).fetch_optional(&mut *c).await?)
}
