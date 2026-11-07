//! Inventory-only pair history retained on a serializable Holding transaction.
//! Quality holds and Service posting lineage require their own owner witnesses.
use crate::domain::service_initial_physical_count::js_trim;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitialCountInventoryHistory {
    pub image: String,
    pub has_history: bool,
}
pub struct CurrentInitialCountInventoryHistory {
    warehouse: String,
    product: String,
    except_count: Option<String>,
    unit: Unit,
    history: InitialCountInventoryHistory,
}
impl CurrentInitialCountInventoryHistory {
    /// The exact pair and optional excluded Count bound to this witness.
    pub fn selection(&self) -> (&str, &str, Option<&str>) {
        (&self.warehouse, &self.product, self.except_count.as_deref())
    }
    pub fn data(&self) -> &InitialCountInventoryHistory {
        &self.history
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture(
                c,
                &self.warehouse,
                &self.product,
                self.except_count.as_deref(),
            )
            .await?
                != self.history
        {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED",
                "The original first-count inventory source changed before completion.",
            )
            .into());
        }
        Ok(())
    }
}
pub async fn lock_initial_count_inventory_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE commerce_stock,inventory_ledger,inventory_counts,inventory_count_lines,inventory_count_lot_lines,inventory_count_decisions,inventory_adjustment_moves,inventory_lots,inventory_lot_balances,inventory_lot_moves,inventory_order_reservations IN SHARE ROW EXCLUSIVE MODE").execute(c).await?;
    Ok(())
}
pub async fn read_initial_count_inventory_history(
    c: &mut PgConnection,
    warehouse: &str,
    product: &str,
    except_count: Option<&str>,
) -> Result<CurrentInitialCountInventoryHistory, Error> {
    if warehouse.is_empty() || product.is_empty() {
        return Err(invalid());
    }
    let transaction = unit(c).await?;
    let history = capture(c, warehouse, product, except_count).await?;
    Ok(CurrentInitialCountInventoryHistory {
        warehouse: warehouse.into(),
        product: product.into(),
        except_count: except_count.map(str::to_owned),
        unit: transaction,
        history,
    })
}
async fn capture(
    c: &mut PgConnection,
    warehouse: &str,
    product: &str,
    except_count: Option<&str>,
) -> Result<InitialCountInventoryHistory, Error> {
    let row=sqlx::query(r#"WITH items AS MATERIALIZED (
 (SELECT 'stock' kind,s.warehouse_id||':'||s.product_id ordering,to_jsonb(s) value FROM commerce_stock s WHERE s.warehouse_id=$1 AND s.product_id=$2 LIMIT 501)
 UNION ALL (SELECT 'ledger',m.id,to_jsonb(m) FROM inventory_ledger m WHERE m.warehouse_id=$1 AND m.product_id=$2 ORDER BY m.id LIMIT 501)
 UNION ALL (SELECT 'lots',l.id,jsonb_build_object('lot',to_jsonb(l),'balance',to_jsonb(b)) FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id WHERE b.warehouse_id=$1 AND l.product_id=$2 ORDER BY l.id LIMIT 501)
 UNION ALL (SELECT 'reservations',r.id,to_jsonb(r) FROM inventory_order_reservations r WHERE r.warehouse_id=$1 AND r.product_id=$2 ORDER BY r.id LIMIT 501)
 UNION ALL (SELECT 'counts',c.id||':'||l.id,jsonb_build_object('count',to_jsonb(c),'line',to_jsonb(l)) FROM inventory_count_lines l JOIN inventory_counts c ON c.id=l.count_id WHERE c.warehouse_id=$1 AND l.product_id=$2 AND ($3::text IS NULL OR c.id<>$3) AND (c.status<>'cancelled' OR c.posted_at IS NOT NULL OR c.journal_id IS NOT NULL) ORDER BY c.id,l.id LIMIT 501)
 ), stats AS (SELECT COALESCE(sum(octet_length(value::text)),0) bytes,COALESCE(max(n),0) max_count FROM (SELECT kind,value,count(*) OVER(PARTITION BY kind) n FROM items) t)
 SELECT CASE WHEN octet_length(image)<=2097152 THEN image END image,history FROM (SELECT CASE WHEN bytes<=2097152 AND max_count<=500 THEN jsonb_build_object('stock',(SELECT jsonb_agg(value ORDER BY ordering) FROM items WHERE kind='stock'),'ledger',(SELECT jsonb_agg(value ORDER BY ordering) FROM items WHERE kind='ledger'),'lots',(SELECT jsonb_agg(value ORDER BY ordering) FROM items WHERE kind='lots'),'reservations',(SELECT jsonb_agg(value ORDER BY ordering) FROM items WHERE kind='reservations'),'counts',(SELECT jsonb_agg(value ORDER BY ordering) FROM items WHERE kind='counts'))::text END image,EXISTS(SELECT 1 FROM items) history FROM stats) bounded"#)
        .bind(warehouse).bind(product).bind(except_count).fetch_one(c).await?;
    let image: String = row
        .try_get::<Option<String>, _>("image")?
        .ok_or_else(invalid)?;
    Ok(InitialCountInventoryHistory {
        image,
        has_history: row.try_get("history")?,
    })
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
    PersistenceError::new("INVENTORY_INITIAL_COUNT_UNIT_INVALID").into()
}

/// Inventory's retained native source and full record image. This is only a
/// factual projection: Workflow must independently validate the original
/// private command receipt and Commerce must validate the Service setup receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitialCountRecordImage {
    pub image: String,
}
pub struct CurrentInitialCountRecord {
    company: String,
    branch: Option<String>,
    count_id: String,
    unit: Unit,
    data: InitialCountRecordImage,
}
impl CurrentInitialCountRecord {
    pub fn data(&self) -> &InitialCountRecordImage {
        &self.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture_record(c, &self.company, self.branch.as_deref(), &self.count_id)
                .await?
                .as_ref()
                != Some(&self.data)
        {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED",
                "The original Count image changed before completion.",
            )
            .into());
        }
        Ok(())
    }
}
pub async fn read_initial_count_record(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    count_id: &str,
) -> Result<Option<CurrentInitialCountRecord>, Error> {
    if company.is_empty() || count_id.is_empty() {
        return Err(invalid());
    }
    let transaction = unit(c).await?;
    let Some(data) = capture_record(c, company, branch, count_id).await? else {
        return Ok(None);
    };
    Ok(Some(CurrentInitialCountRecord {
        company: company.into(),
        branch: branch.map(str::to_owned),
        count_id: count_id.into(),
        unit: transaction,
        data,
    }))
}
async fn capture_record(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    count_id: &str,
) -> Result<Option<InitialCountRecordImage>, Error> {
    let row=sqlx::query(r#"WITH source AS MATERIALIZED (
 SELECT to_jsonb(s) source,to_jsonb(c) count FROM inventory_service_initial_physical_sources s JOIN inventory_counts c ON c.id=s.count_id AND c.company_id=s.company_id AND c.branch_id=s.branch_id AND c.warehouse_id=s.warehouse_id WHERE s.company_id=$1 AND ($2::text IS NULL OR s.branch_id=$2) AND s.count_id=$3 AND c.id='count-'||s.count_command_id AND c.quality_hold_id IS NULL AND (SELECT count(*) FROM inventory_count_lines l WHERE l.count_id=c.id)=1 AND EXISTS(SELECT 1 FROM inventory_count_lines l WHERE l.count_id=c.id AND l.product_id=s.product_id AND l.snapshot_quantity_milli=0 AND l.snapshot_value_minor=0 AND l.snapshot_version=0)
 ), items AS MATERIALIZED (
 (SELECT 'lines' kind,l.id ordering,to_jsonb(l) value FROM inventory_count_lines l WHERE l.count_id=$3 ORDER BY l.id LIMIT 501)
 UNION ALL (SELECT 'lots',lot.id,to_jsonb(lot) FROM inventory_count_lot_lines lot JOIN inventory_count_lines l ON l.id=lot.count_line_id WHERE l.count_id=$3 ORDER BY lot.id LIMIT 501)
 UNION ALL (SELECT 'decisions',d.id,to_jsonb(d) FROM inventory_count_decisions d WHERE d.count_id=$3 ORDER BY d.id LIMIT 501)
 UNION ALL (SELECT 'moves',m.id,to_jsonb(m) FROM inventory_adjustment_moves m WHERE m.count_id=$3 ORDER BY m.id LIMIT 501)
 ), stats AS (SELECT COALESCE(sum(octet_length(value::text)),0) bytes,COALESCE(max(n),0) max_count FROM (SELECT kind,value,count(*) OVER(PARTITION BY kind) n FROM items) t)
 SELECT CASE WHEN octet_length(image)<=2097152 THEN image END image FROM (SELECT CASE WHEN stats.bytes+octet_length(source.source::text)+octet_length(source.count::text)<=2097152 AND stats.max_count<=500 THEN jsonb_build_object('source',source.source,'count',source.count,'lines',(SELECT jsonb_agg(value ORDER BY ordering) FROM items WHERE kind='lines'),'lots',(SELECT jsonb_agg(value ORDER BY ordering) FROM items WHERE kind='lots'),'decisions',(SELECT jsonb_agg(value ORDER BY ordering) FROM items WHERE kind='decisions'),'moves',(SELECT jsonb_agg(value ORDER BY ordering) FROM items WHERE kind='moves'))::text END image FROM source CROSS JOIN stats) bounded"#).bind(company).bind(branch).bind(count_id).fetch_optional(c).await?;
    let image = match row {
        None => None,
        Some(row) => Some(
            row.try_get::<Option<String>, _>("image")?
                .ok_or_else(invalid)?,
        ),
    };
    Ok(image.map(|image| InitialCountRecordImage { image }))
}

/// Persist only Inventory-owned rows for a newly prepared first Count. The
/// caller must insert the exact Workflow receipt in this transaction first and
/// retain Commerce, Finance, Quality, identity, purchase and history witnesses.
/// The database's deferred 1062 constraint verifies the reciprocal receipt.
/// An error must abort the enclosing unit; this does not commit or authorize.
pub async fn insert_initial_count_draft(
    c: &mut PgConnection,
    draft: &crate::application::service_initial_physical_count::InitialCountDraft,
    history: &CurrentInitialCountInventoryHistory,
) -> Result<InitialCountDraftEffects, Error> {
    use sha2::{Digest, Sha256};
    let p = &draft.proof;
    let selection = &p.selection;
    history.require_current(c).await?;
    let checked = crate::domain::service_initial_physical_count::value_initial_count(
        draft.product.tracking,
        draft.value.quantity_milli,
        draft.value.unit_cost_minor,
        draft
            .value
            .lots
            .iter()
            .map(|(lot, _)| lot.clone())
            .collect(),
    )?;
    if checked != draft.value {
        return Err(invalid());
    }
    if history.data().has_history
        || history.warehouse != selection.warehouse_id
        || history.product != selection.product_id
        || history.except_count.is_some()
        || p.count_id != format!("count-{}", p.count_command_id)
        || draft.product.version <= 0
        || !crate::domain::service_initial_physical_count::valid_date(&selection.count_date)
        || draft.actor_id.is_empty()
        || js_trim(&draft.reason).encode_utf16().count() < 5
        || js_trim(&draft.reason).encode_utf16().count() > 2000
    {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED",
            "The original first-count inventory source is invalid or changed.",
        )
        .into());
    }
    let digest = format!("{:x}", Sha256::digest(p.count_id.as_bytes()));
    let number = format!("COUNT-{}", digest[..16].to_uppercase());
    let expected = InitialCountDraftEffects::plan(draft, &number, unit(c).await?)?;
    let line_id = format!("{}-0", p.count_id);
    let proof = serde_json::json!({"kind":"service-initial-physical-stock-count","countId":p.count_id,"companyId":selection.company_id,"branchId":selection.branch_id,"warehouseId":selection.warehouse_id,"productId":selection.product_id,"countCommandId":p.count_command_id,"serviceSetupCommandId":p.service_setup_command_id,"serviceSetupRequestHash":p.service_setup_request_hash,"chartId":p.finance.chart_id,"inventoryAccountId":p.finance.inventory_account_id,"varianceAccountId":p.finance.variance_account_id,"currency":p.finance.currency});
    sqlx::query("INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,net_value_minor,created_by,prepared_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$9,$10,$10)")
        .bind(&p.count_id).bind(&selection.company_id).bind(&selection.branch_id).bind(&selection.warehouse_id).bind(&number).bind(&selection.count_date).bind(js_trim(&draft.reason)).bind(draft.value.value_minor).bind(&draft.actor_id).bind(draft.created_at).execute(&mut *c).await?;
    sqlx::query("INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,unit_cost_minor,ordinal) VALUES($1,$2,$3,$4,$5,$6,0,0,0,$7,$8,$9,0)")
        .bind(&line_id).bind(&p.count_id).bind(&selection.product_id).bind(draft.product.version).bind(&draft.product.description).bind(&draft.product.unit).bind(draft.value.quantity_milli).bind(draft.value.value_minor).bind(draft.value.unit_cost_minor).execute(&mut *c).await?;
    for (index, (lot, value)) in draft.value.lots.iter().enumerate() {
        let (mode, identity) = match draft.product.tracking {
            crate::domain::consumption::TrackingMode::Lot => ("lot", lot.lot_code.as_deref()),
            crate::domain::consumption::TrackingMode::Serial => {
                ("serial", lot.serial_number.as_deref())
            }
            crate::domain::consumption::TrackingMode::None => return Err(invalid()),
        };
        let identity = identity.ok_or_else(invalid)?.to_lowercase();
        let hash = format!(
            "{:x}",
            Sha256::digest(
                format!(
                    "{}:{}:{}:{}",
                    selection.company_id, selection.product_id, mode, identity
                )
                .as_bytes()
            )
        );
        let result_id = format!("lot-{}", &hash[..32]);
        let lot_line = format!("{}-lot-{}", line_id, index);
        sqlx::query("INSERT INTO inventory_count_lot_lines(id,count_line_id,source_lot_id,result_lot_id,tracking_mode,lot_code,serial_number,manufactured_date,expiry_date,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,unit_cost_minor) VALUES($1,$2,NULL,$3,$4,$5,$6,$7,$8,0,0,0,$9,$10,$11)")
            .bind(lot_line).bind(&line_id).bind(result_id).bind(mode).bind(&lot.lot_code).bind(&lot.serial_number).bind(&lot.manufactured_date).bind(&lot.expiry_date).bind(lot.quantity_milli).bind(value).bind(lot.unit_cost_minor).execute(&mut *c).await?;
    }
    sqlx::query("INSERT INTO inventory_service_initial_physical_sources(count_id,company_id,branch_id,warehouse_id,product_id,count_command_id,service_setup_command_id,chart_id,inventory_account_id,variance_account_id,currency,proof_json,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12::jsonb,$13,$14)")
        .bind(&p.count_id).bind(&selection.company_id).bind(&selection.branch_id).bind(&selection.warehouse_id).bind(&selection.product_id).bind(&p.count_command_id).bind(&p.service_setup_command_id).bind(&p.finance.chart_id).bind(&p.finance.inventory_account_id).bind(&p.finance.variance_account_id).bind(&p.finance.currency).bind(proof.to_string()).bind(&draft.actor_id).bind(draft.created_at).execute(&mut *c).await?;
    Ok(expected)
}

/// Apply Inventory effects after the same-unit coordinator has checked its
/// identity, purchase, Flow, Records, Finance and Workflow receipt witnesses.
/// No independent transaction or authority is provided here. All failures must
/// abort the caller's unit, including errors after any individual SQL effect.
pub async fn apply_initial_count_transition(
    c: &mut PgConnection,
    action: &crate::application::service_initial_physical_count::InitialCountTransition,
    before: &CurrentInitialCountRecord,
) -> Result<InitialCountTransitionEffects, Error> {
    use crate::domain::service_initial_physical_count::CountAction;
    before.require_current(c).await?;
    if before.company != action.company_id
        || before.count_id != action.count_id
        || action.expected_version <= 0
        || action.actor_id.is_empty()
        || action.command_id.is_empty()
    {
        return Err(invalid());
    }
    let (name, status, allowed): (&str, &str, &[&str]) = match action.action {
        CountAction::Submit => ("submit", "submitted", &["draft", "rejected"]),
        CountAction::Approve => ("approve", "approved", &["submitted"]),
        CountAction::Post => ("post", "posted", &["approved"]),
        CountAction::Reject => ("reject", "rejected", &["submitted", "approved"]),
        CountAction::Cancel => ("cancel", "cancelled", &["draft", "rejected"]),
    };
    if (action.action != CountAction::Post && action.journal_id.is_some())
        || (action.action != CountAction::Submit && action.reason.is_none())
        || action.reason.as_deref().is_some_and(|r| {
            js_trim(r).encode_utf16().count() < 3 || js_trim(r).encode_utf16().count() > 2000
        })
    {
        return Err(invalid());
    }
    let count = sqlx::query("SELECT c.warehouse_id,c.count_date,c.status,c.version,l.id line_id,l.product_id,l.counted_quantity_milli,l.counted_value_minor,l.snapshot_quantity_milli,l.snapshot_value_minor,l.snapshot_version FROM inventory_counts c JOIN inventory_count_lines l ON l.count_id=c.id WHERE c.id=$1 AND c.company_id=$2").bind(&action.count_id).bind(&action.company_id).fetch_one(&mut *c).await?;
    let status_before: String = count.try_get("status")?;
    let version: i64 = count.try_get("version")?;
    if version != action.expected_version
        || !allowed.contains(&status_before.as_str())
        || count.try_get::<i64, _>("snapshot_quantity_milli")? != 0
        || count.try_get::<i64, _>("snapshot_value_minor")? != 0
        || count.try_get::<i64, _>("snapshot_version")? != 0
    {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "COUNT_VERSION_CHANGED",
            "The original Count changed.",
        )
        .into());
    }
    let expected = InitialCountTransitionEffects::plan(c, action, before, status, name).await?;
    if matches!(
        action.action,
        CountAction::Submit | CountAction::Approve | CountAction::Post
    ) {
        let warehouse: String = count.try_get("warehouse_id")?;
        let product: String = count.try_get("product_id")?;
        let line: String = count.try_get("line_id")?;
        let qty: i64 = count.try_get("counted_quantity_milli")?;
        let mode = match action.tracking {
            crate::domain::consumption::TrackingMode::None => "none",
            crate::domain::consumption::TrackingMode::Lot => "lot",
            crate::domain::consumption::TrackingMode::Serial => "serial",
        };
        let valid: bool = sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM commerce_stock WHERE warehouse_id=$1 AND product_id=$2) AND CASE WHEN $4='none' THEN NOT EXISTS(SELECT 1 FROM inventory_count_lot_lines WHERE count_line_id=$3) ELSE EXISTS(SELECT 1 FROM inventory_count_lot_lines WHERE count_line_id=$3) AND COALESCE((SELECT sum(counted_quantity_milli) FROM inventory_count_lot_lines WHERE count_line_id=$3),0)=$5 AND NOT EXISTS(SELECT 1 FROM inventory_count_lot_lines cl WHERE cl.count_line_id=$3 AND (cl.tracking_mode<>$4 OR cl.source_lot_id IS NOT NULL OR EXISTS(SELECT 1 FROM inventory_lots l WHERE l.id=cl.result_lot_id OR (l.company_id=$6 AND l.product_id=$2 AND ((l.tracking_mode='lot' AND l.lot_code=cl.lot_code) OR (l.tracking_mode='serial' AND l.serial_number=cl.serial_number)))))) END").bind(&warehouse).bind(&product).bind(&line).bind(mode).bind(qty).bind(&action.company_id).fetch_one(&mut *c).await?;
        if !valid {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED",
                "The Count stock or tracking snapshot changed.",
            )
            .into());
        }
    }
    if action.action == CountAction::Post {
        let warehouse: String = count.try_get("warehouse_id")?;
        let product: String = count.try_get("product_id")?;
        let date: String = count.try_get("count_date")?;
        let line_id: String = count.try_get("line_id")?;
        let qty: i64 = count.try_get("counted_quantity_milli")?;
        let value: i64 = count.try_get("counted_value_minor")?;
        if (value == 0) != action.journal_id.is_none() {
            return Err(invalid());
        }
        let blocked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM commerce_stock WHERE warehouse_id=$1 AND product_id=$2) OR EXISTS(SELECT 1 FROM inventory_ledger WHERE warehouse_id=$1 AND product_id=$2 AND movement_date>$3)").bind(&warehouse).bind(&product).bind(&date).fetch_one(&mut *c).await?;
        if blocked {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED",
                "The Count stock snapshot changed.",
            )
            .into());
        }
        let lots = sqlx::query("SELECT id,result_lot_id,tracking_mode,lot_code,serial_number,manufactured_date,expiry_date,counted_quantity_milli,counted_value_minor FROM inventory_count_lot_lines WHERE count_line_id=$1 ORDER BY id").bind(&line_id).fetch_all(&mut *c).await?;
        let identity_conflict: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inventory_count_lot_lines cl JOIN inventory_lots l ON l.id=cl.result_lot_id OR (l.company_id=$1 AND l.product_id=$2 AND ((l.tracking_mode='lot' AND l.lot_code=cl.lot_code) OR (l.tracking_mode='serial' AND l.serial_number=cl.serial_number))) WHERE cl.count_line_id=$3)").bind(&action.company_id).bind(&product).bind(&line_id).fetch_one(&mut *c).await?;
        if identity_conflict {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED",
                "A counted lot identity already exists.",
            )
            .into());
        }
        sqlx::query("INSERT INTO commerce_stock(warehouse_id,product_id) VALUES($1,$2)")
            .bind(&warehouse)
            .bind(&product)
            .execute(&mut *c)
            .await?;
        sqlx::query("UPDATE commerce_stock SET quantity_milli=$1,value_minor=$2,version=version+1 WHERE warehouse_id=$3 AND product_id=$4").bind(qty).bind(value).bind(&warehouse).bind(&product).execute(&mut *c).await?;
        sqlx::query("INSERT INTO inventory_adjustment_moves(id,count_id,warehouse_id,product_id,quantity_milli,value_minor,movement_date,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(format!("{}-0", action.count_id)).bind(&action.count_id).bind(&warehouse).bind(&product).bind(qty).bind(value).bind(&date).bind(action.now).execute(&mut *c).await?;
        for (index, lot) in lots.iter().enumerate() {
            let id: String = lot.try_get("result_lot_id")?;
            let lqty: i64 = lot.try_get("counted_quantity_milli")?;
            let lvalue: i64 = lot.try_get("counted_value_minor")?;
            sqlx::query("INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,serial_number,manufactured_date,expiry_date,status,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'active',$9,$10)").bind(&id).bind(&action.company_id).bind(&product).bind(lot.try_get::<String,_>("tracking_mode")?).bind(lot.try_get::<Option<String>,_>("lot_code")?).bind(lot.try_get::<Option<String>,_>("serial_number")?).bind(lot.try_get::<Option<String>,_>("manufactured_date")?).bind(lot.try_get::<Option<String>,_>("expiry_date")?).bind(&action.actor_id).bind(action.now).execute(&mut *c).await?;
            sqlx::query("INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor) VALUES($1,$2,$3,$4)").bind(&id).bind(&warehouse).bind(lqty).bind(lvalue).execute(&mut *c).await?;
            if lqty != 0 || lvalue != 0 {
                sqlx::query("INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,source_id,quantity_milli,value_minor,movement_date,created_by,created_at) VALUES($1,$2,$3,'stock_count',$4,$5,$6,$7,$8,$9)").bind(format!("{}-0-{}", action.count_id, index)).bind(&id).bind(&warehouse).bind(lot.try_get::<String,_>("id")?).bind(lqty).bind(lvalue).bind(&date).bind(&action.actor_id).bind(action.now).execute(&mut *c).await?;
            }
        }
    }
    let changed = sqlx::query("UPDATE inventory_counts SET status=$1,review_reason=$2,approved_by=CASE WHEN $3='approve' THEN $4 ELSE approved_by END,submitted_at=CASE WHEN $3='submit' THEN $5 ELSE submitted_at END,posted_at=CASE WHEN $3='post' THEN $5 ELSE posted_at END,journal_id=COALESCE($6,journal_id),version=version+1,updated_at=$5 WHERE id=$7 AND company_id=$8 AND version=$9 AND status=$10").bind(status).bind(action.reason.as_deref().map(js_trim)).bind(name).bind(&action.actor_id).bind(action.now).bind(&action.journal_id).bind(&action.count_id).bind(&action.company_id).bind(action.expected_version).bind(status_before).execute(&mut *c).await?.rows_affected();
    if changed != 1 {
        return Err(invalid());
    }
    if action.action != CountAction::Submit {
        sqlx::query("INSERT INTO inventory_count_decisions(id,count_id,command_id,action,actor_id,reviewed_version,reason,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(&action.decision_id).bind(&action.count_id).bind(&action.command_id).bind(name).bind(&action.actor_id).bind(action.expected_version).bind(action.reason.as_deref().map(js_trim)).bind(action.now).execute(&mut *c).await?;
    }
    Ok(expected)
}

/// Planned before the first insert, not observed from post-effect rows. Invoke
/// after all deferred owner effects have flushed, before committing the unit.
pub struct InitialCountDraftEffects {
    unit: Unit,
    count_id: String,
    count: serde_json::Value,
    line: serde_json::Value,
    source: serde_json::Value,
    lots: Vec<serde_json::Value>,
}
impl InitialCountDraftEffects {
    fn plan(
        d: &crate::application::service_initial_physical_count::InitialCountDraft,
        number: &str,
        unit: Unit,
    ) -> Result<Self, Error> {
        use sha2::{Digest, Sha256};
        let p = &d.proof;
        let s = &p.selection;
        let line = format!("{}-0", p.count_id);
        let proof = serde_json::json!({"kind":"service-initial-physical-stock-count","countId":p.count_id,"companyId":s.company_id,"branchId":s.branch_id,"warehouseId":s.warehouse_id,"productId":s.product_id,"countCommandId":p.count_command_id,"serviceSetupCommandId":p.service_setup_command_id,"serviceSetupRequestHash":p.service_setup_request_hash,"chartId":p.finance.chart_id,"inventoryAccountId":p.finance.inventory_account_id,"varianceAccountId":p.finance.variance_account_id,"currency":p.finance.currency});
        let count = serde_json::json!({"id":p.count_id,"company_id":s.company_id,"branch_id":s.branch_id,"warehouse_id":s.warehouse_id,"number":number,"count_date":s.count_date,"reason":js_trim(&d.reason),"net_value_minor":d.value.value_minor,"created_by":d.actor_id,"prepared_by":d.actor_id,"created_at":d.created_at,"updated_at":d.created_at,"status":"draft","version":1,"approved_by":null,"review_reason":null,"submitted_at":null,"posted_at":null,"journal_id":null,"quality_hold_id":null});
        let line_image = serde_json::json!({"id":line,"count_id":p.count_id,"product_id":s.product_id,"product_version":d.product.version,"description":d.product.description,"unit":d.product.unit,"snapshot_quantity_milli":0,"snapshot_value_minor":0,"snapshot_version":0,"counted_quantity_milli":d.value.quantity_milli,"counted_value_minor":d.value.value_minor,"unit_cost_minor":d.value.unit_cost_minor,"ordinal":0});
        let source = serde_json::json!({"count_id":p.count_id,"company_id":s.company_id,"branch_id":s.branch_id,"warehouse_id":s.warehouse_id,"product_id":s.product_id,"count_command_id":p.count_command_id,"service_setup_command_id":p.service_setup_command_id,"chart_id":p.finance.chart_id,"inventory_account_id":p.finance.inventory_account_id,"variance_account_id":p.finance.variance_account_id,"currency":p.finance.currency,"proof_json":proof,"created_by":d.actor_id,"created_at":d.created_at});
        let mut lots = Vec::new();
        for (index, (lot, value)) in d.value.lots.iter().enumerate() {
            let (mode, identity) = match d.product.tracking {
                crate::domain::consumption::TrackingMode::Lot => ("lot", lot.lot_code.as_deref()),
                crate::domain::consumption::TrackingMode::Serial => {
                    ("serial", lot.serial_number.as_deref())
                }
                _ => return Err(invalid()),
            };
            let hash = format!(
                "{:x}",
                Sha256::digest(
                    format!(
                        "{}:{}:{}:{}",
                        s.company_id,
                        s.product_id,
                        mode,
                        identity.ok_or_else(invalid)?.to_lowercase()
                    )
                    .as_bytes()
                )
            );
            lots.push(serde_json::json!({"id":format!("{}-lot-{}",line,index),"count_line_id":line,"source_lot_id":null,"result_lot_id":format!("lot-{}",&hash[..32]),"tracking_mode":mode,"lot_code":lot.lot_code,"serial_number":lot.serial_number,"manufactured_date":lot.manufactured_date,"expiry_date":lot.expiry_date,"snapshot_quantity_milli":0,"snapshot_value_minor":0,"snapshot_version":0,"counted_quantity_milli":lot.quantity_milli,"counted_value_minor":value,"unit_cost_minor":lot.unit_cost_minor}));
        }
        Ok(Self {
            unit,
            count_id: p.count_id.clone(),
            count,
            line: line_image,
            source,
            lots,
        })
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed_effect());
        }
        let ok:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inventory_counts c WHERE c.id=$1 AND to_jsonb(c) @> $2::jsonb) AND EXISTS(SELECT 1 FROM inventory_count_lines l WHERE l.count_id=$1 AND to_jsonb(l) @> $3::jsonb) AND (SELECT count(*) FROM inventory_count_lines WHERE count_id=$1)=1 AND EXISTS(SELECT 1 FROM inventory_service_initial_physical_sources s WHERE s.count_id=$1 AND to_jsonb(s) @> $4::jsonb) AND (SELECT count(*) FROM inventory_count_lot_lines lot JOIN inventory_count_lines l ON l.id=lot.count_line_id WHERE l.count_id=$1)=$5 AND NOT EXISTS(SELECT 1 FROM inventory_count_decisions WHERE count_id=$1) AND NOT EXISTS(SELECT 1 FROM inventory_adjustment_moves WHERE count_id=$1)")
   .bind(&self.count_id).bind(self.count.to_string()).bind(self.line.to_string()).bind(self.source.to_string()).bind(self.lots.len() as i64).fetch_one(&mut *c).await?;
        if !ok {
            return Err(changed_effect());
        }
        for lot in &self.lots {
            let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inventory_count_lot_lines WHERE to_jsonb(inventory_count_lot_lines) @> $1::jsonb)").bind(lot.to_string()).fetch_one(&mut *c).await?;
            if !exists {
                return Err(changed_effect());
            }
        }
        Ok(())
    }
}
fn changed_effect() -> Error {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Conflict,
        "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED",
        "The planned Count effects changed before completion.",
    )
    .into()
}

pub struct InitialCountTransitionEffects {
    unit: Unit,
    company: String,
    branch: Option<String>,
    count_id: String,
    expected: serde_json::Value,
    stock: Option<serde_json::Value>,
    lots: Vec<(
        serde_json::Value,
        serde_json::Value,
        Option<serde_json::Value>,
    )>,
}
impl InitialCountTransitionEffects {
    async fn plan(
        c: &mut PgConnection,
        a: &crate::application::service_initial_physical_count::InitialCountTransition,
        before: &CurrentInitialCountRecord,
        status: &str,
        name: &str,
    ) -> Result<Self, Error> {
        use crate::domain::service_initial_physical_count::CountAction;
        let mut expected: serde_json::Value =
            serde_json::from_str(&before.data.image).map_err(|_| invalid())?;
        let count = expected
            .get_mut("count")
            .and_then(serde_json::Value::as_object_mut)
            .ok_or_else(invalid)?;
        let warehouse = count
            .get("warehouse_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(invalid)?
            .to_owned();
        let date = count
            .get("count_date")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(invalid)?
            .to_owned();
        count.insert("status".into(), serde_json::json!(status));
        count.insert(
            "review_reason".into(),
            serde_json::json!(a.reason.as_deref().map(js_trim)),
        );
        count.insert("version".into(), serde_json::json!(a.expected_version + 1));
        count.insert("updated_at".into(), serde_json::json!(a.now));
        if a.action == CountAction::Approve {
            count.insert("approved_by".into(), serde_json::json!(a.actor_id));
        }
        if a.action == CountAction::Submit {
            count.insert("submitted_at".into(), serde_json::json!(a.now));
        }
        if a.action == CountAction::Post {
            count.insert("posted_at".into(), serde_json::json!(a.now));
            if let Some(journal) = &a.journal_id {
                count.insert("journal_id".into(), serde_json::json!(journal));
            }
        }
        if a.action != CountAction::Submit {
            let value = expected.get_mut("decisions").ok_or_else(invalid)?;
            if value.is_null() {
                *value = serde_json::json!([])
            }
            let rows = value.as_array_mut().ok_or_else(invalid)?;
            rows.push(serde_json::json!({"id":a.decision_id,"count_id":a.count_id,"command_id":a.command_id,"action":name,"actor_id":a.actor_id,"reviewed_version":a.expected_version,"reason":a.reason.as_deref().map(js_trim),"created_at":a.now}));
            rows.sort_by(|x, y| x["id"].as_str().cmp(&y["id"].as_str()));
        }
        let mut stock = None;
        let mut lots = Vec::new();
        if a.action == CountAction::Post {
            let line = expected
                .get("lines")
                .and_then(serde_json::Value::as_array)
                .and_then(|a| a.first())
                .ok_or_else(invalid)?;
            let product = line
                .get("product_id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(invalid)?
                .to_owned();
            let qty = line
                .get("counted_quantity_milli")
                .and_then(serde_json::Value::as_i64)
                .ok_or_else(invalid)?;
            let value = line
                .get("counted_value_minor")
                .and_then(serde_json::Value::as_i64)
                .ok_or_else(invalid)?;
            stock = Some(
                serde_json::json!({"warehouse_id":warehouse,"product_id":product,"quantity_milli":qty,"value_minor":value,"version":2}),
            );
            let moves = expected.get_mut("moves").ok_or_else(invalid)?;
            if moves.is_null() {
                *moves = serde_json::json!([])
            }
            let rows = moves.as_array_mut().ok_or_else(invalid)?;
            rows.push(serde_json::json!({"id":format!("{}-0",a.count_id),"count_id":a.count_id,"warehouse_id":warehouse,"product_id":product,"quantity_milli":qty,"value_minor":value,"movement_date":date,"created_at":a.now}));
            rows.sort_by(|x, y| x["id"].as_str().cmp(&y["id"].as_str()));
            if let Some(lines) = expected.get("lots").and_then(serde_json::Value::as_array) {
                for (index, line) in lines.iter().enumerate() {
                    let id = line
                        .get("result_lot_id")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(invalid)?;
                    let line_id = line
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(invalid)?;
                    let q = line
                        .get("counted_quantity_milli")
                        .and_then(serde_json::Value::as_i64)
                        .ok_or_else(invalid)?;
                    let v = line
                        .get("counted_value_minor")
                        .and_then(serde_json::Value::as_i64)
                        .ok_or_else(invalid)?;
                    let lot = serde_json::json!({"id":id,"company_id":a.company_id,"product_id":product,"tracking_mode":line["tracking_mode"],"lot_code":line["lot_code"],"serial_number":line["serial_number"],"manufactured_date":line["manufactured_date"],"expiry_date":line["expiry_date"],"status":"active","created_by":a.actor_id,"created_at":a.now});
                    let balance = serde_json::json!({"lot_id":id,"warehouse_id":warehouse,"quantity_milli":q,"value_minor":v,"version":1});
                    let movement=(q!=0||v!=0).then(||serde_json::json!({"id":format!("{}-0-{}",a.count_id,index),"lot_id":id,"warehouse_id":warehouse,"source_type":"stock_count","source_id":line_id,"quantity_milli":q,"value_minor":v,"movement_date":date,"created_by":a.actor_id,"created_at":a.now}));
                    lots.push((lot, balance, movement));
                }
            }
        }
        Ok(Self {
            unit: unit(c).await?,
            company: before.company.clone(),
            branch: before.branch.clone(),
            count_id: before.count_id.clone(),
            expected,
            stock,
            lots,
        })
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed_effect());
        }
        let actual = capture_record(c, &self.company, self.branch.as_deref(), &self.count_id)
            .await?
            .ok_or_else(changed_effect)?;
        let image: serde_json::Value =
            serde_json::from_str(&actual.image).map_err(|_| invalid())?;
        if image != self.expected {
            return Err(changed_effect());
        }
        if let Some(stock) = &self.stock {
            let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM commerce_stock WHERE to_jsonb(commerce_stock) @> $1::jsonb)").bind(stock.to_string()).fetch_one(&mut *c).await?;
            if !exists {
                return Err(changed_effect());
            }
            let warehouse = stock
                .get("warehouse_id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(invalid)?;
            let product = stock
                .get("product_id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(invalid)?;
            let expected_moves = self
                .lots
                .iter()
                .filter(|(_, _, movement)| movement.is_some())
                .count() as i64;
            let totals: (i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id WHERE b.warehouse_id=$1 AND l.product_id=$2),(SELECT count(*) FROM inventory_lot_moves m WHERE m.source_type='stock_count' AND EXISTS(SELECT 1 FROM inventory_count_lot_lines cl JOIN inventory_count_lines l ON l.id=cl.count_line_id WHERE l.count_id=$3 AND cl.id=m.source_id))").bind(warehouse).bind(product).bind(&self.count_id).fetch_one(&mut *c).await?;
            if totals != (self.lots.len() as i64, expected_moves) {
                return Err(changed_effect());
            }
        }
        for (lot, balance, movement) in &self.lots {
            let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inventory_lots WHERE to_jsonb(inventory_lots) @> $1::jsonb) AND EXISTS(SELECT 1 FROM inventory_lot_balances WHERE to_jsonb(inventory_lot_balances) @> $2::jsonb) AND ($3::text IS NULL OR EXISTS(SELECT 1 FROM inventory_lot_moves WHERE to_jsonb(inventory_lot_moves) @> $3::jsonb))").bind(lot.to_string()).bind(balance.to_string()).bind(movement.as_ref().map(serde_json::Value::to_string)).fetch_one(&mut *c).await?;
            if !exists {
                return Err(changed_effect());
            }
        }
        Ok(())
    }
}
