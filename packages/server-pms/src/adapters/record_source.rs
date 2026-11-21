//! PMS owns the union and full source/property snapshots; IAM and active
//! Organization branch proofs are retained by the outer selected transaction.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::record_source_contract::SourceRecord;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
fn missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PMS_SOURCE_NOT_FOUND",
        "This hotel work is unavailable.",
    )
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SOURCE_RECORD_CHANGED",
        "The PMS source changed.",
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
        return Err(PersistenceError::new("PMS_SOURCE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
#[derive(Clone, PartialEq, Eq, serde::Deserialize)]
struct Facts {
    kind: String,
    property_name: String,
    number: Option<String>,
    requested_by: Option<String>,
    recorded_by: Option<String>,
    user_id: Option<String>,
}
async fn capture(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<(SourceRecord, String, Facts)>, Error> {
    let rows = sqlx::query(SQL)
        .bind(id)
        .bind(company.as_str())
        .fetch_all(c)
        .await?;
    if rows.len() > 1 {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "PMS_SOURCE_AMBIGUOUS",
            "The PMS source identity is ambiguous.",
        )
        .into());
    }
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    let raw: Option<String> = row.try_get("payload")?;
    let mut source: SourceRecord = serde_json::from_str(&raw.ok_or_else(|| {
        BusinessError::new(
            FailureKind::Unavailable,
            "PMS_SOURCE_LIMIT",
            "The PMS source exceeds its bounded read.",
        )
    })?)
    .map_err(|_| changed())?;
    let facts: Facts =
        serde_json::from_str(&row.try_get::<String, _>("facts")?).map_err(|_| changed())?;
    source.title = crate::domain::record_source::source_title(
        &facts.property_name,
        &facts.kind,
        facts.number.as_deref(),
    )
    .ok_or_else(missing)?;
    Ok(Some((source, row.try_get("digest")?, facts)))
}
pub struct CurrentRecordSource {
    company: CompanyId,
    id: String,
    unit: Unit,
    image: (SourceRecord, String, Facts),
}
impl CurrentRecordSource {
    pub fn data(&self) -> &SourceRecord {
        &self.image.0
    }
    pub fn require_reader(
        &self,
        user: &str,
        can: impl Fn(&str) -> bool,
    ) -> Result<(), BusinessError> {
        let f = &self.image.2;
        if !crate::domain::record_source::source_allowed(
            &f.kind,
            user,
            f.requested_by.as_deref(),
            f.recorded_by.as_deref(),
            f.user_id.as_deref(),
            can,
        ) {
            return Err(missing());
        }
        Ok(())
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture(c, &self.company, &self.id).await?.as_ref() != Some(&self.image)
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_record_source(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<CurrentRecordSource>, Error> {
    let unit = unit(c).await?;
    Ok(capture(c, company, id)
        .await?
        .map(|image| CurrentRecordSource {
            company: company.clone(),
            id: id.into(),
            unit,
            image,
        }))
}
pub async fn lock_record_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("PMS_SOURCE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE pms_records,pms_properties,pms_transfer_requests,pms_charge_corrections IN SHARE MODE").execute(c).await?;
    Ok(())
}
const SQL: &str = "WITH source AS MATERIALIZED(SELECT r.id,r.version,p.company_id,p.branch_id,p.name property_name,r.kind,r.data_json::jsonb data,jsonb_build_object('record',to_jsonb(r),'property',to_jsonb(p)) image FROM pms_records r JOIN pms_properties p ON p.id=r.property_id WHERE r.id=$1 AND p.company_id=$2\nUNION ALL SELECT t.id,t.version,p.company_id,p.branch_id,p.name,'transfer',jsonb_build_object('state',t.state,'recordedBy',t.recorded_by,'guestAccountId',t.guest_account_id),jsonb_build_object('record',to_jsonb(t),'property',to_jsonb(p)) FROM pms_transfer_requests t JOIN pms_properties p ON p.id=t.property_id AND p.company_id=t.company_id WHERE t.id=$1 AND p.company_id=$2\nUNION ALL SELECT r.id,r.version,p.company_id,p.branch_id,p.name,'correction',jsonb_build_object('state',r.status,'requestedBy',r.requested_by,'guestAccountId',r.guest_account_id),jsonb_build_object('record',to_jsonb(r),'property',to_jsonb(p)) FROM pms_charge_corrections r JOIN pms_properties p ON p.id=r.property_id AND p.company_id=r.company_id WHERE r.id=$1 AND p.company_id=$2), payload AS(SELECT jsonb_build_object('kind','pms_record','id',id,'company_id',company_id,'branch_id',branch_id,'department_id',NULL,'version',version,'title',property_name,'status',coalesce(nullif(data->'state','null'::jsonb),'\"open\"'::jsonb))::text value,jsonb_build_object('kind',kind,'property_name',property_name,'number',data->'number','requested_by',data->'requestedBy','recorded_by',data->'recordedBy','user_id',data->'userId')::text facts,image::text image FROM source) SELECT CASE WHEN octet_length(value)+octet_length(facts)<=2097152 AND octet_length(image)<=2097152 THEN value END payload,facts,encode(sha256(convert_to(image,'UTF8')),'hex') digest FROM payload";
