//! Production Inventory implementation of the goods-receipt Quality port.
//!
//! Inventory owns the receipt transaction and its lot/balance witness; Quality
//! owns hold creation, the active inspection spec and the retained hold image.
//! [`QualityReceiptHoldPort`] bridges those owners by supplying Quality's
//! published [`prepare_goods_receipt_hold`] seam with an Inventory witness that
//! is revalidated in the caller's serializable transaction. The dependency
//! direction stays inventory -> quality; Quality never calls back into
//! Inventory.
//!
//! The caller must already hold the receipt source locks from
//! [`crate::adapters::stock_receipt::lock_stock_receipt_sources`], which
//! includes the Quality hold and spec relations. Without them Quality refuses
//! the unit and no hold is written.
use crate::adapters::stock_receipt::StockReceiptQualityPort;
use crate::domain::stock_receipt::StockReceiptQualityRequest;
use erpx_server_quality::public::goods_receipt_hold::{
    GoodsReceiptHoldFacts, GoodsReceiptHoldSource, prepare_goods_receipt_hold,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "STOCK_RECEIPT_QUALITY_REQUEST_INVALID",
        "Use a current receipt allocation before creating its Quality hold.",
    )
}

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "QUALITY_RECEIPT_SOURCE_CHANGED",
        "The receipt lot evidence changed before Quality prepared its hold.",
    )
    .into()
}

/// Inventory-owned production binding for Commerce receipt quality holds.
#[derive(Clone, Copy, Debug, Default)]
pub struct QualityReceiptHoldPort;

impl QualityReceiptHoldPort {
    pub const fn new() -> Self {
        Self
    }
}

fn receipt_hold_facts(
    request: &StockReceiptQualityRequest,
) -> Result<GoodsReceiptHoldFacts, BusinessError> {
    if request.command_key.is_empty()
        || request.fulfillment_id.is_empty()
        || request.company_id.is_empty()
        || request.branch_id.is_empty()
        || request.warehouse_id.is_empty()
        || request.product_id.is_empty()
        || request.lot_id.is_empty()
        || request.actor_id.is_empty()
        || request.quantity_milli <= 0
        || request.now < 0
    {
        return Err(invalid());
    }
    Ok(GoodsReceiptHoldFacts {
        command_key: request.command_key.clone(),
        fulfillment_id: request.fulfillment_id.clone(),
        line_index: request.line_index,
        allocation_index: request.allocation_index,
        company_id: request.company_id.clone(),
        branch_id: request.branch_id.clone(),
        warehouse_id: request.warehouse_id.clone(),
        product_id: request.product_id.clone(),
        lot_id: request.lot_id.clone(),
        quantity_milli: request.quantity_milli,
        reason: request.reason.clone(),
        due_date: request.due_date.clone(),
        actor_id: request.actor_id.clone(),
        now: request.now,
    })
}

/// Inventory's same-transaction witness for Quality. It asserts the selected
/// warehouse belongs to the receipt branch, the lot is the active lot for the
/// product in that warehouse and still covers the quarantined quantity, and no
/// other live hold claims the same receipt allocation.
struct ReceiptHoldSource {
    facts: GoodsReceiptHoldFacts,
}

impl ReceiptHoldSource {
    async fn require_witness(&self, c: &mut PgConnection) -> Result<(), Error> {
        let facts = &self.facts;
        let scope: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM commerce_warehouses \
             WHERE id=$1 AND company_id=$2 AND branch_id=$3)",
        )
        .bind(&facts.warehouse_id)
        .bind(&facts.company_id)
        .bind(&facts.branch_id)
        .fetch_one(&mut *c)
        .await?;
        if !scope {
            return Err(changed());
        }
        let lot: Option<(String, i64)> = sqlx::query_as(
            "SELECT l.status,b.quantity_milli FROM inventory_lots l \
             JOIN inventory_lot_balances b ON b.lot_id=l.id AND b.warehouse_id=$4 \
             WHERE l.id=$1 AND l.company_id=$2 AND l.product_id=$3",
        )
        .bind(&facts.lot_id)
        .bind(&facts.company_id)
        .bind(&facts.product_id)
        .bind(&facts.warehouse_id)
        .fetch_optional(&mut *c)
        .await?;
        let Some((status, quantity_milli)) = lot else {
            return Err(changed());
        };
        if status != "active" || quantity_milli < facts.quantity_milli {
            return Err(changed());
        }
        let conflict: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM inventory_quality_holds \
             WHERE source_type='goods_receipt' AND source_id=$1 AND company_id=$2 \
               AND warehouse_id=$3 AND id<>$4 \
               AND status NOT IN ('released','cancelled','disposed'))",
        )
        .bind(facts.source_id())
        .bind(&facts.company_id)
        .bind(&facts.warehouse_id)
        .bind(facts.hold_id())
        .fetch_one(&mut *c)
        .await?;
        if conflict {
            return Err(changed());
        }
        Ok(())
    }
}

impl GoodsReceiptHoldSource for ReceiptHoldSource {
    fn facts(&self) -> &GoodsReceiptHoldFacts {
        &self.facts
    }

    async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_witness(c).await
    }

    async fn require_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_witness(c).await?;
        let facts = &self.facts;
        let exact: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM inventory_quality_holds \
             WHERE id=$1 AND company_id=$2 AND warehouse_id=$3 AND lot_id=$4 \
               AND source_type='goods_receipt' AND source_id=$5 \
               AND held_quantity_milli=$6 AND status='held')",
        )
        .bind(facts.hold_id())
        .bind(&facts.company_id)
        .bind(&facts.warehouse_id)
        .bind(&facts.lot_id)
        .bind(facts.source_id())
        .bind(facts.quantity_milli)
        .fetch_one(&mut *c)
        .await?;
        let live_holds: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM inventory_quality_holds \
             WHERE source_type='goods_receipt' AND source_id=$1 AND company_id=$2 \
               AND warehouse_id=$3 AND status NOT IN ('released','cancelled','disposed')",
        )
        .bind(facts.source_id())
        .bind(&facts.company_id)
        .bind(&facts.warehouse_id)
        .fetch_one(&mut *c)
        .await?;
        if !exact || live_holds != 1 {
            return Err(changed());
        }
        Ok(())
    }
}

impl StockReceiptQualityPort for QualityReceiptHoldPort {
    fn create_quality_hold<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        request: StockReceiptQualityRequest,
    ) -> PgWorkFuture<'a, String, BusinessError> {
        Box::pin(async move {
            let source = ReceiptHoldSource {
                facts: receipt_hold_facts(&request)?,
            };
            let prepared = prepare_goods_receipt_hold(c, &source).await?;
            let retained = prepared.apply(c).await?;
            retained.require_current(c).await?;
            Ok(retained.hold_id())
        })
    }
}
