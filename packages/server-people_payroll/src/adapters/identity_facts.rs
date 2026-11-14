use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, UserId};
use sqlx::{Connection, PgConnection, Row, postgres::PgRow};

use crate::application::identity_facts::PeopleAuthorityReadPort;
use crate::domain::identity_facts::{
    EngagementAuthorityWitness, PersonAuthorityFacts, invalid_source,
};

pub struct PgPeopleAuthorityReader<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgPeopleAuthorityReader<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }
    fn transaction(&self) -> Result<(), PgTransactionError<BusinessError>> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("PEOPLE_AUTHORITY_TRANSACTION_REQUIRED").into());
        }
        Ok(())
    }
    async fn complete(
        &mut self,
        row: PgRow,
    ) -> Result<PersonAuthorityFacts, PgTransactionError<BusinessError>> {
        let company: String = row.try_get("company_id")?;
        let person: String = row.try_get("id")?;
        let user: Option<String> = row.try_get("user_id")?;
        let merged: Option<String> = row.try_get("merged_into_id")?;
        let engagements = sqlx::query("SELECT id,version,branch_id,department_id FROM hr_engagements WHERE company_id=$1 AND person_id=$2 AND status='active' ORDER BY id")
            .bind(&company).bind(&person).fetch_all(&mut *self.connection).await?;
        let mut witnesses = Vec::with_capacity(engagements.len());
        for row in engagements {
            witnesses.push(EngagementAuthorityWitness {
                id: RecordId::new(row.try_get::<String, _>("id")?)
                    .map_err(|_| PgTransactionError::Business(invalid_source()))?,
                version: row.try_get("version")?,
                branch_id: row
                    .try_get::<Option<String>, _>("branch_id")?
                    .map(RecordId::new)
                    .transpose()
                    .map_err(|_| PgTransactionError::Business(invalid_source()))?,
                department_id: row
                    .try_get::<Option<String>, _>("department_id")?
                    .map(RecordId::new)
                    .transpose()
                    .map_err(|_| PgTransactionError::Business(invalid_source()))?,
            });
        }
        let facts = PersonAuthorityFacts {
            company_id: CompanyId::new(company)
                .map_err(|_| PgTransactionError::Business(invalid_source()))?,
            person_id: RecordId::new(person)
                .map_err(|_| PgTransactionError::Business(invalid_source()))?,
            user_id: user
                .map(UserId::new)
                .transpose()
                .map_err(|_| PgTransactionError::Business(invalid_source()))?,
            status: row.try_get("status")?,
            merged_into_id: merged
                .map(RecordId::new)
                .transpose()
                .map_err(|_| PgTransactionError::Business(invalid_source()))?,
            version: row.try_get("version")?,
            active_engagements: witnesses,
        };
        facts.validate().map_err(PgTransactionError::Business)?;
        Ok(facts)
    }
    pub async fn person_for_user(
        &mut self,
        company: &CompanyId,
        user: &UserId,
    ) -> Result<Option<PersonAuthorityFacts>, PgTransactionError<BusinessError>> {
        self.transaction()?;
        let rows = sqlx::query("SELECT id,company_id,user_id,status,merged_into_id,version FROM hr_people WHERE company_id=$1 AND user_id=$2 AND merged_into_id IS NULL ORDER BY id LIMIT 2")
            .bind(company.as_str()).bind(user.as_str()).fetch_all(&mut *self.connection).await?;
        if rows.len() > 1 {
            return Err(PgTransactionError::Business(invalid_source()));
        }
        match rows.into_iter().next() {
            Some(row) => self.complete(row).await.map(Some),
            None => Ok(None),
        }
    }
    pub async fn person_subject(
        &mut self,
        company: &CompanyId,
        person: &RecordId,
    ) -> Result<Option<PersonAuthorityFacts>, PgTransactionError<BusinessError>> {
        self.transaction()?;
        let row = sqlx::query("SELECT id,company_id,user_id,status,merged_into_id,version FROM hr_people WHERE company_id=$1 AND id=$2")
            .bind(company.as_str()).bind(person.as_str()).fetch_optional(&mut *self.connection).await?;
        match row {
            Some(row) => self.complete(row).await.map(Some),
            None => Ok(None),
        }
    }
}

impl PeopleAuthorityReadPort for PgPeopleAuthorityReader<'_> {
    type Error = PgTransactionError<BusinessError>;
    async fn person_for_user(
        &mut self,
        company: &CompanyId,
        user: &UserId,
    ) -> Result<Option<PersonAuthorityFacts>, Self::Error> {
        PgPeopleAuthorityReader::person_for_user(self, company, user).await
    }
    async fn person_subject(
        &mut self,
        company: &CompanyId,
        person: &RecordId,
    ) -> Result<Option<PersonAuthorityFacts>, Self::Error> {
        PgPeopleAuthorityReader::person_subject(self, company, person).await
    }
}

/// Before the first authority snapshot, retain the same People source set used by queued commands.
pub async fn lock_people_authority_sources(
    connection: &mut PgConnection,
) -> Result<(), PgTransactionError<BusinessError>> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("PEOPLE_AUTHORITY_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE hr_people,hr_engagements,people_access_cutoffs,people_lifecycle_cases,people_lifecycle_grants,people_package_revisions,people_applied_packages,people_provision_operations,people_provision_grants IN SHARE MODE")
        .execute(connection).await?;
    Ok(())
}

/// Verify the nine People owner fences on this connection without acquiring
/// them. The coordinator must ensure they preceded its first data snapshot.
pub async fn require_people_authority_sources_held(
    connection: &mut PgConnection,
) -> Result<(), PgTransactionError<BusinessError>> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("PEOPLE_AUTHORITY_TRANSACTION_REQUIRED").into());
    }
    let held:bool=sqlx::query_scalar("SELECT count(DISTINCT relation)=9 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND relation IN ('hr_people'::regclass,'hr_engagements'::regclass,'people_access_cutoffs'::regclass,'people_lifecycle_cases'::regclass,'people_lifecycle_grants'::regclass,'people_package_revisions'::regclass,'people_applied_packages'::regclass,'people_provision_operations'::regclass,'people_provision_grants'::regclass) AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')").fetch_one(connection).await?;
    if !held {return Err(PersistenceError::new("PEOPLE_AUTHORITY_LOCKS_REQUIRED").into());}
    Ok(())
}

/// People-owned canonical sources used by IAM Sales lifecycle decisions.
/// This grants no actor authority, does not prove pre-snapshot lock timing,
/// and does not fence concurrent DDL or attest migration provenance.
pub struct CurrentPeopleSalesAuthorityBoundary {
    unit: (String,i32,String,String),
    boundary: (String,String,Vec<i64>,String),
}
impl CurrentPeopleSalesAuthorityBoundary {
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),PgTransactionError<BusinessError>> {
        if people_sales_unit(c).await? != self.unit || people_sales_boundary(c).await? != self.boundary {
            return Err(PersistenceError::new("PEOPLE_SALES_AUTHORITY_SOURCE_CHANGED").into());
        }
        Ok(())
    }
}
/// Coordinator sets canonical search_path and takes all owner locks before the
/// first snapshot. This only verifies present People locks and source identity.
pub async fn inspect_people_sales_authority_boundary(c:&mut PgConnection)->Result<CurrentPeopleSalesAuthorityBoundary,PgTransactionError<BusinessError>> {
    let unit=people_sales_unit(c).await?;
    let boundary=people_sales_boundary(c).await?;
    Ok(CurrentPeopleSalesAuthorityBoundary{unit,boundary})
}
async fn people_sales_unit(c:&mut PgConnection)->Result<(String,i32,String,String),PgTransactionError<BusinessError>> {
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" {
        return Err(PersistenceError::new("PEOPLE_AUTHORITY_TRANSACTION_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn people_sales_boundary(c:&mut PgConnection)->Result<(String,String,Vec<i64>,String),PgTransactionError<BusinessError>> {
    let (valid,role,session,oids,functions):(bool,String,String,Vec<i64>,String)=sqlx::query_as(r#"
      WITH names(name) AS (SELECT unnest(ARRAY['hr_people','hr_engagements','people_access_cutoffs','people_lifecycle_cases','people_lifecycle_grants','people_package_revisions','people_applied_packages','people_provision_operations','people_provision_grants']::text[])),
      rels AS (SELECT n.name,to_regclass('public.'||n.name) oid FROM names n),
      sigs(sig) AS (SELECT unnest(ARRAY['people_person_user(text,text,text)','people_lifecycle_grant_active(text,bigint)','people_case_active(text,bigint)','people_legacy_case_active(text,bigint)','people_application_active(text,bigint)','people_provision_grant_active(text,bigint)']::text[])),
      funcs AS (SELECT s.sig,to_regprocedure('public.'||s.sig) oid,to_regprocedure(s.sig) resolved FROM sigs s)
      SELECT current_setting('search_path')='pg_catalog, public, pg_temp'
       AND (SELECT count(*)=9 AND bool_and(c.oid IS NOT NULL AND c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND NOT row_security_active(c.oid) AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))) FROM rels r LEFT JOIN pg_class c ON c.oid=r.oid)
       AND (SELECT count(*)=6 AND bool_and(p.oid IS NOT NULL AND f.oid=f.resolved AND p.pronamespace='public'::regnamespace AND NOT p.prosecdef AND coalesce(array_length(p.proconfig,1),0)=0 AND has_function_privilege(p.oid,'EXECUTE')) FROM funcs f LEFT JOIN pg_proc p ON p.oid=f.oid),
       current_user::text,session_user::text,(SELECT array_agg(r.oid::oid::bigint ORDER BY r.name) FROM rels r),
       (SELECT coalesce(jsonb_agg(jsonb_build_array(f.sig,p.oid::bigint,p.proowner::bigint,p.provolatile,p.prosecdef,p.proconfig,md5(pg_get_functiondef(p.oid))) ORDER BY f.sig),'[]'::jsonb)::text FROM funcs f LEFT JOIN pg_proc p ON p.oid=f.oid)
    "#).fetch_one(c).await?;
    if !valid {return Err(PersistenceError::new("PEOPLE_SALES_AUTHORITY_SOURCE_REQUIRED").into());}
    Ok((role,session,oids,functions))
}
