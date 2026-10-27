//! Real ports for the owner's portable application. Every preparation remains
//! read-only; Workflow must insert its one known-result receipt before apply.
use super::{
    Error, changed,
    inventory::{FieldServiceFinanceFacts, ReservationSource},
    source::FieldServiceReferences,
};
use erpx_server_field_service::public::{parts::*, parts_source::*};
use erpx_server_finance::public::{
    inventory_consumption::CurrentConsumptionFinance, parts_draft::parts_draft_identity,
};
use erpx_server_inventory::public::{consumption::*, postgres_consumption::*};
use erpx_server_shared::public::{Clock, TimestampMillis};
use sqlx::PgConnection;
use std::sync::Arc;

pub(super) struct IssuePreparation {
    pub(super) original: Arc<CurrentFieldServiceReservation>,
    pub(super) reservation: PreparedFieldServiceReservationConsumption,
    pub(super) inventory: PgReservedConsumptionEffects<CurrentConsumptionFinance>,
    pub(super) intent: FieldServiceOrderIntent,
}
pub(super) struct PreparationPorts<'c, 'a, W, G> {
    pub(super) c: &'c mut PgConnection,
    pub(super) write: &'c W,
    pub(super) guard: &'c G,
    pub(super) scope: std::marker::PhantomData<&'a ()>,
}
impl<'a, W: Clock + Sync, G: Clock + Sync> FieldServicePartsPreparationPorts
    for PreparationPorts<'_, 'a, W, G>
{
    type Source = CurrentFieldServiceOrder<'a, FieldServiceReferences>;
    type Reservation = Arc<CurrentFieldServiceReservation>;
    type Consumption = IssuePreparation;
    type Error = Error;
    async fn require_action(
        &mut self,
        s: &Self::Source,
        command: &FieldServicePartsCommand,
    ) -> Result<(), Error> {
        s.require_action(self.c, command, self.guard.now()).await
    }
    fn application_now(&self) -> TimestampMillis {
        self.write.now()
    }
    async fn reservation(
        &mut self,
        s: &Self::Source,
        id: &str,
    ) -> Result<Option<Self::Reservation>, Error> {
        Ok(s.reservation(self.c, id, self.guard.now())
            .await?
            .map(Arc::new))
    }
    async fn prepare_consumption(
        &mut self,
        s: &Self::Source,
        r: &Self::Reservation,
        command: &FieldServicePartsCommand,
        application_at: TimestampMillis,
    ) -> Result<IssuePreparation, Error> {
        let FieldServicePartsInput::Issue {
            quantity_milli,
            entry_date,
            allocations,
            reason,
            ..
        } = command.input()
        else {
            return Err(changed().into());
        };
        let intent = s
            .retain_parts_intent(self.c, command, application_at, self.guard.now())
            .await?;
        let reservation = intent
            .retain_reservation_plan(self.c, s, r, self.guard.now())
            .await?;
        let owner = ReservationSource::new(&reservation, r, command.key(), s)?;
        let date=match entry_date{Some(date)=>date.clone(),None=>sqlx::query_scalar::<_,String>("SELECT to_char(to_timestamp($1::double precision/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD')").bind(application_at.get()).fetch_one(&mut*self.c).await?};
        let mut f = FieldServiceFinanceFacts {
            holding: Some(s.facts().holding_id.clone()),
        };
        let (prepared, at) = prepare_field_service_inventory_consumption_at(
            self.c,
            &mut f,
            FieldServiceConsumptionSelection {
                company_id: s.facts().company_id.clone(),
                branch_id: s.facts().branch_id.clone(),
                warehouse_id: r.facts().warehouse_id.clone(),
                product_id: r.facts().product_id.clone(),
                entry_date: date,
                quantity_milli: *quantity_milli,
                allocations: allocations
                    .iter()
                    .map(|a| LotAllocation {
                        lot_id: a.lot_id.clone(),
                        quantity_milli: a.quantity_milli,
                        balance_version: a.balance_version,
                    })
                    .collect(),
            },
            self.write,
        )
        .await?;
        let ids = parts_draft_identity(command.key())?;
        let inventory = prepared
            .retain_reserved_effects(
                self.c,
                FieldServiceConsumptionIdentity {
                    owner_key: command.key().clone(),
                    order_id: s.facts().id.clone(),
                    order_version_after: intent.version_after(),
                    department_id: s.facts().department_id.clone(),
                    actor_id: s.actor_user_id().clone(),
                    reason: reason.clone(),
                    created_at: at,
                    journal_id: ids.journal_id().clone(),
                },
                &owner,
            )
            .await?;
        Ok(IssuePreparation {
            original: Arc::clone(r),
            reservation,
            inventory,
            intent,
        })
    }
    fn consumption_facts(value: &IssuePreparation) -> FieldServicePreparedConsumptionFacts<'_> {
        FieldServicePreparedConsumptionFacts {
            consumption_id: value.inventory.consumption_id(),
            journal_id: value.inventory.identity().journal_id.as_str(),
            value_minor: value.inventory.plan().valuation.value_minor,
        }
    }
}
