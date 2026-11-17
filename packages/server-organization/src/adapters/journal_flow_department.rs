//! Organization-owned display facts for a retained journal Flow roster.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;

/// Pure fallback rule retained by the Organization owner. Exposed for unit
/// tests and transport callers that already hold an optional display name.
pub fn team_name_from_department(display_name: Option<&str>) -> String {
    crate::domain::journal_flow_department::team_name_from_department(display_name)
}

pub async fn lock_journal_flow_department_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("ORGANIZATION_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE organization_nodes IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}

/// B0 uses an active same-Company node if one exists, otherwise "Company".
pub async fn journal_flow_team_name(
    c: &mut PgConnection,
    company: &CompanyId,
    department: Option<&str>,
) -> Result<String, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("ORGANIZATION_TRANSACTION_REQUIRED").into());
    }
    let Some(department) = department else {
        return Ok("Company".into());
    };
    let display_name = sqlx::query_scalar::<_, String>(
        "SELECT display_name FROM organization_nodes WHERE company_id=$1 AND id=$2 AND status='active'",
    )
    .bind(company.as_str())
    .bind(department)
    .fetch_optional(c)
    .await?;
    Ok(crate::domain::journal_flow_department::team_name_from_department(display_name.as_deref()))
}

type FlowOrganizationUnit=(String,i32,String,String,String,String,String,Vec<i64>);
#[derive(PartialEq,Eq)]
struct FlowOrganizationImage { nodes:Vec<String>, levels:Vec<String>, positions:Vec<String> }
/// Same-unit source evidence only; it never grants an actor permission.
pub struct CurrentStockCountFlowOrganization { unit:FlowOrganizationUnit, company:CompanyId, image:FlowOrganizationImage }
impl CurrentStockCountFlowOrganization {
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error> {
        if flow_organization_unit(c).await? != self.unit || flow_organization_image(c,&self.company).await? != self.image || flow_organization_unit(c).await? != self.unit {
            return Err(PersistenceError::new("STOCK_COUNT_FLOW_ORGANIZATION_CHANGED").into());
        }
        Ok(())
    }
    pub async fn bind_projection<'a>(&'a self,c:&'a mut PgConnection)->Result<BoundStockCountFlowOrganization<'a>,Error> {
        self.require_current(c).await?;
        Ok(BoundStockCountFlowOrganization{source:self,connection:c})
    }
}
/// Closed, transaction-bound owner projections for a composed Flow query.
#[must_use = "finish the projection to revalidate the Organization witness"]
pub struct BoundStockCountFlowOrganization<'a> { source:&'a CurrentStockCountFlowOrganization, connection:&'a mut PgConnection }
impl BoundStockCountFlowOrganization<'_> {
    pub fn nodes_sql(&self)->&'static str { r#"SELECT "id","kind","slug","display_name","parent_id","company_id","branch_id","status","hierarchy_revision" FROM public.organization_nodes"# }
    pub fn levels_sql(&self)->&'static str { r#"SELECT "id","company_id","code","name","rank","active","version","created_at","updated_at","level_number" FROM public.staff_levels"# }
    pub fn positions_sql(&self)->&'static str { r#"SELECT "id","company_id","branch_id","department_id","title","level_id","reports_to_position_id","is_ceo","active","version","created_by","created_at","updated_at","requirements","roster_required" FROM public.organization_positions"# }
    pub fn connection(&mut self)->&mut PgConnection { self.connection }
    /// The caller must finish after its composed query before accepting facts.
    pub async fn finish(self)->Result<(),Error> { self.source.require_current(self.connection).await }
}
/// Acquire alongside all other owner utility locks before the first snapshot.
pub async fn lock_stock_count_flow_organization_sources(c:&mut PgConnection)->Result<(),Error> {
    if !c.is_in_transaction() {return Err(PersistenceError::new("STOCK_COUNT_FLOW_ORGANIZATION_TRANSACTION_REQUIRED").into());}
    sqlx::query("LOCK TABLE public.organization_nodes,public.staff_levels,public.organization_positions IN SHARE MODE NOWAIT").execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    Ok(())
}
pub async fn inspect_stock_count_flow_organization(c:&mut PgConnection,company:&CompanyId)->Result<CurrentStockCountFlowOrganization,Error> {
    let unit=flow_organization_unit(c).await?;
    let image=flow_organization_image(c,company).await?;
    if flow_organization_unit(c).await? != unit {return Err(PersistenceError::new("STOCK_COUNT_FLOW_ORGANIZATION_CHANGED").into());}
    Ok(CurrentStockCountFlowOrganization{unit,company:company.clone(),image})
}
async fn flow_organization_image(c:&mut PgConnection,company:&CompanyId)->Result<FlowOrganizationImage,Error> {
    let nodes=sqlx::query_scalar("SELECT to_jsonb(n)::text FROM public.organization_nodes n WHERE company_id=$1 ORDER BY id").bind(company.as_str()).fetch_all(&mut *c).await?;
    let levels=sqlx::query_scalar("SELECT to_jsonb(l)::text FROM public.staff_levels l WHERE company_id=$1 ORDER BY id").bind(company.as_str()).fetch_all(&mut *c).await?;
    let positions=sqlx::query_scalar("SELECT to_jsonb(p)::text FROM public.organization_positions p WHERE company_id=$1 ORDER BY id").bind(company.as_str()).fetch_all(&mut *c).await?;
    Ok(FlowOrganizationImage{nodes,levels,positions})
}
async fn flow_organization_unit(c:&mut PgConnection)->Result<FlowOrganizationUnit,Error> {
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" {return Err(PersistenceError::new("STOCK_COUNT_FLOW_ORGANIZATION_UNIT_REQUIRED").into());}
    let valid:bool=sqlx::query_scalar("SELECT count(*)=3 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND to_regclass(c.relname::text)=c.oid AND NOT row_security_active(c.oid) AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND l.relation=c.oid)) FROM pg_class c WHERE c.oid IN ('public.organization_nodes'::regclass,'public.staff_levels'::regclass,'public.organization_positions'::regclass)").fetch_one(&mut *c).await?;
    if !valid{return Err(PersistenceError::new("STOCK_COUNT_FLOW_ORGANIZATION_FENCE_REQUIRED").into());}
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,session_user::text,current_setting('search_path'),ARRAY['public.organization_nodes'::regclass::oid::bigint,'public.staff_levels'::regclass::oid::bigint,'public.organization_positions'::regclass::oid::bigint]").fetch_one(c).await?)
}
