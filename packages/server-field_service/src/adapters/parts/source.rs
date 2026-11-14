//! Actual owner-local order/agreement/reservation facts. Current IAM is always
//! the normal sealed Holding context; foreign SQL stays behind mandatory owner
//! references assembled by Service. No inbound Actor DTO or context factory.
use crate::{
    application::{http_parts::FieldServicePhysicalOrderId, parts::FieldServicePartsCommand},
    domain::parts::FieldServicePartsActionKind,
};
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, HoldingId, RecordId, TimestampMillis, UserId,
};
use sqlx::{Connection, PgConnection, Row};
use std::future::Future;
pub(super) type Error = PgTransactionError<BusinessError>;
pub(super) type Unit = (String, i32, String, String);
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldServiceOrderFacts {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub id: FieldServicePhysicalOrderId,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub ticket_id: String,
    pub ticket_version: i64,
    pub agreement_id: String,
    pub agreement_version: i64,
    pub customer_id: String,
    pub asset_id: Option<String>,
    pub agreement_has_asset: bool,
    pub created_by: UserId,
    pub version: i64,
    pub state: String,
    pub technician_id: String,
    pub scheduled_at: Option<TimestampMillis>,
}
#[derive(Eq, PartialEq)]
pub(super) struct Image {
    pub(super) facts: FieldServiceOrderFacts,
    pub(super) identity: String,
    pub(super) order: String,
    pub(super) agreement: String,
}
/// Positive owned rows only; all foreign identity/eligibility ports remain
/// mandatory before it can participate in a producer command.
pub struct FieldServiceOrderBase<'a> {
    pub(super) current: &'a AuthorizedWorkContext,
    pub(super) image: Image,
    pub(super) unit: Unit,
    pub(super) observed_at: i64,
}
impl<'a> FieldServiceOrderBase<'a> {
    pub fn facts(&self) -> &FieldServiceOrderFacts {
        &self.image.facts
    }
    pub async fn with_references<R: PgFieldServiceOrderReferences>(
        self,
        c: &mut PgConnection,
        references: R,
        now: TimestampMillis,
    ) -> Result<CurrentFieldServiceOrder<'a, R>, Error> {
        self.require_current(c, now).await?;
        references
            .require_current(c, self.current, self.facts(), now)
            .await?;
        Ok(CurrentFieldServiceOrder {
            base: self,
            references,
        })
    }
    pub(super) async fn require_current(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        require_actor(self.current, now, self.observed_at)?;
        if unit(c).await? != self.unit
            || read(c, self.current, &self.image.facts.id).await?.as_ref() != Some(&self.image)
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub trait PgFieldServiceOrderReferences: Send + Sync {
    fn require_current(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        order: &FieldServiceOrderFacts,
        now: TimestampMillis,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn require_parts_action(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        order: &FieldServiceOrderFacts,
        action: FieldServicePartsActionKind,
        now: TimestampMillis,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn require_visit_action(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        order: &FieldServiceOrderFacts,
        action: crate::domain::visit::VisitAction,
        now: TimestampMillis,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
pub struct CurrentFieldServiceOrder<'a, R: PgFieldServiceOrderReferences> {
    pub(super) base: FieldServiceOrderBase<'a>,
    pub(super) references: R,
}
impl<R: PgFieldServiceOrderReferences> CurrentFieldServiceOrder<'_, R> {
    pub fn facts(&self) -> &FieldServiceOrderFacts {
        self.base.facts()
    }
    pub fn references(&self) -> &R {
        &self.references
    }
    pub fn actor_user_id(&self) -> &UserId {
        self.base.current.user_id()
    }
    pub async fn require_visit_action(
        &self,
        c: &mut PgConnection,
        action: crate::domain::visit::VisitAction,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.require_current(c, now).await?;
        self.references
            .require_visit_action(c, self.base.current, self.facts(), action, now)
            .await
    }
    pub fn actor_assignment_id(&self) -> &erpx_server_shared::public::AssignmentId {
        &self.base.current.selection().assignment().id
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.base.require_current(c, now).await?;
        self.references
            .require_current(c, self.base.current, self.facts(), now)
            .await
    }
    pub(super) async fn require_original_identity_and_references(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        require_actor(self.base.current, now, self.base.observed_at)?;
        if unit(c).await? != self.base.unit {
            return Err(changed());
        }
        let actual = read(c, self.base.current, &self.facts().id)
            .await?
            .ok_or_else(changed)?;
        if actual.identity != self.base.image.identity
            || actual.agreement != self.base.image.agreement
        {
            return Err(changed());
        }
        self.references
            .require_current(c, self.base.current, self.facts(), now)
            .await
    }
    pub async fn require_action(
        &self,
        c: &mut PgConnection,
        command: &FieldServicePartsCommand,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.require_current(c, now).await?;
        if command.order_id() != &self.facts().id {
            return Err(changed());
        }
        self.references
            .require_parts_action(
                c,
                self.base.current,
                self.facts(),
                command.input().action(),
                now,
            )
            .await
    }
    /// Verify a keyless wire request against the same current owner sources.
    pub async fn require_pending_action(
        &self,
        c: &mut PgConnection,
        pending: &crate::application::http_parts::PendingFieldServiceParts,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.require_current(c, now).await?;
        if pending.target.storage_text() != self.facts().id.as_str() {
            return Err(changed());
        }
        self.references
            .require_parts_action(
                c,
                self.base.current,
                self.facts(),
                pending.normalized.input().action(),
                now,
            )
            .await
    }
    pub async fn reservation(
        &self,
        c: &mut PgConnection,
        id: &str,
        now: TimestampMillis,
    ) -> Result<Option<CurrentFieldServiceReservation>, Error> {
        self.require_current(c, now).await?;
        read_reservation(c, &self.base.unit, self.facts(), id).await
    }
}
pub async fn lock_field_service_order_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE field_service_agreements,field_service_agreement_events,field_service_orders,field_service_order_events,field_service_part_reservations,field_service_billing_releases IN SHARE ROW EXCLUSIVE MODE").execute(c).await?;
    Ok(())
}
pub async fn read_field_service_order_base<'a>(
    c: &mut PgConnection,
    current: &'a AuthorizedWorkContext,
    id: &FieldServicePhysicalOrderId,
    now: TimestampMillis,
) -> Result<Option<FieldServiceOrderBase<'a>>, Error> {
    require_actor(current, now, now.get())?;
    let unit = unit(c).await?;
    Ok(read(c, current, id)
        .await?
        .map(|image| FieldServiceOrderBase {
            current,
            image,
            unit,
            observed_at: now.get(),
        }))
}
pub(super) async fn read(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    id: &FieldServicePhysicalOrderId,
) -> Result<Option<Image>, Error> {
    let w = current.selection().workspace();
    // Source identity is the exact original immutable record/reference selection.
    // Full Work/agreement rows are hashed inside PostgreSQL, never transferred.
    let row=sqlx::query("SELECT b.bounded,CASE WHEN b.bounded THEN o.branch_id END branch_id,CASE WHEN b.bounded THEN o.department_id END department_id,CASE WHEN b.bounded THEN o.ticket_id END ticket_id,o.ticket_version::bigint ticket_version,CASE WHEN b.bounded THEN o.agreement_id END agreement_id,o.agreement_version::bigint agreement_version,CASE WHEN b.bounded THEN g.customer_id END customer_id,CASE WHEN b.bounded THEN o.asset_id END asset_id,g.asset_id IS NOT NULL agreement_has_asset,CASE WHEN octet_length(o.created_by)<=200 THEN o.created_by END created_by,o.version::bigint version,CASE WHEN octet_length(o.status)<=32 THEN o.status END state,CASE WHEN b.bounded THEN o.technician_id END technician_id,o.scheduled_at::bigint scheduled_at,encode(sha256(convert_to(jsonb_build_array(o.id,o.company_id,o.branch_id,o.department_id,o.ticket_id,o.ticket_version,o.agreement_id,o.agreement_version,o.asset_id,o.created_by,o.created_at)::text,'UTF8')),'hex') identity,encode(sha256(convert_to(to_jsonb(o)::text,'UTF8')),'hex') order_image,encode(sha256(convert_to(to_jsonb(g)::text,'UTF8')),'hex') agreement_image FROM field_service_orders o JOIN field_service_agreements g ON g.id=o.agreement_id AND g.company_id=o.company_id AND g.branch_id=o.branch_id AND g.department_id IS NOT DISTINCT FROM o.department_id CROSS JOIN LATERAL (SELECT octet_length(o.branch_id)::bigint+coalesce(octet_length(o.department_id),0)::bigint+octet_length(g.customer_id)::bigint+octet_length(o.ticket_id)::bigint+octet_length(o.agreement_id)::bigint+coalesce(octet_length(o.asset_id),0)::bigint+octet_length(o.technician_id)::bigint<=2097152 AS bounded) b WHERE o.id=$1 AND o.company_id=$2 AND ($3::text IS NULL OR o.branch_id=$3) AND ($4::text IS NULL OR o.department_id=$4) FOR UPDATE OF o FOR SHARE OF g")
  .bind(id.as_str()).bind(w.company_id.as_str()).bind(w.branch_id.as_ref().map(RecordId::as_str)).bind(w.department_id.as_ref().map(RecordId::as_str)).fetch_optional(c).await?;
    let Some(row) = row else { return Ok(None) };
    if !row.try_get::<bool, _>("bounded")? {
        return Err(invalid());
    }
    let text = |name| -> Result<String, Error> {
        row.try_get::<Option<String>, _>(name)?.ok_or_else(invalid)
    };
    let facts = FieldServiceOrderFacts {
        holding_id: current.holding_id().clone(),
        company_id: w.company_id.clone(),
        id: id.clone(),
        branch_id: text("branch_id")?,
        department_id: row.try_get("department_id")?,
        ticket_id: text("ticket_id")?,
        ticket_version: row.try_get("ticket_version")?,
        agreement_id: text("agreement_id")?,
        agreement_version: row.try_get("agreement_version")?,
        customer_id: text("customer_id")?,
        asset_id: row.try_get::<Option<String>, _>("asset_id")?,
        agreement_has_asset: row.try_get("agreement_has_asset")?,
        created_by: UserId::new(text("created_by")?)?,
        version: row.try_get("version")?,
        state: text("state")?,
        technician_id: text("technician_id")?,
        scheduled_at: row
            .try_get::<Option<i64>, _>("scheduled_at")?
            .map(TimestampMillis::new),
    };
    if ![facts.ticket_version, facts.agreement_version, facts.version]
        .iter()
        .all(|n| (1..=9_007_199_254_740_991).contains(n))
        || facts
            .scheduled_at
            .is_some_and(|t| !(1..=9_007_199_254_740_991).contains(&t.get()))
    {
        return Err(invalid());
    }
    Ok(Some(Image {
        facts,
        identity: row.try_get("identity")?,
        order: row.try_get("order_image")?,
        agreement: row.try_get("agreement_image")?,
    }))
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldServiceReservationFacts {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub order_id: FieldServicePhysicalOrderId,
    pub id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub consumed_milli: i64,
    pub released_milli: i64,
    pub expires_at: TimestampMillis,
}
pub struct CurrentFieldServiceReservation {
    pub(super) facts: FieldServiceReservationFacts,
    pub(super) before: String,
    pub(super) unit: Unit,
}
impl CurrentFieldServiceReservation {
    pub fn facts(&self) -> &FieldServiceReservationFacts {
        &self.facts
    }
    pub fn remaining_milli(&self) -> i64 {
        self.facts.quantity_milli - self.facts.consumed_milli - self.facts.released_milli
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let value:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex') FROM field_service_part_reservations r WHERE id=$1 AND order_id=$2 FOR UPDATE").bind(self.facts.id.as_str()).bind(self.facts.order_id.as_str()).fetch_optional(c).await?;
        if value.as_ref() != Some(&self.before) {
            return Err(changed());
        }
        Ok(())
    }
}
async fn read_reservation(
    c: &mut PgConnection,
    expected: &Unit,
    order: &FieldServiceOrderFacts,
    id: &str,
) -> Result<Option<CurrentFieldServiceReservation>, Error> {
    if &unit(c).await? != expected {
        return Err(changed());
    }
    let row=sqlx::query("SELECT warehouse_id,product_id,quantity_milli::bigint quantity_milli,consumed_milli::bigint consumed_milli,released_milli::bigint released_milli,expires_at::bigint expires_at,encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex') image FROM field_service_part_reservations r WHERE id=$1 AND order_id=$2 FOR UPDATE")
  .bind(id).bind(order.id.as_str()).fetch_optional(c).await?;
    let Some(row) = row else { return Ok(None) };
    let f = FieldServiceReservationFacts {
        holding_id: order.holding_id.clone(),
        company_id: order.company_id.clone(),
        order_id: order.id.clone(),
        id: id.to_owned(),
        warehouse_id: row.try_get("warehouse_id")?,
        product_id: row.try_get("product_id")?,
        quantity_milli: row.try_get("quantity_milli")?,
        consumed_milli: row.try_get("consumed_milli")?,
        released_milli: row.try_get("released_milli")?,
        expires_at: TimestampMillis::new(row.try_get("expires_at")?),
    };
    if !(1..=9_007_199_254_740_991).contains(&f.quantity_milli)
        || ![f.consumed_milli, f.released_milli]
            .iter()
            .all(|n| (0..=9_007_199_254_740_991).contains(n))
        || f.consumed_milli
            .checked_add(f.released_milli)
            .is_none_or(|n| n > f.quantity_milli)
    {
        return Err(invalid());
    }
    Ok(Some(CurrentFieldServiceReservation {
        facts: f,
        before: row.try_get("image")?,
        unit: expected.clone(),
    }))
}
pub(super) fn require_actor(
    current: &AuthorizedWorkContext,
    now: TimestampMillis,
    observed: i64,
) -> Result<(), Error> {
    if now.get() < observed || now.get() >= current.expires_at() {
        return Err(changed());
    }
    for permission in ["records.read", "records.write"] {
        current.require_permission(permission)?;
    }
    if !current.modules().iter().any(|m| m == "field_service") {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "MODULE_FORBIDDEN",
            "Your current role cannot access Field Service.",
        )
        .into());
    }
    Ok(())
}
pub(super) async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
pub(super) fn invalid() -> Error {
    PersistenceError::new("FIELD_SERVICE_SOURCE_INVALID").into()
}
pub(super) fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_SERVICE_SOURCE_CHANGED",
        "The original Field Service source changed.",
    )
    .into()
}
