//! Commerce-owned portion of a historical receiving origin. Inventory owns
//! the allocation and Quality owns the hold; neither is inferred here.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityReceiptCommerceFacts {
    pub order_id: String,
    pub company_id: String,
    pub branch_id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub fulfillment_line_id: String,
}
pub struct CurrentQualityReceiptCommerce {
    company: CompanyId,
    branch: Option<RecordId>,
    line: String,
    unit: Unit,
    facts: Option<QualityReceiptCommerceFacts>,
    image: Option<String>,
}
impl CurrentQualityReceiptCommerce {
    pub fn facts(&self) -> Option<&QualityReceiptCommerceFacts> {
        self.facts.as_ref()
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed().into());
        }
        let (facts, image) = read(c, &self.company, self.branch.as_ref(), &self.line).await?;
        if facts != self.facts || image != self.image {
            return Err(changed().into());
        }
        Ok(())
    }
}
/// Acquire alongside all other owner source locks before the first snapshot.
pub async fn lock_quality_receipt_commerce(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("COMMERCE_QUALITY_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE commerce_orders,commerce_order_lines,commerce_fulfillments,commerce_fulfillment_lines IN SHARE MODE").execute(c).await?;
    Ok(())
}
/// The line ID is a selector obtained from an Inventory-owned witness, not an
/// origin proof. Caller must retain that witness and compare hold/lot/quantity.
pub async fn inspect_quality_receipt_commerce(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&RecordId>,
    fulfillment_line_id: &str,
) -> Result<CurrentQualityReceiptCommerce, Error> {
    if fulfillment_line_id.is_empty() || fulfillment_line_id.len() > 1024 {
        return Err(changed().into());
    }
    let unit = unit(c).await?;
    let (facts, image) = read(c, company, branch, fulfillment_line_id).await?;
    Ok(CurrentQualityReceiptCommerce {
        company: company.clone(),
        branch: branch.cloned(),
        line: fulfillment_line_id.into(),
        unit,
        facts,
        image,
    })
}
async fn read(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&RecordId>,
    line: &str,
) -> Result<(Option<QualityReceiptCommerceFacts>, Option<String>), Error> {
    let row = sqlx::query("SELECT o.id order_id,o.company_id,o.branch_id,o.warehouse_id,ol.product_id,fl.id line_id, encode(sha256(convert_to(jsonb_build_array(to_jsonb(o),to_jsonb(ol),to_jsonb(f),to_jsonb(fl))::text,'UTF8')),'hex') image FROM commerce_fulfillment_lines fl JOIN commerce_order_lines ol ON ol.id=fl.order_line_id AND ol.item_type<>'service' JOIN commerce_fulfillments f ON f.id=fl.fulfillment_id JOIN commerce_orders o ON o.id=f.order_id AND o.id=ol.order_id AND o.kind='purchase' WHERE fl.id=$1 AND o.company_id=$2 AND ($3::text IS NULL OR o.branch_id=$3) FOR SHARE OF o,ol,f,fl")
        .bind(line).bind(company.as_str()).bind(branch.map(RecordId::as_str)).fetch_optional(c).await?;
    let Some(row) = row else {
        return Ok((None, None));
    };
    Ok((
        Some(QualityReceiptCommerceFacts {
            order_id: row.try_get("order_id")?,
            company_id: row.try_get("company_id")?,
            branch_id: row.try_get("branch_id")?,
            warehouse_id: row.try_get("warehouse_id")?,
            product_id: row.try_get("product_id")?,
            fulfillment_line_id: row.try_get("line_id")?,
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
        "The receiving source changed.",
    )
}
