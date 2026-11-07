//! Service Count revision owner port. All effects require one retained
//! serializable Holding transaction and external current authority.
use crate::adapters::service_initial_physical_count::{
    CurrentInitialCountInventoryHistory, CurrentInitialCountRecord, read_initial_count_record,
};
use crate::application::service_initial_count_revision::{
    InitialCountRevision, InitialCountRevisionAllocation,
};
use crate::domain::{
    consumption::TrackingMode,
    service_initial_physical_count::{js_trim, valid_date},
    valuation::{safe, scaled_inventory_value},
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection};
use std::collections::{BTreeMap, BTreeSet};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED",
        "The original Service first Count source changed.",
    )
    .into()
}
fn invalid(code: &'static str, message: &'static str) -> Error {
    BusinessError::invalid(code, message).into()
}
fn field<'a>(v: &'a Value, k: &str) -> Result<&'a str, Error> {
    v[k].as_str().ok_or_else(changed)
}
fn number(v: &Value, k: &str) -> Result<i64, Error> {
    v[k].as_i64().ok_or_else(changed)
}
fn rows(v: &Value) -> Result<Vec<&Value>, Error> {
    if v.is_null() {
        Ok(vec![])
    } else {
        Ok(v.as_array().ok_or_else(changed)?.iter().collect())
    }
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("INVENTORY_INITIAL_COUNT_UNIT_INVALID").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
#[derive(Clone)]
struct PlannedLot {
    image: Value,
}
/// An independently planned afterimage, tied to the original source witnesses.
/// It conveys no permission or purchase authority.
pub struct InitialCountRevisionPlan {
    unit: Unit,
    input: InitialCountRevision,
    before: String,
    history: String,
    after: Value,
    lots: Vec<PlannedLot>,
}
fn lot_plan(
    input: &InitialCountRevision,
    a: &InitialCountRevisionAllocation,
    index: usize,
    current: &BTreeMap<String, (&Value, &Value)>,
    seen: &mut BTreeSet<String>,
    identities: &mut BTreeSet<String>,
) -> Result<(PlannedLot, i64), Error> {
    let source = match &a.lot_id {
        Some(id) => Some(*current.get(id).ok_or_else(|| {
            invalid(
                "COUNT_LOT_INVALID",
                "Choose a current lot or serial from this warehouse.",
            )
        })?),
        None => None,
    };
    if let Some(id) = &a.lot_id
        && !seen.insert(id.clone())
    {
        return Err(invalid(
            "DUPLICATE_COUNT_LOT",
            "Count each lot or serial once.",
        ));
    }
    let (mode, raw_id) = match input.product.tracking {
        TrackingMode::Lot => (
            "lot",
            source
                .and_then(|(l, _)| l["lot_code"].as_str())
                .or(a.lot_code.as_deref()),
        ),
        TrackingMode::Serial => (
            "serial",
            source
                .and_then(|(l, _)| l["serial_number"].as_str())
                .or(a.serial_number.as_deref()),
        ),
        TrackingMode::None => {
            return Err(invalid(
                "LOT_ALLOCATION_NOT_ALLOWED",
                "Lot allocations are only valid for tracked products.",
            ));
        }
    };
    let identity = raw_id
        .map(js_trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            invalid(
                "TRACKING_IDENTITY_REQUIRED",
                "Enter the lot or serial number found during the count.",
            )
        })?;
    if !identities.insert(identity.to_lowercase()) {
        return Err(invalid(
            "DUPLICATE_TRACKING_IDENTITY",
            "Each lot code or serial number may appear once per product.",
        ));
    }
    safe(a.quantity_milli)?;
    if a.quantity_milli < 0 {
        return Err(invalid(
            "INVALID_COUNT_QUANTITY",
            "Use a nonnegative physical count.",
        ));
    }
    if mode == "serial" && ![0, 1000].contains(&a.quantity_milli) {
        return Err(invalid(
            "SERIAL_QUANTITY_INVALID",
            "Each serial number must be counted as zero or one unit.",
        ));
    }
    let (old_qty, old_value, old_version) = if let Some((_, b)) = source {
        (
            number(b, "quantity_milli")?,
            number(b, "value_minor")?,
            number(b, "version")?,
        )
    } else {
        (0, 0, 0)
    };
    if source.is_none() && a.quantity_milli == 0 {
        return Err(invalid(
            "FOUND_LOT_QUANTITY_REQUIRED",
            "A newly found lot or serial must have a positive counted quantity.",
        ));
    }
    if source.is_none() && a.unit_cost_minor.unwrap_or(0) <= 0 {
        return Err(invalid(
            "VALUATION_REQUIRED",
            "Enter a verified unit cost for a newly found lot or serial.",
        ));
    }
    let value = if a.quantity_milli == 0 {
        0
    } else if source.is_some() {
        scaled_inventory_value(old_value, a.quantity_milli, old_qty)?
    } else {
        scaled_inventory_value(a.unit_cost_minor.unwrap_or(0), a.quantity_milli, 1000)?
    };
    if a.quantity_milli > 0 && value == 0 {
        return Err(invalid(
            "ZERO_VALUATION",
            "A positive lot count requires a positive inventory valuation.",
        ));
    }
    let result_id = if let Some((lot, _)) = source {
        field(lot, "id")?.to_owned()
    } else {
        let hash = format!(
            "{:x}",
            Sha256::digest(
                format!(
                    "{}:{}:{}:{}",
                    input.company_id,
                    input.product_id,
                    mode,
                    identity.to_lowercase()
                )
                .as_bytes()
            )
        );
        format!("lot-{}", &hash[..32])
    };
    let date = |name: &str, requested: &Option<String>| -> Option<String> {
        source
            .and_then(|(l, _)| l[name].as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .or_else(|| requested.as_ref().filter(|s| !s.is_empty()).cloned())
    };
    let line = format!("{}-0", input.count_id);
    let image = json!({"id":format!("{}-lot-{}",line,index),"count_line_id":line,"source_lot_id":a.lot_id,"result_lot_id":result_id,"tracking_mode":mode,"lot_code":if mode=="lot"{Some(identity)}else{None},"serial_number":if mode=="serial"{Some(identity)}else{None},"manufactured_date":date("manufactured_date",&a.manufactured_date),"expiry_date":date("expiry_date",&a.expiry_date),"snapshot_quantity_milli":old_qty,"snapshot_value_minor":old_value,"snapshot_version":old_version,"counted_quantity_milli":a.quantity_milli,"counted_value_minor":value,"unit_cost_minor":if source.is_some(){None}else{a.unit_cost_minor}});
    Ok((PlannedLot { image }, value))
}
struct GenericLinePlan {
    value: i64,
    lots: Vec<PlannedLot>,
    old_qty: i64,
    old_value: i64,
    old_version: i64,
    observed: Value,
}
fn generic_line(
    input: &InitialCountRevision,
    history: &CurrentInitialCountInventoryHistory,
) -> Result<GenericLinePlan, Error> {
    if !valid_date(&input.count_date)
        || !(5..=2000).contains(&js_trim(&input.reason).encode_utf16().count())
        || input.product.version <= 0
        || input.actor_id.is_empty()
        || input.allocations.len() > 500
    {
        return Err(changed());
    }
    safe(input.quantity_milli)?;
    if input.quantity_milli < 0 {
        return Err(invalid(
            "INVALID_COUNT_QUANTITY",
            "Use a nonnegative physical count.",
        ));
    }
    let observed: Value = serde_json::from_str(&history.data().image).map_err(|_| changed())?;
    let stocks = rows(&observed["stock"])?;
    if stocks.len() > 1 {
        return Err(changed());
    }
    let (old_qty, old_value, old_version) = if let Some(stock) = stocks.first() {
        (
            number(stock, "quantity_milli")?,
            number(stock, "value_minor")?,
            number(stock, "version")?,
        )
    } else {
        (0, 0, 0)
    };
    let mut current = BTreeMap::new();
    for item in rows(&observed["lots"])? {
        let lot = &item["lot"];
        let balance = &item["balance"];
        if number(balance, "quantity_milli")? > 0 {
            current.insert(field(lot, "id")?.to_owned(), (lot, balance));
        }
    }
    let mut seen = BTreeSet::new();
    let mut identities = BTreeSet::new();
    let mut lots = Vec::new();
    let value = if input.product.tracking == TrackingMode::None {
        if !input.allocations.is_empty() {
            return Err(invalid(
                "LOT_ALLOCATION_NOT_ALLOWED",
                "Lot allocations are only valid for tracked products.",
            ));
        }
        if old_qty == 0 && input.quantity_milli > 0 && input.unit_cost_minor.unwrap_or(0) <= 0 {
            return Err(invalid(
                "VALUATION_REQUIRED",
                "Enter a verified unit cost when counting stock without an existing valuation.",
            ));
        }
        if input.quantity_milli == 0 {
            0
        } else if old_qty > 0 {
            scaled_inventory_value(old_value, input.quantity_milli, old_qty)?
        } else {
            scaled_inventory_value(
                input.unit_cost_minor.unwrap_or(0),
                input.quantity_milli,
                1000,
            )?
        }
    } else {
        let mut sum = 0_i64;
        let mut quantity = 0_i64;
        for (index, a) in input.allocations.iter().enumerate() {
            let (lot, value) = lot_plan(input, a, index, &current, &mut seen, &mut identities)?;
            sum = safe(sum.checked_add(value).ok_or_else(changed)?)?;
            quantity = safe(quantity.checked_add(a.quantity_milli).ok_or_else(changed)?)?;
            lots.push(lot);
        }
        if current.keys().any(|id| !seen.contains(id)) {
            return Err(invalid(
                "COUNT_LOT_MISSING",
                "Count every current lot or serial, including zero when it is missing physically.",
            ));
        }
        if quantity != input.quantity_milli {
            return Err(invalid(
                "COUNT_LOT_TOTAL_MISMATCH",
                "The lot or serial quantities must equal the product count.",
            ));
        }
        sum
    };
    if input.quantity_milli > 0 && value == 0 {
        return Err(invalid(
            "ZERO_VALUATION",
            "A positive count requires a positive inventory valuation.",
        ));
    }
    Ok(GenericLinePlan {
        value,
        lots,
        old_qty,
        old_value,
        old_version,
        observed,
    })
}
/// Validate generic line input against its retained pair before Service lineage checks.
pub async fn validate_generic_revision_line(
    c: &mut PgConnection,
    history: &CurrentInitialCountInventoryHistory,
    input: &InitialCountRevision,
) -> Result<(), Error> {
    unit(c).await?;
    history.require_current(c).await?;
    if history.selection()
        != (
            input.warehouse_id.as_str(),
            input.product_id.as_str(),
            Some(input.count_id.as_str()),
        )
    {
        return Err(changed());
    }
    generic_line(input, history)?;
    Ok(())
}
/// Check the original generic movement-date assertion after all line inputs
/// have been validated, without broadening access to another owner.
pub async fn check_generic_revision_movement(
    c: &mut PgConnection,
    history: &CurrentInitialCountInventoryHistory,
    count_date: &str,
) -> Result<(), Error> {
    history.require_current(c).await?;
    let observed: Value = serde_json::from_str(&history.data().image).map_err(|_| changed())?;
    if rows(&observed["ledger"])?
        .iter()
        .any(|v| v["movement_date"].as_str().is_some_and(|d| d > count_date))
    {
        return Err(BusinessError::new(FailureKind::Conflict,"CONFLICT","The record changed, a reference is unavailable, or this value already exists. Refresh and review again.").into());
    }
    Ok(())
}
/// Plan the original generic one-line rewrite from a witnessed preimage.
pub async fn prepare_initial_count_revision(
    c: &mut PgConnection,
    before: &CurrentInitialCountRecord,
    history: &CurrentInitialCountInventoryHistory,
    input: InitialCountRevision,
) -> Result<InitialCountRevisionPlan, Error> {
    let transaction = unit(c).await?;
    before.require_current(c).await?;
    history.require_current(c).await?;
    if history.selection()
        != (
            input.warehouse_id.as_str(),
            input.product_id.as_str(),
            Some(input.count_id.as_str()),
        )
    {
        return Err(changed());
    }
    let prior: Value = serde_json::from_str(&before.data().image).map_err(|_| changed())?;
    let source = &prior["source"];
    let count = &prior["count"];
    let lines = rows(&prior["lines"])?;
    if lines.len() != 1 {
        return Err(changed());
    }
    let line = lines[0];
    if field(source, "company_id")? != input.company_id
        || field(source, "branch_id")? != input.branch_id
        || field(source, "warehouse_id")? != input.warehouse_id
        || field(source, "product_id")? != input.product_id
        || field(count, "id")? != input.count_id
        || field(line, "product_id")? != input.product_id
    {
        return Err(changed());
    }
    if !["draft", "rejected"].contains(&field(count, "status")?) {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "COUNT_LOCKED",
            "Only the preparer can edit a draft or returned count.",
        )
        .into());
    }
    let GenericLinePlan {
        value,
        mut lots,
        old_qty,
        old_value,
        old_version,
        observed,
    } = generic_line(&input, history)?;
    if number(count, "version")? != input.expected_version {
        return Err(BusinessError::new(FailureKind::Conflict,"CONFLICT","The record changed, a reference is unavailable, or this value already exists. Refresh and review again.").into());
    }
    if rows(&observed["ledger"])?.iter().any(|v| {
        v["movement_date"]
            .as_str()
            .is_some_and(|d| d > input.count_date.as_str())
    }) {
        return Err(BusinessError::new(FailureKind::Conflict,"CONFLICT","The record changed, a reference is unavailable, or this value already exists. Refresh and review again.").into());
    }
    // The immutable 1062 source requires the original zero stock snapshot.
    if (old_qty, old_value, old_version) != (0, 0, 0) {
        return Err(changed());
    }
    let mut after = prior.clone();
    let count = after["count"].as_object_mut().ok_or_else(changed)?;
    for (k, v) in [
        ("count_date", json!(input.count_date)),
        ("reason", json!(js_trim(&input.reason))),
        ("net_value_minor", json!(value)),
        ("status", json!("draft")),
        ("prepared_by", json!(input.actor_id)),
        ("review_reason", Value::Null),
        (
            "version",
            json!(input.expected_version.checked_add(1).ok_or_else(changed)?),
        ),
        ("updated_at", json!(input.now)),
    ] {
        count.insert(k.into(), v);
    }
    let line = after["lines"][0].as_object_mut().ok_or_else(changed)?;
    for (k, v) in [
        ("id", json!(format!("{}-0", input.count_id))),
        ("product_version", json!(input.product.version)),
        ("description", json!(input.product.description)),
        ("unit", json!(input.product.unit)),
        ("snapshot_quantity_milli", json!(0)),
        ("snapshot_value_minor", json!(0)),
        ("snapshot_version", json!(0)),
        ("counted_quantity_milli", json!(input.quantity_milli)),
        ("counted_value_minor", json!(value)),
        (
            "unit_cost_minor",
            json!(if input.product.tracking == TrackingMode::None {
                input.unit_cost_minor
            } else {
                None
            }),
        ),
        ("ordinal", json!(0)),
    ] {
        line.insert(k.into(), v);
    }
    lots.sort_by(|a, b| a.image["id"].as_str().cmp(&b.image["id"].as_str()));
    after["lots"] = if lots.is_empty() {
        Value::Null
    } else {
        Value::Array(lots.iter().map(|x| x.image.clone()).collect())
    };
    Ok(InitialCountRevisionPlan {
        unit: transaction,
        input,
        before: before.data().image.clone(),
        history: history.data().image.clone(),
        after,
        lots,
    })
}
impl InitialCountRevisionPlan {
    /// Applies only the planned Inventory rows. The caller must retain and
    /// revalidate authority, source and Workflow evidence in the same unit.
    pub async fn apply(
        &self,
        c: &mut PgConnection,
        before: &CurrentInitialCountRecord,
        history: &CurrentInitialCountInventoryHistory,
    ) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        if before.data().image != self.before || history.data().image != self.history {
            return Err(changed());
        }
        before.require_current(c).await?;
        history.require_current(c).await?;
        let input = &self.input;
        let count = &self.after["count"];
        let line = &self.after["lines"][0];
        let updated=sqlx::query("UPDATE inventory_counts SET count_date=$1,reason=$2,net_value_minor=$3,status='draft',prepared_by=$4,review_reason=NULL,version=version+1,updated_at=$5 WHERE id=$6 AND company_id=$7 AND version=$8 AND status IN ('draft','rejected')").bind(&input.count_date).bind(js_trim(&input.reason)).bind(number(count,"net_value_minor")?).bind(&input.actor_id).bind(input.now).bind(&input.count_id).bind(&input.company_id).bind(input.expected_version).execute(&mut *c).await?.rows_affected();
        if updated != 1 {
            return Err(changed());
        }
        sqlx::query("DELETE FROM inventory_count_lot_lines WHERE count_line_id IN (SELECT id FROM inventory_count_lines WHERE count_id=$1)").bind(&input.count_id).execute(&mut *c).await?;
        sqlx::query("DELETE FROM inventory_count_lines WHERE count_id=$1")
            .bind(&input.count_id)
            .execute(&mut *c)
            .await?;
        sqlx::query("INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,unit_cost_minor,ordinal) VALUES($1,$2,$3,$4,$5,$6,0,0,0,$7,$8,$9,0)").bind(field(line,"id")?).bind(&input.count_id).bind(&input.product_id).bind(input.product.version).bind(&input.product.description).bind(&input.product.unit).bind(input.quantity_milli).bind(number(line,"counted_value_minor")?).bind(line["unit_cost_minor"].as_i64()).execute(&mut *c).await?;
        for lot in &self.lots {
            let l = &lot.image;
            sqlx::query("INSERT INTO inventory_count_lot_lines(id,count_line_id,source_lot_id,result_lot_id,tracking_mode,lot_code,serial_number,manufactured_date,expiry_date,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,unit_cost_minor) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)").bind(field(l,"id")?).bind(field(l,"count_line_id")?).bind(l["source_lot_id"].as_str()).bind(field(l,"result_lot_id")?).bind(field(l,"tracking_mode")?).bind(l["lot_code"].as_str()).bind(l["serial_number"].as_str()).bind(l["manufactured_date"].as_str()).bind(l["expiry_date"].as_str()).bind(number(l,"snapshot_quantity_milli")?).bind(number(l,"snapshot_value_minor")?).bind(number(l,"snapshot_version")?).bind(number(l,"counted_quantity_milli")?).bind(number(l,"counted_value_minor")?).bind(l["unit_cost_minor"].as_i64()).execute(&mut *c).await?;
        }
        history.require_current(c).await?;
        self.require_current(c).await
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let now = read_initial_count_record(
            c,
            &self.input.company_id,
            Some(&self.input.branch_id),
            &self.input.count_id,
        )
        .await?
        .ok_or_else(changed)?;
        let image: Value = serde_json::from_str(&now.data().image).map_err(|_| changed())?;
        if image != self.after {
            return Err(changed());
        }
        now.require_current(c).await
    }
}
