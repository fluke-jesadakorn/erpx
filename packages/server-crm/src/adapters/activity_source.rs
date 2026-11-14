//! CRM-owned identity partition of the activity purchase source. Composition
//! must independently retain Finance and opportunity sources and authorize an
//! opaque current actor; this witness grants no permission or entitlement.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

#[derive(Clone, Copy)]
pub enum CrmActivitySourceKind {
    Lead,
    Activity,
}

pub struct CrmActivitySourceRequest {
    pub company_id: CompanyId,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub source_id: String,
    pub kind: CrmActivitySourceKind,
}

pub struct CurrentCrmActivitySource {
    request: CrmActivitySourceRequest,
    image: Value,
    finance_party: Option<String>,
    has_opportunity: bool,
    unit: Unit,
    boundary: ActivitySourceBoundary,
}
impl CurrentCrmActivitySource {
    pub fn finance_party(&self) -> Option<&str> {
        self.finance_party.as_deref()
    }
    pub fn has_opportunity(&self) -> bool {
        self.has_opportunity
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || activity_source_boundary(c).await? != self.boundary
            || read(c, &self.request).await?.as_ref()
                != Some(&(
                    self.image.clone(),
                    self.finance_party.clone(),
                    self.has_opportunity,
                ))
        {
            return Err(changed());
        }
        Ok(())
    }
}

/// Prepare before any source snapshot, alongside Identity, Commercial and
/// foreign-owner source locks. This function acquires no snapshot itself.
pub async fn lock_crm_activity_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE public.crm_accounts,public.crm_contacts,public.crm_guest_links,public.crm_guest_sharing_events,public.crm_leads,public.crm_opportunities,public.crm_case_events,public.crm_lead_activities,public.crm_lead_activity_events IN SHARE ROW EXCLUSIVE MODE").execute(c).await?;
    Ok(())
}

pub async fn inspect_crm_activity_source(
    c: &mut PgConnection,
    request: CrmActivitySourceRequest,
) -> Result<CurrentCrmActivitySource, Error> {
    let unit = unit(c).await?;
    let boundary = activity_source_boundary(c).await?;
    let (image, finance_party, has_opportunity) = read(c, &request).await?.ok_or_else(not_found)?;
    Ok(CurrentCrmActivitySource {
        request,
        image,
        finance_party,
        has_opportunity,
        unit,
        boundary,
    })
}

type ActivitySourceBoundary = (String, String, String, Vec<i64>);
async fn activity_source_boundary(c: &mut PgConnection) -> Result<ActivitySourceBoundary, Error> {
    let (held, visible, role, session, path, oids): (bool, bool, String, String, String, Vec<i64>) = sqlx::query_as("SELECT (SELECT count(*)=5 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))) FROM pg_class c WHERE c.oid IN ('public.crm_leads'::regclass,'public.crm_accounts'::regclass,'public.crm_contacts'::regclass,'public.crm_lead_activities'::regclass,'public.crm_opportunities'::regclass)), NOT (row_security_active('public.crm_leads'::regclass) OR row_security_active('public.crm_accounts'::regclass) OR row_security_active('public.crm_contacts'::regclass) OR row_security_active('public.crm_lead_activities'::regclass) OR row_security_active('public.crm_opportunities'::regclass)), current_user::text, session_user::text, current_setting('search_path'), ARRAY['public.crm_leads'::regclass::oid::bigint,'public.crm_accounts'::regclass::oid::bigint,'public.crm_contacts'::regclass::oid::bigint,'public.crm_lead_activities'::regclass::oid::bigint,'public.crm_opportunities'::regclass::oid::bigint]").fetch_one(c).await?;
    if !held || !visible { return Err(invalid()); }
    Ok((role,session,path,oids))
}

/// Actual CRM lead-owner identity used by the legacy activity recipient guard.
/// This witness is neither an actor grant nor proof of Sales eligibility.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrmActivityRecipientFacts {
    pub activity_id: String,
    pub company_id: String,
    pub lead_id: String,
    pub lead_owner_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub activity_owner_id: String,
    pub activity_version: i64,
    pub activity_status: String,
}

pub struct CurrentCrmActivityRecipientSource {
    request: CrmActivitySourceRequest,
    facts: CrmActivityRecipientFacts,
    unit: Unit,
    boundary: RecipientBoundary,
}
impl CurrentCrmActivityRecipientSource {
    pub fn facts(&self) -> &CrmActivityRecipientFacts { &self.facts }
    /// Fixed scoped source for composing the original recipient predicate.
    /// Call `bind_source` on the query so its first four parameters come from
    /// this retained witness, then require_current before and after evaluation.
    pub fn cte_sql(&self) -> &'static str {
        "crm_activity_recipient_source AS (SELECT x.id activity_id,l.id lead_id,l.company_id,l.owner_id,l.branch_id,l.department_id FROM public.crm_lead_activities x JOIN public.crm_leads l ON l.id=x.lead_id AND l.company_id=x.company_id WHERE x.id=$1 AND x.company_id=$2 AND ($3::text IS NULL OR l.branch_id=$3) AND ($4::text IS NULL OR l.department_id=$4))"
    }
    pub fn bind_source<'q>(&'q self, query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
        query.bind(&self.request.source_id).bind(self.request.company_id.as_str()).bind(self.request.branch_id.as_deref()).bind(self.request.department_id.as_deref())
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || recipient_boundary(c).await? != self.boundary || read_recipient(c, &self.request).await?.as_ref() != Some(&self.facts) {
            return Err(changed());
        }
        Ok(())
    }
}

/// CRM locks and Sales/Identity/People locks must be prepared before the first
/// snapshot. Compose Sales' request from these facts, not from client labels.
pub async fn inspect_crm_activity_recipient_source(
    c: &mut PgConnection,
    request: CrmActivitySourceRequest,
) -> Result<CurrentCrmActivityRecipientSource, Error> {
    if !matches!(request.kind, CrmActivitySourceKind::Activity) { return Err(invalid()); }
    let unit = unit(c).await?;
    let boundary = recipient_boundary(c).await?;
    let facts = read_recipient(c, &request).await?.ok_or_else(not_found)?;
    Ok(CurrentCrmActivityRecipientSource { request, facts, unit, boundary })
}

type RecipientBoundary = (String, String, String, i64, i64);
async fn recipient_boundary(c: &mut PgConnection) -> Result<RecipientBoundary, Error> {
    let (held, visible, role, session, path, lead_oid, activity_oid): (bool, bool, String, String, String, i64, i64) = sqlx::query_as("SELECT (SELECT count(*)=2 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))) FROM pg_class c WHERE c.oid IN ('public.crm_leads'::regclass,'public.crm_lead_activities'::regclass)), NOT (row_security_active('public.crm_leads'::regclass) OR row_security_active('public.crm_lead_activities'::regclass)),current_user::text,session_user::text,current_setting('search_path'),'public.crm_leads'::regclass::oid::bigint,'public.crm_lead_activities'::regclass::oid::bigint").fetch_one(c).await?;
    if !held || !visible { return Err(invalid()); }
    Ok((role, session, path, lead_oid, activity_oid))
}

async fn read_recipient(c: &mut PgConnection, r: &CrmActivitySourceRequest) -> Result<Option<CrmActivityRecipientFacts>, Error> {
    let row = sqlx::query("SELECT x.id activity_id,x.company_id,l.id lead_id,l.owner_id lead_owner_id,l.branch_id,l.department_id,x.owner_id activity_owner_id,x.version activity_version,x.status activity_status FROM public.crm_lead_activities x JOIN public.crm_leads l ON l.id=x.lead_id AND l.company_id=x.company_id WHERE x.id=$1 AND x.company_id=$2 AND ($3::text IS NULL OR l.branch_id=$3) AND ($4::text IS NULL OR l.department_id=$4)")
        .bind(&r.source_id).bind(r.company_id.as_str()).bind(r.branch_id.as_deref()).bind(r.department_id.as_deref()).fetch_optional(c).await?;
    row.map(|row| Ok(CrmActivityRecipientFacts {
        activity_id:row.try_get("activity_id")?, company_id:row.try_get("company_id")?, lead_id:row.try_get("lead_id")?, lead_owner_id:row.try_get("lead_owner_id")?, branch_id:row.try_get("branch_id")?, department_id:row.try_get("department_id")?, activity_owner_id:row.try_get("activity_owner_id")?, activity_version:row.try_get("activity_version")?, activity_status:row.try_get("activity_status")?,
    })).transpose()
}

/// Reviewed CRM account link only: this is neither Finance customer status nor
/// selected-actor authority. Composition must prepare all owner locks before
/// taking its first snapshot and retain the matching Finance witness as well.
/// Lock presence cannot prove that the locks preceded that first snapshot.
pub struct CrmReviewedAccountLinkRequest {
    pub company_id: CompanyId,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub lead_id: String,
    pub finance_party_id: String,
}

pub struct CurrentCrmReviewedAccountLink {
    request: CrmReviewedAccountLinkRequest,
    image: String,
    unit: Unit,
    boundary: ReviewedBoundary,
}

impl CurrentCrmReviewedAccountLink {
    pub(crate) fn matches(&self, company: &str, lead: &str, party: &str) -> bool {
        self.request.company_id.as_str() == company && self.request.lead_id == lead
            && self.request.finance_party_id == party
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || reviewed_account_boundary(c).await? != self.boundary
            || reviewed_account_link_image(c, &self.request).await?.as_deref()
                != Some(self.image.as_str())
        {
            return Err(changed());
        }
        Ok(())
    }
}

/// Acquire before any source snapshot; does not grant permission.
pub async fn lock_crm_reviewed_account_link_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE public.crm_accounts,public.crm_leads IN SHARE ROW EXCLUSIVE MODE")
        .execute(c).await?;
    Ok(())
}

pub async fn inspect_crm_reviewed_account_link(
    c: &mut PgConnection,
    request: CrmReviewedAccountLinkRequest,
) -> Result<CurrentCrmReviewedAccountLink, Error> {
    let unit = unit(c).await?;
    let boundary = reviewed_account_boundary(c).await?;
    let image = reviewed_account_link_image(c, &request).await?.ok_or_else(not_found)?;
    Ok(CurrentCrmReviewedAccountLink { request, image, unit, boundary })
}

type ReviewedBoundary = (String, String, String, Vec<i64>);
async fn reviewed_account_boundary(c: &mut PgConnection) -> Result<ReviewedBoundary, Error> {
    let (held, visible, role, session, path, oids): (bool, bool, String, String, String, Vec<i64>) = sqlx::query_as("SELECT (SELECT count(*)=2 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))) FROM pg_class c WHERE c.oid IN ('public.crm_accounts'::regclass,'public.crm_leads'::regclass)), NOT (row_security_active('public.crm_accounts'::regclass) OR row_security_active('public.crm_leads'::regclass)),current_user::text,session_user::text,current_setting('search_path'),ARRAY['public.crm_accounts'::regclass::oid::bigint,'public.crm_leads'::regclass::oid::bigint]").fetch_one(c).await?;
    if !held || !visible {
        return Err(invalid());
    }
    Ok((role, session, path, oids))
}

async fn reviewed_account_link_image(c: &mut PgConnection, r: &CrmReviewedAccountLinkRequest) -> Result<Option<String>, Error> {
    Ok(sqlx::query_scalar("SELECT encode(sha256(convert_to(jsonb_build_array(l.id,l.company_id,l.branch_id,l.department_id,l.account_id,a.id,a.company_id,a.finance_party_id,a.reviewed_by)::text,'UTF8')),'hex') FROM public.crm_leads l JOIN public.crm_accounts a ON a.id=l.account_id AND a.company_id=l.company_id WHERE l.id=$1 AND l.company_id=$2 AND ($3::text IS NULL OR l.branch_id=$3) AND ($4::text IS NULL OR l.department_id=$4) AND a.finance_party_id=$5 AND a.reviewed_by IS NOT NULL")
        .bind(&r.lead_id).bind(r.company_id.as_str()).bind(r.branch_id.as_deref()).bind(r.department_id.as_deref()).bind(&r.finance_party_id).fetch_optional(c).await?)
}

async fn read(
    c: &mut PgConnection,
    r: &CrmActivitySourceRequest,
) -> Result<Option<(Value, Option<String>, bool)>, Error> {
    let activity = matches!(r.kind, CrmActivitySourceKind::Activity);
    let row = sqlx::query("SELECT jsonb_build_object('lead',jsonb_build_array(l.id,l.company_id,l.branch_id,l.department_id,l.account_id,l.contact_id,l.created_by),'account',jsonb_build_array(a.id,a.company_id,a.kind,a.created_by),'contact',jsonb_build_array(ct.id,ct.company_id,ct.account_id,ct.created_by),'activity',CASE WHEN $5 THEN jsonb_build_array(x.id,x.company_id,x.lead_id,x.created_by) ELSE NULL END) image,a.finance_party_id,EXISTS(SELECT 1 FROM public.crm_opportunities o WHERE o.lead_id=l.id) has_opportunity FROM public.crm_leads l JOIN public.crm_accounts a ON a.id=l.account_id AND a.company_id=l.company_id JOIN public.crm_contacts ct ON ct.id=l.contact_id AND ct.account_id=a.id AND ct.company_id=l.company_id LEFT JOIN public.crm_lead_activities x ON x.lead_id=l.id AND x.company_id=l.company_id AND x.id=$1 WHERE (CASE WHEN $5 THEN x.id ELSE l.id END)=$1 AND l.company_id=$2 AND ($3::text IS NULL OR l.branch_id=$3) AND ($4::text IS NULL OR l.department_id=$4)")
        .bind(&r.source_id).bind(r.company_id.as_str()).bind(r.branch_id.as_deref()).bind(r.department_id.as_deref()).bind(activity).fetch_optional(c).await?;
    row.map(|row| {
        Ok((
            row.try_get("image")?,
            row.try_get("finance_party_id")?,
            row.try_get("has_opportunity")?,
        ))
    })
    .transpose()
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
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn invalid() -> Error {
    PersistenceError::new("CRM_SOURCE_TRANSACTION_REQUIRED").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "CRM_SOURCE_CHANGED",
        "The original CRM identity source changed.",
    )
    .into()
}
fn not_found() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "CRM_SOURCE_NOT_FOUND",
        "The original CRM source is unavailable in this workspace.",
    )
    .into()
}
