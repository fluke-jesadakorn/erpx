//! Agreement-owned review source and immutable after-image in one transaction.
use crate::adapters::parts_wire::NormalizedAgreementReview;
use erpx_server_identity_access::public::{
    effective_authority::AuthorizedWorkContext, work_context::WorkScopeKind,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, FailureKind, RecordId};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgreementReviewFacts {
    pub id: String,
    pub company_id: CompanyId,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub creator_id: String,
    pub has_asset: bool,
    pub status: String,
    pub version: i64,
}
pub struct CurrentAgreementReview {
    facts: AgreementReviewFacts,
    image: String,
    unit: Unit,
}
impl CurrentAgreementReview {
    pub fn facts(&self) -> &AgreementReviewFacts {
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
}
pub async fn read_agreement_review(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    id: &str,
) -> Result<Option<CurrentAgreementReview>, Error> {
    current.require_permission("records.write")?;
    let unit = unit(c).await?;
    Ok(capture(c, current, id)
        .await?
        .map(|(facts, image)| CurrentAgreementReview { facts, image, unit }))
}
async fn capture(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    id: &str,
) -> Result<Option<(AgreementReviewFacts, String)>, Error> {
    let scope = current.selection().workspace();
    let assignment = current.selection().assignment();
    let row=sqlx::query("SELECT b.bounded,CASE WHEN b.bounded THEN g.branch_id END branch_id,CASE WHEN b.bounded THEN g.department_id END department_id,CASE WHEN b.bounded THEN g.created_by END created_by,CASE WHEN b.bounded THEN g.status END status,g.asset_id IS NOT NULL AND g.asset_id<>'' has_asset,g.version::bigint version,encode(sha256(convert_to(to_jsonb(g)::text,'UTF8')),'hex') image FROM field_service_agreements g CROSS JOIN LATERAL (SELECT octet_length(g.branch_id)::bigint+coalesce(octet_length(g.department_id),0)::bigint<=2097152 AND octet_length(g.created_by)<=200 AND octet_length(g.status)<=32 AS bounded) b WHERE g.id=$1 AND g.company_id=$2 AND ($3::text IS NULL OR g.branch_id=$3) AND ($4::text IS NULL OR g.department_id=$4) FOR SHARE OF g")
        .bind(id).bind(scope.company_id.as_str()).bind(scope.branch_id.as_ref().map(RecordId::as_str)).bind(scope.department_id.as_ref().map(RecordId::as_str)).fetch_optional(c).await?;
    let Some(row) = row else { return Ok(None) };
    if !row.try_get::<bool, _>("bounded")? {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "FIELD_SERVICE_REVIEW_SOURCE_LIMIT",
            "The Service Agreement reference exceeds this bounded read.",
        )
        .into());
    }
    let branch: String = row
        .try_get::<Option<String>, _>("branch_id")?
        .ok_or_else(changed)?;
    let department: Option<String> = row.try_get("department_id")?;
    let allowed = match assignment.scope_kind {
        WorkScopeKind::Holding => true,
        WorkScopeKind::Branch => assignment
            .scope_node_id
            .as_ref()
            .is_some_and(|n| n.as_str() == branch),
        WorkScopeKind::Department => assignment
            .scope_node_id
            .as_ref()
            .is_some_and(|n| department.as_ref().is_some_and(|d| n.as_str() == d)),
    };
    if !allowed {
        return Ok(None);
    }
    let facts = AgreementReviewFacts {
        id: id.to_owned(),
        company_id: scope.company_id.clone(),
        branch_id: branch,
        department_id: department,
        creator_id: row
            .try_get::<Option<String>, _>("created_by")?
            .ok_or_else(changed)?,
        has_asset: row.try_get("has_asset")?,
        status: row
            .try_get::<Option<String>, _>("status")?
            .ok_or_else(changed)?,
        version: row.try_get("version")?,
    };
    if !(1..=9_007_199_254_740_991).contains(&facts.version) {
        return Err(changed());
    }
    Ok(Some((facts, row.try_get("image")?)))
}
pub struct PreparedAgreementReview {
    unit: Unit,
    facts: AgreementReviewFacts,
    before: String,
    expected: String,
    event: String,
    input: NormalizedAgreementReview,
    key: CommandKey,
    actor: String,
    at: i64,
    version: i64,
}
impl PreparedAgreementReview {
    pub async fn prepare(
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        source: &CurrentAgreementReview,
        input: NormalizedAgreementReview,
        key: CommandKey,
        at: i64,
    ) -> Result<Self, Error> {
        source.require_current(c, current).await?;
        if source.facts.status != input.action.expected_status()
            || source.facts.version != input.expected_version
        {
            return Err(changed());
        }
        let version = input
            .expected_version
            .checked_add(1)
            .filter(|v| *v <= 9_007_199_254_740_991)
            .ok_or_else(changed)?;
        let actor = current.user_id().as_str().to_owned();
        let expected:String=sqlx::query_scalar("SELECT encode(sha256(convert_to((to_jsonb(g)||jsonb_build_object('status',$3::text,'reviewed_by',$4::text,'reviewed_at',$5::bigint,'version',$6::bigint,'updated_at',$5::bigint))::text,'UTF8')),'hex') FROM field_service_agreements g WHERE id=$1 AND company_id=$2")
            .bind(&source.facts.id).bind(source.facts.company_id.as_str()).bind(input.action.status()).bind(&actor).bind(at).bind(version).fetch_one(&mut *c).await?;
        let event:String=sqlx::query_scalar("SELECT encode(sha256(convert_to(jsonb_build_object('id',$1::text,'agreement_id',$2::text,'version',$3::bigint,'action',$4::text,'actor_id',$5::text,'reason',$6::text,'status',$7::text,'created_at',$8::bigint)::text,'UTF8')),'hex')")
            .bind(format!("service-agreement-event-{}",key.as_str())).bind(&source.facts.id).bind(version).bind(input.action.operation().strip_prefix("field.agreement.").ok_or_else(changed)?).bind(&actor).bind(&input.reason).bind(input.action.status()).bind(at).fetch_one(c).await?;
        Ok(Self {
            unit: source.unit.clone(),
            facts: source.facts.clone(),
            before: source.image.clone(),
            expected,
            event,
            input,
            key,
            actor,
            at,
            version,
        })
    }
    pub fn version(&self) -> i64 {
        self.version
    }
    pub fn status(&self) -> &str {
        self.input.action.status()
    }
    pub async fn require_before(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        source: &CurrentAgreementReview,
    ) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || source.facts != self.facts
            || source.image != self.before
            || current.user_id().as_str() != self.actor
        {
            return Err(changed());
        }
        source.require_current(c, current).await?;
        if self.input.action == crate::domain::agreement_review::AgreementReviewAction::Retire {
            let open:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM field_service_orders WHERE agreement_id=$1 AND status NOT IN ('cancelled','billed'))").bind(&self.facts.id).fetch_one(c).await?;
            if open {
                return Err(changed());
            }
        }
        Ok(())
    }
    pub async fn apply(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        source: &CurrentAgreementReview,
    ) -> Result<(), Error> {
        self.require_before(c, current, source).await?;
        let n=sqlx::query("UPDATE field_service_agreements SET status=$3,reviewed_by=$4,reviewed_at=$5,version=version+1,updated_at=$5 WHERE id=$1 AND company_id=$2 AND version=$6 AND status=$7")
            .bind(&self.facts.id).bind(self.facts.company_id.as_str()).bind(self.status()).bind(&self.actor).bind(self.at).bind(self.input.expected_version).bind(self.input.action.expected_status()).execute(&mut *c).await?.rows_affected();
        if n != 1 {
            return Err(changed());
        }
        sqlx::query("INSERT INTO field_service_agreement_events(id,agreement_id,version,action,actor_id,reason,status,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(format!("service-agreement-event-{}",self.key.as_str())).bind(&self.facts.id).bind(self.version).bind(self.input.action.operation().strip_prefix("field.agreement.").ok_or_else(changed)?).bind(&self.actor).bind(&self.input.reason).bind(self.status()).bind(self.at).execute(&mut *c).await?;
        self.require_after(c, current).await
    }
    pub async fn require_after(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
    ) -> Result<(), Error> {
        if unit(c).await? != self.unit || current.user_id().as_str() != self.actor {
            return Err(changed());
        }
        let image:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(g)::text,'UTF8')),'hex') FROM field_service_agreements g WHERE id=$1 AND company_id=$2").bind(&self.facts.id).bind(self.facts.company_id.as_str()).fetch_optional(&mut *c).await?;
        let event:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(e)::text,'UTF8')),'hex') FROM field_service_agreement_events e WHERE id=$1 AND agreement_id=$2").bind(format!("service-agreement-event-{}",self.key.as_str())).bind(&self.facts.id).fetch_optional(&mut *c).await?;
        if image.as_ref() != Some(&self.expected) || event.as_ref() != Some(&self.event) {
            return Err(changed());
        }
        if self.input.action == crate::domain::agreement_review::AgreementReviewAction::Retire {
            let open: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM field_service_orders WHERE agreement_id=$1 AND status NOT IN ('cancelled','billed'))")
                .bind(&self.facts.id).fetch_one(c).await?;
            if open {
                return Err(changed());
            }
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
