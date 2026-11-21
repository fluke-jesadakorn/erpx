//! Inventory-owned facts for the generic Service Count detail. Other owners
//! provide enrichment and Workflow history at the composition boundary.
use crate::adapters::service_initial_physical_count::CurrentInitialCountRecord;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use sqlx::{PgConnection, Row};

type Error = PgTransactionError<BusinessError>;
fn invalid() -> Error {
    BusinessError::invalid("SERVICE_COUNT_DETAIL_INVALID", "The original Count detail is unavailable.").into()
}
fn objects(value: &Value, key: &str) -> Result<Vec<Value>, Error> {
    match &value[key] {
        Value::Null => Ok(Vec::new()),
        Value::Array(items) if items.iter().all(Value::is_object) => Ok(items.clone()),
        _ => Err(invalid()),
    }
}
/// Retained source-bound Inventory fields. These are not an authorization
/// token; the outer Holding unit must enforce current authority and purchases.
pub async fn read_service_count_generic_facts(c: &mut PgConnection, source: &CurrentInitialCountRecord) -> Result<Value, Error> {
    // require_current validates the witness's serializable transaction identity,
    // selected Company/branch, and complete retained source image.
    source.require_current(c).await?;
    let image: Value=serde_json::from_str(&source.data().image).map_err(|_|invalid())?;
    let count=image.get("count").filter(|v|v.is_object()).cloned().ok_or_else(invalid)?;
    let mut lines=objects(&image,"lines")?;
    let mut lots=objects(&image,"lots")?;
    let mut decisions=objects(&image,"decisions")?;
    lines.sort_by_key(|v|v["ordinal"].as_i64().unwrap_or(i64::MAX));
    // Service source has exactly one line; order matches generic l.ordinal,cl.id.
    lots.sort_by(|a,b|a["id"].as_str().cmp(&b["id"].as_str()));
    let warehouse=count["warehouse_id"].as_str().ok_or_else(invalid)?;
    let products=lines.iter().map(|v|v["product_id"].as_str().map(str::to_owned).ok_or_else(invalid)).collect::<Result<Vec<_>,_>>()?;
    let stocks=sqlx::query("SELECT product_id,quantity_milli,value_minor,version FROM commerce_stock WHERE warehouse_id=$1 AND product_id=ANY($2)").bind(warehouse).bind(&products).fetch_all(&mut *c).await?;
    let mut stock_by_product=std::collections::BTreeMap::new();
    for row in stocks { stock_by_product.insert(row.try_get::<String,_>("product_id")?,(row.try_get::<i64,_>("quantity_milli")?,row.try_get::<i64,_>("value_minor")?,row.try_get::<i64,_>("version")?)); }
    for line in &mut lines {
        let id=line["product_id"].as_str().ok_or_else(invalid)?.to_owned();
        let value=line.as_object_mut().ok_or_else(invalid)?;
        if let Some((quantity,amount,version))=stock_by_product.get(&id) {
            value.insert("current_quantity_milli".into(),json!(quantity));
            value.insert("current_value_minor".into(),json!(amount));
            value.insert("current_version".into(),json!(version));
        } else {
            value.insert("current_quantity_milli".into(),Value::Null);
            value.insert("current_value_minor".into(),Value::Null);
            value.insert("current_version".into(),json!(0));
        }
    }
    let ids=lots.iter().map(|v|v["result_lot_id"].as_str().map(str::to_owned).ok_or_else(invalid)).collect::<Result<Vec<_>,_>>()?;
    let rows=sqlx::query("SELECT lot_id,quantity_milli,value_minor,version FROM inventory_lot_balances WHERE warehouse_id=$1 AND lot_id=ANY($2)").bind(warehouse).bind(&ids).fetch_all(&mut *c).await?;
    let mut balances=std::collections::BTreeMap::new();
    for row in rows { balances.insert(row.try_get::<String,_>("lot_id")?,(row.try_get::<i64,_>("quantity_milli")?,row.try_get::<i64,_>("value_minor")?,row.try_get::<i64,_>("version")?)); }
    for lot in &mut lots {
        let id=lot["result_lot_id"].as_str().ok_or_else(invalid)?.to_owned();
        let value=lot.as_object_mut().ok_or_else(invalid)?;
        if let Some((quantity,amount,version))=balances.get(&id) {
            value.insert("current_quantity_milli".into(),json!(quantity));
            value.insert("current_value_minor".into(),json!(amount));
            value.insert("current_version".into(),json!(version));
        } else {
            value.insert("current_quantity_milli".into(),Value::Null);
            value.insert("current_value_minor".into(),Value::Null);
            value.insert("current_version".into(),json!(0));
        }
    }
    decisions.sort_by(|a,b|b["created_at"].as_i64().cmp(&a["created_at"].as_i64()));
    let result=json!({"count":count,"lines":lines,"lotLines":lots,"decisions":decisions});
    if serde_json::to_vec(&result).map_err(|_|invalid())?.len()>2*1024*1024 { return Err(invalid()); }
    source.require_current(c).await?;
    Ok(result)
}
