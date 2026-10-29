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
pub(super) struct PendingIssueQuote {
    pub(super) original: Arc<CurrentFieldServiceReservation>,
    pub(super) inventory: PgFieldServiceConsumptionPreparation<CurrentConsumptionFinance>,
    pub(super) application_at: TimestampMillis,
    pub(super) observed_at: TimestampMillis,
    pub(super) receipt_input: String,
    pub(super) order_id: String,
}

#[cfg(feature = "field-service-http")]
pub(super) async fn quote_pending_issue<W: Clock + Sync, G: Clock + Sync>(
    c: &mut PgConnection,
    source: &CurrentFieldServiceOrder<'_, FieldServiceReferences>,
    pending: &PendingFieldServiceParts,
    write: &W,
    guard: &G,
) -> Result<(TimestampMillis, Option<PendingIssueQuote>), Error> {
    source
        .require_pending_action(c, pending, guard.now())
        .await?;
    let application_at = write.now();
    let FieldServicePartsInput::Issue {
        reservation_id,
        quantity_milli,
        entry_date,
        allocations,
        ..
    } = pending.normalized.input()
    else {
        // Reserve's future-time validation belongs before late admission, but stock preparation does not.
        if let FieldServicePartsInput::Reserve { expires_at, .. } = pending.normalized.input() {
            require_future_reservation(*expires_at, application_at.get())?;
        }
        return Ok((application_at, None));
    };
    let original = Arc::new(
        source
            .reservation(c, reservation_id, guard.now())
            .await?
            .ok_or_else(|| {
                erpx_server_shared::public::BusinessError::new(
                    erpx_server_shared::public::FailureKind::NotFound,
                    "FIELD_SERVICE_PART_NOT_FOUND",
                    "Select a part reserved for this service order.",
                )
            })?,
    );
    let date = match entry_date { Some(date) => date.clone(), None => sqlx::query_scalar::<_,String>("SELECT to_char(to_timestamp($1::double precision/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD')").bind(application_at.get()).fetch_one(&mut *c).await? };
    let mut finance = FieldServiceFinanceFacts {
        holding: Some(source.facts().holding_id.clone()),
    };
    let (inventory, observed_at) = prepare_field_service_inventory_consumption_at(
        c,
        &mut finance,
        FieldServiceConsumptionSelection {
            company_id: source.facts().company_id.clone(),
            branch_id: source.facts().branch_id.clone(),
            warehouse_id: original.facts().warehouse_id.clone(),
            product_id: original.facts().product_id.clone(),
            entry_date: date,
            quantity_milli: *quantity_milli,
            lot_identity_utf16: lot_identities_from_input(pending.normalized.receipt_input())?,
            allocations: allocations
                .iter()
                .map(|a| LotAllocation {
                    lot_id: a.lot_id.clone(),
                    quantity_milli: a.quantity_milli,
                    balance_version: a.balance_version,
                })
                .collect(),
        },
        write,
    )
    .await?;
    Ok((
        application_at,
        Some(PendingIssueQuote {
            original,
            inventory,
            application_at,
            observed_at,
            receipt_input: pending.normalized.receipt_input().as_str().to_owned(),
            order_id: pending.target.storage_text(),
        }),
    ))
}

pub(super) struct PreparationPorts<'c, 'a, W, G> {
    pub(super) c: &'c mut PgConnection,
    pub(super) write: &'c W,
    pub(super) guard: &'c G,
    pub(super) scope: std::marker::PhantomData<&'a ()>,
    pub(super) quote: Option<PendingIssueQuote>,
    pub(super) pending_at: Option<TimestampMillis>,
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
        self.pending_at.unwrap_or_else(|| self.write.now())
    }
    async fn reservation(
        &mut self,
        s: &Self::Source,
        id: &str,
    ) -> Result<Option<Self::Reservation>, Error> {
        if let Some(q) = &self.quote {
            if q.original.facts().id.as_str() != id {
                return Err(changed().into());
            }
            return Ok(Some(Arc::clone(&q.original)));
        }
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
        let quoted = self.quote.take();
        if let Some(q) = &quoted
            && (q.order_id != command.order_id().as_str()
                || q.receipt_input != command.receipt_input().as_str()
                || !Arc::ptr_eq(&q.original, r)
                || q.application_at != application_at)
        {
            return Err(changed().into());
        }
        let (prepared, at) = if let Some(q) = quoted {
            q.inventory
                .require_current(
                    self.c,
                    &mut FieldServiceFinanceFacts {
                        holding: Some(s.facts().holding_id.clone()),
                    },
                )
                .await?;
            (q.inventory, q.observed_at)
        } else {
            let date=match entry_date{Some(date)=>date.clone(),None=>sqlx::query_scalar::<_,String>("SELECT to_char(to_timestamp($1::double precision/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD')").bind(application_at.get()).fetch_one(&mut*self.c).await?};
            let mut f = FieldServiceFinanceFacts {
                holding: Some(s.facts().holding_id.clone()),
            };
            prepare_field_service_inventory_consumption_at(
                self.c,
                &mut f,
                FieldServiceConsumptionSelection {
                    company_id: s.facts().company_id.clone(),
                    branch_id: s.facts().branch_id.clone(),
                    warehouse_id: r.facts().warehouse_id.clone(),
                    product_id: r.facts().product_id.clone(),
                    entry_date: date,
                    quantity_milli: *quantity_milli,
                    lot_identity_utf16: lot_identities(command)?,
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
            .await?
        };
        let ids = parts_draft_identity(command.key())?;
        let inventory = prepared
            .retain_reserved_effects(
                self.c,
                FieldServiceConsumptionIdentity {
                    owner_key: command.key().clone(),
                    order_id: s.facts().id.as_str().to_owned(),
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

// Extract original JavaScript string units from the normalized receipt input.
pub(super) fn lot_identities(command: &FieldServicePartsCommand) -> Result<Vec<Vec<u16>>, Error> {
    lot_identities_from_input(command.receipt_input())
}
fn lot_identities_from_input(input: &EncodedFieldServiceInput) -> Result<Vec<Vec<u16>>, Error> {
    let array = field_service_parts_receipt_json(input)?
        .property("allocations")?
        .ok_or_else(changed)?;
    array
        .array_elements()?
        .ok_or_else(changed)?
        .into_iter()
        .map(|entry| {
            Ok(entry
                .property("lotId")?
                .ok_or_else(changed)?
                .string_utf16_units()?
                .ok_or_else(changed)?)
        })
        .collect()
}
