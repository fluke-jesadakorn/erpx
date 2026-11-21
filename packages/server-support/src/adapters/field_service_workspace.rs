//! Support-owned bounded display and source-ticket references for Service.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde::{Deserialize, Serialize};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceTicketDisplay {
    pub id: String,
    pub title: String,
    pub customer_id: Option<String>,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub source_kind: String,
    pub order_id: Option<String>,
    pub service_agreement_id: Option<String>,
    pub service_order_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceTicketReference {
    pub id: String,
    pub title: String,
    pub version: i64,
    pub customer_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
}
#[derive(Clone, Deserialize, PartialEq)]
pub struct ServiceWorkspaceTickets {
    pub display: Vec<ServiceTicketDisplay>,
    pub picker: Vec<ServiceTicketReference>,
}
fn limited() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FIELD_SERVICE_WORKSPACE_LIMIT",
        "The Service workspace exceeds this bounded read.",
    )
    .into()
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
    require_service_workspace_tickets_held(&mut *c).await?;
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
/// Check the workspace Support read fence without acquiring a lock or
/// implying selected authority or pre-snapshot timing.
pub async fn require_service_workspace_tickets_held(c:&mut PgConnection)->Result<(),Error>{
    if !c.is_in_transaction(){return Err(PersistenceError::new("SUPPORT_TRANSACTION_REQUIRED").into());}
    let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND relation='support_tickets'::regclass AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))").fetch_one(c).await?;
    if !held{return Err(PersistenceError::new("SUPPORT_WORKSPACE_TICKET_LOCK_REQUIRED").into());}Ok(())
}
async fn require_settlement_locks(c:&mut PgConnection)->Result<(),Error>{
    let held:bool=sqlx::query_scalar("SELECT count(DISTINCT relation)=2 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND relation IN ('support_ticket_events'::regclass,'support_sla_alerts'::regclass) AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')").fetch_one(c).await?;
    if !held{return Err(PersistenceError::new("SUPPORT_SETTLEMENT_LOCKS_REQUIRED").into());}Ok(())
}
pub async fn lock_service_workspace_tickets(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("SUPPORT_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE support_tickets IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
#[derive(Clone)]
pub struct ServiceWorkspaceTicketScope {
    pub company_id: CompanyId,
    pub branch: Option<String>,
    pub department: Option<String>,
    pub assignment_kind: String,
    pub assignment_node: Option<String>,
    pub include_picker: bool,
    pub ids: Vec<String>,
}
async fn image(
    c: &mut PgConnection,
    r: &ServiceWorkspaceTicketScope,
) -> Result<(ServiceWorkspaceTickets, String), Error> {
    let row=sqlx::query("WITH selected AS MATERIALIZED (SELECT t.*, (t.id=ANY($4::text[])) display_it,($5::boolean AND ($2::text IS NULL OR t.branch_id=$2) AND ($3::text IS NULL OR t.department_id=$3) AND t.source_kind='sales_order' AND t.status IN ('open','in_progress','waiting_customer') AND ($6='holding' OR $6='branch' AND t.branch_id=$7 OR $6='department' AND t.department_id=$7)) picker_it FROM support_tickets t WHERE t.company_id=$1), payload AS (SELECT jsonb_build_object('display',coalesce(jsonb_agg(jsonb_build_object('id',id,'title',title,'customer_id',customer_id,'branch_id',branch_id,'department_id',department_id,'source_kind',source_kind,'order_id',order_id,'service_agreement_id',service_agreement_id,'service_order_id',service_order_id) ORDER BY id) FILTER(WHERE display_it),'[]'::jsonb),'picker',coalesce(jsonb_agg(jsonb_build_object('id',id,'title',title,'version',version,'customer_id',customer_id,'branch_id',branch_id,'department_id',department_id) ORDER BY updated_at DESC,id) FILTER(WHERE picker_it),'[]'::jsonb)) value,coalesce(jsonb_agg(to_jsonb(selected) ORDER BY id) FILTER(WHERE display_it OR picker_it),'[]'::jsonb) full_rows FROM selected), measured AS (SELECT value::text payload,encode(sha256(convert_to(full_rows::text,'UTF8')),'hex') digest FROM payload) SELECT CASE WHEN octet_length(payload)<=2097152 THEN payload END payload,digest FROM measured").bind(r.company_id.as_str()).bind(r.branch.as_deref()).bind(r.department.as_deref()).bind(&r.ids).bind(r.include_picker).bind(&r.assignment_kind).bind(r.assignment_node.as_deref()).fetch_one(c).await?;
    let raw: Option<String> = row.try_get("payload")?;
    Ok((
        serde_json::from_str(&raw.ok_or_else(limited)?).map_err(|_| limited())?,
        row.try_get("digest")?,
    ))
}
pub struct CurrentServiceWorkspaceTickets {
    scope: ServiceWorkspaceTicketScope,
    unit: Unit,
    data: ServiceWorkspaceTickets,
    digest: String,
}
impl CurrentServiceWorkspaceTickets {
    pub fn data(&self) -> &ServiceWorkspaceTickets {
        &self.data
    }
    pub async fn require_field_file_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(limited());
        }
        let actual = image(c, &self.scope).await?.0;
        let identity = |t: &ServiceTicketDisplay| {
            (
                t.id.clone(),
                t.customer_id.clone(),
                t.branch_id.clone(),
                t.department_id.clone(),
                t.source_kind.clone(),
                t.order_id.clone(),
                t.service_agreement_id.clone(),
                t.service_order_id.clone(),
            )
        };
        if actual.display.iter().map(identity).collect::<Vec<_>>()
            != self.data.display.iter().map(identity).collect::<Vec<_>>()
        {
            return Err(limited());
        }
        Ok(())
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || image(c, &self.scope).await?.1 != self.digest {
            return Err(limited());
        }
        Ok(())
    }
}
pub async fn read_service_workspace_tickets(
    c: &mut PgConnection,
    scope: ServiceWorkspaceTicketScope,
) -> Result<CurrentServiceWorkspaceTickets, Error> {
    let unit = unit(c).await?;
    let (data, digest) = image(c, &scope).await?;
    Ok(CurrentServiceWorkspaceTickets {
        scope,
        unit,
        data,
        digest,
    })
}

/// Raw original Service settlement state. No active/visible-ticket filter may
/// remove an orphan, mismatched Field projection or unacknowledged SLA duty.
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
pub struct ServiceSupportSettlementTicket {
    pub id:String,pub company_id:String,pub branch_id:Option<String>,pub department_id:Option<String>,pub customer_id:Option<String>,
    pub source_kind:Option<String>,pub service_order_id:Option<String>,pub service_agreement_id:Option<String>,pub service_order_version:Option<i64>,pub status:String,pub digest:String,
}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
pub struct ServiceSupportSettlementAlert {pub id:String,pub ticket_id:String,pub digest:String}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
pub struct ServiceSupportSettlementRows {pub tickets:Vec<ServiceSupportSettlementTicket>,pub alerts:Vec<ServiceSupportSettlementAlert>}
pub struct CurrentServiceSupportSettlement {
    company:CompanyId,known_tickets:Vec<String>,unit:Unit,data:ServiceSupportSettlementRows,
}
pub async fn lock_service_support_settlement(c:&mut PgConnection)->Result<(),Error>{
    if !c.is_in_transaction(){return Err(PersistenceError::new("SUPPORT_TRANSACTION_REQUIRED").into());}
    sqlx::query("LOCK TABLE support_tickets,support_ticket_events,support_sla_alerts IN SHARE MODE").execute(c).await?;
    Ok(())
}
async fn capture_service_support_settlement(c:&mut PgConnection,company:&CompanyId,known_tickets:&[String])->Result<ServiceSupportSettlementRows,Error>{
    let row=sqlx::query("WITH tickets AS MATERIALIZED(SELECT * FROM support_tickets WHERE (company_id=$1 AND source_kind='field_service') OR id=ANY($2::text[])),payload AS(SELECT jsonb_build_object('tickets',coalesce((SELECT jsonb_agg(jsonb_build_object('id',t.id,'company_id',t.company_id,'branch_id',t.branch_id,'department_id',t.department_id,'customer_id',t.customer_id,'source_kind',t.source_kind,'service_order_id',t.service_order_id,'service_agreement_id',t.service_agreement_id,'service_order_version',t.service_order_version,'status',t.status,'digest',encode(sha256(convert_to(to_jsonb(t)::text,'UTF8')),'hex')) ORDER BY t.id) FROM tickets t),'[]'::jsonb),'alerts',coalesce((SELECT jsonb_agg(jsonb_build_object('id',a.id,'ticket_id',a.ticket_id,'digest',encode(sha256(convert_to(to_jsonb(a)::text,'UTF8')),'hex')) ORDER BY a.id) FROM support_sla_alerts a JOIN tickets t ON t.id=a.ticket_id WHERE t.source_kind='field_service' AND a.acknowledged_by IS NULL),'[]'::jsonb)) value) SELECT CASE WHEN octet_length(value::text)<=2097152 THEN value::text END payload FROM payload").bind(company.as_str()).bind(known_tickets).fetch_one(c).await?;
    let payload:Option<String>=row.try_get("payload")?;
    serde_json::from_str(&payload.ok_or_else(limited)?).map_err(|_|limited())
}
impl CurrentServiceSupportSettlement {
    pub fn data(&self)->&ServiceSupportSettlementRows{&self.data}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        if unit(c).await?!=self.unit{return Err(BusinessError::new(FailureKind::Conflict,"SERVICE_SUPPORT_PACKAGE_SOURCE_CHANGED","The original Support settlement source changed.").into());}
        require_settlement_locks(c).await?;
        if capture_service_support_settlement(c,&self.company,&self.known_tickets).await?!=self.data{
            return Err(BusinessError::new(FailureKind::Conflict,"SERVICE_SUPPORT_PACKAGE_SOURCE_CHANGED","The original Support settlement source changed.").into());
        }
        Ok(())
    }
}
pub async fn read_service_support_settlement(c:&mut PgConnection,company:&CompanyId,known_tickets:&[String])->Result<CurrentServiceSupportSettlement,Error>{
    let unit=unit(c).await?;require_settlement_locks(c).await?;let data=capture_service_support_settlement(c,company,known_tickets).await?;
    Ok(CurrentServiceSupportSettlement{company:company.clone(),known_tickets:known_tickets.to_vec(),unit,data})
}
