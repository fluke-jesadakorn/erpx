//! Real borrowed planning facts. The result is not source-owner authorization,
//! a reservation allowance, a Money/journal effect, or a write capability.
use crate::application::consumption::{
    ConsumptionPlan, ConsumptionPlanningPorts, ConsumptionPostingConfig, ConsumptionSelection,
    prepare_consumption_observed,
};
use crate::domain::consumption::{LotAllocation, LotValuation, StockValuation, TrackingMode};
use erpx_server_commerce::public::inventory_consumption::{
    self as commerce, CommercePostingSettings, CommerceTrackingMode, ConsumptionCommerceRequest,
    CurrentConsumptionCommerce,
};
use erpx_server_quality::public::inventory_consumption::{
    self as quality, ConsumptionHoldRequest, CurrentConsumptionHolds,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId};
use sqlx::{Connection, PgConnection, Row};
use std::future::Future;
mod effects;
pub use effects::{MaintenanceConsumptionIdentity, PgUnreservedConsumptionEffects};
type Error = PgTransactionError<BusinessError>;

/// Mandatory composition bridge to Finance's actual public current-facts
/// facade. No default, no Finance private SQL, no independently begun unit.
pub trait PgConsumptionFinancePort: Send {
    type Source: Send + Sync;
    fn lock_sources(
        &mut self,
        c: &mut PgConnection,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn inspect(
        &mut self,
        c: &mut PgConnection,
        settings: &CommercePostingSettings,
        date: &str,
    ) -> impl Future<Output = Result<Self::Source, Error>> + Send;
    fn config(source: &Self::Source) -> Option<ConsumptionPostingConfig>;
    fn periods(source: &Self::Source) -> Vec<RecordId>;
    fn require_current(
        &mut self,
        c: &mut PgConnection,
        source: &Self::Source,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
#[derive(Eq, PartialEq)]
struct Transaction {
    database: String,
    backend: i32,
    xid: String,
    postmaster: String,
}
#[derive(Clone, Eq, PartialEq)]
struct LotImage {
    valuation: LotValuation,
    version: i64,
    lot_code: Option<String>,
    serial_number: Option<String>,
    manufactured_date: Option<String>,
    status: String,
    full_digest: String,
}
/// Opaque source preparation in this exact transaction. Plain `ConsumptionPlan`
/// remains cloneable data; only this object retains the actual source witnesses.
pub struct PgConsumptionPreparation<S> {
    plan: ConsumptionPlan,
    commerce: CurrentConsumptionCommerce,
    quality: CurrentConsumptionHolds,
    finance: S,
    lots: Vec<(String, LotImage)>,
    transaction: Transaction,
}
impl<S: Send + Sync> PgConsumptionPreparation<S> {
    pub fn plan(&self) -> &ConsumptionPlan {
        &self.plan
    }
    /// Borrow the actual Finance proof retained by this preparation. This is
    /// factual provenance, not a posting permission or a replacement source.
    pub fn finance_source(&self) -> &S {
        &self.finance
    }
    /// Additional original effect-position guards for an unreserved consumer
    /// (Maintenance). No caller-supplied own-reservation allowance is accepted.
    /// The coordinator must acquire lock_unreserved_consumption_sources before
    /// IAM or any other Holding SELECT. This does not grant a stock mutation.
    pub async fn require_unreserved_stock_and_movement_date<
        F: PgConsumptionFinancePort<Source = S>,
    >(
        &self,
        c: &mut PgConnection,
        finance: &mut F,
    ) -> Result<(), Error> {
        self.require_current(c, finance).await?;
        let selection = &self.plan.selection;
        let quantity = self.plan.valuation.before.quantity_milli;
        let quarantined = if self.quality.whole_quantity_hold() {
            quantity
        } else {
            self.quality.partial_held_milli().min(quantity)
        };
        // NUMERIC arithmetic preserves the original predicate without SUM or
        // subtraction overflow; neither reservation bodies nor source labels
        // are transferred. The actual scoped Commerce source is retained.
        let row=sqlx::query("SELECT s.quantity_milli::numeric-COALESCE((SELECT SUM(r.quantity_milli)FROM inventory_active_reservations r WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0)-$3::numeric >= $4::numeric available,NOT EXISTS(SELECT 1 FROM inventory_ledger m WHERE m.warehouse_id=s.warehouse_id AND m.product_id=s.product_id AND m.movement_date>$5) date_current FROM commerce_stock s WHERE s.warehouse_id=$1 AND s.product_id=$2 FOR UPDATE OF s")
            .bind(selection.warehouse_id.as_str()).bind(selection.product_id.as_str()).bind(quarantined).bind(selection.quantity_milli).bind(&selection.entry_date).fetch_optional(c).await?.ok_or_else(guard_conflict)?;
        if !row.try_get::<bool, _>("available")? {
            return Err(guard_conflict());
        }
        if !row.try_get::<bool, _>("date_current")? {
            return Err(guard_conflict());
        }
        Ok(())
    }
    /// Check each actual selected lot immediately before its own effect. The
    /// unchanged-plan check must not be reused after earlier legitimate lot
    /// changes. This method accepts only an index into the retained selection.
    pub async fn require_selected_lot_available(
        &self,
        c: &mut PgConnection,
        index: usize,
    ) -> Result<(), Error> {
        require_unit(c).await?;
        if transaction(c).await? != self.transaction {
            return Err(changed());
        }
        let (id, before) = self.lots.get(index).ok_or_else(|| {
            BusinessError::invalid(
                "INVENTORY_CONSUMPTION_LOT_INDEX",
                "Use an actual retained consumption lot.",
            )
        })?;
        self.commerce.require_current(c).await?;
        self.quality.require_current(c).await?;
        if read_lot(c, &self.plan.selection, id).await?.as_ref() != Some(before) {
            return Err(changed());
        }
        if self
            .quality
            .blocked_lots()
            .iter()
            .any(|lot| lot.as_str() == id)
        {
            return Err(guard_conflict());
        }
        Ok(())
    }
    pub fn commerce(&self) -> &CurrentConsumptionCommerce {
        &self.commerce
    }
    pub fn quality(&self) -> &CurrentConsumptionHolds {
        &self.quality
    }
    /// Before any eventual effects, revalidate these planning inputs and all
    /// external current authority/reservations separately. Canonical stock/lot
    /// changes need a later exact expected-after-image protocol, not this method.
    pub async fn require_current<F: PgConsumptionFinancePort<Source = S>>(
        &self,
        c: &mut PgConnection,
        finance: &mut F,
    ) -> Result<(), Error> {
        require_unit(c).await?;
        if transaction(c).await? != self.transaction {
            return Err(changed());
        }
        self.commerce.require_current(c).await?;
        self.quality.require_current(c).await?;
        finance.require_current(c, &self.finance).await?;
        let stock = read_stock(c, &self.plan.selection, self.commerce.tracking_mode()).await?;
        if stock.as_ref() != Some(&self.plan.valuation.before) {
            return Err(changed());
        }
        for (id, before) in &self.lots {
            if read_lot(c, &self.plan.selection, id).await?.as_ref() != Some(before) {
                return Err(changed());
            }
        }
        Ok(())
    }
}

/// Compose with IAM, source-owner, reservation, Finance/Money and Workflow
/// utility fences before ANY Holding SELECT. This function reads no data and
/// provides no permission; no transaction or retry is started internally.
/// Utility-only exact Inventory view fences. PostgreSQL recursively locks the
/// declared view relations. These writer-side fences must precede the first
/// SERIALIZABLE snapshot, in the owning coordinator's fixed source-lock order.
/// Existing read-only planning does not acquire or imply these extra guards.
pub async fn lock_unreserved_consumption_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("INVENTORY_CONSUMPTION_TRANSACTION_REQUIRED").into());
    }
    sqlx::query(
        "LOCK TABLE inventory_active_reservations,inventory_ledger IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}
pub async fn lock_inventory_consumption_sources<F: PgConsumptionFinancePort>(
    c: &mut PgConnection,
    finance: &mut F,
) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("INVENTORY_CONSUMPTION_TRANSACTION_REQUIRED").into());
    }
    commerce::lock_consumption_sources(c).await?;
    quality::lock_consumption_sources(c).await?;
    sqlx::query("LOCK TABLE commerce_stock,inventory_lots,inventory_lot_balances,inventory_lot_moves,operational_inventory_consumptions,operational_inventory_consumption_lots IN SHARE ROW EXCLUSIVE MODE").execute(&mut *c).await?;
    finance.lock_sources(c).await
}
pub async fn prepare_inventory_consumption<F: PgConsumptionFinancePort>(
    c: &mut PgConnection,
    finance: &mut F,
    selection: ConsumptionSelection,
) -> Result<PgConsumptionPreparation<F::Source>, Error> {
    Ok(
        prepare_inventory_consumption_observed(c, finance, selection, || ())
            .await?
            .0,
    )
}
pub async fn prepare_inventory_consumption_at<
    F: PgConsumptionFinancePort,
    C: erpx_server_shared::public::Clock + Sync,
>(
    c: &mut PgConnection,
    finance: &mut F,
    selection: ConsumptionSelection,
    clock: &C,
) -> Result<
    (
        PgConsumptionPreparation<F::Source>,
        erpx_server_shared::public::TimestampMillis,
    ),
    Error,
> {
    prepare_inventory_consumption_observed(c, finance, selection, || clock.now()).await
}
async fn prepare_inventory_consumption_observed<F, O, Observe>(
    c: &mut PgConnection,
    finance: &mut F,
    selection: ConsumptionSelection,
    observe: Observe,
) -> Result<(PgConsumptionPreparation<F::Source>, O), Error>
where
    F: PgConsumptionFinancePort,
    O: Send,
    Observe: FnOnce() -> O + Send,
{
    require_unit(c).await?;
    let mut ports = Planning {
        c,
        finance,
        commerce: None,
        finance_source: None,
        lots: Vec::new(),
        date: selection.entry_date.clone(),
    };
    // Preserve the canonical portable plan's date/stock/config/period/lot order.
    let (plan, observation) = prepare_consumption_observed(&mut ports, selection, observe).await?;
    let commerce = ports
        .commerce
        .take()
        .ok_or_else(|| PersistenceError::new("INVENTORY_CONSUMPTION_COMMERCE_REQUIRED"))?;
    let finance_source = ports
        .finance_source
        .take()
        .ok_or_else(|| PersistenceError::new("INVENTORY_CONSUMPTION_FINANCE_REQUIRED"))?;
    let request = ConsumptionHoldRequest {
        company_id: plan.selection.company_id.clone(),
        branch_id: plan.selection.branch_id.clone(),
        warehouse_id: plan.selection.warehouse_id.clone(),
        product_id: plan.selection.product_id.clone(),
        lot_ids: plan
            .selection
            .allocations
            .iter()
            .map(|a| RecordId::new(&a.lot_id))
            .collect::<Result<_, _>>()?,
    };
    let quality = quality::read_consumption_holds(ports.c, &request).await?;
    let transaction = transaction(ports.c).await?;
    Ok((
        PgConsumptionPreparation {
            plan,
            commerce,
            quality,
            finance: finance_source,
            lots: ports.lots,
            transaction,
        },
        observation,
    ))
}
struct Planning<'c, 'f, F: PgConsumptionFinancePort> {
    c: &'c mut PgConnection,
    finance: &'f mut F,
    commerce: Option<CurrentConsumptionCommerce>,
    finance_source: Option<F::Source>,
    lots: Vec<(String, LotImage)>,
    date: String,
}
impl<F: PgConsumptionFinancePort> ConsumptionPlanningPorts for Planning<'_, '_, F> {
    type Error = Error;
    async fn stock(
        &mut self,
        selection: &ConsumptionSelection,
    ) -> Result<Option<StockValuation>, Error> {
        let request = ConsumptionCommerceRequest {
            company_id: selection.company_id.clone(),
            branch_id: selection.branch_id.clone(),
            warehouse_id: selection.warehouse_id.clone(),
            product_id: selection.product_id.clone(),
        };
        self.commerce = commerce::read_consumption_source(self.c, &request).await?;
        match &self.commerce {
            Some(source) => read_stock(self.c, selection, source.tracking_mode()).await,
            None => Ok(None),
        }
    }
    async fn posting_config(
        &mut self,
        company: &CompanyId,
    ) -> Result<Option<ConsumptionPostingConfig>, Error> {
        let Some(settings) = self
            .commerce
            .as_ref()
            .and_then(CurrentConsumptionCommerce::posting_settings)
        else {
            return Ok(None);
        };
        if &settings.company_id != company {
            return Err(changed());
        }
        let source = self.finance.inspect(self.c, settings, &self.date).await?;
        let config = F::config(&source);
        self.finance_source = Some(source);
        Ok(config)
    }
    async fn posting_periods(&mut self, _: &CompanyId, _: &str) -> Result<Vec<RecordId>, Error> {
        self.finance_source
            .as_ref()
            .map(F::periods)
            .ok_or_else(changed)
    }
    async fn lot(
        &mut self,
        selection: &ConsumptionSelection,
        allocation: &LotAllocation,
    ) -> Result<Option<LotValuation>, Error> {
        let lot = read_lot(self.c, selection, &allocation.lot_id).await?;
        if let Some(image) = &lot {
            self.lots.push((allocation.lot_id.clone(), image.clone()));
        }
        Ok(lot.map(|l| l.valuation))
    }
}
async fn read_stock(
    c: &mut PgConnection,
    s: &ConsumptionSelection,
    tracking: CommerceTrackingMode,
) -> Result<Option<StockValuation>, Error> {
    let row=sqlx::query("SELECT quantity_milli,value_minor,version FROM commerce_stock WHERE warehouse_id=$1 AND product_id=$2 FOR SHARE")
        .bind(s.warehouse_id.as_str()).bind(s.product_id.as_str()).fetch_optional(c).await?;
    row.map(|r| -> Result<StockValuation, Error> {
        Ok(StockValuation {
            quantity_milli: r.try_get("quantity_milli")?,
            value_minor: r.try_get("value_minor")?,
            version: r.try_get("version")?,
            tracking_mode: match tracking {
                CommerceTrackingMode::None => TrackingMode::None,
                CommerceTrackingMode::Lot => TrackingMode::Lot,
                CommerceTrackingMode::Serial => TrackingMode::Serial,
            },
        })
    })
    .transpose()
}
async fn read_lot(
    c: &mut PgConnection,
    s: &ConsumptionSelection,
    id: &str,
) -> Result<Option<LotImage>, Error> {
    let row=sqlx::query("SELECT b.quantity_milli,b.value_minor,b.version balance_version,l.version lot_version,l.tracking_mode,l.status,encode(sha256(convert_to(to_jsonb(l)::text,'UTF8')),'hex') full_digest,(COALESCE(octet_length(l.lot_code)>200,false) OR COALESCE(octet_length(l.serial_number)>200,false) OR COALESCE(octet_length(l.expiry_date)>10,false) OR COALESCE(octet_length(l.manufactured_date)>10,false)) invalid,CASE WHEN octet_length(l.lot_code)<=200 THEN l.lot_code END lot_code,CASE WHEN octet_length(l.serial_number)<=200 THEN l.serial_number END serial_number,CASE WHEN octet_length(l.expiry_date)<=10 THEN l.expiry_date END expiry_date,CASE WHEN octet_length(l.manufactured_date)<=10 THEN l.manufactured_date END manufactured_date FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id WHERE b.lot_id=$1 AND b.warehouse_id=$2 AND l.product_id=$3 AND l.company_id=$4 FOR SHARE OF b,l")
        .bind(id).bind(s.warehouse_id.as_str()).bind(s.product_id.as_str()).bind(s.company_id.as_str()).fetch_optional(c).await?;
    row.map(|r| -> Result<LotImage, Error> {
        if r.try_get::<bool, _>("invalid")? {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "INVENTORY_CONSUMPTION_SOURCE_LIMIT",
                "The lot source exceeds its bounded identity contract.",
            )
            .into());
        }
        Ok(LotImage {
            valuation: LotValuation {
                quantity_milli: r.try_get("quantity_milli")?,
                value_minor: r.try_get("value_minor")?,
                version: r.try_get("balance_version")?,
                tracking_mode: match r.try_get::<&str, _>("tracking_mode")? {
                    "lot" => TrackingMode::Lot,
                    "serial" => TrackingMode::Serial,
                    _ => return Err(changed()),
                },
                expiry_date: r.try_get("expiry_date")?,
                active: r.try_get::<&str, _>("status")? == "active",
            },
            version: r.try_get("lot_version")?,
            lot_code: r.try_get("lot_code")?,
            serial_number: r.try_get("serial_number")?,
            manufactured_date: r.try_get("manufactured_date")?,
            status: r.try_get("status")?,
            full_digest: r.try_get("full_digest")?,
        })
    })
    .transpose()
}
async fn require_unit(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("INVENTORY_CONSUMPTION_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("INVENTORY_CONSUMPTION_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}
async fn transaction(c: &mut PgConnection) -> Result<Transaction, Error> {
    let r=sqlx::query("SELECT current_database() database,pg_backend_pid() backend,pg_current_xact_id()::text xid,pg_postmaster_start_time()::text postmaster").fetch_one(c).await?;
    Ok(Transaction {
        database: r.try_get("database")?,
        backend: r.try_get("backend")?,
        xid: r.try_get("xid")?,
        postmaster: r.try_get("postmaster")?,
    })
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "INVENTORY_CONSUMPTION_SOURCE_CHANGED",
        "The retained Inventory consumption planning source changed.",
    )
    .into()
}

fn guard_conflict() -> Error {
    BusinessError::new(FailureKind::Conflict,"CONFLICT","The record changed, a reference is unavailable, or this value already exists. Refresh and review again.").into()
}

mod reserved_effects;
pub use reserved_effects::{
    FieldServiceConsumptionIdentity, FieldServiceReservationBinding,
    PgFieldServiceReservationSource, PgReservedConsumptionEffects,
    lock_reserved_consumption_sources,
};
mod reserve_stock;
pub use reserve_stock::{
    FieldServiceNewReservationBinding, LockedFieldServiceStockReservation,
    PgFieldServiceNewReservationSource, PreparedFieldServiceStockReservation,
    prepare_field_service_stock_reservation,
};

mod field_service_raw;
pub use field_service_raw::{
    PgFieldServiceConsumptionPreparation, prepare_field_service_inventory_consumption_at,
};
