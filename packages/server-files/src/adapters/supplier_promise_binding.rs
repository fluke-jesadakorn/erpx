//! Files-owned Supplier Promise evidence. Scope parameters never grant target,
//! session or purchase authority; the enclosing owner must provide those checks.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};
use std::collections::HashSet;
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SupplierPromiseEvidenceError {
    Business(BusinessError),
    RequiredCount { required: i64 },
}
impl From<BusinessError> for SupplierPromiseEvidenceError {
    fn from(value: BusinessError) -> Self {
        Self::Business(value)
    }
}
type Error = PgTransactionError<SupplierPromiseEvidenceError>;
type Unit = (String, i32, String, String);
fn fail(code: &'static str, message: &'static str, kind: FailureKind) -> Error {
    PgTransactionError::Business(SupplierPromiseEvidenceError::Business(BusinessError::new(
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
        return Err(PersistenceError::new("SUPPLIER_PROMISE_EVIDENCE_UNIT_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplierPromiseManifest {
    pub link_id: String,
    pub source_id: String,
    pub purpose_key: String,
    pub checksum_sha256: String,
    pub media_type: String,
    pub bytes: i64,
}
#[derive(Debug, Clone)]
struct Set {
    id: String,
    policy_version: i64,
    links: Vec<SupplierPromiseManifest>,
}
#[derive(Debug, Clone)]
pub struct SupplierPromiseEvidence {
    unit: Unit,
    company: String,
    target: String,
    version: i64,
    prepared_at: i64,
    sets: Vec<Set>,
    owner_facts: Option<SupplierPromiseEvidenceFacts>,
    features: Option<SupplierEvidenceFeatures>,
}
impl SupplierPromiseEvidence {
    pub fn manifest(&self) -> Vec<SupplierPromiseManifest> {
        self.sets.iter().flat_map(|s| s.links.clone()).collect()
    }
}
/// Reads and locks Files rows only; never writes or establishes owner authority.
async fn prepare_supplier_promise_evidence_inner(
    c: &mut PgConnection,
    company: &CompanyId,
    target: &str,
    version: i64,
    ids: &[String],
    prepared_at: i64,
) -> Result<SupplierPromiseEvidence, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("SUPPLIER_PROMISE_EVIDENCE_UNIT_REQUIRED").into());
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
        if row.try_get::<String, _>("target_type")? != "purchase_order"
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
                != Some("supplier-promise")
            || row.try_get::<String, _>("purpose_key")? != "supplier_promise"
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
        if row.try_get::<i64, _>("expected_target_version")? != version {
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
            links.push(SupplierPromiseManifest {
                link_id: row.try_get("id")?,
                source_id: source,
                purpose_key: "supplier_promise".into(),
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
            return Err(PgTransactionError::Business(
                SupplierPromiseEvidenceError::RequiredCount { required },
            ));
        }
        sets.push(Set {
            id: id.clone(),
            policy_version: row.try_get("policy_version")?,
            links,
        });
    }
    Ok(SupplierPromiseEvidence {
        unit,
        company: company.as_str().into(),
        target: target.into(),
        version,
        prepared_at,
        sets,
        owner_facts: None,
        features: None,
    })
}
/// Recheck prepared Files facts and bind on the original unit; caller owns all authority.
async fn bind_supplier_promise_evidence_inner(
    c: &mut PgConnection,
    w: &SupplierPromiseEvidence,
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
    let mut cumulative = Vec::new();
    for set in &w.sets {
        for link in &set.links {
            let n=sqlx::query("UPDATE attachment_links SET state='bound',bound_at=$1,binding_type='supplier_promise',binding_id=$2 WHERE id=$3 AND set_id=$4 AND state='draft'").bind(w.prepared_at).bind(action_id).bind(&link.link_id).bind(&set.id).execute(&mut *c).await?.rows_affected();
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
        let details = format!(
            "{{\"bindingType\":\"supplier_promise\",\"bindingId\":{},\"manifest\":{}}}",
            serde_json::to_string(action_id).map_err(|_| changed())?,
            serde_json::to_string(&cumulative).map_err(|_| changed())?
        )
        .replace('<', "\\u003c");
        sqlx::query("INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,'set.bound',$5,$6)").bind(id).bind(&w.company).bind(&set.id).bind(actor).bind(details).bind(w.prepared_at).execute(&mut *c).await?;
    }
    Ok(())
}

/// These checks are implemented by composition with real Commerce,
/// Procurement, IAM and purchase witnesses. This trait and its inputs confer
/// no permission; the implementing owner must retain all underlying proofs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupplierPromiseEvidenceFacts {
    pub company_id: String,
    pub order_id: String,
    pub order_version: i64,
    pub actor_id: String,
    pub command_key: String,
    pub promise_id: String,
}
pub trait SupplierPromiseEvidenceOwner: Sync {
    fn facts(&self) -> &SupplierPromiseEvidenceFacts;
    fn features(&self) -> SupplierEvidenceFeatures;
    fn require_before(
        &self,
        c: &mut PgConnection,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send;
    fn require_after(
        &self,
        c: &mut PgConnection,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send;
}
/// Provider facts are obtained from the trusted process configuration by the
/// composition, not from the request or an uploaded attachment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SupplierEvidenceFeatures {
    pub enabled: bool,
    pub read_only: bool,
    pub uploads_frozen: bool,
}
/// Acquire in the global order before any source snapshot.
pub async fn lock_supplier_promise_evidence_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("SUPPLIER_PROMISE_EVIDENCE_UNIT_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE attachments,upload_intents,attachment_sets,attachment_links,attachment_events IN SHARE ROW EXCLUSIVE MODE").execute(c).await?;
    Ok(())
}
/// The external owner must supply current authority and an actual order witness.
/// The result alone never grants access or permission to change the order.
pub async fn prepare_supplier_promise_evidence<O: SupplierPromiseEvidenceOwner>(
    c: &mut PgConnection,
    owner: &O,
    company: &CompanyId,
    target: &str,
    version: i64,
    ids: &[String],
    prepared_at: i64,
) -> Result<SupplierPromiseEvidence, Error> {
    let features = owner.features();
    let _unit = unit(c).await?;
    let locked:i64=sqlx::query_scalar("SELECT count(DISTINCT relation) FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND relation IN ('attachments'::regclass,'upload_intents'::regclass,'attachment_sets'::regclass,'attachment_links'::regclass,'attachment_events'::regclass)").fetch_one(&mut *c).await?;
    if locked != 5 {
        return Err(changed());
    }
    let facts = owner.facts();
    if facts.company_id != company.as_str()
        || facts.order_id != target
        || facts.order_version != version
    {
        return Err(changed());
    }
    owner.require_before(c).await?;
    let mut seen = HashSet::new();
    let distinct: Vec<String> = ids
        .iter()
        .filter(|id| seen.insert(id.as_str()))
        .cloned()
        .collect();
    if distinct.is_empty() || distinct.len() > 10 {
        return Err(fail(
            "ATTACHMENT_SETS_REQUIRED",
            "Choose one to twenty file sets.",
            FailureKind::InvalidInput,
        ));
    }
    // Legacy `assertPromiseEvidence` runs before the generic Files feature gates.
    let rows=sqlx::query("SELECT id,purpose_key,component_kind,component_ref FROM attachment_sets WHERE company_id=$1 AND id=ANY($2)").bind(company.as_str()).bind(&distinct).fetch_all(&mut *c).await?;
    if rows.len() != distinct.len()
        || rows.iter().any(|r| {
            r.try_get::<String, _>("purpose_key").ok().as_deref() != Some("supplier_promise")
                || r.try_get::<String, _>("component_kind").ok().as_deref() != Some("action")
                || r.try_get::<Option<String>, _>("component_ref")
                    .ok()
                    .flatten()
                    .as_deref()
                    != Some("supplier-promise")
        })
    {
        return Err(fail(
            "SUPPLIER_PROMISE_ATTACHMENT_PURPOSE",
            "Choose only supplier promise evidence prepared for this confirmation.",
            FailureKind::InvalidInput,
        ));
    }
    if !features.enabled {
        let marked:i64=sqlx::query_scalar("SELECT count(DISTINCT s.id) FROM attachment_sets s WHERE s.company_id=$1 AND s.id=ANY($2) AND EXISTS(SELECT 1 FROM attachment_events e WHERE e.set_id=s.id AND e.company_id=s.company_id AND e.action='set.created' AND COALESCE((e.details_json::jsonb->>'compatibilityAdapter')::boolean,false)=true)").bind(company.as_str()).bind(&distinct).fetch_one(&mut *c).await?;
        if marked != distinct.len() as i64 {
            return Err(fail(
                "ATTACHMENTS_DISABLED",
                "The attachment platform is not enabled for this release.",
                FailureKind::NotFound,
            ));
        }
        if features.read_only {
            return Err(fail(
                "ATTACHMENTS_READ_ONLY",
                "Attachment intake is disabled during rollback. Existing clean bound evidence remains readable.",
                FailureKind::Unavailable,
            ));
        }
    }
    if features.uploads_frozen {
        return Err(fail(
            "ATTACHMENT_UPLOAD_FREEZE",
            "File binding is briefly paused for storage reconciliation.",
            FailureKind::Unavailable,
        ));
    }
    let original_facts = owner.facts().clone();
    let mut result = prepare_supplier_promise_evidence_inner(
        c,
        company,
        target,
        version,
        &distinct,
        prepared_at,
    )
    .await?;
    owner.require_before(c).await?;
    if owner.facts() != &original_facts {
        return Err(changed());
    }
    if owner.features() != features {
        return Err(changed());
    }
    result.owner_facts = Some(original_facts);
    result.features = Some(features);
    Ok(result)
}
/// Opaque same-unit guard result; the caller runs the real Commerce source
/// guard next, then owner effects, then applies these prepared Files steps.
pub struct GuardedSupplierPromiseEvidence {
    evidence: SupplierPromiseEvidence,
    facts: SupplierPromiseEvidenceFacts,
}
fn guard_failed() -> Error {
    PersistenceError::new("SUPPLIER_PROMISE_ATTACHMENT_GUARD_FAILED").into()
}
pub async fn validate_supplier_promise_evidence_guards<O: SupplierPromiseEvidenceOwner>(
    c: &mut PgConnection,
    owner: &O,
    w: &SupplierPromiseEvidence,
) -> Result<GuardedSupplierPromiseEvidence, Error> {
    if unit(c).await? != w.unit || w.owner_facts.as_ref() != Some(owner.facts()) {
        return Err(guard_failed());
    }
    let held:i64=sqlx::query_scalar("SELECT count(DISTINCT relation) FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND relation IN ('attachments'::regclass,'upload_intents'::regclass,'attachment_sets'::regclass,'attachment_links'::regclass,'attachment_events'::regclass)").fetch_one(&mut *c).await?;
    if held != 5 {
        return Err(guard_failed());
    }
    let ids: Vec<String> = w.sets.iter().map(|s| s.id.clone()).collect();
    let current = prepare_supplier_promise_evidence_inner(
        c,
        &CompanyId::new(w.company.clone()).map_err(|_| guard_failed())?,
        &w.target,
        w.version,
        &ids,
        w.prepared_at,
    )
    .await
    .map_err(|_| guard_failed())?;
    if current.manifest() != w.manifest() || current.sets.iter().any(|s| s.policy_version != 1) {
        return Err(guard_failed());
    }
    Ok(GuardedSupplierPromiseEvidence {
        evidence: w.clone(),
        facts: owner.facts().clone(),
    })
}
/// Apply only after the caller's Commerce and Procurement effects. This does
/// not substitute for current IAM, purchase, or real source proofs.
pub async fn apply_supplier_promise_evidence<O: SupplierPromiseEvidenceOwner>(
    c: &mut PgConnection,
    owner: &O,
    g: &GuardedSupplierPromiseEvidence,
) -> Result<RetainedSupplierPromiseEvidence, Error> {
    let w = &g.evidence;
    if owner.facts() != &g.facts || w.owner_facts.as_ref() != Some(&g.facts) {
        return Err(changed());
    }
    bind_supplier_promise_evidence_inner(
        c,
        w,
        &g.facts.actor_id,
        &g.facts.command_key,
        &g.facts.promise_id,
    )
    .await?;
    // Experimental staged proof: an advisory transaction lock acquired after
    // the writes disappears if those writes are rolled back to a savepoint.
    // This extra lock is a mode/contention parity blocker for runtime mounting.
    let marker = applied_marker(c).await?;
    Ok(RetainedSupplierPromiseEvidence {
        evidence: w.clone(),
        facts: g.facts.clone(),
        marker,
    })
}
async fn marker_held(c: &mut PgConnection, marker: i64) -> Result<bool, Error> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype='advisory' AND pid=pg_backend_pid() AND database=(SELECT oid FROM pg_database WHERE datname=current_database()) AND classid::bigint=(($1::bigint >> 32) & 4294967295) AND objid::bigint=($1::bigint & 4294967295) AND objsubid=1 AND mode='ExclusiveLock' AND granted)")
        .bind(marker).fetch_one(c).await?)
}
async fn applied_marker(c: &mut PgConnection) -> Result<i64, Error> {
    for _ in 0..16 {
        let entropy: String = sqlx::query_scalar("SELECT gen_random_uuid()::text")
            .fetch_one(&mut *c).await?;
        let hash = Sha256::digest(entropy.as_bytes());
        let marker = i64::from_be_bytes(hash[..8].try_into().expect("eight bytes"));
        if marker_held(c, marker).await? { continue; }
        let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock($1)")
            .bind(marker).fetch_one(&mut *c).await?;
        if acquired { return Ok(marker); }
    }
    Err(PersistenceError::new("SUPPLIER_PROMISE_EVIDENCE_MARKER_UNAVAILABLE").into())
}
/// Optional diagnostic recheck; TS does not run this after-image check on bind.
/// Composition must preserve mode-dependent owner checks separately.
pub struct RetainedSupplierPromiseEvidence {
    evidence: SupplierPromiseEvidence,
    facts: SupplierPromiseEvidenceFacts,
    marker: i64,
}
impl RetainedSupplierPromiseEvidence {
    /// Verify only the applied binding's identity and transaction. This is not
    /// an after-image revalidation or an authorization proof.
    pub async fn require_applied(
        &self,
        c: &mut PgConnection,
        facts: &SupplierPromiseEvidenceFacts,
        set_ids: &[String],
    ) -> Result<(), Error> {
        let mut seen = HashSet::new();
        let expected: Vec<_> = set_ids.iter().filter(|id| seen.insert(id.as_str())).collect();
        if unit(c).await? != self.evidence.unit
            || &self.facts != facts
            || !self.evidence.sets.iter().map(|set| &set.id).eq(expected)
            || !marker_held(c, self.marker).await?
        {
            return Err(changed());
        }
        Ok(())
    }

    pub async fn require_current<O: SupplierPromiseEvidenceOwner>(
        &self,
        c: &mut PgConnection,
        owner: &O,
    ) -> Result<(), Error> {
        let w = &self.evidence;
        if unit(c).await? != w.unit || owner.facts() != &self.facts {
            return Err(changed());
        }
        let held:i64=sqlx::query_scalar("SELECT count(DISTINCT relation) FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND relation IN ('attachments'::regclass,'upload_intents'::regclass,'attachment_sets'::regclass,'attachment_links'::regclass,'attachment_events'::regclass)").fetch_one(&mut *c).await?;
        if held != 5 {
            return Err(changed());
        }
        owner.require_after(c).await?;
        let mut cumulative = Vec::new();
        for set in &w.sets {
            for item in &set.links {
                cumulative.push(item.clone());
                let ok:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id WHERE l.id=$1 AND l.set_id=$2 AND s.company_id=$3 AND l.state='bound' AND l.binding_type='supplier_promise' AND l.binding_id=$4 AND l.bound_at=$5 AND a.id=$6 AND a.source_state='available' AND a.checksum_sha256=$7 AND a.media_type=$8 AND a.bytes=$9)").bind(&item.link_id).bind(&set.id).bind(&w.company).bind(&self.facts.promise_id).bind(w.prepared_at).bind(&item.source_id).bind(&item.checksum_sha256).bind(&item.media_type).bind(item.bytes).fetch_one(&mut *c).await?;
                if !ok {
                    return Err(changed());
                }
            }
            let hash = hex::encode(Sha256::digest(
                format!("{}\0{}", self.facts.command_key, set.id).as_bytes(),
            ));
            let event_id = format!("attachment-bind-{}", &hash[..32]);
            let details=serde_json::json!({"bindingType":"supplier_promise","bindingId":self.facts.promise_id,"manifest":cumulative}).to_string().replace('<',"\\u003c");
            let ok:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachment_sets s JOIN attachment_events e ON e.set_id=s.id AND e.company_id=s.company_id WHERE s.id=$1 AND s.company_id=$2 AND s.target_type='purchase_order' AND s.target_id=$3 AND s.state='bound' AND s.expected_target_version=$4 AND s.bound_command_id=$5 AND s.bound_at=$6 AND e.id=$7 AND e.actor_id=$8 AND e.action='set.bound' AND e.details_json::jsonb=$9::jsonb AND e.created_at=$6)").bind(&set.id).bind(&w.company).bind(&w.target).bind(w.version).bind(&self.facts.command_key).bind(w.prepared_at).bind(event_id).bind(&self.facts.actor_id).bind(details).fetch_one(&mut *c).await?;
            if !ok {
                return Err(changed());
            }
        }
        owner.require_after(c).await
    }
}
