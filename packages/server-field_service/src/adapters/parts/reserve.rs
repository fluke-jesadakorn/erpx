//! Field Service owns reservation insertion and its intended row. Stock scope,
//! availability/Quality and both trigger version changes belong to Inventory.
use super::{intent::*, source::*};
use crate::application::parts::FieldServicePartsInput;
use erpx_server_shared::public::{CommandKey, TimestampMillis, UserId};
use sqlx::PgConnection;

pub struct PreparedFieldServiceReservation {
    unit: Unit,
    key: CommandKey,
    order: erpx_server_shared::public::RecordId,
    before_order: String,
    reservation_id: String,
    warehouse: String,
    product: String,
    quantity: i64,
    expires: i64,
    actor: UserId,
    reason: String,
    application_at: TimestampMillis,
}
impl FieldServiceOrderIntent {
    pub async fn prepare_new_reservation<R: PgFieldServiceOrderReferences>(
        &self,
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        now: TimestampMillis,
    ) -> Result<PreparedFieldServiceReservation, Error> {
        self.require_source(c, source, now).await?;
        let FieldServicePartsInput::Reserve {
            warehouse_id,
            product_id,
            quantity_milli,
            expires_at,
            reason,
            ..
        } = self.command.input()
        else {
            return Err(changed());
        };
        let reservation_id = format!("field-part-{}", self.command.key().as_str());
        Ok(PreparedFieldServiceReservation {
            unit: self.unit.clone(),
            key: self.command.key().clone(),
            order: self.command.order_id().clone(),
            before_order: self.before_order.clone(),
            reservation_id,
            warehouse: warehouse_id.clone(),
            product: product_id.clone(),
            quantity: *quantity_milli,
            expires: *expires_at,
            actor: self.actor.clone(),
            reason: reason.clone(),
            application_at: self.application_at,
        })
    }
}
impl PreparedFieldServiceReservation {
    pub fn owner_key(&self) -> &CommandKey {
        &self.key
    }
    pub fn reservation_id(&self) -> &str {
        &self.reservation_id
    }
    pub fn warehouse_id(&self) -> &str {
        &self.warehouse
    }
    pub fn product_id(&self) -> &str {
        &self.product
    }
    pub fn quantity_milli(&self) -> i64 {
        self.quantity
    }
    pub async fn require_before<R: PgFieldServiceOrderReferences>(
        &self,
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        intent: &FieldServiceOrderIntent,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        intent.require_source(c, source, now).await?;
        if self.unit != unit(c).await?
            || intent.unit != self.unit
            || intent.command.key() != &self.key
            || intent.command.order_id() != &self.order
            || intent.before_order != self.before_order
            || intent.actor != self.actor
            || intent.application_at != self.application_at
        {
            return Err(changed());
        }
        Ok(())
    }
    async fn expected_digest(&self, c: &mut PgConnection) -> Result<String, Error> {
        Ok(sqlx::query_scalar("SELECT encode(sha256(convert_to(jsonb_build_object('id',$1::text,'order_id',$2::text,'warehouse_id',$3::text,'product_id',$4::text,'quantity_milli',$5::bigint,'consumed_milli',0,'released_milli',0,'expires_at',$6::bigint,'created_by',$7::text,'reason',$8::text,'release_reason',NULL,'created_at',$9::bigint)::text,'UTF8')),'hex')")
            .bind(&self.reservation_id).bind(self.order.as_str()).bind(&self.warehouse).bind(&self.product).bind(self.quantity).bind(self.expires).bind(self.actor.as_str()).bind(&self.reason).bind(self.application_at.get()).fetch_one(c).await?)
    }
    pub async fn require_inserted(
        &self,
        c: &mut PgConnection,
        inserted: &RetainedNewFieldServiceReservation,
    ) -> Result<(), Error> {
        if self.unit != inserted.unit
            || self.key != inserted.key
            || self.reservation_id != inserted.reservation_id
            || self.expected_digest(c).await? != inserted.expected
        {
            return Err(changed());
        }
        inserted.require_current(c).await
    }
    /// Call after the real late order CAS and Inventory availability assertion.
    /// This UPDATE's stock trigger must then match Inventory's intended V+2.
    pub async fn insert<R: PgFieldServiceOrderReferences>(
        &self,
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        intent: &FieldServiceOrderIntent,
        version: &CurrentFieldServiceOrderVersion,
        now: TimestampMillis,
    ) -> Result<RetainedNewFieldServiceReservation, Error> {
        self.require_before(c, source, intent, now).await?;
        intent.require_version_binding(version)?;
        let expected = self.expected_digest(c).await?;
        sqlx::query("INSERT INTO field_service_part_reservations(id,order_id,warehouse_id,product_id,quantity_milli,expires_at,created_by,reason,created_at)VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(&self.reservation_id).bind(self.order.as_str()).bind(&self.warehouse).bind(&self.product).bind(self.quantity).bind(self.expires).bind(self.actor.as_str()).bind(&self.reason).bind(self.application_at.get()).execute(&mut*c).await?;
        let result = RetainedNewFieldServiceReservation {
            unit: self.unit.clone(),
            key: self.key.clone(),
            reservation_id: self.reservation_id.clone(),
            expected,
        };
        result.require_current(c).await?;
        Ok(result)
    }
}
/// Actual same-unit intended reservation image, not a stock mutation grant.
pub struct RetainedNewFieldServiceReservation {
    unit: Unit,
    key: CommandKey,
    reservation_id: String,
    expected: String,
}
impl RetainedNewFieldServiceReservation {
    pub fn owner_key(&self) -> &CommandKey {
        &self.key
    }
    pub fn reservation_id(&self) -> &str {
        &self.reservation_id
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let actual:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex')FROM field_service_part_reservations r WHERE id=$1 FOR SHARE")
            .bind(&self.reservation_id).fetch_optional(c).await?;
        if actual.as_ref() != Some(&self.expected) {
            return Err(changed());
        }
        Ok(())
    }
}
