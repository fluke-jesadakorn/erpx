//! Staged Commercial shadow read observation for a writer transaction.
//! This is telemetry, not an admission or authorization proof. The legacy
//! checked-phase observation is a separate, currently unimplemented contract.
use super::{canonical_purchase_write::CurrentCanonicalPurchaseWriteBoundary, postgres_purchases::PgCompanyPurchaseReader};
use crate::{application::purchases::CompanyPurchaseReadPort, domain::{packages::{CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode}, purchase_wire_revision::purchase_wire_revision}};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, HoldingId, TimestampMillis};
use serde::Serialize;
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseShadowReadObservation {
    event: &'static str,
    phase: &'static str,
    company_id: String,
    capability: String,
    purpose: CapabilityPurpose,
    code: &'static str,
    purchase_revision: String,
}

/// Invoke only at the legacy read phase, after acquiring every relevant owner
/// lock before the first snapshot. The boundary is not personal authority and
/// cannot establish the timing of locks. Disabled/enforced produce no event.
pub async fn inspect_purchase_shadow_read(
    connection: &mut PgConnection,
    boundary: &CurrentCanonicalPurchaseWriteBoundary<'_>,
    holding: &HoldingId,
    company: &CompanyId,
    catalogue: &CommercialCatalogue,
    capability: &str,
    purpose: CapabilityPurpose,
    mode: PackageEnforcementMode,
    now: TimestampMillis,
) -> Result<Option<PurchaseShadowReadObservation>, Error> {
    if mode != PackageEnforcementMode::Shadow { return Ok(None); }
    boundary.require_current(&mut *connection).await?;
    let facts = PgCompanyPurchaseReader::new(holding.clone(), company.clone(), &mut *connection, catalogue).read(company).await?;
    boundary.require_current(connection).await?;
    // The PostgreSQL JSON values pass through JavaScript Number in the legacy
    // read path, including bigint time values beyond Number.MAX_SAFE_INTEGER.
    let mut decision_grants = facts.grants.clone();
    for grant in &mut decision_grants {
        grant.starts_at = (grant.starts_at as f64) as i64;
        grant.ends_at = grant.ends_at.map(|value| (value as f64) as i64);
    }
    let decision = catalogue.purchased_capability(&decision_grants, company.as_str(), capability, purpose, (now.get() as f64) as i64, &facts.history);
    Ok(Some(PurchaseShadowReadObservation {
        event: "commercial.purchase.shadow", phase: "read", company_id: company.as_str().to_owned(),
        capability: capability.to_owned(), purpose, code: decision.code.as_code(),
        purchase_revision: purchase_wire_revision(company.as_str(), &facts.grants, &facts.history, now)?,
    }))
}

/// Logging failure is observational only, as in the legacy console sink.
pub fn write_purchase_shadow_read_observation(writer: &mut impl std::io::Write, observation: &PurchaseShadowReadObservation) {
    if serde_json::to_writer(&mut *writer, observation).is_ok() { let _ = writer.write_all(b"\n"); }
}
