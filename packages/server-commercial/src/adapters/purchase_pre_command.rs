//! Pre-command Company purchase decision. This is not actor authorization.
use super::{canonical_purchase_write::CurrentCanonicalPurchaseWriteBoundary, postgres_purchases::PgCompanyPurchaseReader};
use crate::{application::purchases::CompanyPurchaseReadPort, domain::{packages::{CapabilityPurpose,CommercialCatalogue,PackageDecision,PackageEnforcementMode},purchase_wire_revision::purchase_wire_revision}};
use erpx_server_runtime::public::postgres::{PersistenceError,PgTransactionError};
use erpx_server_shared::public::{BusinessError,Clock,CompanyId,FailureKind,HoldingId};
use serde::Serialize;
use sqlx::PgConnection;
type Error=PgTransactionError<BusinessError>;

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct PurchasePreCommandShadow {event:&'static str,phase:&'static str,company_id:String,capability:String,purpose:CapabilityPurpose,code:&'static str,purchase_revision:String}
pub struct PurchasePreCommandDecision {pub decision:PackageDecision,pub shadow:Option<PurchasePreCommandShadow>,mode:PackageEnforcementMode}
impl PurchasePreCommandDecision {
    pub fn require_allowed(&self)->Result<(),BusinessError>{
        if self.mode==PackageEnforcementMode::Enforced && !self.decision.allowed {return Err(BusinessError::new(FailureKind::Forbidden,self.decision.code.as_code(),"This package does not permit the requested work."));} Ok(())
    }
}
pub fn write_purchase_pre_command_shadow(writer:&mut impl std::io::Write,observation:&PurchasePreCommandShadow){if serde_json::to_writer(&mut *writer,observation).is_ok(){let _=writer.write_all(b"\n");}}
/// Disabled makes no query or clock read. In other modes the caller supplies
/// trusted owner scope and acquires every source lock before the first snapshot.
/// The clock is sampled after the facts read, matching the TS read phase.
pub async fn inspect_purchase_pre_command(c:&mut PgConnection,boundary:Option<&CurrentCanonicalPurchaseWriteBoundary<'_>>,holding:&HoldingId,company:&CompanyId,catalogue:&CommercialCatalogue,capability:&str,purpose:CapabilityPurpose,mode:PackageEnforcementMode,clock:&(dyn Clock + Sync))->Result<Option<PurchasePreCommandDecision>,Error>{
    if mode==PackageEnforcementMode::Disabled{return Ok(None);}
    let boundary=boundary.ok_or_else(||PersistenceError::new("COMMERCIAL_CANONICAL_WRITE_LOCKS_REQUIRED"))?;
    boundary.require_current(&mut *c).await?;
    let facts=PgCompanyPurchaseReader::new(holding.clone(),company.clone(),&mut *c,catalogue).read(company).await?;
    let now=clock.now();boundary.require_current(c).await?;
    let mut grants=facts.grants.clone();let mut history=facts.history.clone();
    for g in &mut grants {g.package_version=(g.package_version as f64) as i64;g.version=(g.version as f64) as i64;g.starts_at=(g.starts_at as f64) as i64;g.ends_at=g.ends_at.map(|v|(v as f64) as i64);}
    for h in &mut history {h.package_version=(h.package_version as f64) as i64;h.source_grant_version=(h.source_grant_version as f64) as i64;h.retained_at=(h.retained_at as f64) as i64;}
    let decision=catalogue.purchased_capability(&grants,company.as_str(),capability,purpose,(now.get() as f64) as i64,&history);
    let shadow=if mode==PackageEnforcementMode::Shadow {Some(PurchasePreCommandShadow{event:"commercial.purchase.shadow",phase:"read",company_id:company.as_str().to_owned(),capability:capability.to_owned(),purpose,code:decision.code.as_code(),purchase_revision:purchase_wire_revision(company.as_str(),&facts.grants,&facts.history,now)?})}else{None};
    Ok(Some(PurchasePreCommandDecision{decision,shadow,mode}))
}
