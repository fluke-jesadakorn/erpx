//! Original order version and reservation transition, not Inventory/Finance or
//! receipt effects. All carriers are owned, source-bound and non-deserializable.
use super::source::*;
use crate::application::parts::{FieldServicePartsCommand, FieldServicePartsInput};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, TimestampMillis, UserId};
use sqlx::PgConnection;

pub struct FieldServiceOrderIntent {
    pub(super) command: FieldServicePartsCommand,
    pub(super) unit: Unit,
    pub(super) before_order: String,
    pub(super) identity: String,
    pub(super) actor: UserId,
    pub(super) assignment: String,
    pub(super) session: String,
    pub(super) application_at: TimestampMillis,
    pub(super) version_after: i64,
    pub(super) expected_order: String,
}
impl<R: PgFieldServiceOrderReferences> CurrentFieldServiceOrder<'_, R> {
    pub async fn retain_parts_intent(
        &self,
        c: &mut PgConnection,
        command: &FieldServicePartsCommand,
        application_at: TimestampMillis,
        guard_now: TimestampMillis,
    ) -> Result<FieldServiceOrderIntent, Error> {
        self.require_action(c, command, guard_now).await?;
        if application_at.get() < self.base.observed_at || application_at.get() > guard_now.get() {
            return Err(changed());
        }
        let version_after = command
            .input()
            .expected_version()
            .checked_add(1)
            .filter(|n| (2..=9_007_199_254_740_992).contains(n))
            .ok_or_else(invalid)?;
        let expected_order=sqlx::query_scalar("SELECT encode(sha256(convert_to((to_jsonb(o)||jsonb_build_object('version',$3::bigint,'updated_at',$4::bigint))::text,'UTF8')),'hex') FROM field_service_orders o WHERE id=$1 AND company_id=$2")
   .bind(self.facts().id.as_str()).bind(self.facts().company_id.as_str()).bind(version_after).bind(application_at.get()).fetch_one(c).await?;
        Ok(FieldServiceOrderIntent {
            command: command.clone(),
            unit: self.base.unit.clone(),
            before_order: self.base.image.order.clone(),
            identity: self.base.image.identity.clone(),
            actor: self.base.current.user_id().clone(),
            assignment: self
                .base
                .current
                .selection()
                .assignment()
                .id
                .as_str()
                .into(),
            session: self.base.current.session_id().as_str().into(),
            application_at,
            version_after,
            expected_order,
        })
    }
}
/// Actual late version assertion evidence. No Clone or public constructor;
/// producer effects cannot accept a copied successful boolean in its place.
pub struct CurrentFieldServiceOrderVersion {
    unit: Unit,
    key: CommandKey,
    order: crate::application::http_parts::FieldServicePhysicalOrderId,
    before: String,
    input: String,
}
impl FieldServiceOrderIntent {
    pub fn command(&self) -> &FieldServicePartsCommand {
        &self.command
    }
    pub fn application_at(&self) -> TimestampMillis {
        self.application_at
    }
    pub fn version_after(&self) -> i64 {
        self.version_after
    }
    pub(super) async fn require_source<R: PgFieldServiceOrderReferences>(
        &self,
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || source.base.unit != self.unit
            || self.command.order_id() != &source.facts().id
            || source.base.image.order != self.before_order
            || source.base.image.identity != self.identity
            || source.actor_user_id() != &self.actor
            || source.actor_assignment_id().as_str() != self.assignment
            || source.base.current.session_id().as_str() != self.session
        {
            return Err(changed());
        }
        source.require_action(c, &self.command, now).await
    }
    /// Revalidate original owner/actor/request facts without moving late CAS.
    pub async fn require_current_source<R: PgFieldServiceOrderReferences>(
        &self,
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.require_source(c, source, now).await
    }
    pub async fn require_version<R: PgFieldServiceOrderReferences>(
        &self,
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        now: TimestampMillis,
    ) -> Result<CurrentFieldServiceOrderVersion, Error> {
        self.require_source(c, source, now).await?;
        if source.facts().version != self.command.input().expected_version() {
            return Err(conflict());
        }
        Ok(CurrentFieldServiceOrderVersion {
            unit: self.unit.clone(),
            key: self.command.key().clone(),
            order: source.facts().id.clone(),
            before: self.before_order.clone(),
            input: self.command.receipt_input().as_str().into(),
        })
    }
    /// Reborrow the original sealed late-CAS proof; no new source is minted.
    pub async fn require_version_proof<R: PgFieldServiceOrderReferences>(
        &self,
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        proof: &CurrentFieldServiceOrderVersion,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.require_source(c, source, now).await?;
        self.require_version_binding(proof)?;
        if source.facts().version != self.command.input().expected_version() {
            return Err(conflict());
        }
        Ok(())
    }
    pub(super) fn require_version_binding(
        &self,
        version: &CurrentFieldServiceOrderVersion,
    ) -> Result<(), Error> {
        if version.unit != self.unit
            || &version.key != self.command.key()
            || &version.order != self.command.order_id()
            || version.before != self.before_order
            || version.input != self.command.receipt_input().as_str()
        {
            return Err(changed());
        }
        Ok(())
    }
    pub async fn retain_reservation_plan<R: PgFieldServiceOrderReferences>(
        &self,
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        reservation: &CurrentFieldServiceReservation,
        now: TimestampMillis,
    ) -> Result<PreparedFieldServiceReservationConsumption, Error> {
        self.require_source(c, source, now).await?;
        reservation.require_current(c).await?;
        let FieldServicePartsInput::Issue {
            reservation_id,
            quantity_milli,
            ..
        } = self.command.input()
        else {
            return Err(changed());
        };
        let r = reservation.facts();
        if reservation.unit != self.unit
            || r.order_id != *self.command.order_id()
            || r.id.as_str() != reservation_id
            || r.holding_id != source.facts().holding_id
            || r.company_id != source.facts().company_id
        {
            return Err(changed());
        }
        // Intended data only. Expiry/remaining failure is deliberately at apply,
        // after the original Finance/Money and actual late stock/mapping guards.
        let after=sqlx::query_scalar("SELECT encode(sha256(convert_to((to_jsonb(r)||jsonb_build_object('consumed_milli',r.consumed_milli+$3::bigint))::text,'UTF8')),'hex') FROM field_service_part_reservations r WHERE id=$1 AND order_id=$2")
   .bind(r.id.as_str()).bind(r.order_id.as_str()).bind(*quantity_milli).fetch_one(c).await?;
        Ok(PreparedFieldServiceReservationConsumption {
            facts: r.clone(),
            unit: self.unit.clone(),
            key: self.command.key().clone(),
            before: reservation.before.clone(),
            after,
            quantity_milli: *quantity_milli,
            application_at: self.application_at,
        })
    }
}
pub struct PreparedFieldServiceReservationConsumption {
    facts: FieldServiceReservationFacts,
    unit: Unit,
    key: CommandKey,
    before: String,
    after: String,
    quantity_milli: i64,
    application_at: TimestampMillis,
}
impl PreparedFieldServiceReservationConsumption {
    pub fn facts(&self) -> &FieldServiceReservationFacts {
        &self.facts
    }
    pub fn quantity_milli(&self) -> i64 {
        self.quantity_milli
    }
    pub fn remaining_before(&self) -> i64 {
        self.facts.quantity_milli - self.facts.consumed_milli - self.facts.released_milli
    }
    pub async fn require_original(
        &self,
        c: &mut PgConnection,
        reservation: &CurrentFieldServiceReservation,
    ) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || reservation.unit != self.unit
            || reservation.facts() != &self.facts
            || reservation.before != self.before
        {
            return Err(changed());
        }
        reservation.require_current(c).await
    }
    pub async fn require_applied(
        &self,
        c: &mut PgConnection,
        applied: &RetainedFieldServiceReservationConsumption,
    ) -> Result<(), Error> {
        if self.unit != applied.unit
            || self.key != applied.key
            || self.facts != applied.facts
            || self.after != applied.after
            || self.quantity_milli != applied.quantity_milli
        {
            return Err(changed());
        }
        applied.require_current(c).await
    }
    pub async fn apply<R: PgFieldServiceOrderReferences>(
        &self,
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        intent: &FieldServiceOrderIntent,
        version: &CurrentFieldServiceOrderVersion,
        now: TimestampMillis,
    ) -> Result<RetainedFieldServiceReservationConsumption, Error> {
        intent.require_source(c, source, now).await?;
        intent.require_version_binding(version)?;
        if intent.unit != self.unit
            || intent.command.key() != &self.key
            || intent.application_at != self.application_at
            || source.facts().id != self.facts.order_id
        {
            return Err(changed());
        }
        let before:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex') FROM field_service_part_reservations r WHERE id=$1 AND order_id=$2 FOR UPDATE")
   .bind(self.facts.id.as_str()).bind(self.facts.order_id.as_str()).fetch_optional(&mut*c).await?;
        if before.as_ref() != Some(&self.before) {
            return Err(changed());
        }
        if self.remaining_before() < self.quantity_milli
            || self.facts.expires_at.get() <= self.application_at.get()
        {
            return Err(conflict());
        }
        let n=sqlx::query("UPDATE field_service_part_reservations SET consumed_milli=consumed_milli+$3 WHERE id=$1 AND order_id=$2 AND quantity_milli-consumed_milli-released_milli>=$3 AND expires_at>$4")
   .bind(self.facts.id.as_str()).bind(self.facts.order_id.as_str()).bind(self.quantity_milli).bind(self.application_at.get()).execute(&mut*c).await?.rows_affected();
        if n != 1 {
            return Err(conflict());
        }
        let receipt = RetainedFieldServiceReservationConsumption {
            facts: self.facts.clone(),
            unit: self.unit.clone(),
            key: self.key.clone(),
            after: self.after.clone(),
            quantity_milli: self.quantity_milli,
        };
        receipt.require_current(c).await?;
        Ok(receipt)
    }
}
/// Sealed actual reservation after-image. Inventory separately proves the exact
/// trigger-caused intermediate stock version; this owner never queries stock.
pub struct RetainedFieldServiceReservationConsumption {
    facts: FieldServiceReservationFacts,
    unit: Unit,
    key: CommandKey,
    after: String,
    quantity_milli: i64,
}
impl RetainedFieldServiceReservationConsumption {
    pub fn facts(&self) -> &FieldServiceReservationFacts {
        &self.facts
    }
    pub fn owner_key(&self) -> &CommandKey {
        &self.key
    }
    pub fn quantity_milli(&self) -> i64 {
        self.quantity_milli
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let actual:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex') FROM field_service_part_reservations r WHERE id=$1 AND order_id=$2 FOR SHARE")
   .bind(self.facts.id.as_str()).bind(self.facts.order_id.as_str()).fetch_optional(c).await?;
        if actual.as_ref() != Some(&self.after) {
            return Err(changed());
        }
        Ok(())
    }
}
fn conflict() -> Error {
    BusinessError::new(FailureKind::Conflict,"CONFLICT","The record changed, a reference is unavailable, or this value already exists. Refresh and review again.").into()
}
