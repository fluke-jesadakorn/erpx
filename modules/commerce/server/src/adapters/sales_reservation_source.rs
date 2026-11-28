//! Commerce-owned order/line source witness for Inventory reservation effects.
//! This does not authorize an actor or purchase; composition must hold and
//! recheck those independently in this same serializable transaction.
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
type Image = (
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    i64,
    Option<String>,
    String,
    String,
    i64,
    i64,
    String,
    String,
);

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("COMMERCE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(&mut *c).await?)
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "RESERVATION_SOURCE_CHANGED",
        "The order changed. Refresh and retry.",
    )
    .into()
}
fn missing() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "ORDER_NOT_FOUND",
        "Order not found in this workspace.",
    )
    .into()
}

/// Scoped source parameters are not an IAM grant. Only the composition layer's
/// current actor and purchase witness can authorize the encompassing command.
pub struct SalesReservationScope<'a> {
    pub company_id: &'a str,
    pub branch_id: Option<&'a str>,
    pub department_id: Option<&'a str>,
    pub order_id: &'a str,
    pub line_id: &'a str,
    pub expected_version: i64,
}

/// Opaque row image tied to the transaction that locked it. It is not an actor
/// or entitlement witness and does not itself permit any mutation.
pub struct SalesReservationSource {
    unit: Unit,
    image: Image,
}
impl SalesReservationSource {
    pub fn line_id(&self) -> &str {
        &self.image.8
    }
    pub fn order_id(&self) -> &str {
        &self.image.0
    }
    pub fn company_id(&self) -> &str {
        &self.image.1
    }
    pub fn warehouse_id(&self) -> Option<&str> {
        self.image.7.as_deref()
    }
    pub fn product_id(&self) -> &str {
        &self.image.9
    }
    /// The Finance owner must independently attest this party before command
    /// execution; retaining the ID prevents swapping it in the same unit.
    pub fn party_id(&self) -> &str {
        &self.image.13
    }
    pub fn quantity_milli(&self) -> i64 {
        self.image.10
    }
    pub fn fulfilled_milli(&self) -> i64 {
        self.image.11
    }
    /// Recheck the legacy active warehouse/product guard on this same unit.
    /// This is source evidence only, not actor or purchase authorization.
    pub async fn require_active_stock_scope(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_current(c).await?;
        let allowed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM commerce_warehouses w JOIN commerce_products p ON p.company_id=w.company_id WHERE w.id=$1 AND p.id=$2 AND w.company_id=$3 AND w.status='active' AND p.status='active')")
            .bind(self.warehouse_id()).bind(self.product_id()).bind(self.company_id())
            .fetch_one(&mut *c).await?;
        if !allowed {
            return Err(changed());
        }
        Ok(())
    }
    /// Recheck both image and transaction; after Commerce mutates its order
    /// version, composition must use its own post-effect witness instead.
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let current = read_image(
            c,
            &self.image.0,
            &self.image.8,
            &self.image.1,
            self.image.2.as_deref(),
            self.image.3.as_deref(),
        )
        .await?;
        let same = current.as_ref().is_some_and(|v| {
            v.0 == self.image.0
                && v.1 == self.image.1
                && v.2 == self.image.2
                && v.3 == self.image.3
                && v.4 == self.image.4
                && v.5 == self.image.5
                && v.6 == self.image.6
                && v.7 == self.image.7
                && v.8 == self.image.8
                && v.9 == self.image.9
                && v.10 == self.image.10
                && v.11 == self.image.11
                && v.12 == self.image.12
                && v.13 == self.image.13
        });
        if !same {
            return Err(changed());
        }
        Ok(())
    }
}
async fn read_image(
    c: &mut PgConnection,
    order_id: &str,
    line_id: &str,
    company_id: &str,
    branch_id: Option<&str>,
    department_id: Option<&str>,
) -> Result<Option<Image>, Error> {
    Ok(sqlx::query_as("SELECT o.id,o.company_id,o.branch_id,o.department_id,o.kind,o.state,o.version,o.warehouse_id,l.id,l.product_id,l.quantity_milli,l.fulfilled_milli,l.item_type,o.party_id FROM commerce_orders o JOIN commerce_order_lines l ON l.order_id=o.id WHERE o.id=$1 AND l.id=$2 AND o.company_id=$3 AND ($4::text IS NULL OR o.branch_id=$4) AND ($5::text IS NULL OR o.department_id=$5) FOR UPDATE OF o,l")
        .bind(order_id).bind(line_id).bind(company_id).bind(branch_id).bind(department_id).fetch_optional(&mut *c).await?)
}

/// Lock and verify the exact order and line in the caller's retained unit.
/// Product kind, current permission, entitlement and actor freshness remain
/// independent required checks; source identity alone grants no authority.
pub async fn lock_sales_reservation_source(
    c: &mut PgConnection,
    scope: SalesReservationScope<'_>,
) -> Result<SalesReservationSource, Error> {
    if scope.company_id.is_empty()
        || scope.order_id.is_empty()
        || scope.line_id.is_empty()
        || scope.expected_version < 1
    {
        return Err(missing());
    }
    let unit = unit(c).await?;
    let image = read_image(
        c,
        scope.order_id,
        scope.line_id,
        scope.company_id,
        scope.branch_id,
        scope.department_id,
    )
    .await?
    .ok_or_else(missing)?;
    if image.1 != scope.company_id
        || scope
            .branch_id
            .is_some_and(|id| image.2.as_deref() != Some(id))
        || scope
            .department_id
            .is_some_and(|id| image.3.as_deref() != Some(id))
    {
        return Err(missing());
    }
    if image.4 != "sales" {
        return Err(BusinessError::invalid(
            "SALES_ORDER_REQUIRED",
            "Reserve stock against an approved sales order.",
        )
        .into());
    }
    if image.6 != scope.expected_version
        || !matches!(image.5.as_str(), "confirmed" | "partially_fulfilled")
    {
        return Err(changed());
    }
    if image.12 == "service" {
        return Err(BusinessError::invalid(
            "SERVICE_RESERVATION_FORBIDDEN",
            "Services do not reserve inventory.",
        )
        .into());
    }
    Ok(SalesReservationSource { unit, image })
}
