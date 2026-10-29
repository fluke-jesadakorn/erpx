//! Pre-effect visit plan and immutable order/event after-image, bound to one unit.
use super::source::*;
use crate::adapters::parts_wire::NormalizedFieldServiceVisit;
use crate::domain::visit::VisitAction;
use erpx_server_shared::public::{CommandKey, TimestampMillis};
use sqlx::PgConnection;

pub trait PgVisitTechnician: Send + Sync {
    fn technician_id(&self) -> &str;
    fn require_current(
        &self,
        c: &mut PgConnection,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send;
}
pub struct PreparedFieldServiceVisit {
    unit: Unit,
    key: CommandKey,
    input: NormalizedFieldServiceVisit,
    order_id: String,
    company_id: String,
    actor: String,
    before: String,
    identity: String,
    expected_order: String,
    expected_event: String,
    technician: String,
    scheduled: Option<i64>,
    at: i64,
    version: i64,
    details: String,
}
impl PreparedFieldServiceVisit {
    pub async fn prepare<R: PgFieldServiceOrderReferences, T: PgVisitTechnician>(
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        key: CommandKey,
        technician_source: &T,
        input: NormalizedFieldServiceVisit,
        at: TimestampMillis,
        now: TimestampMillis,
    ) -> Result<Self, Error> {
        source.require_visit_action(c, input.action, now).await?;
        if at.get() < source.base.observed_at || at.get() > now.get() {
            return Err(changed());
        }
        let version = input
            .expected_version
            .checked_add(1)
            .filter(|n| (2..=9_007_199_254_740_992).contains(n))
            .ok_or_else(invalid)?;
        let technician = input
            .technician_id
            .clone()
            .unwrap_or_else(|| source.facts().technician_id.clone());
        if technician_source.technician_id() != technician {
            return Err(changed());
        }
        if input.action == VisitAction::Schedule {
            technician_source.require_current(c).await?;
        }
        let scheduled = input
            .scheduled_at
            .or(source.facts().scheduled_at.map(TimestampMillis::get));
        let action = input
            .action
            .operation()
            .strip_prefix("field.order.")
            .ok_or_else(invalid)?;
        let details = if input.action == VisitAction::Complete {
            serde_json::json!({"laborMinutes":input.labor_minutes.ok_or_else(invalid)?,"evidence":input.evidence.as_deref().ok_or_else(invalid)?,"customerConfirmation":input.customer_confirmation.as_deref().ok_or_else(invalid)?}).to_string()
        } else {
            "{}".to_owned()
        };
        let expected_order=sqlx::query_scalar("SELECT encode(sha256(convert_to((to_jsonb(o)||jsonb_build_object('status',$3::text,'technician_id',$4::text,'scheduled_at',$5::bigint,'version',$6::bigint,'updated_at',$7::bigint)||CASE WHEN $8::boolean THEN jsonb_build_object('visit_started_at',$7::bigint) ELSE '{}'::jsonb END||CASE WHEN $9::boolean THEN jsonb_build_object('visit_completed_at',$7::bigint,'labor_minutes',$10::bigint,'completion_evidence',$11::text,'customer_confirmation',$12::text,'exception_reason',NULL) ELSE '{}'::jsonb END||CASE WHEN $13::text='exception' THEN jsonb_build_object('exception_reason',$14::text) WHEN $13::text='resume' THEN jsonb_build_object('exception_reason',NULL) ELSE '{}'::jsonb END)::text,'UTF8')),'hex') FROM field_service_orders o WHERE id=$1 AND company_id=$2").bind(source.facts().id.as_str()).bind(source.facts().company_id.as_str()).bind(input.action.status()).bind(&technician).bind(scheduled).bind(version).bind(at.get()).bind(input.action==VisitAction::Start).bind(input.action==VisitAction::Complete).bind(input.labor_minutes).bind(input.evidence.as_deref()).bind(input.customer_confirmation.as_deref()).bind(action).bind(&input.reason).fetch_one(&mut *c).await?;
        let event_id = format!("service-order-event-{}", key.as_str());
        let expected_event=sqlx::query_scalar("SELECT encode(sha256(convert_to(jsonb_build_object('id',$1::text,'order_id',$2::text,'version',$3::bigint,'action',$4::text,'actor_id',$5::text,'reason',$6::text,'status',$7::text,'technician_id',$8::text,'scheduled_at',$9::bigint,'details_json',$10::jsonb,'created_at',$11::bigint)::text,'UTF8')),'hex')").bind(&event_id).bind(source.facts().id.as_str()).bind(version).bind(action).bind(source.actor_user_id().as_str()).bind(&input.reason).bind(input.action.status()).bind(&technician).bind(scheduled).bind(&details).bind(at.get()).fetch_one(c).await?;
        Ok(Self {
            unit: source.base.unit.clone(),
            key,
            input,
            order_id: source.facts().id.as_str().into(),
            company_id: source.facts().company_id.as_str().into(),
            actor: source.actor_user_id().as_str().into(),
            before: source.base.image.order.clone(),
            identity: source.base.image.identity.clone(),
            expected_order,
            expected_event,
            technician,
            scheduled,
            at: at.get(),
            version,
            details,
        })
    }
    pub fn technician(&self) -> &str {
        &self.technician
    }
    pub fn scheduled_at(&self) -> Option<i64> {
        self.scheduled
    }
    pub fn version(&self) -> i64 {
        self.version
    }
    pub fn status(&self) -> &str {
        self.input.action.status()
    }
    pub fn at(&self) -> i64 {
        self.at
    }
    pub fn key(&self) -> &CommandKey {
        &self.key
    }
    pub async fn require_before<R: PgFieldServiceOrderReferences, T: PgVisitTechnician>(
        &self,
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        technician_source: &T,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || source.base.unit != self.unit
            || source.base.image.order != self.before
            || source.base.image.identity != self.identity
            || source.actor_user_id().as_str() != self.actor
        {
            return Err(changed());
        }
        if technician_source.technician_id() != self.technician {
            return Err(changed());
        }
        if self.input.action == VisitAction::Schedule {
            technician_source.require_current(c).await?;
        }
        source.require_visit_action(c, self.input.action, now).await
    }
    pub async fn require_version<R: PgFieldServiceOrderReferences, T: PgVisitTechnician>(
        &self,
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        tech: &T,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.require_before(c, source, tech, now).await?;
        if source.facts().version != self.input.expected_version {
            return Err(changed());
        }
        Ok(())
    }
    pub async fn apply<R: PgFieldServiceOrderReferences, T: PgVisitTechnician>(
        &self,
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        technician_source: &T,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.require_before(c, source, technician_source, now)
            .await?;
        if source.facts().version != self.input.expected_version {
            return Err(changed());
        }
        if self.input.action == VisitAction::Dispatch {
            let expired:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM field_service_part_reservations WHERE order_id=$1 AND quantity_milli>consumed_milli+released_milli AND expires_at<=$2)").bind(&self.order_id).bind(self.at).fetch_one(&mut *c).await?;
            if expired {
                return Err(changed());
            }
        }
        let n=sqlx::query("UPDATE field_service_orders SET status=$3,technician_id=$4,scheduled_at=$5,version=version+1,updated_at=$6,visit_started_at=CASE WHEN $7 THEN $6 ELSE visit_started_at END,visit_completed_at=CASE WHEN $9 THEN $6 ELSE visit_completed_at END,labor_minutes=CASE WHEN $9 THEN $10 ELSE labor_minutes END,completion_evidence=CASE WHEN $9 THEN $11 ELSE completion_evidence END,customer_confirmation=CASE WHEN $9 THEN $12 ELSE customer_confirmation END,exception_reason=CASE WHEN $13::text='exception' THEN $14::text WHEN $13::text='resume' OR $9 THEN NULL ELSE exception_reason END WHERE id=$1 AND company_id=$2 AND version=$8").bind(&self.order_id).bind(&self.company_id).bind(self.input.action.status()).bind(&self.technician).bind(self.scheduled).bind(self.at).bind(self.input.action==VisitAction::Start).bind(self.input.expected_version).bind(self.input.action==VisitAction::Complete).bind(self.input.labor_minutes).bind(self.input.evidence.as_deref()).bind(self.input.customer_confirmation.as_deref()).bind(self.input.action.operation().strip_prefix("field.order.").ok_or_else(invalid)?).bind(&self.input.reason).execute(&mut *c).await?.rows_affected();
        if n != 1 {
            return Err(changed());
        }
        sqlx::query("INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at)VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10::jsonb,$11)").bind(format!("service-order-event-{}",self.key.as_str())).bind(&self.order_id).bind(self.version).bind(self.input.action.operation().strip_prefix("field.order.").ok_or_else(invalid)?).bind(&self.actor).bind(&self.input.reason).bind(self.input.action.status()).bind(&self.technician).bind(self.scheduled).bind(&self.details).bind(self.at).execute(&mut *c).await?;
        self.require_after(c, source, technician_source, now).await
    }
    pub async fn require_after<R: PgFieldServiceOrderReferences, T: PgVisitTechnician>(
        &self,
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        technician_source: &T,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || source.base.unit != self.unit
            || source.base.image.order != self.before
            || source.base.image.identity != self.identity
            || source.actor_user_id().as_str() != self.actor
        {
            return Err(changed());
        }
        if technician_source.technician_id() != self.technician {
            return Err(changed());
        }
        if self.input.action == VisitAction::Schedule {
            technician_source.require_current(c).await?;
        }
        source
            .require_original_identity_and_references(c, now)
            .await?;
        let order:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(o)::text,'UTF8')),'hex') FROM field_service_orders o WHERE id=$1 AND company_id=$2").bind(&self.order_id).bind(&self.company_id).fetch_optional(&mut *c).await?;
        let event:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(e)::text,'UTF8')),'hex') FROM field_service_order_events e WHERE id=$1 AND order_id=$2").bind(format!("service-order-event-{}",self.key.as_str())).bind(&self.order_id).fetch_optional(c).await?;
        if order.as_ref() != Some(&self.expected_order)
            || event.as_ref() != Some(&self.expected_event)
        {
            return Err(changed());
        }
        Ok(())
    }
}

/// Complete original Service reservation set, including expired remaining rows.
pub struct PreparedCompletionReservations {
    unit: Unit,
    order: String,
    key: CommandKey,
    before: String,
    after: String,
    groups: Vec<CompletionStockGroup>,
    action: VisitAction,
}
#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
pub struct CompletionStockGroup {
    pub warehouse_id: String,
    pub product_id: String,
    pub count: i64,
}
impl PreparedCompletionReservations {
    pub async fn prepare<R: PgFieldServiceOrderReferences, T: PgVisitTechnician>(
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        plan: &PreparedFieldServiceVisit,
        tech: &T,
        now: TimestampMillis,
    ) -> Result<Self, Error> {
        plan.require_before(c, source, tech, now).await?;
        if !matches!(
            plan.input.action,
            VisitAction::Complete | VisitAction::Cancel
        ) {
            return Err(changed());
        }
        let order = source.facts().id.as_str().to_owned();
        let (before, after, groups) = reservation_images(c, &order, plan.input.action).await?;
        Ok(Self {
            unit: unit(c).await?,
            order,
            key: plan.key.clone(),
            before,
            after,
            groups,
            action: plan.input.action,
        })
    }
    pub fn groups(&self) -> &[CompletionStockGroup] {
        &self.groups
    }
    pub async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let (before, after, groups) = reservation_images(c, &self.order, self.action).await?;
        if before != self.before || after != self.after || groups != self.groups {
            return Err(changed());
        }
        Ok(())
    }
    pub async fn apply<R: PgFieldServiceOrderReferences, T: PgVisitTechnician>(
        &self,
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        plan: &PreparedFieldServiceVisit,
        tech: &T,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        if plan.key != self.key || plan.order_id != self.order || plan.input.action != self.action {
            return Err(changed());
        }
        plan.require_version(c, source, tech, now).await?;
        self.require_before(c).await?;
        sqlx::query("UPDATE field_service_part_reservations SET released_milli=quantity_milli-consumed_milli,release_reason=$2 WHERE order_id=$1 AND quantity_milli>consumed_milli+released_milli").bind(&self.order).bind(release_reason(self.action)?).execute(&mut *c).await?;
        self.require_after(c).await
    }
    pub async fn require_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let (before, _, groups) = reservation_images(c, &self.order, self.action).await?;
        if before != self.after || !groups.is_empty() {
            return Err(changed());
        }
        Ok(())
    }
}
async fn reservation_images(
    c: &mut PgConnection,
    order: &str,
    action: VisitAction,
) -> Result<(String, String, Vec<CompletionStockGroup>), Error> {
    use sqlx::Row;
    let row=sqlx::query("WITH rows AS (SELECT * FROM field_service_part_reservations WHERE order_id=$1), groups AS (SELECT warehouse_id,product_id,count(*)::bigint n FROM rows WHERE quantity_milli>consumed_milli+released_milli GROUP BY warehouse_id,product_id), packed AS (SELECT coalesce(jsonb_agg(jsonb_build_object('warehouse_id',warehouse_id,'product_id',product_id,'count',n) ORDER BY warehouse_id,product_id),'[]'::jsonb)::text value FROM groups) SELECT encode(sha256(convert_to(coalesce(jsonb_agg(to_jsonb(r) ORDER BY r.id),'[]'::jsonb)::text,'UTF8')),'hex') before,encode(sha256(convert_to(coalesce(jsonb_agg(to_jsonb(r)||CASE WHEN r.quantity_milli>r.consumed_milli+r.released_milli THEN jsonb_build_object('released_milli',r.quantity_milli-r.consumed_milli,'release_reason',$2::text) ELSE '{}'::jsonb END ORDER BY r.id),'[]'::jsonb)::text,'UTF8')),'hex') after,CASE WHEN octet_length((SELECT value FROM packed))<=2097152 THEN (SELECT value FROM packed) END groups FROM rows r").bind(order).bind(release_reason(action)?).fetch_one(c).await?;
    let groups: String = row
        .try_get::<Option<String>, _>("groups")?
        .ok_or_else(invalid)?;
    let groups = serde_json::from_str(&groups).map_err(|_| invalid())?;
    Ok((row.try_get("before")?, row.try_get("after")?, groups))
}

fn release_reason(action: VisitAction) -> Result<&'static str, Error> {
    match action {
        VisitAction::Complete => Ok("Visit completed"),
        VisitAction::Cancel => Ok("Service order cancelled"),
        _ => Err(changed()),
    }
}
