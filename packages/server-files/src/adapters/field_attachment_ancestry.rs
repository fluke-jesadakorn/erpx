//! Files-owned lineage candidates for the 1093/1097 routing predicates.
//! This is neither selected authority nor a global negative classification.
//! A transaction-local multiset retains the exact Files inputs; Finance,
//! Workflow and Inventory must independently resolve every relevant target.
//! Temporary-table permission and PostgreSQL TOAST/work-memory use are not
//! assumed bounded. No caller-provided target is accepted as evidence.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection};
use uuid::Uuid;
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

// Six 1093 attachment branches, plus the selected link's own three direct
// branches. LEFT JOINs, duplicate rows, NULLs and global attachment links are
// deliberate: the migration makes no company, revocation or state filter.
const SOURCE: &str = r#"WITH root AS (
 SELECT l.id link_id,l.attachment_id,l.upload_intent_id,l.set_id
 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id
 WHERE l.id=$1 AND s.company_id=$2
), direct AS (
 SELECT r.link_id,r.attachment_id,r.upload_intent_id,r.set_id,
        s.target_type st,s.target_id sr,u.owner_type ut,u.owner_id ur,us.target_type ust,us.target_id usr
 FROM root r LEFT JOIN attachment_sets s ON s.id=r.set_id
 LEFT JOIN upload_intents u ON u.id=r.upload_intent_id
 LEFT JOIN attachment_sets us ON us.id=u.attachment_set_id
), att AS (
 SELECT a.id attachment_id,a.upload_id,u.attachment_set_id,
        a.owner_type at,a.owner_id ar,u.owner_type ut,u.owner_id ur,s.target_type st,s.target_id sr
 FROM root r JOIN attachments a ON a.id=r.attachment_id
 LEFT JOIN upload_intents u ON u.id=a.upload_id LEFT JOIN attachment_sets s ON s.id=u.attachment_set_id
), links AS (
 SELECT l.id link_id,l.attachment_id,l.upload_intent_id,l.set_id,u.attachment_set_id,
        s.target_type st,s.target_id sr,u.owner_type ut,u.owner_id ur,us.target_type ust,us.target_id usr
 FROM root r JOIN attachment_links l ON l.attachment_id=r.attachment_id
 LEFT JOIN attachment_sets s ON s.id=l.set_id LEFT JOIN upload_intents u ON u.id=l.upload_intent_id
 LEFT JOIN attachment_sets us ON us.id=u.attachment_set_id
)
 SELECT v.path,d.link_id,d.attachment_id,d.upload_intent_id,d.set_id,NULL::text upload_set_id,v.kind,v.target FROM direct d CROSS JOIN LATERAL (VALUES ('direct-set',d.st,d.sr),('direct-upload',d.ut,d.ur),('direct-upload-set',d.ust,d.usr)) v(path,kind,target)
 UNION ALL SELECT v.path,NULL::text,a.attachment_id,a.upload_id,NULL::text,a.attachment_set_id,v.kind,v.target FROM att a CROSS JOIN LATERAL (VALUES ('source-attachment',a.at,a.ar),('source-upload',a.ut,a.ur),('source-upload-set',a.st,a.sr)) v(path,kind,target)
 UNION ALL SELECT v.path,l.link_id,l.attachment_id,l.upload_intent_id,l.set_id,l.attachment_set_id,v.kind,v.target FROM links l CROSS JOIN LATERAL (VALUES ('linked-set',l.st,l.sr),('linked-upload',l.ut,l.ur),('linked-upload-set',l.ust,l.usr)) v(path,kind,target)"#;

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FILES_ATTACHMENT_ANCESTRY_CHANGED",
        "The original Files attachment ancestry changed.",
    )
    .into()
}
/// Acquire all Files relation locks before any source snapshot. NOWAIT must be
/// handled by retrying the entire outer transaction, not the lock alone.
pub async fn lock_field_attachment_ancestry(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FILES_ANCESTRY_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE attachment_sets,attachment_links,attachments,upload_intents IN SHARE MODE NOWAIT").execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    Ok(())
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("FILES_ANCESTRY_SERIALIZABLE_REQUIRED").into());
    }
    let held:bool=sqlx::query_scalar("SELECT count(DISTINCT relation)=4 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND relation IN ('attachment_sets'::regclass,'attachment_links'::regclass,'attachments'::regclass,'upload_intents'::regclass)").fetch_one(&mut *c).await?;
    if !held {
        return Err(PersistenceError::new("FILES_ANCESTRY_LOCK_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
/// A raw routing input, never a grant. Empty or NULL refs retain SQL semantics.
/// `direct-*` describes the selected link itself; it is not evidence that the
/// 1093 attachment predicate is true. Only `source-*` and `linked-*` represent
/// that predicate, and Finance/Workflow/Inventory still resolve their targets.
#[derive(Debug, PartialEq, Eq)]
pub struct FieldAttachmentCandidate {
    pub path: String,
    pub kind: Option<String>,
    pub target: Option<String>,
}
/// Field visit routing observations from retained Files ancestry. They are not
/// selected authority or a negative classification for other owner types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldVisitRouting {
    pub direct_set: bool,
    pub direct: bool,
    pub source: bool,
}
pub struct CurrentFieldAttachmentAncestry {
    unit: Unit,
    table: String,
    relation: String,
    oid: i64,
    collations: Vec<i64>,
    role: String,
    search_path: String,
    link: String,
    company: String,
}
/// Capture a selected link in Company scope, then all 1093 source lineage
/// globally. None means no scoped link, not a non-Field classification.
pub async fn inspect_field_attachment_ancestry(
    c: &mut PgConnection,
    company: &CompanyId,
    link: &str,
) -> Result<Option<CurrentFieldAttachmentAncestry>, Error> {
    let unit = unit(c).await?;
    // A role subject to RLS could see only part of the global predicate. Refuse
    // classification instead of treating invisible lineage as absence.
    let visible:bool=sqlx::query_scalar("SELECT NOT (row_security_active('attachment_sets'::regclass) OR row_security_active('attachment_links'::regclass) OR row_security_active('attachments'::regclass) OR row_security_active('upload_intents'::regclass))").fetch_one(&mut *c).await?;
    if !visible {
        return Err(PersistenceError::new("FILES_ANCESTRY_RLS_UNSUPPORTED").into());
    }
    let (role, search_path): (String, String) =
        sqlx::query_as("SELECT current_user::text,current_setting('search_path')")
            .fetch_one(&mut *c)
            .await?;
    let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.id=$1 AND s.company_id=$2)").bind(link).bind(company.as_str()).fetch_one(&mut *c).await?;
    if !exists {
        return Ok(None);
    }
    let table = format!("erpx_files_ancestry_{}", Uuid::new_v4().simple());
    let sql = format!("CREATE TEMP TABLE {table} ON COMMIT DROP AS {SOURCE}");
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(link)
        .bind(company.as_str())
        .execute(&mut *c)
        .await?;
    let oid: i64 = sqlx::query_scalar("SELECT to_regclass($1)::oid::bigint")
        .bind(format!("pg_temp.{table}"))
        .fetch_one(&mut *c)
        .await?;
    let relation = format!("pg_temp.\"{table}\"");
    let collations:Vec<i64>=sqlx::query_scalar("SELECT attcollation::bigint FROM pg_attribute WHERE attrelid=$1::bigint::oid AND attnum>0 ORDER BY attnum").bind(oid).fetch_all(&mut *c).await?;
    let witness = CurrentFieldAttachmentAncestry {
        unit,
        table,
        relation,
        oid,
        collations,
        role,
        search_path,
        link: link.into(),
        company: company.as_str().into(),
    };
    witness.require_relation(c).await?;
    Ok(Some(witness))
}
impl CurrentFieldAttachmentAncestry {
    async fn require_relation(&self, c: &mut PgConnection) -> Result<(), Error> {
        let (role,search_path,visible):(String,String,bool)=sqlx::query_as("SELECT current_user::text,current_setting('search_path'),NOT (row_security_active('attachment_sets'::regclass) OR row_security_active('attachment_links'::regclass) OR row_security_active('attachments'::regclass) OR row_security_active('upload_intents'::regclass))").fetch_one(&mut *c).await?;
        if role != self.role || search_path != self.search_path || !visible {
            return Err(changed());
        }
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_class c WHERE c.oid=to_regclass($1) AND c.oid::bigint=$2 AND c.relnamespace=pg_my_temp_schema() AND c.relpersistence='t' AND c.relkind='r' AND NOT c.relrowsecurity AND NOT EXISTS(SELECT 1 FROM pg_trigger t WHERE t.tgrelid=c.oid AND NOT t.tgisinternal) AND NOT EXISTS(SELECT 1 FROM pg_rewrite r WHERE r.ev_class=c.oid) AND c.relowner=(SELECT oid FROM pg_roles WHERE rolname=current_user) AND (SELECT array_agg(a.attname::text ORDER BY a.attnum) FROM pg_attribute a WHERE a.attrelid=c.oid AND a.attnum>0 AND NOT a.attisdropped)=ARRAY['path','link_id','attachment_id','upload_intent_id','set_id','upload_set_id','kind','target']::text[] AND NOT EXISTS(SELECT 1 FROM pg_attribute a WHERE a.attrelid=c.oid AND a.attnum>0 AND (a.attisdropped OR a.atttypid<>'text'::regtype OR a.attgenerated<>'' OR a.attcollation::bigint<>($3::bigint[])[a.attnum])))").bind(format!("pg_temp.{}",self.table)).bind(self.oid).bind(&self.collations).fetch_one(c).await?;
        if !valid {
            return Err(changed());
        }
        Ok(())
    }
    /// Revalidation compares the complete multiset, including NULL and duplicates.
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        self.require_relation(&mut *c).await?;
        let columns = r#"path COLLATE "C",link_id COLLATE "C",attachment_id COLLATE "C",upload_intent_id COLLATE "C",set_id COLLATE "C",upload_set_id COLLATE "C",kind COLLATE "C",target COLLATE "C""#;
        let sql = format!(
            r#"WITH fresh AS ({SOURCE}), current_bytes AS (SELECT {columns} FROM fresh), retained_bytes AS (SELECT {columns} FROM pg_temp."{}"), delta AS ((SELECT * FROM current_bytes EXCEPT ALL SELECT * FROM retained_bytes) UNION ALL (SELECT * FROM retained_bytes EXCEPT ALL SELECT * FROM current_bytes)) SELECT NOT EXISTS(SELECT 1 FROM delta)"#,
            self.table
        );
        let same: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
            .bind(&self.link)
            .bind(&self.company)
            .fetch_one(c)
            .await?;
        if !same {
            return Err(changed());
        }
        Ok(())
    }
    /// Separate 1093 source ancestry from the selected link's direct target.
    /// These are routing facts only; the caller retains this witness and must
    /// still establish all other owner predicates and selected authority.
    pub async fn field_visit_routing(
        &self,
        c: &mut PgConnection,
    ) -> Result<FieldVisitRouting, Error> {
        self.require_current(&mut *c).await?;
        let sql = format!(
            "SELECT EXISTS(SELECT 1 FROM {} WHERE path='direct-set' AND kind='field_service_visit'), EXISTS(SELECT 1 FROM {} WHERE path IN ('direct-set','direct-upload','direct-upload-set') AND kind='field_service_visit'), EXISTS(SELECT 1 FROM {} WHERE path IN ('source-attachment','source-upload','source-upload-set','linked-set','linked-upload','linked-upload-set') AND kind='field_service_visit')",
            self.relation, self.relation, self.relation
        );
        let (direct_set, direct, source): (bool, bool, bool) =
            sqlx::query_as(sqlx::AssertSqlSafe(sql))
                .fetch_one(&mut *c)
                .await?;
        self.require_current(c).await?;
        Ok(FieldVisitRouting {
            direct_set,
            direct,
            source,
        })
    }
    /// Iterate a single candidate at a time. The database's per-row TOAST memory
    /// and arbitrarily large returned text are not bounded by this API.
    pub async fn candidate(
        &self,
        c: &mut PgConnection,
        index: i64,
    ) -> Result<Option<FieldAttachmentCandidate>, Error> {
        self.require_current(&mut *c).await?;
        if index < 0 {
            return Err(changed());
        }
        let sql = format!(
            r#"SELECT path,kind,target FROM pg_temp."{}" ORDER BY path COLLATE "C",link_id COLLATE "C",attachment_id COLLATE "C",upload_intent_id COLLATE "C",set_id COLLATE "C",upload_set_id COLLATE "C",kind COLLATE "C",target COLLATE "C" OFFSET $1 LIMIT 1"#,
            self.table
        );
        let row: Option<(String, Option<String>, Option<String>)> =
            sqlx::query_as(sqlx::AssertSqlSafe(sql))
                .bind(index)
                .fetch_optional(c)
                .await?;
        Ok(row.map(|(path, kind, target)| FieldAttachmentCandidate { path, kind, target }))
    }
}

// This trait is a trusted internal owner port, not an unforgeable capability
// or present authority. Composition must supply this actual Files witness;
// requests must never select an implementation or relation identifier.
impl erpx_server_workflow::public::record_origin::FieldPaymentTargets
    for CurrentFieldAttachmentAncestry
{
    fn relation_identifier(&self) -> &str {
        &self.relation
    }
    fn require_current<'a>(
        &'a self,
        c: &'a mut PgConnection,
    ) -> erpx_server_runtime::public::postgres::PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move { CurrentFieldAttachmentAncestry::require_current(self, c).await })
    }
}
