//! Support-owned nullable ticket facts for the Field source-record predicate.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde::Deserialize;
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct FieldSourceTicket {
    pub id: String,
    pub customer_id: Option<String>,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SOURCE_RECORD_CHANGED",
        "The source record changed.",
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
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn image(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<(FieldSourceTicket, String)>, Error> {
    let row=sqlx::query("WITH s AS MATERIALIZED (SELECT * FROM support_tickets WHERE id=$1 AND company_id=$2), v AS (SELECT jsonb_build_object('id',id,'customer_id',customer_id,'branch_id',branch_id,'department_id',department_id) payload,to_jsonb(s) image FROM s) SELECT CASE WHEN octet_length(payload::text)+octet_length(image::text)<=2097152 THEN payload::text END payload,encode(sha256(convert_to(image::text,'UTF8')),'hex') digest FROM v").bind(id).bind(company.as_str()).fetch_optional(c).await?;
    match row {
        None => Ok(None),
        Some(r) => {
            let raw: Option<String> = r.try_get("payload")?;
            Ok(Some((
                serde_json::from_str(&raw.ok_or_else(changed)?).map_err(|_| changed())?,
                r.try_get("digest")?,
            )))
        }
    }
}
pub struct CurrentFieldSourceTicket {
    company: CompanyId,
    id: String,
    unit: Unit,
    image: Option<(FieldSourceTicket, String)>,
}
impl CurrentFieldSourceTicket {
    pub fn data(&self) -> Option<&FieldSourceTicket> {
        self.image.as_ref().map(|v| &v.0)
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || image(c, &self.company, &self.id).await? != self.image {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_field_source_ticket(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<CurrentFieldSourceTicket, Error> {
    let unit = unit(c).await?;
    let image = image(c, company, id).await?;
    Ok(CurrentFieldSourceTicket {
        company: company.clone(),
        id: id.into(),
        unit,
        image,
    })
}
