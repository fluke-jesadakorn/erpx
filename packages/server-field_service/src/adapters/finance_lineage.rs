//! Minimal original parts lineage for an independent Finance reviewer. This
//! intentionally does not borrow operational Person, Agreement, Support, Asset,
//! technician, status, records.write or current stock eligibility.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, HoldingId};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

#[derive(Clone, Eq, PartialEq)]
pub struct FieldServicePartsFinanceRequest {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub order_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub minimum_order_version: i64,
    pub owner_key: String,
    pub consumption_id: String,
    pub journal_id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub creator_user_id: String,
}
pub struct CurrentFieldServicePartsFinanceLineage {
    request: FieldServicePartsFinanceRequest,
    unit: Unit,
}
impl CurrentFieldServicePartsFinanceLineage {
    pub fn request(&self) -> &FieldServicePartsFinanceRequest {
        &self.request
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || !exists(c, &self.request).await? {
            return Err(changed());
        }
        Ok(())
    }
}
/// Compose these owner-only utility fences before the first Holding snapshot.
pub async fn lock_field_service_parts_finance_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE field_service_orders,field_service_order_events,field_service_part_reservations IN SHARE MODE").execute(c).await?;
    Ok(())
}
pub async fn read_field_service_parts_finance_lineage(
    c: &mut PgConnection,
    request: FieldServicePartsFinanceRequest,
) -> Result<Option<CurrentFieldServicePartsFinanceLineage>, Error> {
    let unit = unit(c).await?;
    if [
        request.minimum_order_version,
        request.quantity_milli,
        request.value_minor,
    ]
    .iter()
    .any(|n| !(1..=9_007_199_254_740_991).contains(n))
        || request.consumption_id.as_str()
            != format!("operational-part-{}", request.owner_key.as_str())
    {
        return Err(invalid());
    }
    Ok(exists(c, &request)
        .await?
        .then_some(CurrentFieldServicePartsFinanceLineage { request, unit }))
}
async fn exists(c: &mut PgConnection, r: &FieldServicePartsFinanceRequest) -> Result<bool, Error> {
    // Only an existence predicate leaves the owner. Unused reason/details/body
    // size cannot hide valid original provenance or leak personal data.
    Ok(sqlx::query_scalar::<_,bool>("SELECT true FROM field_service_orders o JOIN field_service_order_events e ON e.order_id=o.id AND e.version=$5 AND e.action='issue-part' AND e.id=$6 AND e.actor_id=$7 AND e.details_json->>'consumptionId'=$8 AND e.details_json->>'journalId'=$9 AND e.details_json->>'quantityMilli'=$10::bigint::text AND e.details_json->>'costMinor'=$11::bigint::text JOIN field_service_part_reservations p ON p.id=e.details_json->>'reservationId' AND p.order_id=o.id AND p.warehouse_id=$12 AND p.product_id=$13 WHERE o.id=$1 AND o.company_id=$2 AND o.branch_id=$3 AND o.department_id IS NOT DISTINCT FROM $4 AND o.version>=$5 FOR SHARE OF o,e,p")
        .bind(r.order_id.as_str()).bind(r.company_id.as_str()).bind(r.branch_id.as_str()).bind(r.department_id.as_deref()).bind(r.minimum_order_version).bind(format!("service-order-event-{}",r.owner_key.as_str())).bind(r.creator_user_id.as_str()).bind(r.consumption_id.as_str()).bind(r.journal_id.as_str()).bind(r.quantity_milli).bind(r.value_minor).bind(r.warehouse_id.as_str()).bind(r.product_id.as_str()).fetch_optional(c).await?.unwrap_or(false))
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn invalid() -> Error {
    PersistenceError::new("FIELD_SERVICE_FINANCE_LINEAGE_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_SERVICE_FINANCE_LINEAGE_CHANGED",
        "The original immutable parts event/reservation and current Job scope changed.",
    )
    .into()
}
