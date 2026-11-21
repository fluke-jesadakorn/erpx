//! Current inventory movement-history read over `inventory_ledger`. The caller
//! supplies the already-authorized Company/branch scope and owning transaction.
use crate::domain::movement::{
    MAX_ROWS, MovementRow, encode_cursor, parse_cursor, validate_row,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

fn invalid() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "INVENTORY_MOVEMENT_INVALID",
        "The Inventory movement source is unavailable.",
    )
    .into()
}

async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("INVENTORY_OWNER_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("INVENTORY_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub async fn lock_movement_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query("LOCK TABLE inventory_ledger,commerce_products,commerce_warehouses IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}

type Row = (
    String,
    String,
    String,
    String,
    String,
    i64,
    i64,
    String,
    String,
    String,
    String,
    String,
    i64,
);

const MOVEMENTS: &str = "SELECT m.id,m.warehouse_id,m.product_id,p.sku,w.name warehouse_name,\
    m.quantity_milli,m.value_minor,m.movement_date,m.source_type,m.source_id,m.source_number,\
    m.movement_date fulfillment_date,m.created_at \
    FROM inventory_ledger m JOIN commerce_products p ON p.id=m.product_id \
    JOIN commerce_warehouses w ON w.id=m.warehouse_id \
    WHERE w.company_id=$1 AND ($2::text IS NULL OR w.branch_id=$2) \
      AND ($3::bigint IS NULL OR m.created_at < $3 OR (m.created_at = $3 AND m.id < $4)) \
    ORDER BY m.created_at DESC,m.id DESC LIMIT 51";

pub async fn read_movements(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    cursor: Option<&str>,
) -> Result<(Vec<MovementRow>, Option<String>), Error> {
    serializable(c).await?;
    if company.is_empty() || company.len() > 200 {
        return Err(invalid());
    }
    let (created_at, id) = match cursor {
        Some(value) => {
            let (created, id) = parse_cursor(value)?;
            (Some(created), id)
        }
        None => (None, String::new()),
    };
    let rows: Vec<Row> = sqlx::query_as(MOVEMENTS)
        .bind(company)
        .bind(branch)
        .bind(created_at)
        .bind(&id)
        .fetch_all(&mut *c)
        .await?;
    let has_more = rows.len() > MAX_ROWS;
    let rows = rows.into_iter().take(MAX_ROWS);
    let mut items = Vec::new();
    for (
        id,
        warehouse_id,
        product_id,
        sku,
        warehouse_name,
        quantity_milli,
        value_minor,
        movement_date,
        source_type,
        source_id,
        source_number,
        fulfillment_date,
        created_at,
    ) in rows
    {
        items.push(validate_row(MovementRow {
            id,
            warehouse_id,
            product_id,
            sku,
            warehouse_name,
            quantity_milli,
            value_minor,
            movement_date,
            source_type,
            source_id,
            order_number: source_number,
            fulfillment_date,
            created_at,
        })?);
    }
    let next = if has_more {
        items
            .last()
            .map(|row| encode_cursor(row.created_at, &row.id))
    } else {
        None
    };
    Ok((items, next))
}
