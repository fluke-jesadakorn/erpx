//! Field-owned detail rows for an already scoped workspace Order.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde::Deserialize;
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Deserialize, PartialEq)]
pub struct ServiceDetailRows {
    pub parts: Vec<serde_json::Value>,
    pub history: Vec<serde_json::Value>,
    pub billing: Option<serde_json::Value>,
}
fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FIELD_SERVICE_DETAIL_LIMIT",
        "The Service detail exceeds this bounded read or its source changed.",
    )
    .into()
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("FIELD_SERVICE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
pub async fn lock_detail_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FIELD_SERVICE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE field_service_orders,field_service_agreements,field_service_order_events,field_service_part_reservations,field_service_billing_releases IN SHARE MODE").execute(c).await?;
    Ok(())
}
async fn image(
    c: &mut PgConnection,
    company: &CompanyId,
    order: &str,
    parts_history: bool,
) -> Result<(ServiceDetailRows, String), Error> {
    let row=sqlx::query("WITH source AS MATERIALIZED (SELECT o.* FROM field_service_orders o WHERE o.id=$1 AND o.company_id=$2), payload AS (SELECT jsonb_build_object('parts',CASE WHEN $3 THEN coalesce((SELECT jsonb_agg(to_jsonb(p) ORDER BY p.created_at,p.id) FROM field_service_part_reservations p JOIN source o ON o.id=p.order_id),'[]'::jsonb) ELSE '[]'::jsonb END,'history',coalesce((SELECT jsonb_agg(to_jsonb(e)||jsonb_build_object('details_json',e.details_json::text) ORDER BY e.version) FROM field_service_order_events e JOIN source o ON o.id=e.order_id WHERE $3 OR e.action NOT IN ('reserve-part','issue-part')),'[]'::jsonb),'billing',(SELECT to_jsonb(b) FROM field_service_billing_releases b JOIN source o ON o.id=b.order_id)) value,(SELECT to_jsonb(source) FROM source) original), measured AS (SELECT value::text payload,encode(sha256(convert_to(jsonb_build_array(value,original)::text,'UTF8')),'hex') digest,original IS NOT NULL found FROM payload) SELECT CASE WHEN octet_length(payload)<=2097152 THEN payload END payload,digest,found FROM measured").bind(order).bind(company.as_str()).bind(parts_history).fetch_one(c).await?;
    if !row.try_get::<bool, _>("found")? {
        return Err(unavailable());
    }
    let raw: Option<String> = row.try_get("payload")?;
    let data = serde_json::from_str(&raw.ok_or_else(unavailable)?).map_err(|_| unavailable())?;
    Ok((data, row.try_get("digest")?))
}
pub struct CurrentServiceDetailRows {
    unit: Unit,
    company: CompanyId,
    order: String,
    parts_history: bool,
    data: ServiceDetailRows,
    digest: String,
}
impl CurrentServiceDetailRows {
    pub fn data(&self) -> &ServiceDetailRows {
        &self.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || image(c, &self.company, &self.order, self.parts_history)
                .await?
                .1
                != self.digest
        {
            return Err(unavailable());
        }
        Ok(())
    }
}
pub async fn read_detail_rows(
    c: &mut PgConnection,
    company: &CompanyId,
    order: &str,
    parts_history: bool,
) -> Result<CurrentServiceDetailRows, Error> {
    let unit = unit(c).await?;
    let (data, digest) = image(c, company, order, parts_history).await?;
    Ok(CurrentServiceDetailRows {
        unit,
        company: company.clone(),
        order: order.into(),
        parts_history,
        data,
        digest,
    })
}
/// Field source portion of the retained invoice-link join; Finance and Workflow
/// independently validate their actual corresponding rows on this same unit.
#[derive(Clone, PartialEq, Eq)]
pub struct ServiceBillingLinkFacts {
    pub key: String,
    pub order: String,
    pub actor: String,
    pub customer: String,
    pub currency: String,
    pub amount: i64,
    pub pricing_date: String,
}
pub struct CurrentServiceBillingLink {
    unit: Unit,
    company: CompanyId,
    facts: ServiceBillingLinkFacts,
    digest: String,
}
async fn link_image(
    c: &mut PgConnection,
    company: &CompanyId,
    order: &str,
) -> Result<Option<(ServiceBillingLinkFacts, String)>, Error> {
    let row=sqlx::query("SELECT CASE WHEN bytes<=2097152 THEN b.id END id,CASE WHEN bytes<=2097152 THEN b.order_id END order_id,CASE WHEN bytes<=2097152 THEN b.released_by END actor,CASE WHEN bytes<=2097152 THEN b.customer_id END customer,CASE WHEN bytes<=2097152 THEN b.currency END currency,b.amount_minor,CASE WHEN bytes<=2097152 THEN b.pricing_date END pricing_date,bytes,encode(sha256(convert_to(jsonb_build_array(to_jsonb(b),to_jsonb(o),to_jsonb(g),to_jsonb(e))::text,'UTF8')),'hex') digest FROM field_service_billing_releases b JOIN field_service_orders o ON o.id=b.order_id AND o.company_id=$1 AND o.agreement_id=b.agreement_id AND o.version>=b.source_order_version JOIN field_service_agreements g ON g.id=o.agreement_id AND g.company_id=o.company_id AND g.branch_id=o.branch_id AND g.department_id IS NOT DISTINCT FROM o.department_id AND g.customer_id=b.customer_id JOIN field_service_order_events e ON e.order_id=o.id AND e.version=b.source_order_version AND e.action='release-billing' AND e.status='billed' AND e.actor_id=b.released_by AND e.id='service-order-event-'||substr(b.id,length('field-billing-')+1) CROSS JOIN LATERAL (SELECT octet_length(b.id)::bigint+octet_length(b.order_id)::bigint+octet_length(b.released_by)::bigint+octet_length(b.customer_id)::bigint+octet_length(b.currency)::bigint+coalesce(octet_length(b.pricing_date),0)::bigint bytes) size WHERE o.id=$2 AND b.id LIKE 'field-billing-%' AND b.pricing_date IS NOT NULL AND b.status='released' FOR SHARE OF b,o,g,e").bind(company.as_str()).bind(order).fetch_optional(c).await?;
    let Some(row) = row else { return Ok(None) };
    if row.try_get::<i64, _>("bytes")? > 2097152 {
        return Err(unavailable());
    }
    let get = |name| -> Result<String, Error> {
        row.try_get::<Option<String>, _>(name)?
            .ok_or_else(unavailable)
    };
    let id = get("id")?;
    let Some(key) = id.strip_prefix("field-billing-") else {
        return Ok(None);
    };
    Ok(Some((
        ServiceBillingLinkFacts {
            key: key.into(),
            order: get("order_id")?,
            actor: get("actor")?,
            customer: get("customer")?,
            currency: get("currency")?,
            amount: row.try_get("amount_minor")?,
            pricing_date: get("pricing_date")?,
        },
        row.try_get("digest")?,
    )))
}
pub async fn read_service_billing_link(
    c: &mut PgConnection,
    company: &CompanyId,
    order: &str,
) -> Result<Option<CurrentServiceBillingLink>, Error> {
    let unit = unit(c).await?;
    Ok(link_image(c, company, order)
        .await?
        .map(|(facts, digest)| CurrentServiceBillingLink {
            unit,
            company: company.clone(),
            facts,
            digest,
        }))
}
impl CurrentServiceBillingLink {
    pub fn facts(&self) -> &ServiceBillingLinkFacts {
        &self.facts
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || link_image(c, &self.company, &self.facts.order)
                .await?
                .as_ref()
                != Some(&(self.facts.clone(), self.digest.clone()))
        {
            return Err(unavailable());
        }
        Ok(())
    }
}
