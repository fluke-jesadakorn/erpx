//! Inventory public seam implementation for the Commerce purchase receipt.
//!
//! The adapter borrows the caller's existing serializable connection, fences
//! every receipt source, re-reads the current purchase source, prepares the
//! same-transaction plan and applies stock/lot evidence. The Quality port is
//! injected by composition because Quality owns hold creation.
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
use erpx_server_runtime::public::postgres::PgWorkFuture;
use erpx_server_shared::public::BusinessError;
use sqlx::PgConnection;

pub struct InventoryStockReceipt<Q> {
    quality: Q,
}

impl<Q> InventoryStockReceipt<Q> {
    pub fn new(quality: Q) -> Self {
        Self { quality }
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
    type Applied = StockReceiptAppliedReceipt;

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
            apply_stock_receipt(
                c,
                plan,
                fulfillment_id,
                command_key,
                actor,
                now,
                &mut self.quality,
            )
            .await
        })
    }

    fn applied_facts(&self, applied: &Self::Applied) -> ReceiptStockApplied {
        applied_facts(applied)
    }

    fn require_applied<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        plan: &'a Self::Plan,
        applied: &'a Self::Applied,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            require_stock_receipt_applied(c, plan, applied).await?;
            Ok(())
        })
    }
}
