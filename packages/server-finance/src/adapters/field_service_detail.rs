//! Finance-owned status and narrow original Service invoice-link facts.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde::Deserialize;
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
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
        return Err(PersistenceError::new("FINANCE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
pub async fn lock_service_detail_finance(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query(
        "LOCK TABLE finance_journals,money_transaction_snapshots,finance_invoices IN SHARE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}
#[derive(Clone, PartialEq, Deserialize)]
pub struct ServiceJournalStatus {
    pub id: String,
    pub status: String,
}
pub struct CurrentServiceDetailJournals {
    company: CompanyId,
    ids: Vec<String>,
    unit: Unit,
    data: Vec<ServiceJournalStatus>,
    digest: String,
}
async fn journal_image(
    c: &mut PgConnection,
    company: &CompanyId,
    ids: &[String],
) -> Result<(Vec<ServiceJournalStatus>, String), Error> {
    let row=sqlx::query("WITH selected AS MATERIALIZED (SELECT j.* FROM finance_journals j WHERE j.company_id=$1 AND j.id=ANY($2::text[])), payload AS(SELECT coalesce(jsonb_agg(jsonb_build_object('id',id,'status',status) ORDER BY id),'[]'::jsonb) value,coalesce(jsonb_agg(to_jsonb(selected) ORDER BY id),'[]'::jsonb) full_rows FROM selected), measured AS (SELECT value::text payload,encode(sha256(convert_to(full_rows::text,'UTF8')),'hex') digest FROM payload) SELECT CASE WHEN octet_length(payload)<=2097152 THEN payload END payload,digest FROM measured").bind(company.as_str()).bind(ids).fetch_one(c).await?;
    let raw: Option<String> = row.try_get("payload")?;
    Ok((
        serde_json::from_str(&raw.ok_or_else(unavailable)?).map_err(|_| unavailable())?,
        row.try_get("digest")?,
    ))
}
pub async fn read_service_detail_journals(
    c: &mut PgConnection,
    company: &CompanyId,
    ids: Vec<String>,
) -> Result<CurrentServiceDetailJournals, Error> {
    let unit = unit(c).await?;
    let (data, digest) = journal_image(c, company, &ids).await?;
    Ok(CurrentServiceDetailJournals {
        company: company.clone(),
        ids,
        unit,
        data,
        digest,
    })
}
impl CurrentServiceDetailJournals {
    pub fn data(&self) -> &[ServiceJournalStatus] {
        &self.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || journal_image(c, &self.company, &self.ids).await?.1 != self.digest
        {
            return Err(unavailable());
        }
        Ok(())
    }
}
#[derive(Clone)]
pub struct ServiceInvoiceLinkRequest {
    pub company: CompanyId,
    pub key: String,
    pub order: String,
    pub customer: String,
    pub currency: String,
    pub amount: i64,
    pub pricing_date: String,
}
pub struct CurrentServiceDetailInvoiceLink {
    request: ServiceInvoiceLinkRequest,
    unit: Unit,
    image: Option<(String, String)>,
}
async fn invoice_image(
    c: &mut PgConnection,
    r: &ServiceInvoiceLinkRequest,
) -> Result<Option<(String, String)>, Error> {
    let row:Option<(Option<String>,String)>=sqlx::query_as("SELECT CASE WHEN octet_length(i.id)<=2097152 THEN i.id END,encode(sha256(convert_to(jsonb_build_array(to_jsonb(i),to_jsonb(m))::text,'UTF8')),'hex') FROM finance_invoices i JOIN money_transaction_snapshots m ON m.company_id=i.company_id AND m.source_type='field_service_billing_release' AND m.source_id=$3 AND m.amount_kind='invoice_total' AND m.currency=$6 AND m.amount_units=$7::bigint WHERE i.id=$2 AND i.company_id=$1 AND i.kind='receivable' AND i.source_type='field_service' AND i.source_id=$4 AND i.party_id=$5 AND i.currency=$6 AND i.total_minor=$7 AND i.invoice_date=$8 FOR SHARE OF i,m").bind(r.company.as_str()).bind(format!("field-invoice-{}",r.key)).bind(format!("field-billing-{}",r.key)).bind(&r.order).bind(&r.customer).bind(&r.currency).bind(r.amount).bind(&r.pricing_date).fetch_optional(c).await?;
    row.map(|(id, digest)| Ok((id.ok_or_else(unavailable)?, digest)))
        .transpose()
}
pub async fn read_service_detail_invoice_link(
    c: &mut PgConnection,
    request: ServiceInvoiceLinkRequest,
) -> Result<CurrentServiceDetailInvoiceLink, Error> {
    let unit = unit(c).await?;
    let image = invoice_image(c, &request).await?;
    Ok(CurrentServiceDetailInvoiceLink {
        request,
        unit,
        image,
    })
}
impl CurrentServiceDetailInvoiceLink {
    pub fn invoice_id(&self) -> Option<&str> {
        self.image.as_ref().map(|x| x.0.as_str())
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || invoice_image(c, &self.request).await? != self.image {
            return Err(unavailable());
        }
        Ok(())
    }
}
