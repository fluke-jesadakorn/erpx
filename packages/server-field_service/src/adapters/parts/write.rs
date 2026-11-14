//! Intended order/event data retained before effects. JSONB encoding uses only
//! these immutable parameters at the original event INSERT position; returned
//! or post-trigger rows never become the expected baseline.
use super::{intent::*, source::*};
use crate::adapters::parts_wire::field_service_parts_receipt_json;
use crate::application::parts::{FieldServicePartsCommand, FieldServicePartsInput};
use erpx_server_shared::public::{BusinessError, FailureKind, TimestampMillis, UserId};
use erpx_server_workflow::public::commands::CommandJson;
use sqlx::PgConnection;

pub struct PreparedFieldServiceOrderWrite {
    intent: FieldServiceOrderIntent,
    event_id: String,
    actor: UserId,
    technician: String,
    state: String,
    scheduled: Option<TimestampMillis>,
    details: CommandJson,
}
impl FieldServiceOrderIntent {
    pub async fn prepare_order_write<R: PgFieldServiceOrderReferences>(
        self,
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        details: CommandJson,
        now: TimestampMillis,
    ) -> Result<PreparedFieldServiceOrderWrite, Error> {
        self.require_source(c, source, now).await?;
        let names = match self.command.input() {
            FieldServicePartsInput::Reserve { .. } => {
                &["warehouseId", "productId", "quantityMilli"][..]
            }
            FieldServicePartsInput::Issue { .. } => &["reservationId", "quantityMilli"][..],
        };
        let original = field_service_parts_receipt_json(self.command.receipt_input())?;
        for name in names {
            if details.property(name)? != original.property(name)? {
                return Err(changed());
            }
        }
        Ok(PreparedFieldServiceOrderWrite {
            event_id: format!("service-order-event-{}", self.command.key().as_str()),
            actor: self.actor.clone(),
            technician: source.facts().technician_id.clone(),
            state: source.facts().state.clone(),
            scheduled: source.facts().scheduled_at,
            intent: self,
            details,
        })
    }
}
impl PreparedFieldServiceOrderWrite {
    pub fn command(&self) -> &FieldServicePartsCommand {
        &self.intent.command
    }
    pub fn application_at(&self) -> TimestampMillis {
        self.intent.application_at
    }
    pub fn version_after(&self) -> i64 {
        self.intent.version_after
    }
    pub fn event_id(&self) -> &str {
        &self.event_id
    }
    pub fn technician_id(&self) -> &str {
        &self.technician
    }
    pub fn scheduled_at(&self) -> Option<TimestampMillis> {
        self.scheduled
    }
    pub fn details(&self) -> &CommandJson {
        &self.details
    }
    pub async fn require_version<R: PgFieldServiceOrderReferences>(
        &self,
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        now: TimestampMillis,
    ) -> Result<CurrentFieldServiceOrderVersion, Error> {
        self.intent.require_version(c, source, now).await
    }
    pub fn intent(&self) -> &FieldServiceOrderIntent {
        &self.intent
    }
    pub async fn write_order_and_event<R: PgFieldServiceOrderReferences>(
        &self,
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        version: &CurrentFieldServiceOrderVersion,
        now: TimestampMillis,
    ) -> Result<RetainedFieldServiceOrderParts, Error> {
        self.intent.require_source(c, source, now).await?;
        self.intent.require_version_binding(version)?;
        let command = &self.intent.command;
        let at = self.intent.application_at.get();
        let n=sqlx::query("UPDATE field_service_orders SET version=version+1,updated_at=$3 WHERE id=$1 AND company_id=$2 AND version=$4")
   .bind(command.order_id().as_str()).bind(source.facts().company_id.as_str()).bind(at).bind(command.input().expected_version()).execute(&mut*c).await?.rows_affected();
        if n != 1 {
            return Err(conflict());
        }
        // No owner table is read here. All expected event data is the original
        // pre-effect payload. Its JSONB cast remains at the original INSERT position,
        // after Finance/stock/receipt failures, including invalid surrogate escapes.
        let expected_event=sqlx::query_scalar("SELECT encode(sha256(convert_to(jsonb_build_object('id',$1::text,'order_id',$2::text,'version',$3::bigint,'action',$4::text,'actor_id',$5::text,'reason',$6::text,'status',$7::text,'technician_id',$8::text,'scheduled_at',$9::bigint,'details_json',$10::jsonb,'created_at',$11::bigint)::text,'UTF8')),'hex')")
   .bind(&self.event_id).bind(command.order_id().as_str()).bind(self.intent.version_after).bind(command.input().action().operation().strip_prefix("field.order.").ok_or_else(invalid)?).bind(self.actor.as_str()).bind(command.input().reason()).bind(&self.state).bind(self.technician.as_str()).bind(self.scheduled.map(TimestampMillis::get)).bind(self.details.as_str()).bind(at).fetch_one(&mut*c).await?;
        sqlx::query("INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at)VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10::jsonb,$11)")
   .bind(&self.event_id).bind(command.order_id().as_str()).bind(self.intent.version_after).bind(command.input().action().operation().strip_prefix("field.order.").ok_or_else(invalid)?).bind(self.actor.as_str()).bind(command.input().reason()).bind(&self.state).bind(self.technician.as_str()).bind(self.scheduled.map(TimestampMillis::get)).bind(self.details.as_str()).bind(at).execute(&mut*c).await?;
        let receipt = RetainedFieldServiceOrderParts {
            unit: self.intent.unit.clone(),
            before_order: self.intent.before_order.clone(),
            identity: self.intent.identity.clone(),
            expected_order: self.intent.expected_order.clone(),
            event_id: self.event_id.clone(),
            expected_event,
            command_input: command.receipt_input().as_str().into(),
            details: self.details.as_str().into(),
            actor: self.actor.clone(),
            assignment: self.intent.assignment.clone(),
            session: self.intent.session.clone(),
        };
        receipt.require_current(c, source, now).await?;
        Ok(receipt)
    }
}
pub struct RetainedFieldServiceOrderParts {
    unit: Unit,
    before_order: String,
    identity: String,
    expected_order: String,
    event_id: String,
    expected_event: String,
    command_input: String,
    details: String,
    actor: UserId,
    assignment: String,
    session: String,
}
impl RetainedFieldServiceOrderParts {
    pub async fn require_plan<R: PgFieldServiceOrderReferences>(
        &self,
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        plan: &PreparedFieldServiceOrderWrite,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        if self.unit != plan.intent.unit
            || self.before_order != plan.intent.before_order
            || self.identity != plan.intent.identity
            || self.expected_order != plan.intent.expected_order
            || self.event_id != plan.event_id
            || self.command_input != plan.command().receipt_input().as_str()
            || self.details != plan.details.as_str()
            || self.actor != plan.actor
            || self.assignment != plan.intent.assignment
            || self.session != plan.intent.session
        {
            return Err(changed());
        }
        self.require_current(c, source, now).await
    }

    pub async fn require_current<R: PgFieldServiceOrderReferences>(
        &self,
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || source.base.unit != self.unit
            || source.base.image.order != self.before_order
            || source.base.image.identity != self.identity
            || source.actor_user_id() != &self.actor
            || source.actor_assignment_id().as_str() != self.assignment
            || source.base.current.session_id().as_str() != self.session
        {
            return Err(changed());
        }
        source
            .require_original_identity_and_references(c, now)
            .await?;
        let actual_order:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(o)::text,'UTF8')),'hex') FROM field_service_orders o WHERE id=$1 AND company_id=$2")
   .bind(source.facts().id.as_str()).bind(source.facts().company_id.as_str()).fetch_optional(&mut*c).await?;
        let actual_event:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(e)::text,'UTF8')),'hex') FROM field_service_order_events e WHERE id=$1 AND order_id=$2 FOR SHARE")
   .bind(&self.event_id).bind(source.facts().id.as_str()).fetch_optional(c).await?;
        if actual_order.as_ref() != Some(&self.expected_order)
            || actual_event.as_ref() != Some(&self.expected_event)
        {
            return Err(changed());
        }
        Ok(())
    }
}
fn conflict() -> Error {
    BusinessError::new(FailureKind::Conflict,"CONFLICT","The record changed, a reference is unavailable, or this value already exists. Refresh and review again.").into()
}
