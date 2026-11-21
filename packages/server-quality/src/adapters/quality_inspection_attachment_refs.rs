//! Quality-owned inspection/attachment references, not Files metadata or authority.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId};
use futures_util::TryStreamExt;
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
const MAX_REFS: i64 = 5000;
const MAX_ID_BYTES: i64 = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualityInspectionAttachmentRef {
    pub inspection_id: String,
    pub attachment_id: String,
    pub ordinal: i64,
    pub inspection_created_at: i64,
}

/// A position in the Quality-owned reference stream. A cursor is never an
/// authorization or a Files metadata witness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualityInspectionRefCursor {
    pub inspection_created_at: i64,
    pub ordinal: i64,
    pub inspection_id: String,
    pub attachment_id: String,
}

pub struct CurrentQualityInspectionAttachmentRefPage {
    unit: Unit, company: CompanyId, branch: Option<RecordId>, hold: RecordId,
    after: Option<QualityInspectionRefCursor>, limit: usize,
    refs: Vec<QualityInspectionAttachmentRef>, next: Option<QualityInspectionRefCursor>, revision: String,
}

impl CurrentQualityInspectionAttachmentRefPage {
    pub fn refs(&self) -> &[QualityInspectionAttachmentRef] { &self.refs }
    pub fn next(&self) -> Option<&QualityInspectionRefCursor> { self.next.as_ref() }
    pub fn revision(&self) -> &str { &self.revision }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        let now = unit(c).await?;
        if now != self.unit { return Err(changed()); }
        let current = inspect_quality_inspection_attachment_ref_page(c, &self.company, self.branch.as_ref(), &self.hold, self.after.as_ref(), Some(&self.revision), self.limit).await?;
        if current.refs != self.refs || current.next != self.next { return Err(changed()); }
        Ok(())
    }
}

fn frame(hash: &mut Sha256, bytes: &[u8]) { hash.update((bytes.len() as u64).to_be_bytes()); hash.update(bytes); }

// Stream the COMPLETE source set without materializing a vector or filtering
// Files facts. Each identifier is explicitly bounded before transfer; an
// oversized historical identifier returns unavailable, never a truncated set.
async fn reference_revision(c: &mut PgConnection, company: &CompanyId, branch: Option<&RecordId>, hold: &RecordId) -> Result<String, Error> {
    let mut hash = Sha256::new();
    frame(&mut hash, b"erpx-quality-inspection-refs-v1");
    frame(&mut hash, company.as_str().as_bytes());
    match branch { Some(branch) => { hash.update([1]); frame(&mut hash, branch.as_str().as_bytes()); }, None => hash.update([0]) }
    frame(&mut hash, hold.as_str().as_bytes());
    let mut rows = sqlx::query("SELECT CASE WHEN octet_length(ia.inspection_id)<=$2 THEN ia.inspection_id END inspection_id,CASE WHEN octet_length(ia.attachment_id)<=$2 THEN ia.attachment_id END attachment_id,CASE WHEN ia.ordinal BETWEEN -9007199254740991 AND 9007199254740991 THEN ia.ordinal END ordinal,i.created_at FROM inventory_quality_inspection_attachments ia JOIN inventory_quality_inspections i ON i.id=ia.inspection_id WHERE i.hold_id=$1 ORDER BY i.created_at,ia.ordinal,ia.inspection_id COLLATE \"C\",ia.attachment_id COLLATE \"C\"")
        .bind(hold.as_str()).bind(MAX_ID_BYTES).fetch(&mut *c);
    let mut count = 0_u64;
    while let Some(row) = rows.try_next().await? {
        let inspection: String = row.try_get::<Option<String>, _>("inspection_id")?.ok_or_else(unavailable)?;
        let attachment: String = row.try_get::<Option<String>, _>("attachment_id")?.ok_or_else(unavailable)?;
        let ordinal: i64 = row.try_get::<Option<i64>, _>("ordinal")?.ok_or_else(unavailable)?;
        let created: i64 = row.try_get("created_at")?;
        hash.update(created.to_be_bytes()); hash.update(ordinal.to_be_bytes());
        frame(&mut hash, inspection.as_bytes()); frame(&mut hash, attachment.as_bytes());
        count = count.checked_add(1).ok_or_else(unavailable)?;
    }
    drop(rows);
    hash.update(count.to_be_bytes());
    Ok(format!("qir1:{:x}", hash.finalize()))
}

/// Page Quality references only. Composition must pass these witnessed IDs to
/// Files for revocation and metadata checks before presenting anything.
pub async fn inspect_quality_inspection_attachment_ref_page(
    c: &mut PgConnection, company: &CompanyId, branch: Option<&RecordId>, hold: &RecordId,
    after: Option<&QualityInspectionRefCursor>, expected_revision: Option<&str>, limit: usize,
) -> Result<CurrentQualityInspectionAttachmentRefPage, Error> {
    let current_unit = unit(c).await?;
    if !(1..=200).contains(&limit) || (after.is_some() && expected_revision.is_none())
        || expected_revision.is_some_and(|v| v.len()!=69 || !v.starts_with("qir1:") || !v[5..].bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
        || after.is_some_and(|v| v.inspection_id.len()>MAX_ID_BYTES as usize || v.attachment_id.len()>MAX_ID_BYTES as usize || !(-9007199254740991..=9007199254740991).contains(&v.ordinal)) {
        return Err(BusinessError::invalid("QUALITY_INSPECTION_REFS_CURSOR_INVALID", "Reload Quality inspection evidence.").into());
    }
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inventory_quality_holds WHERE id=$1 AND company_id=$2 AND ($3::text IS NULL OR branch_id=$3))")
        .bind(hold.as_str()).bind(company.as_str()).bind(branch.map(RecordId::as_str)).fetch_one(&mut *c).await?;
    if !exists { return Err(BusinessError::new(FailureKind::NotFound, "QUALITY_HOLD_NOT_FOUND", "Quality hold not found.").into()); }
    let revision = reference_revision(c, company, branch, hold).await?;
    if expected_revision.is_some_and(|expected| expected != revision) { return Err(changed()); }
    let rows = sqlx::query("SELECT CASE WHEN octet_length(ia.inspection_id)<=$2 THEN ia.inspection_id END inspection_id,CASE WHEN octet_length(ia.attachment_id)<=$2 THEN ia.attachment_id END attachment_id,CASE WHEN ia.ordinal BETWEEN -9007199254740991 AND 9007199254740991 THEN ia.ordinal END ordinal,i.created_at FROM inventory_quality_inspection_attachments ia JOIN inventory_quality_inspections i ON i.id=ia.inspection_id WHERE i.hold_id=$1 AND ($3::bigint IS NULL OR (i.created_at,ia.ordinal,ia.inspection_id COLLATE \"C\",ia.attachment_id COLLATE \"C\") > ($3,$4,$5::text COLLATE \"C\",$6::text COLLATE \"C\")) ORDER BY i.created_at,ia.ordinal,ia.inspection_id COLLATE \"C\",ia.attachment_id COLLATE \"C\" LIMIT $7")
        .bind(hold.as_str()).bind(MAX_ID_BYTES).bind(after.map(|v|v.inspection_created_at)).bind(after.map(|v|v.ordinal)).bind(after.map(|v|v.inspection_id.as_str())).bind(after.map(|v|v.attachment_id.as_str())).bind((limit+1) as i64).fetch_all(&mut *c).await?;
    let has_more = rows.len() > limit;
    let refs = rows.into_iter().take(limit).map(|row| Ok(QualityInspectionAttachmentRef {
        inspection_id: row.try_get::<Option<String>, _>("inspection_id")?.ok_or_else(unavailable)?,
        attachment_id: row.try_get::<Option<String>, _>("attachment_id")?.ok_or_else(unavailable)?,
        ordinal: row.try_get::<Option<i64>, _>("ordinal")?.ok_or_else(unavailable)?,
        inspection_created_at: row.try_get("created_at")?,
    })).collect::<Result<Vec<_>, Error>>()?;
    let next = if has_more { refs.last().map(|v| QualityInspectionRefCursor { inspection_created_at: v.inspection_created_at, ordinal: v.ordinal, inspection_id: v.inspection_id.clone(), attachment_id: v.attachment_id.clone() }) } else { None };
    Ok(CurrentQualityInspectionAttachmentRefPage { unit: current_unit, company: company.clone(), branch: branch.cloned(), hold: hold.clone(), after: after.cloned(), limit, refs, next, revision })
}

/// Caller must invoke this utility before any Holding transaction snapshot.
pub async fn lock_quality_inspection_attachment_refs(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() { return Err(PersistenceError::new("QUALITY_INSPECTION_REFS_TRANSACTION_REQUIRED").into()); }
    sqlx::query("LOCK TABLE inventory_quality_holds,inventory_quality_inspections,inventory_quality_inspection_attachments IN SHARE MODE").execute(c).await?;
    Ok(())
}

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() || sqlx::query_scalar::<_, String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" {
        return Err(PersistenceError::new("QUALITY_INSPECTION_REFS_TRANSACTION_REQUIRED").into());
    }
    let held: bool = sqlx::query_scalar("SELECT count(DISTINCT relation)=3 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND relation IN ('inventory_quality_holds'::regclass,'inventory_quality_inspections'::regclass,'inventory_quality_inspection_attachments'::regclass)").fetch_one(&mut *c).await?;
    if !held { return Err(PersistenceError::new("QUALITY_INSPECTION_REFS_LOCKS_REQUIRED").into()); }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}

fn changed() -> Error { BusinessError::new(FailureKind::Conflict, "QUALITY_INSPECTION_REFS_CHANGED", "Quality inspection evidence changed.").into() }
fn unavailable() -> Error { BusinessError::new(FailureKind::Unavailable, "QUALITY_INSPECTION_REFS_LIMIT", "Quality inspection evidence exceeds the bounded read.").into() }

// Legacy orders by created_at and ordinal only. Distinct inspections can tie,
// so compare tie members canonically without changing the returned order.
fn equivalent(left: &[QualityInspectionAttachmentRef], right: &[QualityInspectionAttachmentRef]) -> bool {
    if left.len() != right.len() { return false; }
    let mut a: Vec<_> = left.iter().collect();
    let mut b: Vec<_> = right.iter().collect();
    let compare = |a: &&QualityInspectionAttachmentRef, b: &&QualityInspectionAttachmentRef| {
        (a.inspection_created_at, a.ordinal, &a.inspection_id, &a.attachment_id)
            .cmp(&(b.inspection_created_at, b.ordinal, &b.inspection_id, &b.attachment_id))
    };
    a.sort_unstable_by(compare);
    b.sort_unstable_by(compare);
    a == b
}

async fn capture(c: &mut PgConnection, company: &CompanyId, branch: Option<&RecordId>, hold: &RecordId) -> Result<Option<Vec<QualityInspectionAttachmentRef>>, Error> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inventory_quality_holds WHERE id=$1 AND company_id=$2 AND ($3::text IS NULL OR branch_id=$3))")
        .bind(hold.as_str()).bind(company.as_str()).bind(branch.map(RecordId::as_str)).fetch_one(&mut *c).await?;
    if !exists { return Ok(None); }
    // Project oversized identifiers as NULL so PostgreSQL does not transfer
    // an unbounded historic value. Exceeding either bound is explicit failure.
    let rows = sqlx::query("SELECT CASE WHEN octet_length(ia.inspection_id)<=$2 THEN ia.inspection_id END inspection_id, CASE WHEN octet_length(ia.attachment_id)<=$2 THEN ia.attachment_id END attachment_id,CASE WHEN ia.ordinal BETWEEN -9007199254740991 AND 9007199254740991 THEN ia.ordinal END ordinal,i.created_at FROM inventory_quality_inspection_attachments ia JOIN inventory_quality_inspections i ON i.id=ia.inspection_id WHERE i.hold_id=$1 ORDER BY i.created_at,ia.ordinal LIMIT $3")
        .bind(hold.as_str()).bind(MAX_ID_BYTES).bind(MAX_REFS + 1).fetch_all(&mut *c).await?;
    if rows.len() > MAX_REFS as usize { return Err(unavailable()); }
    let mut refs = Vec::with_capacity(rows.len());
    for row in rows {
        refs.push(QualityInspectionAttachmentRef {
            inspection_id: row.try_get::<Option<String>, _>("inspection_id")?.ok_or_else(unavailable)?,
            attachment_id: row.try_get::<Option<String>, _>("attachment_id")?.ok_or_else(unavailable)?,
            ordinal: row.try_get::<Option<i64>, _>("ordinal")?.ok_or_else(unavailable)?, inspection_created_at: row.try_get("created_at")?,
        });
    }
    Ok(Some(refs))
}

/// Retains the complete Quality-side reference set, including its absence.
/// This grants neither record authority nor Files metadata access.
pub struct CurrentQualityInspectionAttachmentRefs {
    unit: Unit, company: CompanyId, branch: Option<RecordId>, hold: RecordId,
    refs: Vec<QualityInspectionAttachmentRef>,
}
impl CurrentQualityInspectionAttachmentRefs {
    pub fn refs(&self) -> &[QualityInspectionAttachmentRef] { &self.refs }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit { return Err(changed()); }
        let Some(current) = capture(c, &self.company, self.branch.as_ref(), &self.hold).await? else { return Err(changed()); };
        if !equivalent(&current, &self.refs) { return Err(changed()); }
        Ok(())
    }
}
pub async fn inspect_quality_inspection_attachment_refs(c: &mut PgConnection, company: &CompanyId, branch: Option<&RecordId>, hold: &RecordId) -> Result<CurrentQualityInspectionAttachmentRefs, Error> {
    let unit = unit(c).await?;
    let refs = capture(c, company, branch, hold).await?.ok_or_else(|| BusinessError::new(FailureKind::NotFound, "QUALITY_HOLD_NOT_FOUND", "Quality hold not found."))?;
    Ok(CurrentQualityInspectionAttachmentRefs { unit, company: company.clone(), branch: branch.cloned(), hold: hold.clone(), refs })
}
