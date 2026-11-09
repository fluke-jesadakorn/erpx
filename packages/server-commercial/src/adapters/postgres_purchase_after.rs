//! Re-evaluate the Commercial SQL predicate after effects. This is not actor
//! authorization, and must be called in the original fenced transaction.
use super::canonical_purchase_write::CurrentCanonicalPurchaseWriteBoundary;
use crate::domain::{packages::{CapabilityPurpose, CommercialCatalogue, CompanyPackageGrant, CompanyPackageHistoryRight, PackageEnforcementMode}, purchase_wire_revision::purchase_wire_revision};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use sqlx::{PgConnection, Postgres, QueryBuilder, Row};

type Error = PgTransactionError<BusinessError>;

/// Shadow observations require the composition to emit its normal telemetry.
pub struct PurchaseAfterDecision { pub allowed: bool, pub sampled_at: i64, pub shadow: Option<PurchaseShadowCheckedObservation>, mode: PackageEnforcementMode }
#[derive(Debug, serde::Serialize)]
#[serde(rename_all="camelCase")]
pub struct PurchaseShadowCheckedObservation {
    event: &'static str, phase: &'static str, company_id: String, capability: String,
    purpose: CapabilityPurpose, code: &'static str, purchase_revision: String,
}
/// A failed telemetry sink never changes the command outcome.
pub fn write_purchase_shadow_checked_observation(writer: &mut impl std::io::Write, observation: &PurchaseShadowCheckedObservation) {
    if serde_json::to_writer(&mut *writer, observation).is_ok() { let _ = writer.write_all(b"\n"); }
}
fn checked_observation(company: &str, capability: &str, purpose: CapabilityPurpose, now: i64, catalogue: &CommercialCatalogue, grants: serde_json::Value, history: serde_json::Value) -> Option<PurchaseShadowCheckedObservation> {
    fn string(v:&serde_json::Value,k:&str)->Option<String>{Some(v.get(k)?.as_str()?.to_owned())}
    fn number(v:&serde_json::Value,k:&str)->Option<i64>{v.get(k)?.as_i64()}
    let grants:Vec<CompanyPackageGrant>=grants.as_array()?.iter().map(|v|Some(CompanyPackageGrant{company_id:string(v,"companyId")?,package_id:string(v,"packageId")?,package_version:number(v,"packageVersion")?,state:string(v,"state")?,starts_at:number(v,"startsAt")?,ends_at:if v.get("endsAt")?.is_null(){None}else{Some(number(v,"endsAt")?)},version:number(v,"version")?,reason:String::new(),evidence:String::new()})).collect::<Option<_>>()?;
    let history:Vec<CompanyPackageHistoryRight>=history.as_array()?.iter().map(|v|Some(CompanyPackageHistoryRight{company_id:string(v,"companyId")?,package_id:string(v,"packageId")?,package_version:number(v,"packageVersion")?,source_grant_version:number(v,"sourceGrantVersion")?,source_event_id:string(v,"sourceEventId")?,retained_at:number(v,"retainedAt")?})).collect::<Option<_>>()?;
    let revision=purchase_wire_revision(company,&grants,&history,TimestampMillis::new(now)).ok()?;
    // The PostgreSQL JSON aggregate passes through JSON.parse before the JS
    // decision, including unsafe BIGINTs rounded to IEEE-754 Number.
    let mut decision_grants=grants;
    for g in &mut decision_grants {g.package_version=(g.package_version as f64) as i64;g.version=(g.version as f64) as i64;g.starts_at=(g.starts_at as f64) as i64;g.ends_at=g.ends_at.map(|v|(v as f64) as i64);}
    let mut decision_history=history;
    for h in &mut decision_history {h.package_version=(h.package_version as f64) as i64;h.source_grant_version=(h.source_grant_version as f64) as i64;h.retained_at=(h.retained_at as f64) as i64;}
    let code=catalogue.purchased_capability(&decision_grants,company,capability,purpose,(now as f64) as i64,&decision_history).code.as_code();
    Some(PurchaseShadowCheckedObservation{event:"commercial.purchase.shadow",phase:"checked",company_id:company.to_owned(),capability:capability.to_owned(),purpose,code,purchase_revision:revision})
}
impl PurchaseAfterDecision {
    pub fn require_allowed(&self) -> Result<(), BusinessError> {
        if self.mode == PackageEnforcementMode::Enforced && !self.allowed {
            return Err(BusinessError::new(FailureKind::Conflict, "PACKAGE_CAPABILITY_CHANGED", "Package access changed. Reload and review before retrying."));
        }
        Ok(())
    }
}

fn history_source(q: &mut QueryBuilder<Postgres>) {
    q.push("SELECT package_id,package_version FROM public.company_package_grants WHERE company_id=s.company_id AND starts_at<=s.now_ms AND state<>'inactive' UNION SELECT package_id,package_version FROM public.company_package_history_rights WHERE company_id=s.company_id AND retained_at<=s.now_ms AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>''");
}
fn available(q: &mut QueryBuilder<Postgres>, alias: &'static str, purpose: CapabilityPurpose) {
    // alias is one of two code-owned constants, never an input identifier.
    q.push(alias).push(".starts_at<=s.now_ms");
    match purpose {
        CapabilityPurpose::NewWork => { q.push(" AND ").push(alias).push(".state='active' AND (").push(alias).push(".ends_at IS NULL OR ").push(alias).push(".ends_at>s.now_ms)"); }
        CapabilityPurpose::SettleExisting => { q.push(" AND ").push(alias).push(".state<>'inactive'"); }
        CapabilityPurpose::History => unreachable!(),
    }
}
/// The caller supplies a trusted, owner-derived Company and capability. Disabled
/// does no SQL. A missing fence is an error for the other modes. Invoke
/// `require_allowed` immediately at the post-effects guard; this does not attest
/// the earlier decision or arbitrary later writes.
pub async fn inspect_purchase_after(
    c: &mut PgConnection, boundary: Option<&CurrentCanonicalPurchaseWriteBoundary<'_>>,
    catalogue: &CommercialCatalogue, company: &CompanyId, capability: &str,
    purpose: CapabilityPurpose, mode: PackageEnforcementMode,
) -> Result<Option<PurchaseAfterDecision>, Error> {
    if mode == PackageEnforcementMode::Disabled { return Ok(None); }
    let boundary = boundary.ok_or_else(|| PersistenceError::new("COMMERCIAL_CANONICAL_WRITE_LOCKS_REQUIRED"))?;
    boundary.require_current(&mut *c).await?;
    let mut q = QueryBuilder::<Postgres>::new("WITH purchase_scope AS MATERIALIZED (SELECT ");
    q.push_bind(company.as_str().to_owned()).push("::text company_id, floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms) SELECT s.now_ms, (");
    let candidates: Vec<_> = catalogue.definitions().iter().filter(|d| d.visible_capabilities.iter().chain(&d.included_capabilities).any(|v| v == capability)).collect();
    if candidates.is_empty() { q.push("FALSE"); }
    for (i,d) in candidates.iter().enumerate() {
        if i > 0 { q.push(" OR "); }
        q.push("(");
        if purpose == CapabilityPurpose::History {
            q.push("EXISTS(SELECT 1 FROM ("); history_source(&mut q);
            q.push(") purchase WHERE purchase.package_id=").push_bind(d.id.clone()).push(" AND purchase.package_version=").push_bind(d.version).push(")");
        } else {
            q.push("EXISTS(SELECT 1 FROM public.company_package_grants purchase WHERE purchase.company_id=s.company_id AND purchase.package_id=").push_bind(d.id.clone()).push(" AND purchase.package_version=").push_bind(d.version).push(" AND ");
            available(&mut q,"purchase",purpose);
        }
        for dependency in &d.dependencies {
            if purpose == CapabilityPurpose::History { q.push(" AND EXISTS(SELECT 1 FROM ("); history_source(&mut q); q.push(") parent WHERE parent.package_id="); }
            else { q.push(" AND EXISTS(SELECT 1 FROM public.company_package_grants parent WHERE parent.company_id=s.company_id AND parent.package_id="); }
            q.push_bind(dependency.clone()).push(" AND parent.package_version IN (");
            let versions: Vec<_> = catalogue.definitions().iter().filter(|p| &p.id == dependency).map(|p| p.version).collect();
            for (j,version) in versions.iter().enumerate() { if j > 0 { q.push(","); } q.push_bind(*version); }
            q.push(")");
            if purpose != CapabilityPurpose::History { q.push(" AND "); available(&mut q,"parent",purpose); }
            q.push(")");
        }
        // Non-history dependencies are nested inside the purchase EXISTS.
        if purpose != CapabilityPurpose::History { q.push(")"); }
        q.push(")");
    }
    q.push(") allowed");
    if mode == PackageEnforcementMode::Shadow {
        q.push(", COALESCE((SELECT jsonb_agg(jsonb_build_object('companyId',g.company_id,'packageId',g.package_id,'packageVersion',g.package_version,'state',g.state,'startsAt',g.starts_at,'endsAt',g.ends_at,'version',g.version)) FROM public.company_package_grants g WHERE g.company_id=s.company_id),'[]'::jsonb) grants, COALESCE((SELECT jsonb_agg(jsonb_build_object('companyId',h.company_id,'packageId',h.package_id,'packageVersion',h.package_version,'sourceGrantVersion',h.source_grant_version,'sourceEventId',h.source_event_id,'retainedAt',h.retained_at)) FROM public.company_package_history_rights h WHERE h.company_id=s.company_id),'[]'::jsonb) history");
    }
    q.push(" FROM purchase_scope s");
    let row = q.build().fetch_one(&mut *c).await?;
    let sampled_at = row.try_get("now_ms")?;
    let shadow = if mode == PackageEnforcementMode::Shadow { row.try_get::<serde_json::Value,_>("grants").ok().zip(row.try_get::<serde_json::Value,_>("history").ok()).and_then(|(g,h)|checked_observation(company.as_str(),capability,purpose,sampled_at,catalogue,g,h)) } else { None };
    let decision = PurchaseAfterDecision { sampled_at, allowed: row.try_get("allowed")?, shadow, mode };
    boundary.require_current(c).await?;
    Ok(Some(decision))
}
