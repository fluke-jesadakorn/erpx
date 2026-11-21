//! Agreement-owned draft and original immutable event in the caller's transaction.
use crate::adapters::parts_wire::NormalizedAgreementCreate;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
pub struct PreparedAgreementCreate {
    unit: Unit,
    company: CompanyId,
    id: String,
    branch: String,
    department: Option<String>,
    actor: String,
    key: CommandKey,
    input: NormalizedAgreementCreate,
    at: i64,
    agreement: String,
    event: String,
}
impl PreparedAgreementCreate {
    pub async fn prepare(
        c: &mut PgConnection,
        company: CompanyId,
        scope: (String, Option<String>),
        actor: String,
        key: CommandKey,
        input: NormalizedAgreementCreate,
        at: i64,
    ) -> Result<Self, Error> {
        let (branch, department) = scope;
        if input.end_date < input.start_date {
            return Err(BusinessError::invalid(
                "FIELD_SERVICE_DATE_RANGE",
                "Agreement end date must be on or after its start date.",
            )
            .into());
        }
        let unit = unit(c).await?;
        let id = format!("service-agreement-{}", key.as_str());
        let agreement:String=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(jsonb_populate_record(NULL::field_service_agreements,jsonb_build_object('id',$1::text,'company_id',$2::text,'branch_id',$3::text,'department_id',$4::text,'customer_id',$5::text,'asset_id',$6::text,'title',$7::text,'start_date',$8::text,'end_date',$9::text,'currency',$10::text,'visit_rate_minor',$11::bigint,'status','draft','created_by',$12::text,'reviewed_by',NULL,'reviewed_at',NULL,'version',1,'created_at',$13::bigint,'updated_at',$13::bigint,'warranty_discount_bps',$14::int)))::text,'UTF8')),'hex')")
            .bind(&id).bind(company.as_str()).bind(&branch).bind(&department).bind(&input.customer_id).bind(&input.asset_id).bind(&input.title).bind(&input.start_date).bind(&input.end_date).bind(&input.currency).bind(input.visit_rate_minor).bind(&actor).bind(at).bind(input.warranty_discount_bps).fetch_one(&mut *c).await?;
        let event:String=sqlx::query_scalar("SELECT encode(sha256(convert_to(jsonb_build_object('id',$1::text,'agreement_id',$2::text,'version',1,'action','create','actor_id',$3::text,'reason','Agreement drafted for independent review','status','draft','created_at',$4::bigint)::text,'UTF8')),'hex')").bind(format!("service-agreement-event-{}",key.as_str())).bind(&id).bind(&actor).bind(at).fetch_one(&mut *c).await?;
        let plan = Self {
            unit,
            company,
            id,
            branch,
            department,
            actor,
            key,
            input,
            at,
            agreement,
            event,
        };
        plan.require_before(c).await?;
        Ok(plan)
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM field_service_agreements WHERE id=$1) OR EXISTS(SELECT 1 FROM field_service_agreement_events WHERE id=$2)").bind(&self.id).bind(format!("service-agreement-event-{}",self.key.as_str())).fetch_one(c).await?;
        if exists {
            return Err(changed());
        }
        Ok(())
    }
    pub async fn apply(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_before(c).await?;
        sqlx::query("INSERT INTO field_service_agreements(id,company_id,branch_id,department_id,customer_id,asset_id,title,start_date,end_date,currency,visit_rate_minor,warranty_discount_bps,created_by,created_at,updated_at)VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$14)")
            .bind(&self.id).bind(self.company.as_str()).bind(&self.branch).bind(&self.department).bind(&self.input.customer_id).bind(&self.input.asset_id).bind(&self.input.title).bind(&self.input.start_date).bind(&self.input.end_date).bind(&self.input.currency).bind(self.input.visit_rate_minor).bind(self.input.warranty_discount_bps).bind(&self.actor).bind(self.at).execute(&mut *c).await?;
        sqlx::query("INSERT INTO field_service_agreement_events(id,agreement_id,version,action,actor_id,reason,status,created_at)VALUES($1,$2,1,'create',$3,'Agreement drafted for independent review','draft',$4)").bind(format!("service-agreement-event-{}",self.key.as_str())).bind(&self.id).bind(&self.actor).bind(self.at).execute(&mut *c).await?;
        self.require_after(c).await
    }
    pub async fn require_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let agreement:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(g)::text,'UTF8')),'hex') FROM field_service_agreements g WHERE id=$1 AND company_id=$2").bind(&self.id).bind(self.company.as_str()).fetch_optional(&mut *c).await?;
        let event:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(e)::text,'UTF8')),'hex') FROM field_service_agreement_events e WHERE id=$1 AND agreement_id=$2").bind(format!("service-agreement-event-{}",self.key.as_str())).bind(&self.id).fetch_optional(c).await?;
        if agreement.as_ref() != Some(&self.agreement) || event.as_ref() != Some(&self.event) {
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
        return Err(PersistenceError::new("FIELD_SERVICE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_SERVICE_AGREEMENT_CHANGED",
        "The Agreement create source changed.",
    )
    .into()
}
