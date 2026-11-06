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
    sqlx::query("LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE ROW EXCLUSIVE MODE").execute(c).await?;
    Ok(())
}

pub async fn inspect_crm_activity_source(
    c: &mut PgConnection,
    request: CrmActivitySourceRequest,
) -> Result<CurrentCrmActivitySource, Error> {
    let unit = unit(c).await?;
    let (image, finance_party, has_opportunity) = read(c, &request).await?.ok_or_else(not_found)?;
    Ok(CurrentCrmActivitySource {
        request,
        image,
        finance_party,
        has_opportunity,
        unit,
    })
}

async fn read(
    c: &mut PgConnection,
    r: &CrmActivitySourceRequest,
) -> Result<Option<(Value, Option<String>, bool)>, Error> {
    let activity = matches!(r.kind, CrmActivitySourceKind::Activity);
    let row = sqlx::query("SELECT jsonb_build_object('lead',jsonb_build_array(l.id,l.company_id,l.branch_id,l.department_id,l.account_id,l.contact_id,l.created_by),'account',jsonb_build_array(a.id,a.company_id,a.kind,a.created_by),'contact',jsonb_build_array(ct.id,ct.company_id,ct.account_id,ct.created_by),'activity',CASE WHEN $5 THEN jsonb_build_array(x.id,x.company_id,x.lead_id,x.created_by) ELSE NULL END) image,a.finance_party_id,EXISTS(SELECT 1 FROM crm_opportunities o WHERE o.lead_id=l.id) has_opportunity FROM crm_leads l JOIN crm_accounts a ON a.id=l.account_id AND a.company_id=l.company_id JOIN crm_contacts ct ON ct.id=l.contact_id AND ct.account_id=a.id AND ct.company_id=l.company_id LEFT JOIN crm_lead_activities x ON x.lead_id=l.id AND x.company_id=l.company_id AND x.id=$1 WHERE (CASE WHEN $5 THEN x.id ELSE l.id END)=$1 AND l.company_id=$2 AND ($3::text IS NULL OR l.branch_id=$3) AND ($4::text IS NULL OR l.department_id=$4)")
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
