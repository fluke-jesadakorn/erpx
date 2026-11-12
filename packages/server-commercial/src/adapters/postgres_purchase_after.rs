//! Re-evaluate the Commercial SQL predicate after effects. This is not actor
//! authorization, and must be called in the original fenced transaction.
pub use super::purchased_capability_sql::PurchaseShadowCheckedObservation;
use super::{
    canonical_purchase_write::CurrentCanonicalPurchaseWriteBoundary,
    purchased_capability_sql::{
        decision_and_observation, push_capability_predicate, push_shadow_facts,
    },
};
use crate::domain::packages::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{PgConnection, Postgres, QueryBuilder, Row};

type Error = PgTransactionError<BusinessError>;

/// Shadow observations require the composition to emit its normal telemetry.
pub struct PurchaseAfterDecision { pub allowed: bool, pub sampled_at: i64, pub shadow: Option<PurchaseShadowCheckedObservation>, mode: PackageEnforcementMode }
/// A failed telemetry sink never changes the command outcome.
pub fn write_purchase_shadow_checked_observation(writer: &mut impl std::io::Write, observation: &PurchaseShadowCheckedObservation) {
    if serde_json::to_writer(&mut *writer, observation).is_ok() { let _ = writer.write_all(b"\n"); }
}
impl PurchaseAfterDecision {
    pub fn require_allowed(&self) -> Result<(), BusinessError> {
        if self.mode == PackageEnforcementMode::Enforced && !self.allowed {
            return Err(BusinessError::new(FailureKind::Conflict, "PACKAGE_CAPABILITY_CHANGED", "Package access changed. Reload and review before retrying."));
        }
        Ok(())
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
    push_capability_predicate(&mut q, catalogue, capability, purpose);
    q.push(") allowed");
    if mode == PackageEnforcementMode::Shadow {
        push_shadow_facts(&mut q);
    }
    q.push(" FROM purchase_scope s");
    let row = q.build().fetch_one(&mut *c).await?;
    let sampled_at = row.try_get("now_ms")?;
    let shadow = if mode == PackageEnforcementMode::Shadow { row.try_get::<serde_json::Value,_>("grants").ok().zip(row.try_get::<serde_json::Value,_>("history").ok()).and_then(|(g,h)|decision_and_observation(company.as_str(),capability,purpose,sampled_at,catalogue,g,h).map(|(_,observation)|observation)) } else { None };
    let decision = PurchaseAfterDecision { sampled_at, allowed: row.try_get("allowed")?, shadow, mode };
    boundary.require_current(c).await?;
    Ok(Some(decision))
}
