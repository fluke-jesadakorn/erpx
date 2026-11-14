//! Inventory public seam implementation for the Commerce purchase receipt.
//!
//! The adapter borrows the caller's existing serializable connection, fences
//! every receipt source, re-reads the current purchase source, prepares the
//! same-transaction plan and applies stock/lot evidence. The Quality port is
//! injected by composition because Quality owns hold creation.
use crate::adapters::quality_receipt_hold::QualityReceiptHoldPort;
use crate::adapters::stock_receipt::{
    StockReceiptQualityPort, apply_stock_receipt, lock_stock_receipt_sources,
    read_stock_receipt_source, require_stock_receipt_applied,
};
use crate::domain::stock_receipt::{
    ReceiptDisposition, StockReceiptAllocationInput, StockReceiptAppliedReceipt, StockReceiptInput,
    StockReceiptLineInput, StockReceiptPlan, prepare_stock_receipt,
};
use erpx_server_commerce::public::receipt::{
    CommerceReceiptStock, PreparedStockReceipt, ReceiptPlanLine, ReceiptStockApplied,
    ReceiptStockAppliedAllocation, ReceiptStockAppliedLine, ReceiptStockRequest,
};
use erpx_server_runtime::public::postgres::{PgWorkFuture, PgTransactionError, PersistenceError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
type Balance = (i64, i64, i64);
type LineImage = (String, Balance, Vec<(String, Balance)>);

/// Same-unit owner evidence; the serializable public result remains separate.
pub struct InventoryReceiptApplied {
    facts: StockReceiptAppliedReceipt,
    unit: Unit,
    images: Vec<LineImage>,
}

fn image_changed() -> Error {
    BusinessError::new(FailureKind::Conflict,"STOCK_RECEIPT_SOURCE_CHANGED","The original stock receipt source changed.").into()
}
async fn receipt_unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" {
        return Err(PersistenceError::new("INVENTORY_OWNER_TRANSACTION_REQUIRED").into());
    }
    let locked: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND granted AND mode='ShareRowExclusiveLock' AND relation IN ('commerce_orders'::regclass,'commerce_order_lines'::regclass,'commerce_fulfillments'::regclass,'commerce_fulfillment_lines'::regclass,'commerce_stock'::regclass,'commerce_stock_moves'::regclass,'inventory_lots'::regclass,'inventory_lot_balances'::regclass,'inventory_lot_moves'::regclass,'inventory_fulfillment_lot_allocations'::regclass,'inventory_quality_holds'::regclass,'pos_reservations'::regclass,'inventory_order_reservations'::regclass,'field_service_part_reservations'::regclass,'manufacturing_material_reservations'::regclass)").fetch_one(&mut *c).await?;
    if locked != 15 { return Err(PersistenceError::new("INVENTORY_RECEIPT_LOCKS_REQUIRED").into()); }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn stock_balance(c: &mut PgConnection, warehouse: &str, product: &str) -> Result<Option<Balance>, Error> {
    Ok(sqlx::query_as("SELECT quantity_milli,value_minor,version FROM commerce_stock WHERE warehouse_id=$1 AND product_id=$2").bind(warehouse).bind(product).fetch_optional(c).await?)
}
async fn lot_balance(c: &mut PgConnection, warehouse: &str, lot: &str) -> Result<Option<Balance>, Error> {
    Ok(sqlx::query_as("SELECT quantity_milli,value_minor,version FROM inventory_lot_balances WHERE warehouse_id=$1 AND lot_id=$2").bind(warehouse).bind(lot).fetch_optional(c).await?)
}

pub struct InventoryStockReceipt<Q> {
    quality: Q,
}

impl<Q> InventoryStockReceipt<Q> {
    pub fn new(quality: Q) -> Self {
        Self { quality }
    }
}

impl InventoryStockReceipt<QualityReceiptHoldPort> {
    /// Production binding used by composition: the Inventory receipt seam over
    /// the Inventory-owned Quality receipt-hold port. No second warehouse,
    /// stock or Quality writer is constructed.
    pub fn production() -> Self {
        Self::new(QualityReceiptHoldPort::new())
    }
}

fn allocation_input(
    allocation: &erpx_server_commerce::public::receipt::ReceiptStockAllocation,
) -> StockReceiptAllocationInput {
    StockReceiptAllocationInput {
        lot_code: allocation.lot_code.clone(),
        serial_number: allocation.serial_number.clone(),
        quantity_milli: allocation.quantity_milli,
        disposition: match allocation.disposition {
            erpx_server_commerce::public::receipt::ReceiptDisposition::Available => {
                ReceiptDisposition::Available
            }
            erpx_server_commerce::public::receipt::ReceiptDisposition::Quarantine => {
                ReceiptDisposition::Quarantine
            }
        },
        quarantine_reason: allocation.quarantine_reason.clone(),
        quarantine_due_date: allocation.quarantine_due_date.clone(),
        manufactured_date: allocation.manufactured_date.clone(),
        expiry_date: allocation.expiry_date.clone(),
    }
}

fn applied_facts(applied: &StockReceiptAppliedReceipt) -> ReceiptStockApplied {
    ReceiptStockApplied {
        fulfillment_id: applied.fulfillment_id.clone(),
        replayed: applied.replayed,
        lines: applied
            .lines
            .iter()
            .map(|line| ReceiptStockAppliedLine {
                line_id: line.line_id.clone(),
                stock_version: line.stock_version,
                quantity_milli: line.quantity_milli,
                value_minor: line.value_minor,
                allocations: line
                    .allocations
                    .iter()
                    .map(|allocation| ReceiptStockAppliedAllocation {
                        id: allocation.id.clone(),
                        lot_id: allocation.lot_id.clone(),
                        quantity_milli: allocation.quantity_milli,
                        value_minor: allocation.value_minor,
                        disposition: allocation.disposition.clone(),
                        quality_hold_id: allocation.quality_hold_id.clone(),
                        balance_version: allocation.balance_version,
                    })
                    .collect(),
            })
            .collect(),
    }
}

impl<Q> CommerceReceiptStock for InventoryStockReceipt<Q>
where
    Q: StockReceiptQualityPort + Send + Sync,
{
    type Plan = StockReceiptPlan;
    type Applied = InventoryReceiptApplied;

    fn lock<'a>(&'a mut self, c: &'a mut PgConnection) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            lock_stock_receipt_sources(c).await?;
            Ok(())
        })
    }

    fn prepare<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        request: &'a ReceiptStockRequest,
    ) -> PgWorkFuture<'a, PreparedStockReceipt<Self::Plan>, BusinessError> {
        Box::pin(async move {
            let source = read_stock_receipt_source(
                c,
                &request.scope.company_id,
                request.scope.branch_id.as_deref(),
                request.scope.department_id.as_deref(),
                &request.scope.order_id,
            )
            .await?;
            let input = StockReceiptInput {
                expected_order_version: request.expected_order_version,
                date: request.date.clone(),
                lines: request
                    .lines
                    .iter()
                    .map(|line| StockReceiptLineInput {
                        line_id: line.line_id.clone(),
                        quantity_milli: line.quantity_milli,
                        stock_value_minor: line.stock_value_minor,
                        allocations: line.allocations.iter().map(allocation_input).collect(),
                    })
                    .collect(),
            };
            let plan = prepare_stock_receipt(&source, &input)?;
            let lines = plan
                .lines
                .iter()
                .map(|line| ReceiptPlanLine {
                    line_id: line.line_id.clone(),
                    product_id: line.product_id.clone(),
                    quantity_milli: line.quantity_milli,
                    stock_value_minor: line.stock_value_minor,
                    expected_fulfilled_milli: line.expected_fulfilled_milli,
                })
                .collect();
            Ok(PreparedStockReceipt { plan, lines })
        })
    }

    fn apply<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        plan: &'a Self::Plan,
        fulfillment_id: &'a str,
        command_key: &'a str,
        actor: &'a str,
        now: i64,
    ) -> PgWorkFuture<'a, Self::Applied, BusinessError> {
        Box::pin(async move {
            let mut savepoint=c.begin().await?;
            let result: Result<InventoryReceiptApplied,Error> = async {
            let c=&mut *savepoint;
            let unit = receipt_unit(c).await?;
            let mut pre = Vec::with_capacity(plan.lines.len());
            for line in &plan.lines {
                let stock=stock_balance(c,&plan.warehouse_id,&line.product_id).await?;
                let mut lots=Vec::with_capacity(line.allocations.len());
                for allocation in &line.allocations {
                    lots.push(match &allocation.existing_lot_id { Some(id)=>lot_balance(c,&plan.warehouse_id,id).await?,None=>None });
                }
                pre.push((stock,lots));
            }
            let facts = apply_stock_receipt(
                c,
                plan,
                fulfillment_id,
                command_key,
                actor,
                now,
                &mut self.quality,
            )
            .await?;
            if facts.fulfillment_id!=fulfillment_id || facts.order_id!=plan.order_id || facts.lines.len()!=plan.lines.len() { return Err(image_changed()); }
            for (i,(line,actual)) in plan.lines.iter().zip(&facts.lines).enumerate() {
                if actual.line_id!=line.line_id || actual.fulfillment_line_id!=format!("{fulfillment_id}-{i}") || actual.stock_move_id!=format!("{fulfillment_id}-{i}") || actual.quantity_milli!=line.quantity_milli || actual.value_minor!=line.stock_value_minor || actual.allocations.len()!=line.allocations.len() { return Err(image_changed()); }
                for (j,(allocation,actual)) in line.allocations.iter().zip(&actual.allocations).enumerate() {
                    if actual.id!=format!("lot-allocation-{fulfillment_id}-{i}-{}",allocation.allocation_index) || allocation.allocation_index!=j || actual.quantity_milli!=allocation.quantity_milli || actual.value_minor!=allocation.value_minor || actual.lot_code!=allocation.lot_code || actual.serial_number!=allocation.serial_number || allocation.existing_lot_id.as_ref().is_some_and(|id|id!=&actual.lot_id) { return Err(image_changed()); }
                }
            }
            if !facts.replayed {
                let mut stocks=std::collections::BTreeMap::<String,Balance>::new();
                let mut balances=std::collections::BTreeMap::<String,Balance>::new();
                for (i,line) in plan.lines.iter().enumerate() {
                    let stock=stocks.entry(line.product_id.clone()).or_insert(pre[i].0.unwrap_or((0,0,0)));
                    stock.0=stock.0.checked_add(line.quantity_milli).ok_or_else(image_changed)?;
                    stock.1=stock.1.checked_add(line.stock_value_minor).ok_or_else(image_changed)?;
                    let hold_count=line.allocations.iter().filter(|a|a.disposition==ReceiptDisposition::Quarantine).count() as i64;
                    stock.2=stock.2.checked_add(1).and_then(|v|v.checked_add(hold_count)).ok_or_else(image_changed)?;
                    for (j,allocation) in line.allocations.iter().enumerate() {
                        let id=&facts.lines[i].allocations[j].lot_id;
                        let balance=balances.entry(id.clone()).or_insert(pre[i].1[j].unwrap_or((0,0,0)));
                        balance.0=balance.0.checked_add(allocation.quantity_milli).ok_or_else(image_changed)?;
                        balance.1=balance.1.checked_add(allocation.value_minor).ok_or_else(image_changed)?;
                        balance.2=balance.2.checked_add(1).ok_or_else(image_changed)?;
                    }
                }
                for (product,expected) in stocks { if stock_balance(c,&plan.warehouse_id,&product).await?.as_ref()!=Some(&expected) { return Err(image_changed()); } }
                for (id,expected) in balances { if lot_balance(c,&plan.warehouse_id,&id).await?.as_ref()!=Some(&expected) { return Err(image_changed()); } }
            }
            let mut images=Vec::with_capacity(plan.lines.len());
            for (i,line) in plan.lines.iter().enumerate() {
                let stock=stock_balance(c,&plan.warehouse_id,&line.product_id).await?.ok_or_else(image_changed)?;
                let mut lots=Vec::with_capacity(line.allocations.len());
                for (j,_) in line.allocations.iter().enumerate() {
                    let id=&facts.lines[i].allocations[j].lot_id;
                    let balance=lot_balance(c,&plan.warehouse_id,id).await?.ok_or_else(image_changed)?;
                    lots.push((id.clone(),balance));
                }
                images.push((line.product_id.clone(),stock,lots));
            }
            Ok(InventoryReceiptApplied {facts,unit,images})
            }.await;
            match result {
                Ok(applied)=>{savepoint.commit().await?;Ok(applied)}
                Err(error)=>{savepoint.rollback().await?;Err(error)}
            }
        })
    }

    fn applied_facts(&self, applied: &Self::Applied) -> ReceiptStockApplied {
        applied_facts(&applied.facts)
    }

    fn require_applied<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        plan: &'a Self::Plan,
        applied: &'a Self::Applied,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            if receipt_unit(c).await? != applied.unit || applied.images.len()!=plan.lines.len() { return Err(image_changed()); }
            for (i,(product,stock,lots)) in applied.images.iter().enumerate() {
                if product!=&plan.lines[i].product_id || stock_balance(c,&plan.warehouse_id,product).await?.as_ref()!=Some(stock) { return Err(image_changed()); }
                for (id,balance) in lots { if lot_balance(c,&plan.warehouse_id,id).await?.as_ref()!=Some(balance) { return Err(image_changed()); } }
            }
            require_stock_receipt_applied(c, plan, &applied.facts).await?;
            Ok(())
        })
    }
}
