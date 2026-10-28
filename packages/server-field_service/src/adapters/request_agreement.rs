//! Service-owned original Agreement source for request creation. This data
//! proves no actor, Customer, Asset, purchase, or technician authority.
use erpx_server_identity_access::public::{
    effective_authority::AuthorizedWorkContext, work_context::WorkScopeKind,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceRequestAgreementFacts {
    pub id: String,
    pub company_id: CompanyId,
    pub branch_id: RecordId,
    pub department_id: Option<RecordId>,
    pub customer_id: RecordId,
    pub asset_id: Option<RecordId>,
    pub version: i64,
}
pub struct CurrentServiceRequestAgreement {
    facts: ServiceRequestAgreementFacts,
    image: String,
    unit: Unit,
}
impl CurrentServiceRequestAgreement {
    pub fn facts(&self) -> &ServiceRequestAgreementFacts {
        &self.facts
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
    ) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let fresh = capture(c, current, &self.facts.id)
            .await?
            .ok_or_else(changed)?;
        if fresh.0 != self.facts || fresh.1 != self.image {
            return Err(changed());
        }
        Ok(())
    }
    pub async fn require_new_work(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let active:bool=sqlx::query_scalar("SELECT status='active' AND start_date<=to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD') AND end_date>=to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD') FROM field_service_agreements WHERE id=$1 AND company_id=$2 FOR SHARE")
            .bind(&self.facts.id).bind(self.facts.company_id.as_str()).fetch_one(c).await?;
        if !active {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_service_request_agreement(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    id: &str,
) -> Result<Option<CurrentServiceRequestAgreement>, Error> {
    current.require_permission("records.write")?;
    let unit = unit(c).await?;
    Ok(capture(c, current, id)
        .await?
        .map(|(facts, image)| CurrentServiceRequestAgreement { facts, image, unit }))
}
pub async fn read_service_request_agreement_history(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    id: &str,
) -> Result<Option<CurrentServiceRequestAgreement>, Error> {
    current.require_permission("records.read")?;
    let unit = unit(c).await?;
    Ok(capture(c, current, id)
        .await?
        .map(|(facts, image)| CurrentServiceRequestAgreement { facts, image, unit }))
}
async fn capture(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    id: &str,
) -> Result<Option<(ServiceRequestAgreementFacts, String)>, Error> {
    let scope = current.selection().workspace();
    let assignment = current.selection().assignment();
    // The Agreement ID remains a raw physical identity, never selected or
    // length-capped here. Each transferred reference has the existing RecordId
    // limit; the aggregate bounds the entire selected reference projection.
    let row=sqlx::query("SELECT b.bounded,CASE WHEN b.bounded THEN g.branch_id END branch_id,CASE WHEN b.bounded THEN g.department_id END department_id,CASE WHEN b.bounded THEN g.customer_id END customer_id,CASE WHEN b.bounded THEN g.asset_id END asset_id,g.version::bigint version,encode(sha256(convert_to(to_jsonb(g)::text,'UTF8')),'hex') image FROM field_service_agreements g CROSS JOIN LATERAL (SELECT octet_length(g.branch_id)<=200 AND coalesce(octet_length(g.department_id),0)<=200 AND octet_length(g.customer_id)<=200 AND coalesce(octet_length(g.asset_id),0)<=200 AND octet_length(g.branch_id)::bigint+coalesce(octet_length(g.department_id),0)::bigint+octet_length(g.customer_id)::bigint+coalesce(octet_length(g.asset_id),0)::bigint<=800 AS bounded) b WHERE g.id=$1 AND g.company_id=$2 AND ($3::text IS NULL OR g.branch_id=$3) AND ($4::text IS NULL OR g.department_id=$4) FOR SHARE OF g")
        .bind(id).bind(scope.company_id.as_str()).bind(scope.branch_id.as_ref().map(RecordId::as_str)).bind(scope.department_id.as_ref().map(RecordId::as_str)).fetch_optional(c).await?;
    let Some(row) = row else { return Ok(None) };
    if !row.try_get::<bool, _>("bounded")? {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "FIELD_SERVICE_REQUEST_SOURCE_LIMIT",
            "The Service Agreement reference exceeds this bounded read.",
        )
        .into());
    }
    let branch = RecordId::new(
        row.try_get::<Option<String>, _>("branch_id")?
            .ok_or_else(changed)?,
    )?;
    let department = row
        .try_get::<Option<String>, _>("department_id")?
        .map(RecordId::new)
        .transpose()?;
    let allowed = match assignment.scope_kind {
        WorkScopeKind::Holding => true,
        WorkScopeKind::Branch => assignment
            .scope_node_id
            .as_ref()
            .is_some_and(|n| n.as_str() == branch.as_str()),
        WorkScopeKind::Department => assignment.scope_node_id.as_ref().is_some_and(|n| {
            department
                .as_ref()
                .is_some_and(|d| n.as_str() == d.as_str())
        }),
    };
    if !allowed {
        return Ok(None);
    }
    let facts = ServiceRequestAgreementFacts {
        id: id.to_owned(),
        company_id: scope.company_id.clone(),
        branch_id: branch,
        department_id: department,
        customer_id: RecordId::new(
            row.try_get::<Option<String>, _>("customer_id")?
                .ok_or_else(changed)?,
        )?,
        asset_id: row
            .try_get::<Option<String>, _>("asset_id")?
            .map(RecordId::new)
            .transpose()?,
        version: row.try_get("version")?,
    };
    if !(1..=9_007_199_254_740_991).contains(&facts.version) {
        return Err(changed());
    }
    Ok(Some((facts, row.try_get("image")?)))
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("FIELD_SERVICE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_SERVICE_AGREEMENT_CHANGED",
        "The original Agreement source changed.",
    )
    .into()
}
