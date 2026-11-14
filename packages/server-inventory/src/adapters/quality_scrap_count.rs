//! Inventory-owned draft Count creation for a Quality scrap disposition.
//!
//! The caller retains the actual Quality hold, Commerce stock/product and
//! Inventory lot witnesses. This port writes only the submitted draft Count
//! and its snapshot lines; posting and any stock/ledger movement remain the
//! separate Finance-approved Count flow.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityScrapCountLot {
    pub id: String,
    pub tracking_mode: String,
    pub lot_code: Option<String>,
    pub serial_number: Option<String>,
    pub manufactured_date: Option<String>,
    pub expiry_date: Option<String>,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub balance_version: i64,
    pub counted_quantity_milli: i64,
    pub counted_value_minor: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityScrapCountRequest {
    pub company_id: CompanyId,
    pub branch_id: String,
    pub warehouse_id: String,
    pub hold_id: String,
    pub count_id: String,
    pub line_id: String,
    pub number: String,
    pub count_date: String,
    pub reason: String,
    pub net_value_minor: i64,
    pub actor_id: String,
    pub now: i64,
    pub product_id: String,
    pub product_version: i64,
    pub description: String,
    pub unit: String,
    pub snapshot_quantity_milli: i64,
    pub snapshot_value_minor: i64,
    pub snapshot_version: i64,
    pub counted_quantity_milli: i64,
    pub counted_value_minor: i64,
    pub lots: Vec<QualityScrapCountLot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CountImage {
    status: String,
    net_value_minor: i64,
    quality_hold_id: Option<String>,
    number: String,
    count_date: String,
    reason: String,
}

pub struct PreparedQualityScrapCount {
    request: QualityScrapCountRequest,
    unit: Unit,
    before: Option<CountImage>,
}

/// Lock order is shared with the other Inventory count writers: counts and
/// their lines are acquired before the lot evidence tables.
pub async fn lock_quality_scrap_count_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query(
        "LOCK TABLE inventory_counts,inventory_count_lines,inventory_count_lot_lines IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}

/// Pure owner preparation: proves the target count identity is free and
/// retains an exact before-image. No Quality or stock policy is decided here.
pub async fn prepare_quality_scrap_count(
    c: &mut PgConnection,
    request: QualityScrapCountRequest,
) -> Result<PreparedQualityScrapCount, Error> {
    if request.count_id.is_empty()
        || request.line_id.is_empty()
        || request.lots.len() > 1000
        || request.net_value_minor > 0
        || request.snapshot_quantity_milli < 0
        || request.snapshot_value_minor < 0
        || request.counted_quantity_milli < 0
        || request.counted_value_minor < 0
    {
        return Err(invalid());
    }
    let unit = unit(c).await?;
    let before = read_count(c, &request.company_id, &request.count_id).await?;
    if before.is_some() {
        return Err(changed());
    }
    Ok(PreparedQualityScrapCount {
        request,
        unit,
        before,
    })
}

impl PreparedQualityScrapCount {
    pub fn count_id(&self) -> &str {
        &self.request.count_id
    }

    pub async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || read_count(c, &self.request.company_id, &self.request.count_id).await?
                != self.before
        {
            return Err(changed());
        }
        Ok(())
    }

    pub async fn apply(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_before(c).await?;
        let r = &self.request;
        sqlx::query(
            "INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,net_value_minor,created_by,prepared_by,submitted_at,created_at,updated_at,quality_hold_id) \
             VALUES($1,$2,$3,$4,$5,$6,$7,'draft',$8,$9,$9,$10,$10,$10,$11)",
        )
        .bind(&r.count_id)
        .bind(r.company_id.as_str())
        .bind(&r.branch_id)
        .bind(&r.warehouse_id)
        .bind(&r.number)
        .bind(&r.count_date)
        .bind(&r.reason)
        .bind(r.net_value_minor)
        .bind(&r.actor_id)
        .bind(r.now)
        .bind(&r.hold_id)
        .execute(&mut *c)
        .await?;
        sqlx::query(
            "INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,unit_cost_minor,ordinal) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,NULL,0)",
        )
        .bind(&r.line_id)
        .bind(&r.count_id)
        .bind(&r.product_id)
        .bind(r.product_version)
        .bind(&r.description)
        .bind(&r.unit)
        .bind(r.snapshot_quantity_milli)
        .bind(r.snapshot_value_minor)
        .bind(r.snapshot_version)
        .bind(r.counted_quantity_milli)
        .bind(r.counted_value_minor)
        .execute(&mut *c)
        .await?;
        for (index, lot) in r.lots.iter().enumerate() {
            sqlx::query(
                "INSERT INTO inventory_count_lot_lines(id,count_line_id,source_lot_id,result_lot_id,tracking_mode,lot_code,serial_number,manufactured_date,expiry_date,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,unit_cost_minor) \
                 VALUES($1,$2,$3,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,NULL)",
            )
            .bind(format!("{}-lot-{index}", r.line_id))
            .bind(&r.line_id)
            .bind(&lot.id)
            .bind(&lot.tracking_mode)
            .bind(&lot.lot_code)
            .bind(&lot.serial_number)
            .bind(&lot.manufactured_date)
            .bind(&lot.expiry_date)
            .bind(lot.quantity_milli)
            .bind(lot.value_minor)
            .bind(lot.balance_version)
            .bind(lot.counted_quantity_milli)
            .bind(lot.counted_value_minor)
            .execute(&mut *c)
            .await?;
        }
        let submitted = sqlx::query("UPDATE inventory_counts SET status='submitted' WHERE id=$1 AND company_id=$2 AND status='draft'")
            .bind(&r.count_id)
            .bind(r.company_id.as_str())
            .execute(&mut *c)
            .await?;
        if submitted.rows_affected() != 1 {
            return Err(changed());
        }
        self.require_after(c).await
    }

    pub async fn require_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let r = &self.request;
        let expected = CountImage {
            status: "submitted".into(),
            net_value_minor: r.net_value_minor,
            quality_hold_id: Some(r.hold_id.clone()),
            number: r.number.clone(),
            count_date: r.count_date.clone(),
            reason: r.reason.clone(),
        };
        if read_count(c, &r.company_id, &r.count_id).await?.as_ref() != Some(&expected) {
            return Err(changed());
        }
        let line: Option<(String, i64, i64, i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT product_id,product_version,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor FROM inventory_count_lines WHERE id=$1 AND count_id=$2",
        )
        .bind(&r.line_id)
        .bind(&r.count_id)
        .fetch_optional(&mut *c)
        .await?;
        if line
            != Some((
                r.product_id.clone(),
                r.product_version,
                r.snapshot_quantity_milli,
                r.snapshot_value_minor,
                r.snapshot_version,
                r.counted_quantity_milli,
                r.counted_value_minor,
            ))
        {
            return Err(changed());
        }
        let rows = sqlx::query(
            "SELECT id,tracking_mode,lot_code,serial_number,manufactured_date,expiry_date,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor \
             FROM inventory_count_lot_lines WHERE count_line_id=$1 ORDER BY id",
        )
        .bind(&r.line_id)
        .fetch_all(&mut *c)
        .await?;
        let mut actual = Vec::with_capacity(rows.len());
        for row in rows {
            actual.push((
                row.try_get::<String, _>("id")?,
                row.try_get::<String, _>("tracking_mode")?,
                row.try_get::<Option<String>, _>("lot_code")?,
                row.try_get::<Option<String>, _>("serial_number")?,
                row.try_get::<Option<String>, _>("manufactured_date")?,
                row.try_get::<Option<String>, _>("expiry_date")?,
                row.try_get::<i64, _>("snapshot_quantity_milli")?,
                row.try_get::<i64, _>("snapshot_value_minor")?,
                row.try_get::<i64, _>("snapshot_version")?,
                row.try_get::<i64, _>("counted_quantity_milli")?,
                row.try_get::<i64, _>("counted_value_minor")?,
            ));
        }
        let expected = r
            .lots
            .iter()
            .enumerate()
            .map(|(index, lot)| {
                (
                    format!("{}-lot-{index}", r.line_id),
                    lot.tracking_mode.clone(),
                    lot.lot_code.clone(),
                    lot.serial_number.clone(),
                    lot.manufactured_date.clone(),
                    lot.expiry_date.clone(),
                    lot.quantity_milli,
                    lot.value_minor,
                    lot.balance_version,
                    lot.counted_quantity_milli,
                    lot.counted_value_minor,
                )
            })
            .collect::<Vec<_>>();
        if actual != expected {
            return Err(changed());
        }
        Ok(())
    }
}

async fn read_count(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<CountImage>, Error> {
    let row = sqlx::query(
        "SELECT status,net_value_minor,quality_hold_id,number,count_date,reason FROM inventory_counts WHERE id=$1 AND company_id=$2",
    )
    .bind(id)
    .bind(company.as_str())
    .fetch_optional(c)
    .await?;
    row.map(|row| {
        Ok(CountImage {
            status: row.try_get("status")?,
            net_value_minor: row.try_get("net_value_minor")?,
            quality_hold_id: row.try_get("quality_hold_id")?,
            number: row.try_get("number")?,
            count_date: row.try_get("count_date")?,
            reason: row.try_get("reason")?,
        })
    })
    .transpose()
}

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}

fn invalid() -> Error {
    PersistenceError::new("INVENTORY_QUALITY_SCRAP_COUNT_TRANSACTION_REQUIRED").into()
}

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "INVENTORY_QUALITY_SCRAP_COUNT_CHANGED",
        "The Quality scrap Count changed.",
    )
    .into()
}
