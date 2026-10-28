//! Original Support ticket lineage and 1048 deferred parts projection. No
//! Field Service/Person/purchase grant and no foreign-owner private SQL.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, HoldingId, RecordId, TimestampMillis, UserId,
};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupportServiceTicketRequest {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub ticket_id: String,
    pub branch_id: RecordId,
    pub department_id: Option<RecordId>,
    pub customer_id: RecordId,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupportServiceTicketFacts {
    pub request: SupportServiceTicketRequest,
    pub source_kind: String,
    pub sales_order_id: Option<String>,
    pub service_agreement_id: Option<String>,
    pub service_order_id: Option<String>,
}
pub struct CurrentSupportServiceTicket {
    facts: SupportServiceTicketFacts,
    unit: Unit,
}
impl CurrentSupportServiceTicket {
    pub fn facts(&self) -> &SupportServiceTicketFacts {
        &self.facts
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || read_facts(c, &self.facts.request).await?.as_ref() != Some(&self.facts)
        {
            return Err(changed());
        }
        Ok(())
    }
    pub async fn prepare_parts_projection(
        &self,
        c: &mut PgConnection,
        input: SupportServicePartsProjection,
    ) -> Result<PreparedSupportServicePartsProjection, Error> {
        self.require_current(c).await?;
        input.require_data()?;
        let before = full_ticket(c, &self.facts.request)
            .await?
            .ok_or_else(changed)?;
        let (service_version, ticket_version): (Option<i64>, i64) = sqlx::query_as(
            "SELECT service_order_version,version FROM support_tickets WHERE id=$1 AND company_id=$2",
        )
        .bind(self.facts.request.ticket_id.as_str())
        .bind(self.facts.request.company_id.as_str())
        .fetch_one(&mut *c)
        .await?;
        // The actual trigger leaves legacy cases unchanged. It also performs
        // a no-op consistency check when the same version was already projected.
        // Do not invent an early gate before original Finance/late Work checks.
        let project = self.facts.source_kind == "field_service"
            && service_version != Some(input.order_version_after);
        let expected_ticket = if project {
            sqlx::query_scalar("SELECT encode(sha256(convert_to((to_jsonb(t)||jsonb_build_object('status','open','owner_id',$3::text,'due_date',to_char(to_timestamp(coalesce($4::bigint,$5::bigint)/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD'),'resolution',NULL,'resolved_by',NULL,'resolved_at',NULL,'confirmed_by',NULL,'confirmed_at',NULL,'confirmation_reason',NULL,'service_order_version',$6::bigint,'version',t.version+1,'updated_at',$5::bigint))::text,'UTF8')),'hex') FROM support_tickets t WHERE id=$1 AND company_id=$2")
                .bind(self.facts.request.ticket_id.as_str()).bind(self.facts.request.company_id.as_str()).bind(input.technician_id.as_str()).bind(input.scheduled_at.map(TimestampMillis::get)).bind(input.application_at.get()).bind(input.order_version_after).fetch_one(&mut*c).await?
        } else {
            before.clone()
        };
        let history_id = format!("ticket-service-event-{}", input.event_id);
        let expected_history = project.then_some((input, ticket_version));
        Ok(PreparedSupportServicePartsProjection {
            facts: self.facts.clone(),
            unit: self.unit.clone(),
            before,
            expected_ticket,
            expected_history,
            history_id,
        })
    }
}
#[derive(Clone, Copy)]
pub enum SupportServicePartsAction {
    Reserve,
    Issue,
}
impl SupportServicePartsAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Reserve => "reserve-part",
            Self::Issue => "issue-part",
        }
    }
}
/// Data derived from the actual retained order/event plan, not permission or
/// a command key. The exact derived event may be 200 bytes; its Support history
/// ID may be 221, and is deliberately not forced into a new CommandKey.
pub struct SupportServicePartsProjection {
    pub order_id: String,
    pub event_id: String,
    pub order_version_after: i64,
    pub action: SupportServicePartsAction,
    pub actor_id: UserId,
    pub technician_id: String,
    pub scheduled_at: Option<TimestampMillis>,
    pub application_at: TimestampMillis,
    pub reason: String,
}
impl SupportServicePartsProjection {
    fn require_data(&self) -> Result<(), Error> {
        if !(2..=9_007_199_254_740_992).contains(&self.order_version_after)
            || self.event_id.len() > 200
            || !self.event_id.starts_with("service-order-event-")
            || self.reason.chars().count() > 3000
            || self.reason.len() > 12000
            || !(0..=9_007_199_254_740_991).contains(&self.application_at.get())
            || self
                .scheduled_at
                .is_some_and(|t| !(1..=9_007_199_254_740_991).contains(&t.get()))
        {
            return Err(invalid());
        }
        Ok(())
    }
}
/// Original intended hashes, computed before effects; no public constructor,
/// Clone, Deserialize, mutable baseline refresh or persistence capability.
pub struct PreparedSupportServicePartsProjection {
    facts: SupportServiceTicketFacts,
    unit: Unit,
    before: String,
    expected_ticket: String,
    expected_history: Option<(SupportServicePartsProjection, i64)>,
    history_id: String,
}
impl PreparedSupportServicePartsProjection {
    pub async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_identity(c).await?;
        if full_ticket(c, &self.facts.request).await?.as_deref() != Some(&self.before) {
            return Err(changed());
        }
        Ok(())
    }
    /// Call after the actual order/event, notification, Workflow effects and
    /// `SET CONSTRAINTS ALL IMMEDIATE`; this verifies the original expected data.
    pub async fn require_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_identity(c).await?;
        if full_ticket(c, &self.facts.request).await?.as_deref() != Some(&self.expected_ticket) {
            return Err(changed());
        }
        if let Some((input, ticket_version)) = &self.expected_history {
            // Derived only from the retained pre-effect version and immutable input.
            let expected: String = sqlx::query_scalar("SELECT encode(sha256(convert_to(jsonb_build_object('id',$1::text,'ticket_id',$2::text,'version',$3::bigint,'action',$4::text,'actor_id',$5::text,'reason',$6::text,'owner_id',$7::text,'due_date',to_char(to_timestamp(coalesce($8::bigint,$9::bigint)/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD'),'status','open','created_at',$9::bigint,'evidence_json',jsonb_build_object('serviceOrderId',$10::text,'serviceOrderVersion',$11::bigint,'serviceEventId',$12::text))::text,'UTF8')),'hex')")
                .bind(&self.history_id).bind(self.facts.request.ticket_id.as_str()).bind(ticket_version+1).bind(format!("service-{}",input.action.as_str())).bind(input.actor_id.as_str()).bind(&input.reason).bind(input.technician_id.as_str()).bind(input.scheduled_at.map(TimestampMillis::get)).bind(input.application_at.get()).bind(input.order_id.as_str()).bind(input.order_version_after).bind(&input.event_id).fetch_one(&mut*c).await?;
            let actual:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(e)::text,'UTF8')),'hex') FROM support_ticket_events e WHERE id=$1 AND ticket_id=$2")
                .bind(&self.history_id).bind(self.facts.request.ticket_id.as_str()).fetch_optional(c).await?;
            if actual.as_ref() != Some(&expected) {
                return Err(changed());
            }
        }
        Ok(())
    }
    async fn require_identity(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || read_facts(c, &self.facts.request).await?.as_ref() != Some(&self.facts)
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn lock_service_ticket_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE support_tickets,support_ticket_events IN SHARE ROW EXCLUSIVE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_current_service_ticket(
    c: &mut PgConnection,
    request: &SupportServiceTicketRequest,
) -> Result<Option<CurrentSupportServiceTicket>, Error> {
    let unit = unit(c).await?;
    Ok(read_facts(c, request)
        .await?
        .map(|facts| CurrentSupportServiceTicket { facts, unit }))
}
async fn read_facts(
    c: &mut PgConnection,
    r: &SupportServiceTicketRequest,
) -> Result<Option<SupportServiceTicketFacts>, Error> {
    // This is the original closed lineage projection. Bodies, issue, names,
    // status/version and evidence never leave Support through this source API.
    let row=sqlx::query("SELECT b.bounded,CASE WHEN b.bounded THEN source_kind END source_kind,CASE WHEN b.bounded THEN order_id END order_id,CASE WHEN b.bounded THEN service_agreement_id END service_agreement_id,CASE WHEN b.bounded THEN service_order_id END service_order_id FROM support_tickets CROSS JOIN LATERAL (SELECT octet_length(source_kind)<=32 AND coalesce(octet_length(order_id),0)::bigint+coalesce(octet_length(service_agreement_id),0)::bigint+coalesce(octet_length(service_order_id),0)::bigint<=2097152 AS bounded) b WHERE id=$1 AND company_id=$2 AND branch_id=$3 AND department_id IS NOT DISTINCT FROM $4 AND customer_id=$5 FOR SHARE OF support_tickets")
        .bind(r.ticket_id.as_str()).bind(r.company_id.as_str()).bind(r.branch_id.as_str()).bind(r.department_id.as_ref().map(RecordId::as_str)).bind(r.customer_id.as_str()).fetch_optional(c).await?;
    let Some(row) = row else { return Ok(None) };
    if !row.try_get::<bool, _>("bounded")? {
        return Err(invalid());
    }
    Ok(Some(SupportServiceTicketFacts {
        request: r.clone(),
        source_kind: row
            .try_get::<Option<String>, _>("source_kind")?
            .ok_or_else(invalid)?,
        sales_order_id: row.try_get("order_id")?,
        service_agreement_id: row.try_get("service_agreement_id")?,
        service_order_id: row.try_get::<Option<String>, _>("service_order_id")?,
    }))
}
async fn full_ticket(
    c: &mut PgConnection,
    r: &SupportServiceTicketRequest,
) -> Result<Option<String>, Error> {
    Ok(sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(t)::text,'UTF8')),'hex') FROM support_tickets t WHERE id=$1 AND company_id=$2").bind(r.ticket_id.as_str()).bind(r.company_id.as_str()).fetch_optional(c).await?)
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
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
fn invalid() -> Error {
    PersistenceError::new("SUPPORT_SERVICE_SOURCE_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SUPPORT_SERVICE_SOURCE_CHANGED",
        "The original Support case or intended Service projection changed.",
    )
    .into()
}
