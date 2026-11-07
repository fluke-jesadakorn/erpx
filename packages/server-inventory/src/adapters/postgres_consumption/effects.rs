//! Inventory-owned Maintenance effects on the original borrowed unit. These
//! objects retain data, never IAM, purchases, a receipt, or a commit capability.
//! The source coordinator must retain those actual authorities and the Finance
//! journal/Money phases, Work event, notification, audit and outbox atomically.
use super::*;
use crate::domain::valuation::safe;
use erpx_server_shared::public::{CommandKey, TimestampMillis, UserId};

/// Original owner input, not an actor grant. Field Service cannot use this path:
/// its reservation-consumption trigger needs a different stock version protocol.
pub struct MaintenanceConsumptionIdentity {
    pub owner_key: CommandKey,
    pub work_id: RecordId,
    pub work_version_after: i64,
    pub department_id: Option<RecordId>,
    pub actor_id: UserId,
    pub reason: String,
    pub created_at: TimestampMillis,
    pub journal_id: RecordId,
}
struct ExpectedLot {
    id: String,
    after: LotImage,
    other_balances: Vec<(String, i64, i64, i64)>,
}
/// Intended after-images are computed before any effects, never accepted from
/// rows returned by a trigger. No public constructor, Clone or Deserialize.
pub struct PgUnreservedConsumptionEffects<S> {
    source: PgConsumptionPreparation<S>,
    identity: MaintenanceConsumptionIdentity,
    consumption_id: String,
    stock_after: StockValuation,
    lots: Vec<ExpectedLot>,
}
impl<S: Send + Sync> PgConsumptionPreparation<S> {
    /// Call before Finance preparation so its mandatory owner source can borrow
    /// this exact Finance proof throughout all subsequent phases. Pure identity
    /// input is not a replacement for the caller's actual Maintenance source.
    pub async fn retain_unreserved_effects(
        self,
        c: &mut PgConnection,
        identity: MaintenanceConsumptionIdentity,
    ) -> Result<PgUnreservedConsumptionEffects<S>, Error> {
        require_unit(c).await?;
        if transaction(c).await? != self.transaction {
            return Err(changed());
        }
        // This is provisional original expectedVersion+1 data. MAX_SAFE is
        // valid wire input; do not turn its successor into an early failure
        // before Journal/Money preparation and the actual late Work CAS.
        if !(2..=9_007_199_254_740_992).contains(&identity.work_version_after)
            || identity.reason.is_empty()
            || identity.reason.chars().count() > 2000
        {
            return Err(BusinessError::invalid(
                "INVENTORY_CONSUMPTION_OWNER_INPUT",
                "Retain the validated Maintenance command and next Work version.",
            )
            .into());
        }
        let valuation = &self.plan.valuation;
        let mut stock_after = valuation.before.clone();
        stock_after.quantity_milli = safe(stock_after.quantity_milli - valuation.quantity_milli)?;
        stock_after.value_minor = safe(stock_after.value_minor - valuation.value_minor)?;
        stock_after.version = safe(stock_after.version + 1)?;
        let mut lots = Vec::with_capacity(self.lots.len());
        for ((id, before), cost) in self.lots.iter().zip(&valuation.lots) {
            if id != &cost.allocation.lot_id {
                return Err(changed());
            }
            let other_balances = other_balances(c, id, &self.plan.selection).await?;
            let mut after = before.clone();
            after.valuation.quantity_milli -= cost.allocation.quantity_milli;
            after.valuation.value_minor -= cost.value_minor;
            after.valuation.version = safe(after.valuation.version + 1)?;
            if after.valuation.quantity_milli == 0
                && !other_balances
                    .iter()
                    .any(|(_, quantity, _, _)| *quantity > 0)
            {
                after.status = "depleted".into();
                after.valuation.active = false;
                // 0331 attachment trigger increments the physical lot version
                // only when depletion actually changes its status.
                after.version = safe(after.version + 1)?;
            }
            after.full_digest = sqlx::query_scalar("SELECT encode(sha256(convert_to((to_jsonb(l)||jsonb_build_object('status',$2::text,'version',$3::bigint))::text,'UTF8')),'hex') FROM inventory_lots l WHERE id=$1")
                .bind(id).bind(&after.status).bind(after.version).fetch_one(&mut *c).await?;
            // A partial issue keeps status and physical-lot version unchanged.
            lots.push(ExpectedLot {
                id: id.clone(),
                after,
                other_balances,
            });
        }
        let consumption_id = format!("operational-part-{}", identity.owner_key.as_str());
        Ok(PgUnreservedConsumptionEffects {
            source: self,
            identity,
            consumption_id,
            stock_after,
            lots,
        })
    }
}
impl<S: Send + Sync> PgUnreservedConsumptionEffects<S> {
    pub fn plan(&self) -> &ConsumptionPlan {
        self.source.plan()
    }
    pub fn finance_source(&self) -> &S {
        self.source.finance_source()
    }
    pub fn identity(&self) -> &MaintenanceConsumptionIdentity {
        &self.identity
    }
    pub fn consumption_id(&self) -> &str {
        &self.consumption_id
    }
    /// Unchanged factual sources before Finance preparation, without moving
    /// availability/date effect assertions ahead of Journal/Money errors.
    pub async fn require_current_sources<F: PgConsumptionFinancePort<Source = S>>(
        &self,
        c: &mut PgConnection,
        finance: &mut F,
    ) -> Result<(), Error> {
        self.source.require_current(c, finance).await
    }
    /// At the actual original apply position, after deferred Finance/Money
    /// preparation and the Work version assertion, before journal insertion.
    pub async fn require_before<F: PgConsumptionFinancePort<Source = S>>(
        &self,
        c: &mut PgConnection,
        finance: &mut F,
    ) -> Result<(), Error> {
        self.source
            .require_unreserved_stock_and_movement_date(c, finance)
            .await?;
        for lot in &self.lots {
            if other_balances(c, &lot.id, &self.source.plan.selection).await? != lot.other_balances
            {
                return Err(changed());
            }
        }
        Ok(())
    }
    /// Inventory part of the original effect sequence: journal must already
    /// exist, consumption precedes its immediate-FK lot evidence. Money and the
    /// final stock/Work/event writes remain later phases of the same owner unit.
    pub async fn write_consumption_and_lots(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_unit(c).await?;
        let p = &self.source.plan;
        let i = &self.identity;
        sqlx::query("INSERT INTO operational_inventory_consumptions(id,company_id,branch_id,department_id,source_kind,source_id,source_version,warehouse_id,product_id,quantity_milli,value_minor,currency,movement_date,journal_id,reason,created_by,created_at) VALUES($1,$2,$3,$4,'maintenance',$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)")
            .bind(&self.consumption_id).bind(p.selection.company_id.as_str()).bind(p.selection.branch_id.as_str()).bind(i.department_id.as_ref().map(RecordId::as_str)).bind(i.work_id.as_str()).bind(i.work_version_after).bind(p.selection.warehouse_id.as_str()).bind(p.selection.product_id.as_str()).bind(p.valuation.quantity_milli).bind(p.valuation.value_minor).bind(&p.config.currency).bind(&p.selection.entry_date).bind(i.journal_id.as_str()).bind(&i.reason).bind(i.actor_id.as_str()).bind(i.created_at.get()).execute(&mut *c).await?;
        for (index, cost) in p.valuation.lots.iter().enumerate() {
            self.source.require_selected_lot_available(c, index).await?;
            let n = sqlx::query("UPDATE inventory_lot_balances SET quantity_milli=quantity_milli-$1,value_minor=value_minor-$2,version=version+1 WHERE lot_id=$3 AND warehouse_id=$4 AND version=$5")
                .bind(cost.allocation.quantity_milli).bind(cost.value_minor).bind(&cost.allocation.lot_id).bind(p.selection.warehouse_id.as_str()).bind(cost.allocation.balance_version).execute(&mut *c).await?.rows_affected();
            if n != 1 {
                return Err(changed());
            }
            sqlx::query("UPDATE inventory_lots SET status=CASE WHEN NOT EXISTS(SELECT 1 FROM inventory_lot_balances WHERE lot_id=$1 AND quantity_milli>0) THEN 'depleted' ELSE status END WHERE id=$1")
                .bind(&cost.allocation.lot_id).execute(&mut *c).await?;
            sqlx::query("INSERT INTO operational_inventory_consumption_lots(id,consumption_id,lot_id,quantity_milli,value_minor,source_balance_version) VALUES($1,$2,$3,$4,$5,$6)")
                .bind(format!("{}-lot-{index}",self.consumption_id)).bind(&self.consumption_id).bind(&cost.allocation.lot_id).bind(cost.allocation.quantity_milli).bind(cost.value_minor).bind(cost.allocation.balance_version).execute(&mut *c).await?;
            sqlx::query("INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,source_id,quantity_milli,value_minor,movement_date,created_by,created_at) VALUES($1,$2,$3,'maintenance',$4,$5,$6,$7,$8,$9)")
                .bind(format!("{}-lot-move-{index}",self.consumption_id)).bind(&cost.allocation.lot_id).bind(p.selection.warehouse_id.as_str()).bind(&self.consumption_id).bind(-cost.allocation.quantity_milli).bind(-cost.value_minor).bind(&p.selection.entry_date).bind(i.actor_id.as_str()).bind(i.created_at.get()).execute(&mut *c).await?;
        }
        self.require_lots_after(c).await
    }
    /// After the real Finance stock-cost snapshot. This zero-reservation writer
    /// deliberately does not accept a caller-supplied version increment/credit.
    pub async fn write_stock(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_unit(c).await?;
        self.require_lots_after(c).await?;
        let p = &self.source.plan;
        let before = &p.valuation.before;
        let n=sqlx::query("UPDATE commerce_stock SET quantity_milli=$1,value_minor=$2,version=$3 WHERE warehouse_id=$4 AND product_id=$5 AND quantity_milli=$6 AND value_minor=$7 AND version=$8")
            .bind(self.stock_after.quantity_milli).bind(self.stock_after.value_minor).bind(self.stock_after.version).bind(p.selection.warehouse_id.as_str()).bind(p.selection.product_id.as_str()).bind(before.quantity_milli).bind(before.value_minor).bind(before.version).execute(&mut *c).await?.rows_affected();
        if n != 1 {
            return Err(changed());
        }
        self.require_inventory_after(c).await
    }
    /// The actual owner must call again immediately before commit, after its
    /// Work/event/notification/receipt effects and all deferred triggers. It must
    /// separately retain Finance/Money, IAM, purchases, source and Workflow.
    pub async fn require_inventory_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_unit(c).await?;
        self.source.commerce.require_current(c).await?;
        self.source.quality.require_current(c).await?;
        let p = &self.source.plan;
        if read_stock(c, &p.selection, self.source.commerce.tracking_mode())
            .await?
            .as_ref()
            != Some(&self.stock_after)
        {
            return Err(changed());
        }
        self.require_lots_after(c).await?;
        self.require_evidence(c).await
    }
    async fn require_unit(&self, c: &mut PgConnection) -> Result<(), Error> {
        require_unit(c).await?;
        if transaction(c).await? != self.source.transaction {
            return Err(changed());
        }
        Ok(())
    }
    async fn require_lots_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        for lot in &self.lots {
            if read_lot(c, &self.source.plan.selection, &lot.id)
                .await?
                .as_ref()
                != Some(&lot.after)
                || other_balances(c, &lot.id, &self.source.plan.selection).await?
                    != lot.other_balances
            {
                return Err(changed());
            }
        }
        Ok(())
    }
    async fn require_evidence(&self, c: &mut PgConnection) -> Result<(), Error> {
        let p = &self.source.plan;
        let i = &self.identity;
        let exact:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM operational_inventory_consumptions WHERE id=$1 AND company_id=$2 AND branch_id=$3 AND department_id IS NOT DISTINCT FROM $4 AND source_kind='maintenance' AND source_id=$5 AND source_version=$6 AND warehouse_id=$7 AND product_id=$8 AND quantity_milli=$9 AND value_minor=$10 AND currency=$11 AND movement_date=$12 AND journal_id=$13 AND reason=$14 AND created_by=$15 AND created_at=$16)")
            .bind(&self.consumption_id).bind(p.selection.company_id.as_str()).bind(p.selection.branch_id.as_str()).bind(i.department_id.as_ref().map(RecordId::as_str)).bind(i.work_id.as_str()).bind(i.work_version_after).bind(p.selection.warehouse_id.as_str()).bind(p.selection.product_id.as_str()).bind(p.valuation.quantity_milli).bind(p.valuation.value_minor).bind(&p.config.currency).bind(&p.selection.entry_date).bind(i.journal_id.as_str()).bind(&i.reason).bind(i.actor_id.as_str()).bind(i.created_at.get()).fetch_one(&mut *c).await?;
        if !exact {
            return Err(changed());
        }
        let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM operational_inventory_consumption_lots WHERE consumption_id=$1),(SELECT count(*) FROM inventory_lot_moves WHERE source_type='maintenance' AND source_id=$1)")
            .bind(&self.consumption_id).fetch_one(&mut *c).await?;
        let expected = p.valuation.lots.len() as i64;
        if counts != (expected, expected) {
            return Err(changed());
        }
        for (index, cost) in p.valuation.lots.iter().enumerate() {
            let exact:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM operational_inventory_consumption_lots WHERE id=$1 AND consumption_id=$2 AND lot_id=$3 AND quantity_milli=$4 AND value_minor=$5 AND source_balance_version=$6) AND EXISTS(SELECT 1 FROM inventory_lot_moves WHERE id=$7 AND lot_id=$3 AND warehouse_id=$8 AND source_type='maintenance' AND source_id=$2 AND quantity_milli=-$4 AND value_minor=-$5 AND movement_date=$9 AND created_by=$10 AND created_at=$11)")
                .bind(format!("{}-lot-{index}",self.consumption_id)).bind(&self.consumption_id).bind(&cost.allocation.lot_id).bind(cost.allocation.quantity_milli).bind(cost.value_minor).bind(cost.allocation.balance_version).bind(format!("{}-lot-move-{index}",self.consumption_id)).bind(p.selection.warehouse_id.as_str()).bind(&p.selection.entry_date).bind(i.actor_id.as_str()).bind(i.created_at.get()).fetch_one(&mut *c).await?;
            if !exact {
                return Err(changed());
            }
        }
        Ok(())
    }
}
async fn other_balances(
    c: &mut PgConnection,
    lot: &str,
    selection: &ConsumptionSelection,
) -> Result<Vec<(String, i64, i64, i64)>, Error> {
    // Bounded transfer before mutation; same table utility fence retains absence.
    let rows:Vec<(Option<String>,i64,i64,i64)>=sqlx::query_as("SELECT CASE WHEN octet_length(warehouse_id)<=200 THEN warehouse_id END,quantity_milli,value_minor,version FROM inventory_lot_balances WHERE lot_id=$1 AND warehouse_id<>$2 ORDER BY warehouse_id LIMIT 501 FOR SHARE")
        .bind(lot).bind(selection.warehouse_id.as_str()).fetch_all(c).await?;
    if rows.len() > 500 || rows.iter().any(|(id, _, _, _)| id.is_none()) {
        return Err(PersistenceError::new("INVENTORY_CONSUMPTION_BALANCE_LIMIT").into());
    }
    rows.into_iter()
        .map(|(id, q, v, version)| {
            Ok((
                id.ok_or_else(|| PersistenceError::new("INVENTORY_CONSUMPTION_BALANCE_LIMIT"))?,
                q,
                v,
                version,
            ))
        })
        .collect()
}
