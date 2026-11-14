//! Commerce-owned customer-return facts. Inventory and Workflow proofs are
//! composed separately; these facts alone grant no authority or purchase.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

#[derive(Clone)]
pub enum QualityReturnSelector {
    UntrackedLine(String),
    TrackedAllocation(String),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityReturnCommerceFacts {
    pub return_id: String,
    pub company_id: String,
    pub branch_id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub source_id: String,
    pub approver: String,
    pub line_ordinal: i64,
    pub lot_ordinal: Option<i64>,
    pub lot_id: Option<String>,
    pub fulfillment_line_id: String,
    pub fulfillment_allocation_id: Option<String>,
    pub quantity_milli: i64,
    pub value_minor: i64,
}
pub struct CurrentQualityReturnCommerce {
    company: CompanyId,
    branch: Option<RecordId>,
    selector: QualityReturnSelector,
    unit: Unit,
    facts: Option<QualityReturnCommerceFacts>,
    image: Option<String>,
}
impl CurrentQualityReturnCommerce {
    pub fn facts(&self) -> Option<&QualityReturnCommerceFacts> {
        self.facts.as_ref()
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed().into());
        }
        let (facts, image) = read(c, &self.company, self.branch.as_ref(), &self.selector).await?;
        if facts != self.facts || image != self.image {
            return Err(changed().into());
        }
        Ok(())
    }
}
/// Lock alongside all owner locks before the first snapshot. In particular the
/// allocation table protects the untracked no-allocation result from inserts.
pub async fn lock_quality_return_commerce(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("COMMERCE_QUALITY_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE commerce_orders,commerce_order_lines,commerce_fulfillments,commerce_fulfillment_lines,commerce_returns,commerce_return_lines,commerce_return_lot_allocations IN SHARE MODE").execute(c).await?;
    Ok(())
}
/// Selector is only a lookup key; callers must compare all facts with an
/// actual Quality hold and retain approval and original allocation proofs.
pub async fn inspect_quality_return_commerce(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&RecordId>,
    selector: QualityReturnSelector,
) -> Result<CurrentQualityReturnCommerce, Error> {
    let value = match &selector {
        QualityReturnSelector::UntrackedLine(v) | QualityReturnSelector::TrackedAllocation(v) => v,
    };
    if value.is_empty() || value.len() > 1024 {
        return Err(changed().into());
    }
    let unit = unit(c).await?;
    let (facts, image) = read(c, company, branch, &selector).await?;
    Ok(CurrentQualityReturnCommerce {
        company: company.clone(),
        branch: branch.cloned(),
        selector,
        unit,
        facts,
        image,
    })
}
async fn read(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&RecordId>,
    selector: &QualityReturnSelector,
) -> Result<(Option<QualityReturnCommerceFacts>, Option<String>), Error> {
    let (line, allocation) = match selector {
        QualityReturnSelector::UntrackedLine(v) => (Some(v.as_str()), None),
        QualityReturnSelector::TrackedAllocation(v) => (None, Some(v.as_str())),
    };
    let row=sqlx::query("SELECT r.id return_id,r.company_id,r.branch_id,o.warehouse_id,l.product_id,r.approved_by,l.ordinal line_ordinal,fl.id fulfillment_line_id, a.id allocation_id,a.lot_id,a.fulfillment_allocation_id, CASE WHEN a.id IS NULL THEN NULL ELSE (SELECT count(*) FROM commerce_return_lot_allocations x WHERE x.return_line_id=l.id AND x.id<a.id)::bigint END lot_ordinal, COALESCE(a.quantity_milli,l.quantity_milli) quantity_milli,COALESCE(a.value_minor,l.value_minor) value_minor, encode(sha256(convert_to(jsonb_build_array(to_jsonb(r),to_jsonb(o),to_jsonb(f),to_jsonb(fl),to_jsonb(ol),to_jsonb(l),to_jsonb(a))::text,'UTF8')),'hex') image FROM commerce_return_lines l JOIN commerce_returns r ON r.id=l.return_id JOIN commerce_orders o ON o.id=r.order_id AND o.company_id=r.company_id AND o.kind='sales' JOIN commerce_fulfillments f ON f.id=r.fulfillment_id AND f.order_id=o.id AND f.invoice_id=r.invoice_id JOIN commerce_fulfillment_lines fl ON fl.id=l.fulfillment_line_id AND fl.fulfillment_id=f.id JOIN commerce_order_lines ol ON ol.id=fl.order_line_id AND ol.order_id=o.id AND ol.product_id=l.product_id AND ol.item_type<>'service' LEFT JOIN commerce_return_lot_allocations a ON a.return_line_id=l.id AND a.id=$4 WHERE r.kind='customer_return' AND r.status IN ('approved','posted') AND r.approved_by IS NOT NULL AND r.company_id=$1 AND ($2::text IS NULL OR r.branch_id=$2) AND (($3::text IS NOT NULL AND l.id=$3 AND NOT EXISTS(SELECT 1 FROM commerce_return_lot_allocations x WHERE x.return_line_id=l.id)) OR ($4::text IS NOT NULL AND a.id=$4))")
        .bind(company.as_str()).bind(branch.map(RecordId::as_str)).bind(line).bind(allocation).fetch_optional(c).await?;
    let Some(row) = row else {
        return Ok((None, None));
    };
    let allocation_id: Option<String> = row.try_get("allocation_id")?;
    Ok((
        Some(QualityReturnCommerceFacts {
            return_id: row.try_get("return_id")?,
            company_id: row.try_get("company_id")?,
            branch_id: row.try_get("branch_id")?,
            warehouse_id: row.try_get("warehouse_id")?,
            product_id: row.try_get("product_id")?,
            source_id: allocation_id
                .clone()
                .unwrap_or_else(|| line.unwrap_or_default().into()),
            approver: row.try_get("approved_by")?,
            line_ordinal: row.try_get("line_ordinal")?,
            lot_ordinal: row.try_get("lot_ordinal")?,
            lot_id: row.try_get("lot_id")?,
            fulfillment_line_id: row.try_get("fulfillment_line_id")?,
            fulfillment_allocation_id: row.try_get("fulfillment_allocation_id")?,
            quantity_milli: row.try_get("quantity_milli")?,
            value_minor: row.try_get("value_minor")?,
        }),
        Some(row.try_get("image")?),
    ))
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("COMMERCE_QUALITY_TRANSACTION_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMERCE_QUALITY_SOURCE_CHANGED",
        "The customer return source changed.",
    )
}
