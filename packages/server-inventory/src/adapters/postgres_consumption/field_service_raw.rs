//! Field Service-only raw stock/lot planning. Actual foreign proofs stay mandatory.
use super::*;
use crate::application::field_service_consumption::{
    FieldServiceConsumptionPlan, FieldServiceConsumptionPlanningPorts,
    FieldServiceConsumptionSelection, prepare_field_service_consumption_observed,
};
use erpx_server_commerce::public::inventory_consumption::{
    CurrentRawConsumptionCommerce, RawConsumptionCommerceRequest,
};
use erpx_server_quality::public::inventory_consumption::{
    CurrentRawConsumptionHolds, RawConsumptionHoldRequest,
};
pub struct PgFieldServiceConsumptionPreparation<S> {
    pub(super) plan: FieldServiceConsumptionPlan,
    pub(super) commerce: CurrentRawConsumptionCommerce,
    pub(super) quality: CurrentRawConsumptionHolds,
    pub(super) finance: S,
    pub(super) lots: Vec<(String, LotImage)>,
    pub(super) transaction: Transaction,
}
impl<S: Send + Sync> PgFieldServiceConsumptionPreparation<S> {
    pub fn plan(&self) -> &FieldServiceConsumptionPlan {
        &self.plan
    }
    pub fn finance_source(&self) -> &S {
        &self.finance
    }
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
        if read_raw_stock(c, &self.plan.selection, self.commerce.tracking_mode())
            .await?
            .as_ref()
            != Some(&self.plan.valuation.before)
        {
            return Err(changed());
        }
        for (id, before) in &self.lots {
            if read_raw_lot(c, &self.plan.selection, id).await?.as_ref() != Some(before) {
                return Err(changed());
            }
        }
        Ok(())
    }
    pub async fn require_selected_lot_available(
        &self,
        c: &mut PgConnection,
        index: usize,
    ) -> Result<(), Error> {
        self.require_selected_lot_image(c, index, None).await
    }
    pub(super) async fn require_selected_lot_image(
        &self,
        c: &mut PgConnection,
        index: usize,
        expected: Option<&LotImage>,
    ) -> Result<(), Error> {
        require_unit(c).await?;
        if transaction(c).await? != self.transaction {
            return Err(changed());
        }
        let (id, before) = self.lots.get(index).ok_or_else(changed)?;
        self.commerce.require_current(c).await?;
        self.quality.require_current(c).await?;
        if read_raw_lot(c, &self.plan.selection, id).await?.as_ref()
            != Some(expected.unwrap_or(before))
        {
            return Err(changed());
        }
        if self.quality.blocked_lots().iter().any(|lot| lot == id) {
            return Err(guard_conflict());
        }
        Ok(())
    }
}
pub async fn prepare_field_service_inventory_consumption_at<
    F: PgConsumptionFinancePort,
    C: erpx_server_shared::public::Clock + Sync,
>(
    c: &mut PgConnection,
    finance: &mut F,
    selection: FieldServiceConsumptionSelection,
    clock: &C,
) -> Result<
    (
        PgFieldServiceConsumptionPreparation<F::Source>,
        erpx_server_shared::public::TimestampMillis,
    ),
    Error,
> {
    prepare_field_service_inventory_consumption_observed(c, finance, selection, || clock.now())
        .await
}
async fn prepare_field_service_inventory_consumption_observed<F, O, Observe>(
    c: &mut PgConnection,
    finance: &mut F,
    selection: FieldServiceConsumptionSelection,
    observe: Observe,
) -> Result<(PgFieldServiceConsumptionPreparation<F::Source>, O), Error>
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
    let (plan, observation) =
        prepare_field_service_consumption_observed(&mut ports, selection, observe).await?;
    let commerce = ports
        .commerce
        .take()
        .ok_or_else(|| PersistenceError::new("INVENTORY_CONSUMPTION_COMMERCE_REQUIRED"))?;
    let finance_source = ports
        .finance_source
        .take()
        .ok_or_else(|| PersistenceError::new("INVENTORY_CONSUMPTION_FINANCE_REQUIRED"))?;
    let mut physical_ids = Vec::new();
    for allocation in &plan.selection.allocations {
        if !physical_ids.contains(&allocation.lot_id) {
            physical_ids.push(allocation.lot_id.clone());
        }
    }
    let request = RawConsumptionHoldRequest {
        company_id: plan.selection.company_id.clone(),
        branch_id: plan.selection.branch_id.clone(),
        warehouse_id: plan.selection.warehouse_id.clone(),
        product_id: plan.selection.product_id.clone(),
        lot_ids: physical_ids,
    };
    let quality = quality::read_consumption_holds_raw(ports.c, &request).await?;
    let transaction = transaction(ports.c).await?;
    Ok((
        PgFieldServiceConsumptionPreparation {
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
    commerce: Option<CurrentRawConsumptionCommerce>,
    finance_source: Option<F::Source>,
    lots: Vec<(String, LotImage)>,
    date: String,
}
impl<F: PgConsumptionFinancePort> FieldServiceConsumptionPlanningPorts for Planning<'_, '_, F> {
    type Error = Error;
    async fn stock(
        &mut self,
        selection: &FieldServiceConsumptionSelection,
    ) -> Result<Option<StockValuation>, Error> {
        let request = RawConsumptionCommerceRequest {
            company_id: selection.company_id.clone(),
            branch_id: selection.branch_id.clone(),
            warehouse_id: selection.warehouse_id.clone(),
            product_id: selection.product_id.clone(),
        };
        self.commerce = commerce::read_consumption_source_raw(self.c, &request).await?;
        match &self.commerce {
            Some(source) => read_raw_stock(self.c, selection, source.tracking_mode()).await,
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
            .and_then(CurrentRawConsumptionCommerce::posting_settings)
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
        selection: &FieldServiceConsumptionSelection,
        allocation: &LotAllocation,
    ) -> Result<Option<LotValuation>, Error> {
        let lot = read_raw_lot(self.c, selection, &allocation.lot_id).await?;
        if let Some(image) = &lot {
            self.lots.push((allocation.lot_id.clone(), image.clone()));
        }
        Ok(lot.map(|l| l.valuation))
    }
}
pub(super) async fn read_raw_stock(
    c: &mut PgConnection,
    s: &FieldServiceConsumptionSelection,
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
pub(super) async fn read_raw_lot(
    c: &mut PgConnection,
    s: &FieldServiceConsumptionSelection,
    id: &str,
) -> Result<Option<LotImage>, Error> {
    let row=sqlx::query("SELECT b.quantity_milli,b.value_minor,b.version balance_version,l.version lot_version,l.tracking_mode,l.status,l.expiry_date,encode(sha256(convert_to(to_jsonb(l)::text,'UTF8')),'hex') full_digest FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id WHERE b.lot_id=$1 AND b.warehouse_id=$2 AND l.product_id=$3 AND l.company_id=$4 FOR SHARE OF b,l")
        .bind(id).bind(&s.warehouse_id).bind(&s.product_id).bind(s.company_id.as_str()).fetch_optional(c).await?;
    row.map(|r| -> Result<LotImage, Error> {
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
            lot_code: None,
            serial_number: None,
            manufactured_date: None,
            status: r.try_get("status")?,
            full_digest: r.try_get("full_digest")?,
        })
    })
    .transpose()
}
