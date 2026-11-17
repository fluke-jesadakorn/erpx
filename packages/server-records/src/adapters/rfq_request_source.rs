//! Exact raw employee request facts used by the retained RFQ source check.
//! This port does not establish access, purchase authority or approval.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection};

/// Records projection and predicate used in the original linked RFQ guard.
/// $1/$2 are reserved for the Workflow command key and step.
pub fn rfq_request_approval_assertion_predicate() -> &'static str {
    "EXISTS(SELECT 1 FROM (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"category\",\"title\",\"description\",\"location\",\"impact\",\"needed_date\",\"items_json\",\"currency\",\"requester_user_id\",\"created_by\",\"parent_request_id\",\"state\",\"team_id\",\"assignee_user_id\",\"routing_json\",\"approval_json\",\"blocked_reason\",\"resolution\",\"purchase_order_id\",\"handover_json\",\"version\",\"evidence_version\",\"created_at\",\"updated_at\",\"engagement_id\" FROM employee_requests) employee_requests WHERE id=$3 AND company_id=$4 AND kind='purchase' AND state='in_progress' AND evidence_version=$5 AND items_json=$6 AND approval_json::jsonb->>'decision'='approved')"
}

/// Original Records employeeRequestDetail row projection before access.
pub fn rfq_request_detail_row_sql() -> &'static str {
    "SELECT * FROM employee_requests WHERE id=$1 AND company_id=$2"
}

/// Original RFQ sourceState projection; JSON approval remains text here.
pub fn rfq_request_source_state_sql() -> &'static str {
    "SELECT state,evidence_version,items_json,approval_json FROM (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"category\",\"title\",\"description\",\"location\",\"impact\",\"needed_date\",\"items_json\",\"currency\",\"requester_user_id\",\"created_by\",\"parent_request_id\",\"state\",\"team_id\",\"assignee_user_id\",\"routing_json\",\"approval_json\",\"blocked_reason\",\"resolution\",\"purchase_order_id\",\"handover_json\",\"version\",\"evidence_version\",\"created_at\",\"updated_at\",\"engagement_id\" FROM employee_requests) employee_requests WHERE id=$1 AND company_id=$2"
}
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String, String, String, i64);

/// Fixed Records projection for the original request event read, to be joined
/// with People's actor-name projection by the composition root.
pub fn rfq_request_events_sql() -> &'static str {
    "rfq_request_events AS (SELECT id,company_id,request_id,action,actor_id,note,evidence_json,request_version,created_at FROM public.employee_request_events)"
}

fn invalid() -> Error {
    PersistenceError::new("RFQ_REQUEST_UNIT_REQUIRED").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "RFQ_REQUEST_SOURCE_CHANGED",
        "The quotation request source changed.",
    )
    .into()
}
/// Acquire in the global owner lock order before the first protected snapshot.
pub async fn lock_rfq_request_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE public.employee_requests IN SHARE ROW EXCLUSIVE MODE")
        .execute(c)
        .await?;
    Ok(())
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(invalid());
    }
    let ok:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_class c JOIN pg_locks l ON l.relation=c.oid WHERE c.oid=to_regclass('public.employee_requests') AND c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND NOT row_security_active(c.oid) AND l.pid=pg_backend_pid() AND l.granted AND l.mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))").fetch_one(&mut *c).await?;
    if !ok {
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,session_user::text,'public.employee_requests'::regclass::oid::bigint").fetch_one(c).await?)
}
/// Raw DB fields; `approval_json` stays unparsed to preserve JS source semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RfqRequestFacts {
    pub kind: String,
    pub state: String,
    pub evidence_version: i64,
    pub items_json: String,
    pub approval_json: String,
}
async fn read(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<RfqRequestFacts>, Error> {
    let row:Option<(String,String,i64,String,String)>=sqlx::query_as("SELECT kind,state,evidence_version,items_json,approval_json FROM public.employee_requests WHERE id=$1 AND company_id=$2").bind(id).bind(company.as_str()).fetch_optional(c).await?;
    Ok(row.map(
        |(kind, state, evidence_version, items_json, approval_json)| RfqRequestFacts {
            kind,
            state,
            evidence_version,
            items_json,
            approval_json,
        },
    ))
}
pub struct CurrentRfqRequest {
    unit: Unit,
    company: CompanyId,
    id: String,
    facts: Option<RfqRequestFacts>,
}
impl CurrentRfqRequest {
    pub fn company(&self) -> &CompanyId {
        &self.company
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn facts(&self) -> Option<&RfqRequestFacts> {
        self.facts.as_ref()
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(&mut *c).await? != self.unit
            || read(&mut *c, &self.company, &self.id).await? != self.facts
            || unit(c).await? != self.unit
        {
            return Err(changed());
        }
        Ok(())
    }
}
/// Observe an exact row or absence from the authorized Company. The caller
/// derives the id from its retained RFQ source, never as proof of authority.
pub async fn observe_rfq_request(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<CurrentRfqRequest, Error> {
    let initial = unit(&mut *c).await?;
    let facts = read(&mut *c, company, id).await?;
    if unit(c).await? != initial {
        return Err(changed());
    }
    Ok(CurrentRfqRequest {
        unit: initial,
        company: company.clone(),
        id: id.into(),
        facts,
    })
}

/// Full Records-owned row for the separate quotation/detail lookup. It is
/// intentionally independent of the narrow sourceBlocked projection above:
/// visibility, team, event, attachment and allocation reads remain caller
/// duties, and no permission is conferred by observing this row.
pub struct CurrentRfqQuotationRow {
    unit: Unit,
    company: CompanyId,
    id: String,
    image_json: Option<String>,
}
async fn quotation_row(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<String>, Error> {
    Ok(sqlx::query_scalar("SELECT to_jsonb(r)::text FROM public.employee_requests r WHERE r.id=$1 AND r.company_id=$2")
        .bind(id).bind(company.as_str()).fetch_optional(c).await?)
}
impl CurrentRfqQuotationRow {
    pub fn company(&self) -> &CompanyId {
        &self.company
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    /// None represents the company-scoped 404; JSON fields remain raw strings.
    pub fn image_json(&self) -> Option<&str> {
        self.image_json.as_deref()
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(&mut *c).await? != self.unit
            || quotation_row(&mut *c, &self.company, &self.id).await? != self.image_json
            || unit(c).await? != self.unit
        {
            return Err(changed());
        }
        Ok(())
    }
}
/// Call at the original Records row-read point, before access/detail reads.
/// Like TS, there is no length cap on id or JSON payload.
pub async fn observe_rfq_quotation_row(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<CurrentRfqQuotationRow, Error> {
    let initial = unit(&mut *c).await?;
    let image_json = quotation_row(&mut *c, company, id).await?;
    if unit(c).await? != initial {
        return Err(changed());
    }
    Ok(CurrentRfqQuotationRow {
        unit: initial,
        company: company.clone(),
        id: id.into(),
        image_json,
    })
}

/// Raw Records event fields. Payroll supplies actor_name in the original
/// joined projection; obtaining these fields does not replace that query or
/// its error behavior. The caller must execute this after request access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RfqRequestEvent {
    pub id: String,
    pub action: String,
    pub actor_id: String,
    pub note: String,
    pub evidence_json: String,
    pub request_version: i64,
    pub created_at: i64,
}
pub async fn read_rfq_request_events(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Vec<RfqRequestEvent>, Error> {
    let rows: Vec<(String,String,String,String,String,i64,i64)> = sqlx::query_as("SELECT id,action,actor_id,note,evidence_json,request_version,created_at FROM employee_request_events WHERE request_id=$1 AND company_id=$2 ORDER BY created_at,id LIMIT 250")
        .bind(id).bind(company.as_str()).fetch_all(c).await?;
    Ok(rows
        .into_iter()
        .map(
            |(id, action, actor_id, note, evidence_json, request_version, created_at)| {
                RfqRequestEvent {
                    id,
                    action,
                    actor_id,
                    note,
                    evidence_json,
                    request_version,
                    created_at,
                }
            },
        )
        .collect())
}

/// Exact Records-owned child selection before the caller applies requestAccess
/// to each returned child, swallowing only the original 404 BusinessRuleError.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RfqRequestChild {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub state: String,
}
pub async fn read_rfq_request_children(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Vec<RfqRequestChild>, Error> {
    let rows: Vec<(String,String,String,String)> = sqlx::query_as("SELECT id,title,kind,state FROM employee_requests WHERE parent_request_id=$1 AND company_id=$2 ORDER BY created_at LIMIT 50")
        .bind(id).bind(company.as_str()).fetch_all(c).await?;
    Ok(rows
        .into_iter()
        .map(|(id, title, kind, state)| RfqRequestChild {
            id,
            title,
            kind,
            state,
        })
        .collect())
}
