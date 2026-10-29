//! Files-owned Count evidence on the caller's single Holding transaction.
use crate::domain::count_submission::{
    CountBindingFeatures, CountBindingLink, CountBindingSet, CountManifestItem,
    prepare_count_binding, unique_set_ids,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, HoldingId};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection};
use std::future::Future;
type Error = PgTransactionError<BusinessError>;
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "ATTACHMENT_SET_CHANGED",
        "The original Count evidence changed.",
    )
    .into()
}
fn limit() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "ATTACHMENT_SOURCE_LIMIT",
        "The Count evidence exceeds its bounded source budget.",
    )
    .into()
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CountBindingFacts {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub actor_id: String,
    pub target_type: String,
    pub target_id: String,
    pub target_version: i64,
    pub command_id: String,
    pub set_ids: Vec<String>,
    pub features: CountBindingFeatures,
    pub now: i64,
}
/// Mandatory real Inventory/Records/IAM/Workflow owner source, never an
/// implementation that merely accepts a caller's booleans or request DTO.
pub trait PgCountBindingOwner: Sync {
    fn facts(&self) -> &CountBindingFacts;
    fn require_before(
        &self,
        c: &mut PgConnection,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn require_after(&self, c: &mut PgConnection)
    -> impl Future<Output = Result<(), Error>> + Send;
    /// Verify the original Workflow receipt and current authority for replay.
    fn require_replay(
        &self,
        c: &mut PgConnection,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
#[derive(Clone, PartialEq, Eq)]
struct Unit(String, i32, String, String);
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FILES_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("FILES_SERIALIZABLE_REQUIRED").into());
    }
    let (a,b,d,e)=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?;
    Ok(Unit(a, b, d, e))
}
/// Acquire before the first protected snapshot and in the enclosing owner bundle's lock order.
pub async fn lock_count_binding_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FILES_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE attachment_sets,attachment_links,attachments,attachment_events IN SHARE ROW EXCLUSIVE MODE").execute(c).await?;
    Ok(())
}
/// Files-owned scoped target-type facts for the Records selection policy. No
/// result is a target authorization; Records must check completeness/mixed types.
pub async fn read_count_binding_target_types(
    c: &mut PgConnection,
    company: &CompanyId,
    ids: &[String],
) -> Result<Vec<(String, String)>, Error> {
    unit(c).await?;
    let ids = unique_set_ids(ids);
    if ids.len() > 20 {
        return Err(limit());
    }
    Ok(sqlx::query_as("SELECT id,target_type FROM attachment_sets WHERE company_id=$1 AND id=ANY($2::text[]) ORDER BY id").bind(company.as_str()).bind(ids).fetch_all(c).await?)
}
#[derive(Clone, PartialEq)]
struct Image {
    set: Value,
    links: Vec<Value>,
    events: Vec<Value>,
}
async fn image(c: &mut PgConnection, company: &CompanyId, id: &str) -> Result<Image, Error> {
    // Limit output inside PostgreSQL; retain complete rows including retention,
    // ACL, source metadata and all set event history.
    let raw:Option<Option<String>>=sqlx::query_scalar(r#"WITH s AS MATERIALIZED (SELECT * FROM attachment_sets WHERE id=$1 AND company_id=$2), l AS MATERIALIZED (SELECT l, a FROM attachment_links l JOIN s ON s.id=l.set_id LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id ORDER BY l.created_at,l.id LIMIT 1001), e AS MATERIALIZED (SELECT e FROM attachment_events e JOIN s ON s.id=e.set_id WHERE e.company_id=s.company_id ORDER BY e.id LIMIT 20001), img AS (SELECT jsonb_build_object('set',to_jsonb(s),'links',COALESCE((SELECT jsonb_agg(jsonb_build_object('link',to_jsonb(l.l),'source',to_jsonb(l.a)) ORDER BY (l.l).created_at,(l.l).id) FROM l),'[]'::jsonb),'events',COALESCE((SELECT jsonb_agg(to_jsonb(e.e) ORDER BY (e.e).id) FROM e),'[]'::jsonb)) value FROM s) SELECT CASE WHEN (SELECT count(*) FROM l)<=1000 AND (SELECT count(*) FROM e)<=20000 AND octet_length(value::text)<=4194304 THEN value::text END FROM img"#).bind(id).bind(company.as_str()).fetch_optional(c).await?;
    let raw = raw
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::InvalidInput,
                "ATTACHMENT_BINDING_CONTEXT",
                "Choose files for this exact owner and action.",
            )
        })?
        .ok_or_else(limit)?;
    let v: Value = serde_json::from_str(&raw).map_err(|_| changed())?;
    Ok(Image {
        set: v["set"].clone(),
        links: v["links"].as_array().ok_or_else(changed)?.clone(),
        events: v["events"].as_array().ok_or_else(changed)?.clone(),
    })
}
fn add_image_budget(total: &mut usize, item: &Image) -> Result<(), Error> {
    for value in [&item.set, &json!(item.links), &json!(item.events)] {
        *total = total
            .checked_add(serde_json::to_vec(value).map_err(|_| changed())?.len())
            .ok_or_else(limit)?;
    }
    if *total > 4 * 1024 * 1024 {
        return Err(limit());
    }
    Ok(())
}
fn text<'a>(v: &'a Value, k: &str) -> Result<&'a str, Error> {
    v[k].as_str().ok_or_else(changed)
}
fn optional(v: &Value, k: &str) -> Option<String> {
    v[k].as_str().map(str::to_owned)
}
fn data(img: &Image) -> Result<CountBindingSet, Error> {
    let s = &img.set;
    let compatibility_created = img.events.iter().any(|e| {
        e["action"] == "set.created"
            && e["details_json"]
                .as_str()
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                .is_some_and(|d| d["compatibilityAdapter"] == true)
    });
    let mut links = Vec::new();
    for x in &img.links {
        let l = &x["link"];
        let a = &x["source"];
        links.push(CountBindingLink {
            id: text(l, "id")?.into(),
            state: text(l, "state")?.into(),
            source_id: optional(l, "attachment_id"),
            source_state: optional(a, "source_state"),
            checksum: optional(a, "checksum_sha256"),
            media_type: optional(a, "media_type"),
            bytes: a["bytes"].as_i64(),
        });
    }
    Ok(CountBindingSet {
        id: text(s, "id")?.into(),
        target_type: text(s, "target_type")?.into(),
        target_id: text(s, "target_id")?.into(),
        purpose: text(s, "purpose_key")?.into(),
        component: text(s, "component_kind")?.into(),
        component_ref: optional(s, "component_ref"),
        state: text(s, "state")?.into(),
        expected_version: s["expected_target_version"].as_i64().ok_or_else(changed)?,
        policy_version: s["policy_version"].as_i64().ok_or_else(changed)?,
        required_count: s["required_count"].as_i64().ok_or_else(changed)?,
        compatibility_created,
        links,
    })
}
fn binding_id(f: &CountBindingFacts) -> String {
    format!("count-submit-{}", f.command_id)
}
fn event_id(key: &str, id: &str) -> String {
    format!(
        "attachment-bind-{}",
        &hex::encode(Sha256::digest(format!("{key}\0{id}").as_bytes()))[..32]
    )
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BindingDetails<'a> {
    binding_type: &'static str,
    binding_id: String,
    manifest: &'a [CountManifestItem],
}
fn details(f: &CountBindingFacts, prefix: &[CountManifestItem]) -> Result<String, Error> {
    Ok(serde_json::to_string(&BindingDetails {
        binding_type: "stock_count_submission",
        binding_id: binding_id(f),
        manifest: prefix,
    })
    .map_err(|_| changed())?
    .replace('<', "\\u003c"))
}
fn expected_before(f: &CountBindingFacts, images: &[Image]) -> Result<Vec<Image>, Error> {
    let sets = images.iter().map(data).collect::<Result<Vec<_>, _>>()?;
    let prefixes = prepare_count_binding(
        &sets,
        &f.target_type,
        &f.target_id,
        f.target_version,
        &f.features,
    )?;
    let mut expected = images.to_vec();
    for (i, img) in expected.iter_mut().enumerate() {
        img.set["state"] = json!("bound");
        img.set["bound_at"] = json!(f.now);
        img.set["bound_command_id"] = json!(f.command_id);
        img.set["updated_at"] = json!(f.now);
        for link in &mut img.links {
            if link["link"]["state"] == "draft" {
                link["link"]["state"] = json!("bound");
                link["link"]["bound_at"] = json!(f.now);
                link["link"]["binding_type"] = json!("stock_count_submission");
                link["link"]["binding_id"] = json!(binding_id(f));
            }
        }
        img.events.push(json!({"id":event_id(&f.command_id,&sets[i].id),"company_id":f.company_id.as_str(),"attachment_id":null,"link_id":null,"set_id":sets[i].id,"actor_id":f.actor_id,"action":"set.bound","details_json":details(f,&prefixes[i])?,"created_at":f.now}));
        img.events
            .sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    }
    Ok(expected)
}
/// Prepared from the actual Files before-image. Empty IDs are handled by Records.
pub struct PreparedCountBinding<'a, O: PgCountBindingOwner> {
    owner: &'a O,
    facts: CountBindingFacts,
    unit: Unit,
    before: Vec<Image>,
    after: Vec<Image>,
    manifest: Vec<CountManifestItem>,
}
pub async fn prepare_count_submission<'a, O: PgCountBindingOwner>(
    c: &mut PgConnection,
    owner: &'a O,
) -> Result<PreparedCountBinding<'a, O>, Error> {
    owner.require_before(c).await?;
    let u = unit(c).await?;
    let mut f = owner.facts().clone();
    f.set_ids = unique_set_ids(&f.set_ids);
    if f.set_ids.is_empty()
        || f.set_ids.len() > 20
        || f.target_version <= 0
        || f.actor_id.is_empty()
        || f.command_id.is_empty()
        || f.target_id.is_empty()
        || !(0..=9_007_199_254_740_991).contains(&f.now)
    {
        return Err(changed());
    }
    let mut before = Vec::new();
    let mut total = 0usize;
    for id in &f.set_ids {
        let item = image(c, &f.company_id, id).await?;
        add_image_budget(&mut total, &item)?;
        before.push(item);
    }
    let sets = before.iter().map(data).collect::<Result<Vec<_>, _>>()?;
    let prefixes = prepare_count_binding(
        &sets,
        &f.target_type,
        &f.target_id,
        f.target_version,
        &f.features,
    )?;
    let manifest = prefixes.last().cloned().ok_or_else(changed)?;
    let after = expected_before(&f, &before)?;
    owner.require_before(c).await?;
    let mut again = owner.facts().clone();
    again.set_ids = unique_set_ids(&again.set_ids);
    if f != again || unit(c).await? != u {
        return Err(changed());
    }
    Ok(PreparedCountBinding {
        owner,
        facts: f,
        unit: u,
        before,
        after,
        manifest,
    })
}
impl<'a, O: PgCountBindingOwner> PreparedCountBinding<'a, O> {
    pub fn manifest(&self) -> &[CountManifestItem] {
        &self.manifest
    }
    pub async fn apply(self, c: &mut PgConnection) -> Result<RetainedCountBinding<'a, O>, Error> {
        self.owner.require_before(c).await?;
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let mut f = self.owner.facts().clone();
        f.set_ids = unique_set_ids(&f.set_ids);
        if f != self.facts {
            return Err(changed());
        }
        for (i, id) in f.set_ids.iter().enumerate() {
            if image(c, &f.company_id, id).await? != self.before[i] {
                return Err(changed());
            }
        }
        // The retained Files policy-version assertion is an effect guard,
        // rather than an earlier preparation-time policy decision.
        if self.before.iter().any(|img| img.set["policy_version"] != 1) {
            return Err(changed());
        }
        for (i, id) in f.set_ids.iter().enumerate() {
            for entry in &self.before[i].links {
                let link = &entry["link"];
                if link["state"] == "draft" {
                    let result=sqlx::query("UPDATE attachment_links SET state='bound',bound_at=$1,binding_type='stock_count_submission',binding_id=$2 WHERE id=$3 AND set_id=$4 AND state='draft'").bind(f.now).bind(binding_id(&f)).bind(text(link,"id")?).bind(id).execute(&mut *c).await?;
                    if result.rows_affected() != 1 {
                        return Err(changed());
                    }
                }
            }
            let result=sqlx::query("UPDATE attachment_sets SET state='bound',bound_at=$1,bound_command_id=$2,updated_at=$1 WHERE id=$3 AND company_id=$4 AND state='draft'").bind(f.now).bind(&f.command_id).bind(id).bind(f.company_id.as_str()).execute(&mut *c).await?;
            if result.rows_affected() != 1 {
                return Err(changed());
            }
            let event = self.after[i]
                .events
                .iter()
                .find(|e| e["id"] == event_id(&f.command_id, id))
                .ok_or_else(changed)?;
            sqlx::query("INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,'set.bound',$5,$6)").bind(event_id(&f.command_id,id)).bind(f.company_id.as_str()).bind(id).bind(&f.actor_id).bind(text(event,"details_json")?).bind(f.now).execute(&mut *c).await?;
        }
        let receipt = RetainedCountBinding {
            owner: self.owner,
            facts: f,
            unit: self.unit,
            after: self.after,
            manifest: self.manifest,
        };
        receipt.require_files_current(c).await?;
        Ok(receipt)
    }
}
pub struct RetainedCountBinding<'a, O: PgCountBindingOwner> {
    owner: &'a O,
    facts: CountBindingFacts,
    unit: Unit,
    after: Vec<Image>,
    manifest: Vec<CountManifestItem>,
}
impl<O: PgCountBindingOwner> RetainedCountBinding<'_, O> {
    pub fn manifest(&self) -> &[CountManifestItem] {
        &self.manifest
    }
    pub async fn require_files_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let mut facts = self.owner.facts().clone();
        facts.set_ids = unique_set_ids(&facts.set_ids);
        if facts != self.facts {
            return Err(changed());
        }
        for (i, id) in self.facts.set_ids.iter().enumerate() {
            if image(c, &self.facts.company_id, id).await? != self.after[i] {
                return Err(changed());
            }
        }
        Ok(())
    }
    pub async fn require_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_files_current(c).await?;
        self.owner.require_after(c).await?;
        self.require_files_current(c).await
    }
}

/// Replay checks immutable binding evidence against a manifest from the real
/// Workflow receipt. It writes nothing and grants no independent access.
pub async fn verify_count_submission_replay<O: PgCountBindingOwner>(
    c: &mut PgConnection,
    owner: &O,
    receipt_manifest: &[CountManifestItem],
) -> Result<(), Error> {
    owner.require_replay(c).await?;
    let u = unit(c).await?;
    let mut f = owner.facts().clone();
    f.set_ids = unique_set_ids(&f.set_ids);
    if f.set_ids.is_empty() || f.set_ids.len() > 20 || receipt_manifest.is_empty() {
        return Err(changed());
    }
    let mut prefix = Vec::new();
    let mut total = 0usize;
    for id in &f.set_ids {
        let img = image(c, &f.company_id, id).await?;
        add_image_budget(&mut total, &img)?;
        let set = data(&img)?;
        if set.target_id != f.target_id
            || set.target_type != f.target_type
            || set.component != "action"
            || set
                .component_ref
                .as_deref()
                .map(crate::domain::count_submission::trim_js)
                != Some("submit")
            || set.expected_version != f.target_version
            || set.state != "bound"
            || img.set["bound_command_id"] != f.command_id
            || set.policy_version != 1
            || !match f.target_type.as_str() {
                "stock_count" => matches!(
                    set.purpose.as_str(),
                    "count_sheet" | "count_photo" | "count_evidence"
                ),
                "inventory_stock_count" => set.purpose == "count_evidence",
                _ => false,
            }
        {
            return Err(changed());
        }
        let bound_at = img.set["bound_at"].as_i64().ok_or_else(changed)?;
        if !(0..=9_007_199_254_740_991).contains(&bound_at) {
            return Err(changed());
        }
        let mut count = 0usize;
        for x in &img.links {
            let l = &x["link"];
            let a = &x["source"];
            if l["state"] == "bound"
                && l["binding_type"] == "stock_count_submission"
                && l["binding_id"] == binding_id(&f)
            {
                if l["bound_at"] != bound_at {
                    return Err(changed());
                }
                count += 1;
                prefix.push(CountManifestItem {
                    link_id: text(l, "id")?.into(),
                    source_id: text(l, "attachment_id")?.into(),
                    purpose_key: set.purpose.clone(),
                    checksum_sha256: text(a, "checksum_sha256")?.into(),
                    media_type: text(a, "media_type")?.into(),
                    bytes: a["bytes"].as_i64().ok_or_else(changed)?,
                });
            }
        }
        if count == 0
            || (count as i64) < set.required_count
            || prefix.len() > receipt_manifest.len()
            || prefix != receipt_manifest[..prefix.len()]
        {
            return Err(changed());
        }
        let expected = json!({"id":event_id(&f.command_id,id),"company_id":f.company_id.as_str(),"attachment_id":null,"link_id":null,"set_id":id,"actor_id":f.actor_id,"action":"set.bound","details_json":details(&f,&prefix)?,"created_at":bound_at});
        if !img.events.iter().any(|e| e == &expected) {
            return Err(changed());
        }
    }
    if prefix != receipt_manifest || unit(c).await? != u {
        return Err(changed());
    }
    owner.require_replay(c).await
}
