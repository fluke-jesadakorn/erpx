//! Same-unit Legal source fence for a citation. Files, IAM and Commercial
//! authority are independent witnesses that composition must also retain.
use crate::domain::auxiliary::{LegalAuxiliaryActor, visibility_for};
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String, String, String);
const TABLES: [&str; 10] = [
    "legal_items",
    "legal_members",
    "legal_documents",
    "legal_source_chunks",
    "legal_text_reviews",
    "legal_law_sources",
    "legal_law_revisions",
    "legal_law_applicability",
    "legal_disposals",
    "legal_destructions",
];

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "LEGAL_CITATION_SOURCE_CHANGED",
        "The cited Legal source changed.",
    )
    .into()
}

/// Acquire with the other owner write/read fences before the first snapshot.
/// NOWAIT prevents waiting while earlier owner locks are held. The audit table
/// is write fenced up front, avoiding two SHARE holders upgrading on INSERT.
pub async fn lock_legal_citation_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("LEGAL_CITATION_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE public.legal_items,public.legal_members,public.legal_documents,public.legal_source_chunks,public.legal_text_reviews,public.legal_law_sources,public.legal_law_revisions,public.legal_law_applicability,public.legal_disposals,public.legal_destructions,public.legal_access_events IN SHARE ROW EXCLUSIVE MODE NOWAIT")
        .execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    Ok(())
}

pub struct CurrentLegalCitationSource {
    actor: LegalAuxiliaryActor,
    source_id: String,
    item_id: String,
    document_id: String,
    kind: String,
    image: String,
    document: LegalCitationDocument,
    unit: Unit,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalCitationDocument {
    pub object_key: Option<String>,
    pub attachment_link_id: Option<String>,
    pub attachment_source_id: Option<String>,
    pub checksum: String,
    pub disposed: bool,
    pub destroying: bool,
}
impl CurrentLegalCitationSource {
    pub fn item_id(&self) -> &str {
        &self.item_id
    }
    pub fn document_id(&self) -> &str {
        &self.document_id
    }
    pub fn kind(&self) -> &str {
        &self.kind
    }
    pub fn document(&self) -> &LegalCitationDocument {
        &self.document
    }
    /// The caller must additionally recheck actual Files, IAM and purchase
    /// proofs before commit; this checks Legal-owned facts only.
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let current = image(c, &self.actor, &self.source_id).await?;
        if current.as_ref()
            != Some(&(
                self.item_id.clone(),
                self.document_id.clone(),
                self.kind.clone(),
                self.image.clone(),
                self.document.clone(),
            ))
        {
            return Err(changed());
        }
        Ok(())
    }
}

/// Inspect only a visible actual Legal item and document. A caller-provided ID
/// is merely a selector and cannot grant Files, IAM or purchase access.
pub async fn inspect_legal_citation_source(
    c: &mut PgConnection,
    actor: &LegalAuxiliaryActor,
    source_id: &str,
) -> Result<Option<CurrentLegalCitationSource>, Error> {
    let identity = unit(c).await?;
    let found = image(c, actor, source_id).await?;
    if unit(c).await? != identity {
        return Err(changed());
    }
    Ok(found.map(
        |(item_id, document_id, kind, image, document)| CurrentLegalCitationSource {
            actor: actor.clone(),
            source_id: source_id.to_owned(),
            item_id,
            document_id,
            kind,
            image,
            document,
            unit: identity,
        },
    ))
}

async fn image(
    c: &mut PgConnection,
    actor: &LegalAuxiliaryActor,
    source: &str,
) -> Result<Option<(String, String, String, String, LegalCitationDocument)>, Error> {
    let law = source.starts_with("law:");
    let id = source
        .strip_prefix("law:")
        .or_else(|| source.strip_prefix("document:"))
        .unwrap_or(source);
    let explicit_chunk = law || source.starts_with("document:");
    let sql = format!(
        "WITH chosen AS MATERIALIZED (SELECT c.* FROM public.legal_source_chunks c LEFT JOIN public.legal_text_reviews r ON r.id=c.text_review_id AND r.state='approved' AND r.company_id=c.company_id AND r.item_id=c.item_id AND r.document_id=c.document_id AND r.document_checksum=c.document_checksum WHERE c.id=$5 AND c.company_id=$1 AND (($6 AND c.revision_id IS NOT NULL) OR (NOT $6 AND r.id IS NOT NULL))), target AS MATERIALIZED (SELECT d.* FROM public.legal_documents d WHERE d.id=COALESCE((SELECT document_id FROM chosen),CASE WHEN NOT $7 THEN $5 END)), selected AS MATERIALIZED (SELECT i.*,d.id doc_id FROM target d JOIN public.legal_items i ON i.id=d.item_id WHERE {}) SELECT i.id,i.doc_id,i.kind,jsonb_build_object('item',to_jsonb(i),'document',(SELECT to_jsonb(d) FROM target d),'chunk',(SELECT to_jsonb(x) FROM chosen x),'reviews',(SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY r.id),'[]'::jsonb) FROM public.legal_text_reviews r WHERE r.document_id=i.doc_id),'members',(SELECT COALESCE(jsonb_agg(to_jsonb(m) ORDER BY m.user_id),'[]'::jsonb) FROM public.legal_members m WHERE m.item_id=i.id),'disposals',(SELECT COALESCE(jsonb_agg(to_jsonb(x) ORDER BY x.document_id),'[]'::jsonb) FROM public.legal_disposals x WHERE x.document_id=i.doc_id),'destructions',(SELECT COALESCE(jsonb_agg(to_jsonb(x) ORDER BY x.id),'[]'::jsonb) FROM public.legal_destructions x WHERE x.item_id=i.id),'laws',(SELECT COALESCE(jsonb_agg(to_jsonb(s) ORDER BY s.id),'[]'::jsonb) FROM public.legal_law_sources s WHERE s.id=(SELECT r.source_id FROM public.legal_law_revisions r JOIN chosen x ON x.revision_id=r.id)),'revisions',(SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY r.id),'[]'::jsonb) FROM public.legal_law_revisions r WHERE r.source_id=(SELECT rr.source_id FROM public.legal_law_revisions rr JOIN chosen x ON x.revision_id=rr.id)),'applicability',(SELECT COALESCE(jsonb_agg(to_jsonb(a) ORDER BY a.id),'[]'::jsonb) FROM public.legal_law_applicability a WHERE a.company_id=$1 AND a.revision_id=(SELECT revision_id FROM chosen)))::text image FROM selected i",
        visibility_for(actor, "i")
    );
    let row = sqlx::query(AssertSqlSafe(sql))
        .bind(actor.company_id.as_str())
        .bind(actor.branch_id.as_ref().map(|v| v.as_str()))
        .bind(actor.department_id.as_ref().map(|v| v.as_str()))
        .bind(actor.user_id.as_str())
        .bind(id)
        .bind(law)
        .bind(explicit_chunk)
        .fetch_optional(&mut *c)
        .await?;
    if let Some(r) = row {
        let document_id: String = r.try_get("doc_id")?;
        let d=sqlx::query("SELECT d.object_key,d.attachment_link_id,d.attachment_source_id,d.checksum,EXISTS(SELECT 1 FROM public.legal_disposals x WHERE x.document_id=d.id) disposed,EXISTS(SELECT 1 FROM public.legal_destructions x WHERE x.item_id=d.item_id AND x.state='executing') destroying FROM public.legal_documents d WHERE d.id=$1").bind(&document_id).fetch_one(c).await?;
        Ok(Some((
            r.try_get("id")?,
            document_id,
            r.try_get("kind")?,
            r.try_get("image")?,
            LegalCitationDocument {
                object_key: d.try_get("object_key")?,
                attachment_link_id: d.try_get("attachment_link_id")?,
                attachment_source_id: d.try_get("attachment_source_id")?,
                checksum: d.try_get("checksum")?,
                disposed: d.try_get("disposed")?,
                destroying: d.try_get("destroying")?,
            },
        )))
    } else {
        Ok(None)
    }
}

pub(crate) async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("LEGAL_CITATION_TRANSACTION_REQUIRED").into());
    }
    let mut names = TABLES.to_vec();
    names.push("legal_access_events");
    let valid:bool=sqlx::query_scalar("SELECT count(*)=$2 AND bool_and(p.relnamespace='public'::regnamespace AND p.relkind IN ('r','p') AND p.relpersistence='p' AND to_regclass(p.relname::text)=p.oid AND NOT row_security_active(p.oid) AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.granted AND l.relation=p.oid AND l.mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))) FROM pg_class p WHERE p.oid=ANY(ARRAY(SELECT to_regclass('public.'||n)::oid FROM unnest($1::text[]) n))")
        .bind(&names).bind(names.len() as i64).fetch_one(&mut *c).await?;
    if !valid {
        return Err(PersistenceError::new("LEGAL_CITATION_FENCE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,current_setting('search_path')").fetch_one(c).await?)
}

/// Exact audit rows returned by the actual citation INSERTs in this unit.
/// This is evidence of a read, never actor, Files or purchase authority.
pub struct RetainedLegalCitationAudit {
    unit: Unit,
    rows: Vec<(String, String)>,
}
impl RetainedLegalCitationAudit {
    pub(crate) async fn capture(
        c: &mut PgConnection,
        rows: Vec<(String, String)>,
    ) -> Result<Self, Error> {
        if rows.len() != 1 {
            return Err(changed());
        }
        let unit = unit(c).await?;
        let proof = Self { unit, rows };
        proof.require_current(c).await?;
        Ok(proof)
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        for (id, expected) in &self.rows {
            let actual: Option<String> = sqlx::query_scalar(
                "SELECT to_jsonb(e)::text FROM public.legal_access_events e WHERE e.id=$1",
            )
            .bind(id)
            .fetch_optional(&mut *c)
            .await?;
            if actual.as_ref() != Some(expected) {
                return Err(changed());
            }
        }
        Ok(())
    }
}
