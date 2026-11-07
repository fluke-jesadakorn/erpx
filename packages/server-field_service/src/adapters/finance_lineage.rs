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

/// Field-owned original billing facts for a Finance reader; no Service role is implied.
#[derive(Clone, Eq, PartialEq)]
pub struct FieldServiceInvoiceFinanceRequest {
    pub company_id: CompanyId,
    pub order_id: String,
    pub release_id: String,
    pub key: String,
    pub actor_id: String,
    pub currency: String,
    pub amount_units: String,
    pub customer_id: String,
    pub pricing_date: String,
}
pub struct CurrentFieldServiceInvoiceFinanceLineage {
    request: FieldServiceInvoiceFinanceRequest,
    unit: Unit,
    digest: String,
    origin: erpx_server_workflow::public::record_origin::PgRawRecordOrigin,
}
pub async fn lock_field_service_invoice_finance_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE field_service_agreements,field_service_orders,field_service_order_events,field_service_billing_releases IN SHARE MODE").execute(c).await?;
    Ok(())
}
async fn invoice_image(
    c: &mut PgConnection,
    r: &FieldServiceInvoiceFinanceRequest,
) -> Result<Option<(String, i64)>, Error> {
    Ok(sqlx::query_as("SELECT encode(sha256(convert_to(jsonb_build_array(to_jsonb(b),to_jsonb(o),to_jsonb(g),to_jsonb(e))::text,'UTF8')),'hex'),b.source_order_version::bigint FROM field_service_billing_releases b JOIN field_service_orders o ON o.id=b.order_id AND o.agreement_id=b.agreement_id AND o.version>=b.source_order_version JOIN field_service_agreements g ON g.id=o.agreement_id AND g.company_id=o.company_id AND g.branch_id=o.branch_id AND g.department_id IS NOT DISTINCT FROM o.department_id AND g.customer_id=b.customer_id JOIN field_service_order_events e ON e.order_id=o.id AND e.id=$3 AND e.version=b.source_order_version AND e.action='release-billing' AND e.status='billed' AND e.actor_id=b.released_by AND e.reason=b.reason WHERE b.id=$1 AND o.company_id=$2 AND o.id=$4 AND b.released_by=$5 AND b.currency=$6 AND b.amount_minor::numeric=$7::numeric AND b.customer_id=$8 AND b.pricing_date=$9 AND b.status='released' FOR SHARE OF b,o,g,e").bind(&r.release_id).bind(r.company_id.as_str()).bind(format!("service-order-event-{}",r.key)).bind(&r.order_id).bind(&r.actor_id).bind(&r.currency).bind(&r.amount_units).bind(&r.customer_id).bind(&r.pricing_date).fetch_optional(c).await?)
}
pub async fn read_field_service_invoice_finance_lineage(
    c: &mut PgConnection,
    r: FieldServiceInvoiceFinanceRequest,
) -> Result<Option<CurrentFieldServiceInvoiceFinanceLineage>, Error> {
    use erpx_server_workflow::public::record_origin::{
        RawRecordOriginRequest, read_raw_record_origin_result_with_limit,
    };
    let unit = unit(c).await?;
    if r.release_id != format!("field-billing-{}", r.key) {
        return Ok(None);
    }
    let Some((digest, version)) = invoice_image(c, &r).await? else {
        return Ok(None);
    };
    let Some((origin, result)) = read_raw_record_origin_result_with_limit(
        c,
        RawRecordOriginRequest {
            command_key: r.key.clone(),
            operation: "field.order.release-billing".into(),
            record_id: r.order_id.clone(),
            creator_user_id: r.actor_id.clone(),
            company_id: r.company_id.clone(),
        },
        2 * 1024 * 1024,
    )
    .await?
    else {
        return Ok(None);
    };
    if result
        .property("id")?
        .and_then(|v| v.string_utf16_units().ok().flatten())
        != Some(r.order_id.encode_utf16().collect())
        || result
            .property("status")?
            .and_then(|v| v.string_utf8_storage().ok().flatten())
            .as_deref()
            != Some("billed")
        || result
            .property("version")?
            .and_then(|v| v.as_str().parse::<i64>().ok())
            != Some(version)
    {
        return Ok(None);
    }
    Ok(Some(CurrentFieldServiceInvoiceFinanceLineage {
        request: r,
        unit,
        digest,
        origin,
    }))
}
impl CurrentFieldServiceInvoiceFinanceLineage {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || invoice_image(c, &self.request).await?.map(|v| v.0).as_ref() != Some(&self.digest)
        {
            return Err(changed());
        }
        erpx_server_workflow::public::record_origin::verify_raw_record_origin(c, &self.origin).await
    }
}
