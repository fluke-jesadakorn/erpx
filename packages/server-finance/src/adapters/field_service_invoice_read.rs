//! Finance-owned invoice workspace and original captured-money witness.
use crate::application::journal::FinanceActor;
type FinanceInvoiceRow = std::collections::BTreeMap<String, serde_json::Value>;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use futures_util::TryStreamExt;
use serde::Serialize;
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
const LIMIT: usize = 2 * 1024 * 1024;
fn denied() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "INVOICE_NOT_FOUND",
        "Invoice not found in this company.",
    )
    .into()
}
fn limited() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_INVOICE_READ_LIMIT",
        "The invoice workspace exceeds this bounded read.",
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
pub async fn lock_service_invoice_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE money_transaction_snapshots,finance_invoices,finance_allocations,finance_payments,finance_invoice_line_versions,finance_issued_thai_documents,finance_parties,finance_accounts,finance_charts,finance_periods IN SHARE MODE").execute(c).await?;
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FinanceServiceInvoiceSource {
    pub release_id: String,
    pub order_id: String,
    pub command_key: String,
    pub actor_id: String,
    pub currency: String,
    pub amount_units: String,
    pub usd_units: String,
    pub fx_rate_id: Option<String>,
    pub used_latest_fallback: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FinanceInvoiceOptions {
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature="contracts",schema(value_type=Vec<Object>))]
    pub charts: Vec<FinanceInvoiceRow>,
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature="contracts",schema(value_type=Vec<Object>))]
    pub accounts: Vec<FinanceInvoiceRow>,
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature="contracts",schema(value_type=Vec<Object>))]
    pub periods: Vec<FinanceInvoiceRow>,
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature="contracts",schema(value_type=Vec<Object>))]
    pub parties: Vec<FinanceInvoiceRow>,
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature="contracts",schema(value_type=Vec<Object>))]
    pub invoices: Vec<FinanceInvoiceRow>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FinanceServiceInvoiceWorkspace {
    #[cfg_attr(feature = "contracts", ts(type = "Record<string, unknown>"))]
    #[cfg_attr(feature="contracts",schema(value_type=Object))]
    pub invoice: FinanceInvoiceRow,
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature="contracts",schema(value_type=Vec<Object>))]
    pub allocations: Vec<FinanceInvoiceRow>,
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature="contracts",schema(value_type=Vec<Object>))]
    pub lines: Vec<FinanceInvoiceRow>,
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature="contracts",schema(value_type=Vec<Object>))]
    pub line_history: Vec<FinanceInvoiceRow>,
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature="contracts",schema(value_type=Vec<Object>))]
    pub thai_documents: Vec<FinanceInvoiceRow>,
    pub options: FinanceInvoiceOptions,
    pub source: FinanceServiceInvoiceSource,
}
async fn rows(
    c: &mut PgConnection,
    sql: &str,
    company: &str,
    id: &str,
    budget: &mut usize,
) -> Result<Vec<FinanceInvoiceRow>, Error> {
    let q = format!(
        "WITH requested AS MATERIALIZED ({sql}), measured AS (SELECT value,row_number() OVER () ordinal,octet_length(value)::bigint bytes FROM requested bounded(value)), admitted AS (SELECT value,ordinal,sum(bytes) OVER (ORDER BY ordinal) total FROM measured) SELECT CASE WHEN total>$3 THEN NULL ELSE value END FROM admitted ORDER BY ordinal"
    );
    let mut stream = sqlx::query_scalar::<_, Option<String>>(sqlx::AssertSqlSafe(q))
        .bind(company)
        .bind(id)
        .bind(i64::try_from(LIMIT.saturating_sub(*budget)).map_err(|_| limited())?)
        .fetch(c);
    let mut out = Vec::new();
    while let Some(value) = stream.try_next().await? {
        let raw = value.ok_or_else(limited)?;
        *budget = budget
            .checked_add(raw.len())
            .filter(|n| *n <= LIMIT)
            .ok_or_else(limited)?;
        if out.len() >= 10000 {
            return Err(limited());
        }
        let row: FinanceInvoiceRow = serde_json::from_str(&raw).map_err(|_| limited())?;
        if !row.values().all(safe_json) {
            return Err(limited());
        }
        out.push(row)
    }
    Ok(out)
}
fn safe_json(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Number(n) => n
            .as_f64()
            .is_some_and(|x| x.is_finite() && x.abs() <= 9_007_199_254_740_991.0),
        serde_json::Value::Array(a) => a.iter().all(safe_json),
        serde_json::Value::Object(m) => m.values().all(safe_json),
        _ => true,
    }
}
fn text<'a>(r: &'a FinanceInvoiceRow, k: &str) -> Result<&'a str, Error> {
    r.get(k).and_then(|v| v.as_str()).ok_or_else(denied)
}
async fn read(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &str,
) -> Result<(FinanceServiceInvoiceWorkspace, String), Error> {
    let key = id
        .strip_prefix("field-invoice-")
        .filter(|x| !x.is_empty())
        .ok_or_else(denied)?;
    erpx_server_shared::public::CommandKey::new(key.to_owned())?;
    let company = actor.company_id.as_str();
    let mut budget = 0;
    let mut header=rows(c,"SELECT (to_jsonb(i)||jsonb_build_object('party_code',p.code,'party_name',p.name,'snapshot_usd_units',s.usd_units::text,'snapshot_source_units',s.amount_units::text,'snapshot_rate_id',s.fx_rate_id))::text FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id AND p.company_id=i.company_id LEFT JOIN money_transaction_snapshots s ON s.company_id=i.company_id AND s.source_type='finance_invoice' AND s.source_id=i.id AND s.amount_kind='total' WHERE i.company_id=$1 AND i.id=$2 AND i.kind='receivable' AND i.source_type='field_service' FOR SHARE OF i,p",company,id,&mut budget).await?;
    if header.len() != 1 {
        return Err(denied());
    }
    let invoice = header.remove(0);
    let money=sqlx::query("SELECT CASE WHEN b.bounded THEN m.source_id END release_id,CASE WHEN b.bounded THEN i.source_id END order_id,CASE WHEN b.bounded THEN i.created_by END actor_id,CASE WHEN b.bounded THEN m.currency END currency,CASE WHEN b.bounded THEN m.amount_units::text END amount_units,CASE WHEN b.bounded THEN m.usd_units::text END usd_units,CASE WHEN b.bounded THEN m.fx_rate_id END fx_rate_id,m.used_latest_fallback,b.bounded,encode(sha256(convert_to(jsonb_build_array(to_jsonb(i),to_jsonb(m),to_jsonb(copy))::text,'UTF8')),'hex') digest FROM finance_invoices i JOIN money_transaction_snapshots m ON m.company_id=i.company_id AND m.source_type='field_service_billing_release' AND m.source_id=$3 AND m.amount_kind='invoice_total' JOIN money_transaction_snapshots copy ON copy.company_id=i.company_id AND copy.source_type='finance_invoice' AND copy.source_id=i.id AND copy.amount_kind='draft:v1' CROSS JOIN LATERAL (SELECT octet_length(m.source_id)::bigint+octet_length(i.source_id)::bigint+octet_length(i.created_by)::bigint+octet_length(m.currency)::bigint+octet_length(m.amount_units::text)::bigint+octet_length(m.usd_units::text)::bigint+coalesce(octet_length(m.fx_rate_id),0)::bigint<=2097152 bounded) b WHERE i.company_id=$1 AND i.id=$2 AND i.kind='receivable' AND i.source_type='field_service' AND m.currency=i.currency AND m.amount_units=i.total_minor AND m.transaction_date::text=i.invoice_date AND m.currency_scale=2 AND m.original_source_type IS NULL AND m.original_source_id IS NULL AND m.original_amount_kind IS NULL AND m.carried_source_type IS NULL AND m.carried_source_id IS NULL AND m.carried_amount_kind IS NULL AND m.carried_allocation_offset_units IS NULL AND copy.original_source_type IS NULL AND copy.original_source_id IS NULL AND copy.original_amount_kind IS NULL AND copy.carried_allocation_offset_units IS NULL AND (copy.transaction_date,copy.currency,copy.currency_scale,copy.amount_units,copy.usd_units,copy.fx_rate_id,copy.used_latest_fallback,copy.captured_at) IS NOT DISTINCT FROM (m.transaction_date,m.currency,m.currency_scale,m.amount_units,m.usd_units,m.fx_rate_id,m.used_latest_fallback,m.captured_at) AND ((m.amount_units>0 AND (copy.carried_source_type,copy.carried_source_id,copy.carried_amount_kind)=('field_service_billing_release',m.source_id,'invoice_total')) OR (m.amount_units=0 AND copy.carried_source_type IS NULL AND copy.carried_source_id IS NULL AND copy.carried_amount_kind IS NULL)) FOR SHARE OF i,m,copy").bind(company).bind(id).bind(format!("field-billing-{key}")).fetch_optional(&mut *c).await?.ok_or_else(denied)?;
    if !money.try_get::<bool, _>("bounded")? {
        return Err(limited());
    }
    let get = |name| -> Result<String, Error> {
        money.try_get::<Option<String>, _>(name)?.ok_or_else(denied)
    };
    let source = FinanceServiceInvoiceSource {
        release_id: get("release_id")?,
        order_id: get("order_id")?,
        command_key: key.into(),
        actor_id: get("actor_id")?,
        currency: get("currency")?,
        amount_units: get("amount_units")?,
        usd_units: get("usd_units")?,
        fx_rate_id: money.try_get("fx_rate_id")?,
        used_latest_fallback: money.try_get("used_latest_fallback")?,
    };
    let chart = text(&invoice, "debit_account_id")?;
    let chart_rows=rows(c,"SELECT to_jsonb(ch)::text FROM finance_accounts a JOIN finance_charts ch ON ch.id=a.chart_id AND ch.company_id=a.company_id WHERE a.company_id=$1 AND a.id=$2 FOR SHARE OF a,ch",company,chart,&mut budget).await?;
    if chart_rows.len() != 1 {
        return Err(denied());
    }
    let chart_id = text(&chart_rows[0], "id")?;
    let accounts=rows(c,"SELECT to_jsonb(a)::text FROM finance_accounts a WHERE a.company_id=$1 AND a.chart_id=$2 ORDER BY a.code,a.id LIMIT 10001 FOR SHARE OF a",company,chart_id,&mut budget).await?;
    let periods=rows(c,"SELECT to_jsonb(p)::text FROM finance_periods p WHERE p.company_id=$1 AND $2::text IS NOT NULL ORDER BY p.start_date DESC,p.id LIMIT 10001 FOR SHARE OF p",company,id,&mut budget).await?;
    let parties=rows(c,"SELECT to_jsonb(p)::text FROM finance_parties p JOIN finance_invoices i ON i.party_id=p.id AND i.company_id=p.company_id WHERE i.company_id=$1 AND i.id=$2 ORDER BY p.name,p.id FOR SHARE OF p,i",company,id,&mut budget).await?;
    let allocations=rows(c,"SELECT jsonb_build_object('id',a.id,'payment_id',a.payment_id,'amount_minor',a.amount_minor,'created_at',a.created_at,'payment_number',p.payment_number,'status',p.status,'payment_slip_id',p.payment_slip_id)::text FROM finance_allocations a JOIN finance_payments p ON p.id=a.payment_id WHERE a.company_id=$1 AND a.invoice_id=$2 ORDER BY a.created_at LIMIT 10001 FOR SHARE OF a,p",company,id,&mut budget).await?;
    let lines=rows(c,"SELECT to_jsonb(l)::text FROM finance_invoice_line_versions l JOIN finance_invoices i ON i.id=l.invoice_id AND i.company_id=l.company_id AND i.line_revision=l.source_version WHERE l.company_id=$1 AND l.invoice_id=$2 ORDER BY l.ordinal LIMIT 10001 FOR SHARE OF l,i",company,id,&mut budget).await?;
    let line_history=rows(c,"SELECT jsonb_build_object('source_version',l.source_version,'origin_kind',l.origin_kind,'line_count',count(*),'subtotal_minor',sum(l.subtotal_minor),'tax_minor',sum(l.tax_minor),'created_at',min(l.created_at))::text FROM finance_invoice_line_versions l WHERE l.company_id=$1 AND l.invoice_id=$2 GROUP BY l.source_version,l.origin_kind ORDER BY l.source_version LIMIT 10001",company,id,&mut budget).await?;
    let thai_documents=rows(c,"SELECT jsonb_build_object('id',d.id,'document_number',d.document_number,'document_kind',d.document_kind,'edition',d.edition,'issue_date',d.issue_date)::text FROM finance_issued_thai_documents d WHERE d.company_id=$1 AND d.invoice_id=$2 ORDER BY d.edition LIMIT 10001 FOR SHARE OF d",company,id,&mut budget).await?;
    let full_digest:String=sqlx::query_scalar("SELECT encode(sha256(convert_to(jsonb_build_array(to_jsonb(i),to_jsonb(p),to_jsonb(a),to_jsonb(ch),(SELECT coalesce(jsonb_agg(to_jsonb(x) ORDER BY x.id),'[]'::jsonb) FROM finance_accounts x WHERE x.company_id=i.company_id AND x.chart_id=ch.id),(SELECT coalesce(jsonb_agg(to_jsonb(x) ORDER BY x.id),'[]'::jsonb) FROM finance_periods x WHERE x.company_id=i.company_id),(SELECT coalesce(jsonb_agg(jsonb_build_array(to_jsonb(x),to_jsonb(pay)) ORDER BY x.id),'[]'::jsonb) FROM finance_allocations x JOIN finance_payments pay ON pay.id=x.payment_id WHERE x.company_id=i.company_id AND x.invoice_id=i.id),(SELECT coalesce(jsonb_agg(to_jsonb(x) ORDER BY x.id),'[]'::jsonb) FROM finance_invoice_line_versions x WHERE x.company_id=i.company_id AND x.invoice_id=i.id),(SELECT coalesce(jsonb_agg(to_jsonb(x) ORDER BY x.id),'[]'::jsonb) FROM finance_issued_thai_documents x WHERE x.company_id=i.company_id AND x.invoice_id=i.id),to_jsonb(m),to_jsonb(copy))::text,'UTF8')),'hex') FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id AND p.company_id=i.company_id JOIN finance_accounts a ON a.id=i.debit_account_id AND a.company_id=i.company_id JOIN finance_charts ch ON ch.id=a.chart_id AND ch.company_id=a.company_id JOIN money_transaction_snapshots m ON m.company_id=i.company_id AND m.source_type='field_service_billing_release' AND m.source_id=$3 AND m.amount_kind='invoice_total' JOIN money_transaction_snapshots copy ON copy.company_id=i.company_id AND copy.source_type='finance_invoice' AND copy.source_id=i.id AND copy.amount_kind='draft:v1' WHERE i.company_id=$1 AND i.id=$2").bind(company).bind(id).bind(&source.release_id).fetch_one(&mut *c).await?;
    Ok((
        FinanceServiceInvoiceWorkspace {
            invoice,
            allocations,
            lines,
            line_history,
            thai_documents,
            options: FinanceInvoiceOptions {
                charts: chart_rows,
                accounts,
                periods,
                parties,
                invoices: Vec::new(),
            },
            source,
        },
        full_digest,
    ))
}
pub struct CurrentFinanceServiceInvoiceWorkspace {
    actor: FinanceActor,
    id: String,
    unit: Unit,
    data: FinanceServiceInvoiceWorkspace,
    digest: String,
}
impl CurrentFinanceServiceInvoiceWorkspace {
    pub fn data(&self) -> &FinanceServiceInvoiceWorkspace {
        &self.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || read(c, &self.actor, &self.id).await? != (self.data.clone(), self.digest.clone())
        {
            return Err(denied());
        }
        Ok(())
    }
}
pub async fn read_service_invoice_workspace(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &str,
) -> Result<CurrentFinanceServiceInvoiceWorkspace, Error> {
    let unit = unit(c).await?;
    let (data, digest) = read(c, actor, id).await?;
    Ok(CurrentFinanceServiceInvoiceWorkspace {
        actor: actor.clone(),
        id: id.into(),
        unit,
        data,
        digest,
    })
}
