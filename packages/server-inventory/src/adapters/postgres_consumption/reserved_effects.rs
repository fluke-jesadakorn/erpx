//! Inventory-owned Field Service reserved-consumption effects. Original stock
//! data fixes both the reservation-trigger intermediate image and final issue
//! image before effects. The mandatory source port must reborrow actual owner
//! reservation proofs; these data objects never grant IAM or a purchase.
use super::field_service_raw::{
    PgFieldServiceConsumptionPreparation, read_raw_lot, read_raw_stock,
};
use super::*;
use crate::application::field_service_consumption::{
    FieldServiceConsumptionPlan, FieldServiceConsumptionSelection,
};
use crate::domain::valuation::safe;
use erpx_server_shared::public::{CommandKey, TimestampMillis, UserId};

/// Factual reservation binding. Scalar fields are never sufficient evidence;
/// every mutation phase additionally invokes the mandatory owner proof port.
#[derive(Clone, Eq, PartialEq)]
pub struct FieldServiceReservationBinding {
    pub owner_key: CommandKey,
    pub order_id: String,
    pub reservation_id: String,
    pub company_id: CompanyId,
    pub branch_id: RecordId,
    pub department_id: Option<RecordId>,
    pub warehouse_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub remaining_before: i64,
}
/// Implement at the service root with actual Field Service reservation objects.
/// Original and Applied must revalidate that owner's sealed same-unit proofs.
/// No source-kind label, caller boolean or default implementation grants access.
pub trait PgFieldServiceReservationSource: Send + Sync {
    type Applied: Send + Sync;
    fn binding(&self) -> &FieldServiceReservationBinding;
    fn require_original(
        &self,
        c: &mut PgConnection,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn require_applied(
        &self,
        c: &mut PgConnection,
        applied: &Self::Applied,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
/// Reserved effects need the actual reservation and movement view fences before
/// the first Holding snapshot, in addition to the normal planning utility set.
/// This utility reads no rows and does not provide a stock write permission.
pub async fn lock_reserved_consumption_sources(c: &mut PgConnection) -> Result<(), Error> {
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
/// Original owner data. Only the mandatory sealed reservation source can bind
/// this request to an actual Field Service reservation in the same transaction.
pub struct FieldServiceConsumptionIdentity {
    pub owner_key: CommandKey,
    pub order_id: String,
    pub order_version_after: i64,
    pub department_id: Option<RecordId>,
    pub actor_id: UserId,
    pub reason: String,
    pub created_at: TimestampMillis,
    pub journal_id: RecordId,
}
struct ExpectedLot {
    id: String,
    after: LotImage,
    other_balances: (String, i64, bool),
}
/// Intended after-images are computed before any effects, never accepted from
/// rows returned by a trigger. No public constructor, Clone or Deserialize.
pub struct PgReservedConsumptionEffects<S> {
    source: PgFieldServiceConsumptionPreparation<S>,
    identity: FieldServiceConsumptionIdentity,
    consumption_id: String,
    reservation: FieldServiceReservationBinding,
    stock_intermediate: StockValuation,
    stock_after: StockValuation,
    lots: Vec<ExpectedLot>,
}
impl<S: Send + Sync> PgFieldServiceConsumptionPreparation<S> {
    /// Call before Finance preparation so its mandatory owner source can borrow
    /// this exact Finance proof throughout all subsequent phases. Pure identity
    /// input is not a replacement for the caller's actual Field Service source.
    pub async fn retain_reserved_effects<R: PgFieldServiceReservationSource>(
        self,
        c: &mut PgConnection,
        identity: FieldServiceConsumptionIdentity,
        reservation: &R,
    ) -> Result<PgReservedConsumptionEffects<S>, Error> {
        require_unit(c).await?;
        if transaction(c).await? != self.transaction {
            return Err(changed());
        }
        reservation.require_original(c).await?;
        let binding = reservation.binding().clone();
        let selection = &self.plan.selection;
        if binding.owner_key != identity.owner_key
            || binding.order_id != identity.order_id
            || binding.company_id != selection.company_id
            || binding.branch_id != selection.branch_id
            || binding.department_id != identity.department_id
            || binding.warehouse_id != selection.warehouse_id
            || binding.product_id != selection.product_id
            || binding.quantity_milli != selection.quantity_milli
            || binding.remaining_before < 0
            || binding.remaining_before > 9_007_199_254_740_991
        {
            return Err(changed());
        }
        // This is provisional original expectedVersion+1 data. MAX_SAFE is
        // valid wire input; do not turn its successor into an early failure
        // before Journal/Money preparation and the actual late Work CAS.
        if !(2..=9_007_199_254_740_992).contains(&identity.order_version_after)
            || identity.reason.is_empty()
            || identity.reason.chars().count() > 3000
        {
            return Err(BusinessError::invalid(
                "INVENTORY_CONSUMPTION_OWNER_INPUT",
                "Retain the validated Field Service command and next order version.",
            )
            .into());
        }
        let valuation = &self.plan.valuation;
        let mut stock_intermediate = valuation.before.clone();
        stock_intermediate.version = safe(stock_intermediate.version + 1)?;
        let mut stock_after = stock_intermediate.clone();
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
            if after.valuation.quantity_milli == 0 && !other_balances.2 {
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
        Ok(PgReservedConsumptionEffects {
            source: self,
            identity,
            consumption_id,
            reservation: binding,
            stock_intermediate,
            stock_after,
            lots,
        })
    }
}
impl<S: Send + Sync> PgReservedConsumptionEffects<S> {
    pub fn plan(&self) -> &FieldServiceConsumptionPlan {
        self.source.plan()
    }
    pub fn finance_source(&self) -> &S {
        self.source.finance_source()
    }
    pub fn identity(&self) -> &FieldServiceConsumptionIdentity {
        &self.identity
    }
    pub fn consumption_id(&self) -> &str {
        &self.consumption_id
    }
    /// Unchanged factual sources before Finance preparation, without moving
    /// availability/date effect assertions ahead of Journal/Money errors.
    pub async fn require_current_sources<
        F: PgConsumptionFinancePort<Source = S>,
        R: PgFieldServiceReservationSource,
    >(
        &self,
        c: &mut PgConnection,
        finance: &mut F,
        reservation: &R,
    ) -> Result<(), Error> {
        self.require_binding(reservation)?;
        reservation.require_original(c).await?;
        self.source.require_current(c, finance).await
    }
    /// At the original effect position, after Journal/Money preparation and
    /// late order CAS, before mapping and the owner's reservation mutation.
    /// Expiry and remaining-quantity assertions stay in the later owner phase.
    pub async fn require_before<
        F: PgConsumptionFinancePort<Source = S>,
        R: PgFieldServiceReservationSource,
    >(
        &self,
        c: &mut PgConnection,
        finance: &mut F,
        reservation: &R,
    ) -> Result<(), Error> {
        self.require_current_sources(c, finance, reservation)
            .await?;
        let p = &self.source.plan;
        let quantity = p.valuation.before.quantity_milli;
        let quarantined = if self.source.quality.whole_quantity_hold() {
            quantity
        } else {
            self.source.quality.partial_held_milli().min(quantity)
        };
        // This is the ORIGINAL reservation allowance. Do not re-evaluate the
        // view against the later consumed reservation and credit it twice.
        let row=sqlx::query("SELECT s.quantity_milli::numeric-COALESCE((SELECT SUM(r.quantity_milli)FROM inventory_active_reservations r WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0)-$3::numeric+$4::numeric >= $5::numeric available,NOT EXISTS(SELECT 1 FROM inventory_ledger m WHERE m.warehouse_id=s.warehouse_id AND m.product_id=s.product_id AND m.movement_date>$6) date_current FROM commerce_stock s WHERE s.warehouse_id=$1 AND s.product_id=$2 FOR UPDATE OF s")
            .bind(p.selection.warehouse_id.as_str()).bind(p.selection.product_id.as_str()).bind(quarantined).bind(self.reservation.remaining_before).bind(p.selection.quantity_milli).bind(&p.selection.entry_date).fetch_optional(&mut*c).await?.ok_or_else(guard_conflict)?;
        if !row.try_get::<bool, _>("available")? || !row.try_get::<bool, _>("date_current")? {
            return Err(guard_conflict());
        }
        for lot in &self.lots {
            if other_balances(c, &lot.id, &p.selection).await? != lot.other_balances {
                return Err(changed());
            }
        }
        Ok(())
    }
    /// After the real reservation UPDATE and its trigger, before the journal.
    /// A copied version/quantity cannot stand in for R::Applied's owner proof.
    pub async fn require_intermediate<
        F: PgConsumptionFinancePort<Source = S>,
        R: PgFieldServiceReservationSource,
    >(
        &self,
        c: &mut PgConnection,
        finance: &mut F,
        reservation: &R,
        applied: &R::Applied,
    ) -> Result<(), Error> {
        self.require_unit(c).await?;
        self.require_binding(reservation)?;
        reservation.require_applied(c, applied).await?;
        self.source.commerce.require_current(c).await?;
        self.source.quality.require_current(c).await?;
        finance.require_current(c, &self.source.finance).await?;
        if read_raw_stock(
            c,
            &self.source.plan.selection,
            self.source.commerce.tracking_mode(),
        )
        .await?
        .as_ref()
            != Some(&self.stock_intermediate)
        {
            return Err(changed());
        }
        for (id, before) in &self.source.lots {
            if read_raw_lot(c, &self.source.plan.selection, id)
                .await?
                .as_ref()
                != Some(before)
            {
                return Err(changed());
            }
        }
        for lot in &self.lots {
            if other_balances(c, &lot.id, &self.source.plan.selection).await? != lot.other_balances
            {
                return Err(changed());
            }
        }
        Ok(())
    }
    fn require_binding<R: PgFieldServiceReservationSource>(
        &self,
        reservation: &R,
    ) -> Result<(), Error> {
        if reservation.binding() != &self.reservation {
            return Err(changed());
        }
        Ok(())
    }
    /// Inventory part of the original effect sequence: journal must already
    /// exist, consumption precedes its immediate-FK lot evidence. Money and the
    /// final stock/Work/event writes remain later phases of the same owner unit.
    pub async fn write_consumption_and_lots<
        F: PgConsumptionFinancePort<Source = S>,
        R: PgFieldServiceReservationSource,
    >(
        &self,
        c: &mut PgConnection,
        finance: &mut F,
        reservation: &R,
        applied: &R::Applied,
    ) -> Result<(), Error> {
        self.require_intermediate(c, finance, reservation, applied)
            .await?;
        let p = &self.source.plan;
        let i = &self.identity;
        sqlx::query("INSERT INTO operational_inventory_consumptions(id,company_id,branch_id,department_id,source_kind,source_id,source_version,warehouse_id,product_id,quantity_milli,value_minor,currency,movement_date,journal_id,reason,created_by,created_at) VALUES($1,$2,$3,$4,'field_service',$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)")
            .bind(&self.consumption_id).bind(p.selection.company_id.as_str()).bind(p.selection.branch_id.as_str()).bind(i.department_id.as_ref().map(RecordId::as_str)).bind(i.order_id.as_str()).bind(i.order_version_after).bind(p.selection.warehouse_id.as_str()).bind(p.selection.product_id.as_str()).bind(p.valuation.quantity_milli).bind(p.valuation.value_minor).bind(&p.config.currency).bind(&p.selection.entry_date).bind(i.journal_id.as_str()).bind(&i.reason).bind(i.actor_id.as_str()).bind(i.created_at.get()).execute(&mut *c).await?;
        for (index, cost) in p.valuation.lots.iter().enumerate() {
            let previous = self.lots[..index]
                .iter()
                .position(|lot| lot.id == cost.allocation.lot_id);
            if let Some(first) = previous {
                let expected = &self.lots[first];
                let first_cost = &p.valuation.lots[first];
                let evidence: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM operational_inventory_consumption_lots WHERE id=$1 AND consumption_id=$2 AND lot_id=$3 AND quantity_milli=$4 AND value_minor=$5 AND source_balance_version=$6) AND EXISTS(SELECT 1 FROM inventory_lot_moves WHERE id=$7 AND lot_id=$3 AND warehouse_id=$8 AND source_type='field_service' AND source_id=$2 AND quantity_milli=-$4 AND value_minor=-$5 AND movement_date=$9 AND created_by=$10 AND created_at=$11)")
                    .bind(format!("{}-lot-{first}",self.consumption_id)).bind(&self.consumption_id).bind(&first_cost.allocation.lot_id).bind(first_cost.allocation.quantity_milli).bind(first_cost.value_minor).bind(first_cost.allocation.balance_version).bind(format!("{}-lot-move-{first}",self.consumption_id)).bind(&p.selection.warehouse_id).bind(&p.selection.entry_date).bind(i.actor_id.as_str()).bind(i.created_at.get()).fetch_one(&mut *c).await?;
                if !evidence {
                    return Err(changed());
                }
                if other_balances(c, &expected.id, &p.selection).await? != expected.other_balances {
                    return Err(changed());
                }
                self.source
                    .require_selected_lot_image(c, index, Some(&expected.after))
                    .await?;
            } else {
                self.source.require_selected_lot_available(c, index).await?;
            }
            let n = sqlx::query("UPDATE inventory_lot_balances SET quantity_milli=quantity_milli-$1,value_minor=value_minor-$2,version=version+1 WHERE lot_id=$3 AND warehouse_id=$4 AND version=$5")
                .bind(cost.allocation.quantity_milli).bind(cost.value_minor).bind(&cost.allocation.lot_id).bind(p.selection.warehouse_id.as_str()).bind(cost.allocation.balance_version).execute(&mut *c).await?.rows_affected();
            if n != 1 {
                return Err(if previous.is_some() {
                    guard_conflict()
                } else {
                    changed()
                });
            }
            sqlx::query("UPDATE inventory_lots SET status=CASE WHEN NOT EXISTS(SELECT 1 FROM inventory_lot_balances WHERE lot_id=$1 AND quantity_milli>0) THEN 'depleted' ELSE status END WHERE id=$1")
                .bind(&cost.allocation.lot_id).execute(&mut *c).await?;
            sqlx::query("INSERT INTO operational_inventory_consumption_lots(id,consumption_id,lot_id,quantity_milli,value_minor,source_balance_version) VALUES($1,$2,$3,$4,$5,$6)")
                .bind(format!("{}-lot-{index}",self.consumption_id)).bind(&self.consumption_id).bind(&cost.allocation.lot_id).bind(cost.allocation.quantity_milli).bind(cost.value_minor).bind(cost.allocation.balance_version).execute(&mut *c).await?;
            sqlx::query("INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,source_id,quantity_milli,value_minor,movement_date,created_by,created_at) VALUES($1,$2,$3,'field_service',$4,$5,$6,$7,$8,$9)")
                .bind(format!("{}-lot-move-{index}",self.consumption_id)).bind(&cost.allocation.lot_id).bind(p.selection.warehouse_id.as_str()).bind(&self.consumption_id).bind(-cost.allocation.quantity_milli).bind(-cost.value_minor).bind(&p.selection.entry_date).bind(i.actor_id.as_str()).bind(i.created_at.get()).execute(&mut *c).await?;
        }
        self.require_lots_after(c).await
    }
    /// After the real Finance stock-cost snapshot. The only accepted stock
    /// predecessor is the original predetermined reservation-trigger image.
    pub async fn write_stock<R: PgFieldServiceReservationSource>(
        &self,
        c: &mut PgConnection,
        reservation: &R,
        applied: &R::Applied,
    ) -> Result<(), Error> {
        self.require_unit(c).await?;
        self.require_binding(reservation)?;
        reservation.require_applied(c, applied).await?;
        self.require_lots_after(c).await?;
        let p = &self.source.plan;
        let before = &self.stock_intermediate;
        let n=sqlx::query("UPDATE commerce_stock SET quantity_milli=$1,value_minor=$2,version=$3 WHERE warehouse_id=$4 AND product_id=$5 AND quantity_milli=$6 AND value_minor=$7 AND version=$8")
            .bind(self.stock_after.quantity_milli).bind(self.stock_after.value_minor).bind(self.stock_after.version).bind(p.selection.warehouse_id.as_str()).bind(p.selection.product_id.as_str()).bind(before.quantity_milli).bind(before.value_minor).bind(before.version).execute(&mut *c).await?.rows_affected();
        if n != 1 {
            return Err(changed());
        }
        self.require_inventory_after(c, reservation, applied).await
    }
    /// The actual owner must call again immediately before commit, after its
    /// order/event/notification/receipt effects and all deferred triggers. It must
    /// separately retain Finance/Money, IAM, purchases, source and Workflow.
    pub async fn require_inventory_after<R: PgFieldServiceReservationSource>(
        &self,
        c: &mut PgConnection,
        reservation: &R,
        applied: &R::Applied,
    ) -> Result<(), Error> {
        self.require_unit(c).await?;
        self.require_binding(reservation)?;
        reservation.require_applied(c, applied).await?;
        self.source.commerce.require_current(c).await?;
        self.source.quality.require_current(c).await?;
        let p = &self.source.plan;
        if read_raw_stock(c, &p.selection, self.source.commerce.tracking_mode())
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
            if read_raw_lot(c, &self.source.plan.selection, &lot.id)
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
        let exact:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM operational_inventory_consumptions WHERE id=$1 AND company_id=$2 AND branch_id=$3 AND department_id IS NOT DISTINCT FROM $4 AND source_kind='field_service' AND source_id=$5 AND source_version=$6 AND warehouse_id=$7 AND product_id=$8 AND quantity_milli=$9 AND value_minor=$10 AND currency=$11 AND movement_date=$12 AND journal_id=$13 AND reason=$14 AND created_by=$15 AND created_at=$16)")
            .bind(&self.consumption_id).bind(p.selection.company_id.as_str()).bind(p.selection.branch_id.as_str()).bind(i.department_id.as_ref().map(RecordId::as_str)).bind(i.order_id.as_str()).bind(i.order_version_after).bind(p.selection.warehouse_id.as_str()).bind(p.selection.product_id.as_str()).bind(p.valuation.quantity_milli).bind(p.valuation.value_minor).bind(&p.config.currency).bind(&p.selection.entry_date).bind(i.journal_id.as_str()).bind(&i.reason).bind(i.actor_id.as_str()).bind(i.created_at.get()).fetch_one(&mut *c).await?;
        if !exact {
            return Err(changed());
        }
        let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM operational_inventory_consumption_lots WHERE consumption_id=$1),(SELECT count(*) FROM inventory_lot_moves WHERE source_type='field_service' AND source_id=$1)")
            .bind(&self.consumption_id).fetch_one(&mut *c).await?;
        let expected = p.valuation.lots.len() as i64;
        if counts != (expected, expected) {
            return Err(changed());
        }
        for (index, cost) in p.valuation.lots.iter().enumerate() {
            let exact:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM operational_inventory_consumption_lots WHERE id=$1 AND consumption_id=$2 AND lot_id=$3 AND quantity_milli=$4 AND value_minor=$5 AND source_balance_version=$6) AND EXISTS(SELECT 1 FROM inventory_lot_moves WHERE id=$7 AND lot_id=$3 AND warehouse_id=$8 AND source_type='field_service' AND source_id=$2 AND quantity_milli=-$4 AND value_minor=-$5 AND movement_date=$9 AND created_by=$10 AND created_at=$11)")
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
    selection: &FieldServiceConsumptionSelection,
) -> Result<(String, i64, bool), Error> {
    // Lock and hash the complete deterministic original set in PostgreSQL;
    // never transfer or impose a bound on unrelated warehouse identities.
    Ok(sqlx::query_as("SELECT encode(sha256(convert_to(COALESCE(jsonb_agg(jsonb_build_array(x.warehouse_id,x.quantity_milli,x.value_minor,x.version) ORDER BY x.warehouse_id COLLATE \"C\"),'[]'::jsonb)::text,'UTF8')),'hex'),count(*)::bigint,COALESCE(bool_or(x.quantity_milli>0),false) FROM (SELECT warehouse_id,quantity_milli,value_minor,version FROM inventory_lot_balances WHERE lot_id=$1 AND warehouse_id<>$2 FOR SHARE) x")
        .bind(lot).bind(&selection.warehouse_id).fetch_one(c).await?)
}
