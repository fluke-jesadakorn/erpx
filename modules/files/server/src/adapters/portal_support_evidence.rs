//! Files-owned Portal support evidence. Scope parameters never grant target,
//! session or purchase authority; the enclosing owner must provide those checks.
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};
use std::collections::HashSet;
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PortalSupportEvidenceError {
    Business(BusinessError),
    RequiredCount { required: u8 },
}
impl From<BusinessError> for PortalSupportEvidenceError {
    fn from(value: BusinessError) -> Self {
        Self::Business(value)
    }
}
type Error = PgTransactionError<PortalSupportEvidenceError>;
type Unit = (String, i32, String, String);
fn fail(code: &'static str, message: &'static str, kind: FailureKind) -> Error {
    PgTransactionError::Business(PortalSupportEvidenceError::Business(BusinessError::new(
        kind, code, message,
    )))
}
fn changed() -> Error {
    fail(
        "ATTACHMENT_BINDING_CHANGED",
        "The original file set changed. Reload the files and try again.",
        FailureKind::Conflict,
    )
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("PORTAL_EVIDENCE_UNIT_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortalEvidenceManifest {
    pub link_id: String,
    pub source_id: String,
    pub purpose_key: String,
    pub checksum_sha256: String,
    pub media_type: String,
    pub bytes: i64,
}
#[derive(Debug)]
struct Set {
    id: String,
    links: Vec<PortalEvidenceManifest>,
}
#[derive(Debug)]
pub struct PortalSupportEvidence {
    unit: Unit,
    company: String,
    target: String,
    version: i64,
    prepared_at: i64,
    sets: Vec<Set>,
}
impl PortalSupportEvidence {
    pub fn manifest(&self) -> Vec<PortalEvidenceManifest> {
        self.sets.iter().flat_map(|s| s.links.clone()).collect()
    }
}
/// Reads and locks Files rows only; never writes or establishes owner authority.
pub async fn prepare_portal_support_evidence(
    c: &mut PgConnection,
    company: &CompanyId,
    target: &str,
    version: i64,
    ids: &[String],
    prepared_at: i64,
) -> Result<PortalSupportEvidence, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("PORTAL_EVIDENCE_UNIT_REQUIRED").into());
    }
    // Fence Files writers before the first protected source snapshot. The caller
    // must acquire earlier owner/purchase locks in the common global order.
    sqlx::query("LOCK TABLE attachments,upload_intents,attachment_sets,attachment_links,attachment_events IN SHARE ROW EXCLUSIVE MODE").execute(&mut *c).await?;
    let unit = unit(c).await?;
    let mut seen = HashSet::new();
    let ids: Vec<_> = ids.iter().filter(|id| seen.insert(id.as_str())).collect();
    if ids.is_empty() || ids.len() > 20 {
        return Err(fail(
            "ATTACHMENT_SETS_REQUIRED",
            "Choose one to twenty file sets.",
            FailureKind::InvalidInput,
        ));
    }
    let mut sets = Vec::new();
    for id in ids {
        let row=sqlx::query("SELECT target_type,target_id,component_kind,component_ref,purpose_key,state,expected_target_version,policy_version,required_count FROM attachment_sets WHERE id=$1 AND company_id=$2 FOR UPDATE").bind(id).bind(company.as_str()).fetch_optional(&mut *c).await?.ok_or_else(||fail("ATTACHMENT_SET_NOT_FOUND","File set not found.",FailureKind::NotFound))?;
        if row.try_get::<String, _>("target_type")? != "customer_access_binding"
            || row.try_get::<String, _>("target_id")? != target
        {
            return Err(fail(
                "ATTACHMENT_SET_TARGET",
                "A file set belongs to another record.",
                FailureKind::InvalidInput,
            ));
        }
        if row.try_get::<String, _>("component_kind")? != "action"
            || row
                .try_get::<Option<String>, _>("component_ref")?
                .as_deref()
                != Some("request_support")
            || row.try_get::<String, _>("purpose_key")? != "customer_support_evidence"
        {
            return Err(fail(
                "ATTACHMENT_BINDING_CONTEXT",
                "Choose only files prepared for this exact domain action and component.",
                FailureKind::InvalidInput,
            ));
        }
        if row.try_get::<String, _>("state")? != "draft" {
            return Err(fail(
                "ATTACHMENT_SET_LOCKED",
                "A file set was already bound or closed.",
                FailureKind::Conflict,
            ));
        }
        if row.try_get::<i64, _>("expected_target_version")? != version
            || row.try_get::<i64, _>("policy_version")? != 1
        {
            return Err(fail(
                "ATTACHMENT_SET_STALE",
                "Create a file set for the current record version.",
                FailureKind::Conflict,
            ));
        }
        let rows=sqlx::query("SELECT l.id,l.attachment_id,a.source_state,a.checksum_sha256,a.media_type,a.bytes FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id WHERE l.set_id=$1 AND s.company_id=$2 AND l.state='draft' ORDER BY l.created_at,l.id FOR UPDATE OF l").bind(id).bind(company.as_str()).fetch_all(&mut *c).await?;
        if rows.is_empty() {
            return Err(fail(
                "ATTACHMENT_SET_EMPTY",
                "Bind at least one available file.",
                FailureKind::InvalidInput,
            ));
        }
        let mut links = Vec::new();
        for row in rows {
            let source: Option<String> = row.try_get("attachment_id")?;
            let state: Option<String> = row.try_get("source_state")?;
            if source.is_none() || state.as_deref() != Some("available") {
                return Err(fail(
                    "ATTACHMENT_SCAN_PENDING",
                    "Every active file must pass malware scanning before this action.",
                    FailureKind::Conflict,
                ));
            }
            let source = source.unwrap();
            // Lock the actual source before returning the snapshot.
            sqlx::query("SELECT id FROM attachments WHERE id=$1 AND company_id=$2 FOR UPDATE")
                .bind(&source)
                .bind(company.as_str())
                .fetch_one(&mut *c)
                .await?;
            links.push(PortalEvidenceManifest {
                link_id: row.try_get("id")?,
                source_id: source,
                purpose_key: "customer_support_evidence".into(),
                checksum_sha256: row
                    .try_get::<Option<String>, _>("checksum_sha256")?
                    .ok_or_else(changed)?,
                media_type: row
                    .try_get::<Option<String>, _>("media_type")?
                    .ok_or_else(changed)?,
                bytes: row
                    .try_get::<Option<i64>, _>("bytes")?
                    .ok_or_else(changed)?,
            });
        }
        let required = row.try_get::<i64, _>("required_count")?;
        if (links.len() as i64) < required {
            let required = u8::try_from(required)
                .ok()
                .filter(|n| (1..=100).contains(n))
                .ok_or_else(changed)?;
            return Err(PgTransactionError::Business(
                PortalSupportEvidenceError::RequiredCount { required },
            ));
        }
        sets.push(Set {
            id: id.clone(),
            links,
        });
    }
    Ok(PortalSupportEvidence {
        unit,
        company: company.as_str().into(),
        target: target.into(),
        version,
        prepared_at,
        sets,
    })
}
/// Recheck prepared Files facts and bind on the original unit; caller owns all authority.
pub async fn bind_portal_support_evidence(
    c: &mut PgConnection,
    w: &PortalSupportEvidence,
    actor: &str,
    key: &str,
    action_id: &str,
) -> Result<(), Error> {
    if unit(c).await? != w.unit {
        return Err(changed());
    }
    // A savepoint rollback may release locks while preserving the transaction ID.
    // Never reacquire after losing the pre-snapshot fence and trust an old witness.
    let held:i64=sqlx::query_scalar("SELECT count(DISTINCT relation) FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND mode='ShareRowExclusiveLock' AND relation IN ('attachments'::regclass,'upload_intents'::regclass,'attachment_sets'::regclass,'attachment_links'::regclass,'attachment_events'::regclass)").fetch_one(&mut *c).await?;
    if held != 5 {
        return Err(changed());
    }
    let ids: Vec<String> = w.sets.iter().map(|s| s.id.clone()).collect();
    let current = prepare_portal_support_evidence(
        c,
        &CompanyId::new(w.company.clone()).map_err(|_| changed())?,
        &w.target,
        w.version,
        &ids,
        w.prepared_at,
    )
    .await?;
    if current.manifest() != w.manifest() {
        return Err(changed());
    }
    let mut cumulative = Vec::new();
    for set in &w.sets {
        for link in &set.links {
            let n=sqlx::query("UPDATE attachment_links SET state='bound',bound_at=$1,binding_type='customer_portal_support_request',binding_id=$2 WHERE id=$3 AND set_id=$4 AND state='draft'").bind(w.prepared_at).bind(action_id).bind(&link.link_id).bind(&set.id).execute(&mut *c).await?.rows_affected();
            if n != 1 {
                return Err(changed());
            }
            cumulative.push(link.clone());
        }
        let n=sqlx::query("UPDATE attachment_sets SET state='bound',bound_at=$1,bound_command_id=$2,updated_at=$1 WHERE id=$3 AND company_id=$4 AND state='draft'").bind(w.prepared_at).bind(key).bind(&set.id).bind(&w.company).execute(&mut *c).await?.rows_affected();
        if n != 1 {
            return Err(changed());
        }
        let hash = hex::encode(Sha256::digest(format!("{key}\0{}", set.id).as_bytes()));
        let id = format!("attachment-bind-{}", &hash[..32]);
        let details=serde_json::json!({"bindingType":"customer_portal_support_request","bindingId":action_id,"manifest":cumulative}).to_string();
        sqlx::query("INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,'set.bound',$5,$6)").bind(id).bind(&w.company).bind(&set.id).bind(actor).bind(details).bind(w.prepared_at).execute(&mut *c).await?;
    }
    Ok(())
}
