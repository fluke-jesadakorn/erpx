//! Exact Field Service→Inventory→Finance binding. The prepared and applied
//! source types are distinct: only an actual sealed reservation transition can
//! bridge Finance persistence after the stock-trigger intermediate image.
use super::{
    Error, changed,
    inventory::{FieldServiceFinanceFacts, ReservationSource},
    purchases::FieldServicePurchases,
    source::FieldServiceReferences,
};
use erpx_server_field_service::public::{
    parts::FieldServicePartsInput,
    parts_source::{
        CurrentFieldServiceOrder, CurrentFieldServiceOrderVersion, PreparedFieldServiceOrderWrite,
        RetainedFieldServiceOrderParts, RetainedFieldServiceReservationConsumption,
    },
};
use erpx_server_finance::public::{
    FinanceActor,
    inventory_consumption::CurrentConsumptionFinance,
    parts_draft::{PartsDraftExpected, PartsDraftFacts, PartsOwnerKind, PgPartsDraftOwnerSource},
};
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_inventory::public::postgres_consumption::PgReservedConsumptionEffects;
use erpx_server_shared::public::TimestampMillis;
use sqlx::PgConnection;
use std::sync::OnceLock;

pub(super) struct PreparedPartsSource<'p, 'a> {
    pub(super) current: &'p AuthorizedWorkContext,
    pub(super) source: &'p CurrentFieldServiceOrder<'a, FieldServiceReferences>,
    pub(super) work: &'p PreparedFieldServiceOrderWrite,
    pub(super) inventory: &'p PgReservedConsumptionEffects<CurrentConsumptionFinance>,
    pub(super) reservation: &'p ReservationSource<'p>,
    pub(super) purchases: &'p FieldServicePurchases,
    facts: PartsDraftFacts,
}
impl<'p, 'a> PreparedPartsSource<'p, 'a> {
    pub(super) fn new(
        current: &'p AuthorizedWorkContext,
        source: &'p CurrentFieldServiceOrder<'a, FieldServiceReferences>,
        work: &'p PreparedFieldServiceOrderWrite,
        inventory: &'p PgReservedConsumptionEffects<CurrentConsumptionFinance>,
        reservation: &'p ReservationSource<'p>,
        purchases: &'p FieldServicePurchases,
    ) -> Result<Self, Error> {
        let p = inventory.plan();
        let i = inventory.identity();
        let r = reservation.plan.facts();
        let FieldServicePartsInput::Issue {
            reservation_id,
            quantity_milli,
            entry_date,
            allocations,
            reason,
            ..
        } = work.command().input()
        else {
            return Err(changed().into());
        };
        if work.command().order_id() != &source.facts().id
            || i.owner_key != *work.command().key()
            || i.order_id != source.facts().id
            || i.order_version_after != work.version_after()
            || i.department_id != source.facts().department_id
            || &i.actor_id != source.actor_user_id()
            || current.user_id() != source.actor_user_id()
            || &current.selection().assignment().id != source.actor_assignment_id()
            || current.holding_id() != &source.facts().holding_id
            || current.selection().workspace().company_id != source.facts().company_id
            || i.reason != *reason
            || p.selection.company_id != source.facts().company_id
            || p.selection.branch_id != source.facts().branch_id
            || p.selection.warehouse_id != r.warehouse_id
            || p.selection.product_id != r.product_id
            || r.id.as_str() != reservation_id
            || r.order_id != source.facts().id
            || p.selection.quantity_milli != *quantity_milli
            || entry_date
                .as_ref()
                .is_some_and(|date| date != &p.selection.entry_date)
            || super::preparation::lot_identities(work.command())? != p.selection.lot_identity_utf16
            || allocations.len() != p.selection.allocations.len()
            || allocations
                .iter()
                .zip(&p.selection.allocations)
                .any(|(a, b)| {
                    a.lot_id != b.lot_id
                        || a.quantity_milli != b.quantity_milli
                        || a.balance_version != b.balance_version
                })
        {
            return Err(changed().into());
        }
        let facts = PartsDraftFacts {
            actor: FinanceActor {
                user_id: source.actor_user_id().clone(),
                holding_id: source.facts().holding_id.clone(),
                company_id: source.facts().company_id.clone(),
                assignment_id: source.actor_assignment_id().clone(),
            },
            owner_key: work.command().key().clone(),
            kind: PartsOwnerKind::FieldService,
            source_id: source.facts().id.clone(),
            source_version: work.version_after(),
            branch_id: source.facts().branch_id.clone(),
            department_id: source.facts().department_id.clone(),
            quantity_milli: p.valuation.quantity_milli,
            value_minor: p.valuation.value_minor,
            consumption_at: i.created_at,
            reason: reason.clone(),
        };
        Ok(Self {
            current,
            source,
            work,
            inventory,
            reservation,
            purchases,
            facts,
        })
    }
    fn expected(&self, e: &PartsDraftExpected) -> Result<(), Error> {
        let p = self.inventory.plan();
        let i = self.inventory.identity();
        if e.consumption_id().as_str() != self.inventory.consumption_id()
            || e.journal_id() != i.journal_id.as_str()
            || e.currency() != p.config.currency
            || e.period_id() != p.period_id.as_str()
            || e.value_minor() != p.valuation.value_minor
            || e.consumption_created_at() != i.created_at.get()
            || i.created_at.get() < self.work.application_at().get()
        {
            return Err(changed().into());
        }
        Ok(())
    }
}
impl PgPartsDraftOwnerSource for PreparedPartsSource<'_, '_> {
    fn facts(&self) -> &PartsDraftFacts {
        &self.facts
    }
    fn finance_source(&self) -> &CurrentConsumptionFinance {
        self.inventory.finance_source()
    }
    async fn require_before(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.work
            .intent()
            .require_current_source(c, self.source, now)
            .await?;
        self.purchases.require_current(c, self.current, now).await?;
        self.inventory
            .require_current_sources(
                c,
                &mut FieldServiceFinanceFacts {
                    holding: Some(self.facts.actor.holding_id.clone()),
                },
                self.reservation,
            )
            .await
    }
    async fn require_apply(
        &self,
        _: &mut PgConnection,
        _: &PartsDraftExpected,
        _: TimestampMillis,
    ) -> Result<(), Error> {
        Err(changed().into())
    }
    async fn require_after(
        &self,
        _: &mut PgConnection,
        _: &PartsDraftExpected,
        _: TimestampMillis,
    ) -> Result<(), Error> {
        Err(changed().into())
    }
}
pub(super) struct AppliedPartsSource<'p, 'a> {
    before: &'p PreparedPartsSource<'p, 'a>,
    applied: &'p RetainedFieldServiceReservationConsumption,
    version: &'p CurrentFieldServiceOrderVersion,
    order: OnceLock<RetainedFieldServiceOrderParts>,
}
impl<'p, 'a> AppliedPartsSource<'p, 'a> {
    pub(super) async fn new(
        c: &mut PgConnection,
        before: &'p PreparedPartsSource<'p, 'a>,
        applied: &'p RetainedFieldServiceReservationConsumption,
        version: &'p CurrentFieldServiceOrderVersion,
        now: TimestampMillis,
    ) -> Result<Self, Error> {
        before
            .work
            .intent()
            .require_version_proof(c, before.source, version, now)
            .await?;
        before
            .inventory
            .require_intermediate(
                c,
                &mut FieldServiceFinanceFacts {
                    holding: Some(before.facts.actor.holding_id.clone()),
                },
                before.reservation,
                applied,
            )
            .await?;
        Ok(Self {
            before,
            applied,
            version,
            order: OnceLock::new(),
        })
    }
    pub(super) async fn retain_order(
        &self,
        c: &mut PgConnection,
        order: RetainedFieldServiceOrderParts,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        order
            .require_plan(c, self.before.source, self.before.work, now)
            .await?;
        self.order.set(order).map_err(|_| changed())?;
        Ok(())
    }
    pub(super) async fn require_order(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.order
            .get()
            .ok_or_else(changed)?
            .require_plan(c, self.before.source, self.before.work, now)
            .await
    }
    pub(super) fn into_order(self) -> Result<RetainedFieldServiceOrderParts, Error> {
        Ok(self.order.into_inner().ok_or_else(changed)?)
    }
}
impl PgPartsDraftOwnerSource for AppliedPartsSource<'_, '_> {
    fn facts(&self) -> &PartsDraftFacts {
        &self.before.facts
    }
    fn finance_source(&self) -> &CurrentConsumptionFinance {
        self.before.inventory.finance_source()
    }
    async fn require_before(&self, _: &mut PgConnection, _: TimestampMillis) -> Result<(), Error> {
        Err(changed().into())
    }
    async fn require_apply(
        &self,
        c: &mut PgConnection,
        e: &PartsDraftExpected,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.before.expected(e)?;
        self.before
            .work
            .intent()
            .require_version_proof(c, self.before.source, self.version, now)
            .await?;
        self.before
            .purchases
            .require_current(c, self.before.current, now)
            .await?;
        self.before
            .inventory
            .require_intermediate(
                c,
                &mut FieldServiceFinanceFacts {
                    holding: Some(self.before.facts.actor.holding_id.clone()),
                },
                self.before.reservation,
                self.applied,
            )
            .await
    }
    async fn require_after(
        &self,
        c: &mut PgConnection,
        e: &PartsDraftExpected,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.before.expected(e)?;
        self.require_order(c, now).await?;
        self.before
            .inventory
            .require_inventory_after(c, self.before.reservation, self.applied)
            .await?;
        self.before
            .purchases
            .require_current(c, self.before.current, now)
            .await
    }
}
