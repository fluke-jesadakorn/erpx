//! Support-owned creation of the source ticket and immutable intake event.
//! Service supplies an actual retained Agreement/current-work witness.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use sqlx::{Connection, PgConnection};
use std::future::Future;
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceIntakeTicketFacts {
    pub key: String,
    pub company_id: CompanyId,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub customer_id: String,
    pub agreement_id: String,
    pub agreement_version: i64,
    pub title: String,
    pub issue: String,
    pub technician_id: String,
    pub scheduled_at: Option<i64>,
    pub actor_id: String,
    pub at: i64,
}
impl ServiceIntakeTicketFacts {
    pub fn ticket_id(&self) -> String {
        format!("ticket-{}", self.key)
    }
    pub fn order_id(&self) -> String {
        format!("service-order-{}", self.key)
    }
    pub fn event_id(&self) -> String {
        format!("ticket-event-{}-intake", self.key)
    }
}
pub trait PgServiceIntakeTicketSource: Sync {
    fn facts(&self) -> &ServiceIntakeTicketFacts;
    fn require_before(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn require_after(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
pub struct PreparedServiceIntakeTicket {
    facts: ServiceIntakeTicketFacts,
    unit: Unit,
}
pub struct RetainedServiceIntakeTicket {
    facts: ServiceIntakeTicketFacts,
    unit: Unit,
}
pub async fn prepare_service_intake_ticket<S: PgServiceIntakeTicketSource>(
    c: &mut PgConnection,
    source: &S,
    now: TimestampMillis,
) -> Result<PreparedServiceIntakeTicket, Error> {
    source.require_before(c, now).await?;
    let facts = source.facts();
    if facts.key.is_empty() || facts.agreement_version < 1 || facts.at < 0 {
        return Err(changed());
    }
    let unit = unit(c).await?;
    let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM support_tickets WHERE id=$1) OR EXISTS(SELECT 1 FROM support_ticket_events WHERE id=$2)").bind(facts.ticket_id()).bind(facts.event_id()).fetch_one(c).await?;
    if exists {
        return Err(changed());
    }
    Ok(PreparedServiceIntakeTicket {
        facts: facts.clone(),
        unit,
    })
}
impl PreparedServiceIntakeTicket {
    pub async fn persist<S: PgServiceIntakeTicketSource>(
        self,
        c: &mut PgConnection,
        source: &S,
        now: TimestampMillis,
    ) -> Result<RetainedServiceIntakeTicket, Error> {
        if self.unit != unit(c).await? || source.facts() != &self.facts {
            return Err(changed());
        }
        source.require_before(c, now).await?;
        let f = &self.facts;
        sqlx::query("INSERT INTO support_tickets(id,company_id,branch_id,department_id,order_id,customer_id,title,issue,owner_id,due_date,status,source_kind,created_by,created_at,updated_at,service_agreement_id,service_order_id,service_order_version) VALUES($1,$2,$3,$4,NULL,$5,$6,$7,$8,to_char(to_timestamp(coalesce($9::bigint,$10::bigint)/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD'),'open','field_service',$11,$10,$10,$12,$13,1)")
            .bind(f.ticket_id()).bind(f.company_id.as_str()).bind(&f.branch_id).bind(&f.department_id).bind(&f.customer_id).bind(&f.title).bind(&f.issue).bind(&f.technician_id).bind(f.scheduled_at).bind(f.at).bind(&f.actor_id).bind(&f.agreement_id).bind(f.order_id()).execute(&mut *c).await?;
        sqlx::query("INSERT INTO support_ticket_events(id,ticket_id,version,action,actor_id,reason,owner_id,due_date,status,created_at,evidence_json) VALUES($1,$2,1,'service-intake',$3,$4,$5,to_char(to_timestamp(coalesce($6::bigint,$7::bigint)/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD'),'open',$7,jsonb_build_object('serviceOrderId',$8::text,'agreementId',$9::text,'agreementVersion',$10::bigint))")
            .bind(f.event_id()).bind(f.ticket_id()).bind(&f.actor_id).bind(&f.issue).bind(&f.technician_id).bind(f.scheduled_at).bind(f.at).bind(f.order_id()).bind(&f.agreement_id).bind(f.agreement_version).execute(&mut *c).await?;
        Ok(RetainedServiceIntakeTicket {
            facts: self.facts,
            unit: self.unit,
        })
    }
}
impl RetainedServiceIntakeTicket {
    pub async fn require_current<S: PgServiceIntakeTicketSource>(
        &self,
        c: &mut PgConnection,
        source: &S,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        if self.unit != unit(c).await? || source.facts() != &self.facts {
            return Err(changed());
        }
        source.require_after(c, now).await?;
        let f = &self.facts;
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM support_tickets t JOIN support_ticket_events e ON e.ticket_id=t.id WHERE t.id=$1 AND t.company_id=$2 AND t.branch_id=$3 AND t.department_id IS NOT DISTINCT FROM $4 AND t.customer_id=$5 AND t.title=$6 AND t.issue=$7 AND t.owner_id=$8 AND t.created_by=$9 AND t.created_at=$10 AND t.updated_at=$10 AND t.version=1 AND t.status='open' AND t.source_kind='field_service' AND t.order_id IS NULL AND t.service_order_version=1 AND t.due_date=to_char(to_timestamp(coalesce($15::bigint,$10::bigint)/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD') AND t.service_agreement_id=$11 AND t.service_order_id=$12 AND e.id=$13 AND e.version=1 AND e.action='service-intake' AND e.actor_id=$9 AND e.reason=$7 AND e.owner_id=$8 AND e.status='open' AND e.due_date=t.due_date AND e.created_at=$10 AND e.evidence_json::jsonb=jsonb_build_object('serviceOrderId',$12::text,'agreementId',$11::text,'agreementVersion',$14::bigint))")
            .bind(f.ticket_id()).bind(f.company_id.as_str()).bind(&f.branch_id).bind(&f.department_id).bind(&f.customer_id).bind(&f.title).bind(&f.issue).bind(&f.technician_id).bind(&f.actor_id).bind(f.at).bind(&f.agreement_id).bind(f.order_id()).bind(f.event_id()).bind(f.agreement_version).bind(f.scheduled_at).fetch_one(c).await?;
        if !exists {
            return Err(changed());
        }
        Ok(())
    }
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("SUPPORT_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SUPPORT_SERVICE_SOURCE_CHANGED",
        "The original Service intake source changed.",
    )
    .into()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceIntakeReadFacts {
    pub ticket_id: String,
    pub status: String,
    pub version: i64,
    pub customer_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub agreement_id: String,
    pub agreement_version: i64,
    pub order_id: String,
    pub actor_id: String,
    pub initial_issue: String,
    pub initial_owner: String,
    pub initial_at: i64,
}
pub struct CurrentServiceIntakeRead {
    facts: ServiceIntakeReadFacts,
    unit: Unit,
    image: String,
}
impl CurrentServiceIntakeRead {
    pub fn facts(&self) -> &ServiceIntakeReadFacts {
        &self.facts
    }
    pub async fn require_initial_schedule(
        &self,
        c: &mut PgConnection,
        key: &str,
        scheduled_at: Option<i64>,
        created_at: i64,
    ) -> Result<(), Error> {
        if self.unit != unit(c).await? {
            return Err(changed());
        }
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM support_ticket_events WHERE id=$1 AND ticket_id=$2 AND status='open' AND created_at=$3 AND due_date=to_char(to_timestamp(coalesce($4::bigint,$3::bigint)/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD'))").bind(format!("ticket-event-{key}-intake")).bind(&self.facts.ticket_id).bind(created_at).bind(scheduled_at).fetch_one(c).await?;
        if !valid {
            return Err(changed());
        }
        Ok(())
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        company: &CompanyId,
        key: &str,
    ) -> Result<(), Error> {
        if self.unit != unit(c).await? {
            return Err(changed());
        }
        let fresh = read_intake(c, company, key).await?.ok_or_else(changed)?;
        if fresh != (self.facts.clone(), self.image.clone()) {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_service_intake(
    c: &mut PgConnection,
    company: &CompanyId,
    key: &str,
) -> Result<Option<CurrentServiceIntakeRead>, Error> {
    let unit = unit(c).await?;
    Ok(read_intake(c, company, key)
        .await?
        .map(|(facts, image)| CurrentServiceIntakeRead { facts, image, unit }))
}
async fn read_intake(
    c: &mut PgConnection,
    company: &CompanyId,
    key: &str,
) -> Result<Option<(ServiceIntakeReadFacts, String)>, Error> {
    use sqlx::Row;
    // Limit transferred projection, not unrelated ticket/event row content.
    let row=sqlx::query("SELECT b.bounded,CASE WHEN b.bounded THEN t.id END id,CASE WHEN b.bounded THEN t.status END status,t.version::bigint version,CASE WHEN b.bounded THEN t.customer_id END customer_id,CASE WHEN b.bounded THEN t.branch_id END branch_id,CASE WHEN b.bounded THEN t.department_id END department_id,CASE WHEN b.bounded THEN t.service_agreement_id END service_agreement_id,CASE WHEN b.bounded THEN e.evidence_json::jsonb->>'agreementVersion' END agreement_version,CASE WHEN b.bounded THEN t.service_order_id END service_order_id,CASE WHEN b.bounded THEN e.actor_id END actor_id,CASE WHEN b.bounded THEN e.reason END initial_issue,CASE WHEN b.bounded THEN e.owner_id END initial_owner,e.created_at::bigint initial_at,encode(sha256(convert_to(jsonb_build_array(to_jsonb(t),to_jsonb(e))::text,'UTF8')),'hex') image FROM support_tickets t JOIN support_ticket_events e ON e.ticket_id=t.id CROSS JOIN LATERAL (SELECT octet_length(t.id)::bigint+octet_length(t.status)::bigint+octet_length(t.customer_id)::bigint+octet_length(t.branch_id)::bigint+coalesce(octet_length(t.department_id),0)::bigint+coalesce(octet_length(t.service_agreement_id),0)::bigint+coalesce(octet_length(e.evidence_json::jsonb->>'agreementVersion'),0)::bigint+coalesce(octet_length(t.service_order_id),0)::bigint+octet_length(e.actor_id)::bigint+octet_length(e.reason)::bigint+octet_length(e.owner_id)::bigint<=2097152 AS bounded) b WHERE t.id=$1 AND t.company_id=$2 AND t.source_kind='field_service' AND e.id=$3 AND e.version=1 AND e.action='service-intake' AND e.actor_id=t.created_by AND e.evidence_json::jsonb->>'serviceOrderId'=t.service_order_id AND e.evidence_json::jsonb->>'agreementId'=t.service_agreement_id FOR SHARE OF t,e").bind(format!("ticket-{key}")).bind(company.as_str()).bind(format!("ticket-event-{key}-intake")).fetch_optional(c).await?;
    let Some(r) = row else { return Ok(None) };
    if !r.try_get::<bool, _>("bounded")? {
        return Err(read_limit());
    }
    let required = |name| -> Result<String, Error> {
        r.try_get::<Option<String>, _>(name)?.ok_or_else(changed)
    };
    let version: i64 = r.try_get("version")?;
    let av = required("agreement_version")?
        .parse::<i64>()
        .map_err(|_| changed())?;
    if !(1..=9_007_199_254_740_991).contains(&version) || !(1..=9_007_199_254_740_991).contains(&av)
    {
        return Err(changed());
    }
    let facts = ServiceIntakeReadFacts {
        ticket_id: required("id")?,
        status: required("status")?,
        version,
        customer_id: required("customer_id")?,
        branch_id: required("branch_id")?,
        department_id: r.try_get("department_id")?,
        agreement_id: required("service_agreement_id")?,
        agreement_version: av,
        order_id: required("service_order_id")?,
        actor_id: required("actor_id")?,
        initial_issue: required("initial_issue")?,
        initial_owner: required("initial_owner")?,
        initial_at: r.try_get("initial_at")?,
    };
    Ok(Some((facts, r.try_get("image")?)))
}

fn read_limit() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FIELD_SERVICE_REQUEST_SOURCE_LIMIT",
        "The Service request exceeds this bounded read.",
    )
    .into()
}
