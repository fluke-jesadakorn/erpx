//! Current roster facts for journal Flow. This port grants no IAM authority.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalFlowPerson {
    pub user_id: String,
    pub name: String,
    pub department_id: Option<String>,
}
/// Must be called before any Holding snapshot, in the same retained unit.
pub async fn lock_journal_flow_people_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("PEOPLE_AUTHORITY_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE hr_people,hr_engagements IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
/// Mirrors the current activeStaffSelect GROUP BY user HAVING COUNT(*)=1:
/// active persons and date-current active engagements, exactly one resulting row.
pub async fn current_journal_flow_people(
    c: &mut PgConnection,
    company: &CompanyId,
) -> Result<Vec<JournalFlowPerson>, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("PEOPLE_AUTHORITY_TRANSACTION_REQUIRED").into());
    }
    let rows = sqlx::query("SELECT p.user_id, min(p.legal_name) name, min(e.department_id) department_id FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=$1 AND p.status='active' AND p.merged_into_id IS NULL AND p.user_id IS NOT NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) GROUP BY p.user_id HAVING count(*)=1 ORDER BY p.user_id")
      .bind(company.as_str()).fetch_all(c).await?;
    rows.into_iter()
        .map(|r| {
            Ok(JournalFlowPerson {
                user_id: r.try_get("user_id")?,
                name: r.try_get("name")?,
                department_id: r.try_get("department_id")?,
            })
        })
        .collect()
}

/// One People-owned row from the stock-count Flow roster. IAM eligibility and
/// Organization team names are separate composition witnesses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StockCountFlowEngagement {
    pub person_id: String,
    pub user_id: String,
    pub name: String,
    pub engagement_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}
type StockCountPeopleUnit = (String, i32, String, String, String, String, Vec<i64>);
#[derive(Clone, Debug, PartialEq)]
struct StockCountPeopleImage {
    row: StockCountFlowEngagement,
    person: String,
    engagement: String,
}
pub struct CurrentStockCountFlowPeople {
    unit: StockCountPeopleUnit,
    company: CompanyId,
    images: Vec<StockCountPeopleImage>,
    rows: Vec<StockCountFlowEngagement>,
}
impl CurrentStockCountFlowPeople {
    /// Deterministic owner witness order, not the unspecified order of the
    /// legacy cross-owner Flow SQL. Composition must resolve that separately.
    pub fn rows(&self) -> &[StockCountFlowEngagement] { &self.rows }
    /// Bind the closed projections to this witnessed connection. Composition
    /// must use the legacy company/active/date predicates and join owner
    /// sources in the same unit, then call `finish` before accepting a result.
    pub async fn bind_projection<'a>(
        &'a self,
        c: &'a mut PgConnection,
    ) -> Result<BoundStockCountFlowPeopleProjection<'a>, Error> {
        self.require_current(c).await?;
        Ok(BoundStockCountFlowPeopleProjection { source: self, connection: c })
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if stock_count_people_unit(c).await? != self.unit
            || stock_count_people_images(c, &self.company).await? != self.images
            || stock_count_people_unit(c).await? != self.unit
        {
            return Err(PersistenceError::new("STOCK_COUNT_FLOW_PEOPLE_CHANGED").into());
        }
        Ok(())
    }
}
/// A People source capability bound to the witness's actual connection.
/// These constants expose no caller-supplied SQL and confer no IAM authority.
#[must_use = "finish the bound projection to revalidate the People witness"]
pub struct BoundStockCountFlowPeopleProjection<'a> {
    source: &'a CurrentStockCountFlowPeople,
    connection: &'a mut PgConnection,
}
impl BoundStockCountFlowPeopleProjection<'_> {
    pub fn people_sql(&self) -> &'static str {
        r#"SELECT "id","user_id","company_id","legal_name","preferred_name","email","phone","country_code","status","version","created_by","created_at","updated_at","merged_into_id","work_email","work_phone" FROM public.hr_people"#
    }
    pub fn engagements_sql(&self) -> &'static str {
        r#"SELECT "id","person_id","company_id","branch_id","department_id","worker_type","employment_type","status","start_date","end_date","manager_person_id","job_title","currency","version","created_by","created_at","updated_at","pay_group_id","level_id","position_id","self_service_top_level" FROM public.hr_engagements"#
    }
    pub fn connection(&mut self) -> &mut PgConnection { self.connection }
    /// Mandatory after evaluating the composed query and before its result is
    /// used as authority; verifies the same unit, fences and source image.
    pub async fn finish(self) -> Result<(), Error> {
        self.source.require_current(self.connection).await
    }
}
/// Caller must acquire People and all other owner fences before the first
/// protected snapshot, then separately join actual current IAM eligibility.
pub async fn inspect_stock_count_flow_people(
    c: &mut PgConnection,
    company: &CompanyId,
) -> Result<CurrentStockCountFlowPeople, Error> {
    let unit = stock_count_people_unit(c).await?;
    let images = stock_count_people_images(c, company).await?;
    if stock_count_people_unit(c).await? != unit {
        return Err(PersistenceError::new("STOCK_COUNT_FLOW_PEOPLE_CHANGED").into());
    }
    let rows = images.iter().map(|image| image.row.clone()).collect();
    Ok(CurrentStockCountFlowPeople { unit, company: company.clone(), images, rows })
}
async fn stock_count_people_images(c: &mut PgConnection, company: &CompanyId) -> Result<Vec<StockCountPeopleImage>, Error> {
    // The SQL date predicate is the TS activeStaffSelect predicate. There is
    // no row or byte cap; complete source images remain in memory.
    let rows = sqlx::query("SELECT p.id person_id,p.user_id,p.legal_name,e.id engagement_id,e.branch_id,e.department_id,to_jsonb(p)::text person_image,to_jsonb(e)::text engagement_image FROM public.hr_people p JOIN public.hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=$1 AND p.status='active' AND p.merged_into_id IS NULL AND p.user_id IS NOT NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) ORDER BY p.user_id COLLATE \"C\",p.id COLLATE \"C\",e.id COLLATE \"C\"")
        .bind(company.as_str()).fetch_all(c).await?;
    rows.into_iter().map(|r| Ok(StockCountPeopleImage {
        row: StockCountFlowEngagement { person_id:r.try_get("person_id")?, user_id:r.try_get("user_id")?, name:r.try_get("legal_name")?, engagement_id:r.try_get("engagement_id")?, branch_id:r.try_get("branch_id")?, department_id:r.try_get("department_id")? },
        person:r.try_get("person_image")?, engagement:r.try_get("engagement_image")?,
    })).collect()
}
async fn stock_count_people_unit(c: &mut PgConnection) -> Result<StockCountPeopleUnit, Error> {
    if !c.is_in_transaction() || sqlx::query_scalar::<_, String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" {
        return Err(PersistenceError::new("STOCK_COUNT_FLOW_PEOPLE_UNIT_REQUIRED").into());
    }
    let held:bool=sqlx::query_scalar("SELECT count(*)=2 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND to_regclass(c.relname::text)=c.oid AND NOT row_security_active(c.oid) AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND l.relation=c.oid)) FROM pg_class c WHERE c.oid IN ('public.hr_people'::regclass,'public.hr_engagements'::regclass)").fetch_one(&mut *c).await?;
    if !held { return Err(PersistenceError::new("STOCK_COUNT_FLOW_PEOPLE_FENCE_REQUIRED").into()); }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,current_setting('search_path'),ARRAY['public.hr_people'::regclass::oid::bigint,'public.hr_engagements'::regclass::oid::bigint]").fetch_one(c).await?)
}
