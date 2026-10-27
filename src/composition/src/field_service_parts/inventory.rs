//! Actual sealed reservation bridges, plus factual Finance planning only.
use super::{Error, changed, source::FieldServiceReferences};
use erpx_server_commerce::public::field_service_reservation::FieldServiceStockScopeRequest;
use erpx_server_commerce::public::inventory_consumption::CommercePostingSettings;
use erpx_server_field_service::public::parts_source::{
    CurrentFieldServiceOrder, CurrentFieldServiceReservation, FieldServiceOrderIntent,
    PreparedFieldServiceReservation, PreparedFieldServiceReservationConsumption,
    RetainedFieldServiceReservationConsumption, RetainedNewFieldServiceReservation,
};
use erpx_server_finance::public::inventory_consumption::{
    CurrentConsumptionFinance, InventoryConsumptionMapping,
    lock_inventory_consumption_finance_sources, read_inventory_consumption_finance,
};
use erpx_server_inventory::public::{consumption::*, postgres_consumption::*};
use erpx_server_shared::public::{Clock, CommandKey, RecordId};
use sqlx::PgConnection;
/// Real Finance public-facade bridge used by the Field Service composition. The
/// Holding identity comes from the sealed native current Work context.
pub(super) struct FieldServiceFinanceFacts {
    pub(super) holding: Option<erpx_server_shared::public::HoldingId>,
}
impl PgConsumptionFinancePort for FieldServiceFinanceFacts {
    type Source = CurrentConsumptionFinance;
    async fn lock_sources(&mut self, c: &mut PgConnection) -> Result<(), Error> {
        lock_inventory_consumption_finance_sources(c).await
    }
    async fn inspect(
        &mut self,
        c: &mut PgConnection,
        s: &CommercePostingSettings,
        date: &str,
    ) -> Result<Self::Source, Error> {
        let holding = self.holding.as_ref().ok_or_else(changed)?;
        read_inventory_consumption_finance(
            c,
            holding,
            &InventoryConsumptionMapping {
                company_id: s.company_id.clone(),
                chart_id: s.chart_id.clone(),
                inventory_account_id: s.inventory_account_id.clone(),
                cogs_account_id: s.cogs_account_id.clone(),
            },
            date,
        )
        .await
    }
    fn config(s: &Self::Source) -> Option<ConsumptionPostingConfig> {
        s.config().map(|c| ConsumptionPostingConfig {
            chart_id: c.chart_id.clone(),
            currency: c.currency.clone(),
            base_currency: c.base_currency.clone(),
            inventory_account_id: c.inventory_account_id.clone(),
            expense_account_id: c.cogs_account_id.clone(),
        })
    }
    fn periods(s: &Self::Source) -> Vec<RecordId> {
        s.period_ids().to_vec()
    }
    async fn require_current(
        &mut self,
        c: &mut PgConnection,
        s: &Self::Source,
    ) -> Result<(), Error> {
        s.require_current(c).await
    }
}

pub(super) struct ReservationSource<'p> {
    pub(super) plan: &'p PreparedFieldServiceReservationConsumption,
    original: &'p CurrentFieldServiceReservation,
    binding: FieldServiceReservationBinding,
}
impl<'p> ReservationSource<'p> {
    pub(super) fn new(
        plan: &'p PreparedFieldServiceReservationConsumption,
        original: &'p CurrentFieldServiceReservation,
        key: &CommandKey,
        order: &CurrentFieldServiceOrder<'_, FieldServiceReferences>,
    ) -> Result<Self, Error> {
        let f = plan.facts();
        if f != original.facts()
            || f.order_id != order.facts().id
            || f.company_id != order.facts().company_id
            || f.holding_id != order.facts().holding_id
        {
            return Err(changed().into());
        }
        Ok(Self {
            plan,
            original,
            binding: FieldServiceReservationBinding {
                owner_key: key.clone(),
                order_id: f.order_id.clone(),
                reservation_id: f.id.clone(),
                company_id: f.company_id.clone(),
                branch_id: order.facts().branch_id.clone(),
                department_id: order.facts().department_id.clone(),
                warehouse_id: f.warehouse_id.clone(),
                product_id: f.product_id.clone(),
                quantity_milli: plan.quantity_milli(),
                remaining_before: plan.remaining_before(),
            },
        })
    }
}
impl PgFieldServiceReservationSource for ReservationSource<'_> {
    type Applied = RetainedFieldServiceReservationConsumption;
    fn binding(&self) -> &FieldServiceReservationBinding {
        &self.binding
    }
    async fn require_original(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.plan.require_original(c, self.original).await
    }
    async fn require_applied(
        &self,
        c: &mut PgConnection,
        applied: &Self::Applied,
    ) -> Result<(), Error> {
        self.plan.require_applied(c, applied).await
    }
}
pub(super) struct NewReservationSource<'p, 'a, G> {
    prepared: &'p PreparedFieldServiceReservation,
    source: &'p CurrentFieldServiceOrder<'a, FieldServiceReferences>,
    intent: &'p FieldServiceOrderIntent,
    guard: &'p G,
    binding: FieldServiceNewReservationBinding,
}
impl<'p, 'a, G: Clock + Sync> NewReservationSource<'p, 'a, G> {
    pub(super) fn new(
        prepared: &'p PreparedFieldServiceReservation,
        source: &'p CurrentFieldServiceOrder<'a, FieldServiceReferences>,
        intent: &'p FieldServiceOrderIntent,
        guard: &'p G,
    ) -> Result<Self, Error> {
        if prepared.owner_key() != intent.command().key()
            || intent.command().order_id() != &source.facts().id
        {
            return Err(changed().into());
        }
        Ok(Self {
            prepared,
            source,
            intent,
            guard,
            binding: FieldServiceNewReservationBinding {
                owner_key: intent.command().key().clone(),
                order_id: source.facts().id.clone(),
                reservation_id: prepared.reservation_id().into(),
                stock: FieldServiceStockScopeRequest {
                    company_id: source.facts().company_id.clone(),
                    branch_id: source.facts().branch_id.clone(),
                    warehouse_id: prepared.warehouse_id().to_owned(),
                    product_id: prepared.product_id().to_owned(),
                },
                quantity_milli: prepared.quantity_milli(),
            },
        })
    }
}
impl<G: Clock + Sync> PgFieldServiceNewReservationSource for NewReservationSource<'_, '_, G> {
    type Applied = RetainedNewFieldServiceReservation;
    fn binding(&self) -> &FieldServiceNewReservationBinding {
        &self.binding
    }
    async fn require_prepared(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.prepared
            .require_before(c, self.source, self.intent, self.guard.now())
            .await
    }
    async fn require_inserted(
        &self,
        c: &mut PgConnection,
        applied: &Self::Applied,
    ) -> Result<(), Error> {
        self.prepared.require_inserted(c, applied).await
    }
}
