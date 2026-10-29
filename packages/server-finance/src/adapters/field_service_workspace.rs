//! Scoped Service customer display and picker; this supplies no Finance grant.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceCustomerDisplay {
    pub id: String,
    pub name: String,
    pub kind: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceCustomerReference {
    pub id: String,
    pub name: String,
    pub currency: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceWorkspaceCustomers {
    pub display: Vec<ServiceCustomerDisplay>,
    pub picker: Vec<ServiceCustomerReference>,
}
#[derive(Clone, PartialEq, Eq)]
struct Image {
    data: ServiceWorkspaceCustomers,
    digests: Vec<String>,
}
pub struct CurrentServiceWorkspaceCustomers {
    company: CompanyId,
    include_picker: bool,
    ids: Vec<String>,
    unit: Unit,
    image: Image,
}
impl CurrentServiceWorkspaceCustomers {
    pub fn data(&self) -> &ServiceWorkspaceCustomers {
        &self.image.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture(c, &self.company, self.include_picker, &self.ids).await? != self.image
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn lock_service_workspace_customers(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE finance_parties IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_service_workspace_customers(
    c: &mut PgConnection,
    company: &CompanyId,
    include_picker: bool,
    ids: &[String],
) -> Result<CurrentServiceWorkspaceCustomers, Error> {
    let unit = unit(c).await?;
    let image = capture(c, company, include_picker, ids).await?;
    Ok(CurrentServiceWorkspaceCustomers {
        company: company.clone(),
        include_picker,
        ids: ids.to_vec(),
        unit,
        image,
    })
}
async fn capture(
    c: &mut PgConnection,
    company: &CompanyId,
    include_picker: bool,
    ids: &[String],
) -> Result<Image, Error> {
    if ids.len() > 10000
        || ids
            .iter()
            .try_fold(0usize, |n, id| n.checked_add(id.len()))
            .is_none_or(|n| n > 2097152)
    {
        return Err(limited());
    }
    // Measure before transferring any physical identifiers or row values.
    let rows=sqlx::query("WITH source AS MATERIALIZED (SELECT p.*, p.id=ANY($2::text[]) AS display, ($3 AND p.kind='customer' AND p.status='active') AS picker FROM finance_parties p WHERE p.company_id=$1 AND (p.id=ANY($2::text[]) OR ($3 AND p.kind='customer' AND p.status='active'))), measured AS (SELECT *,sum(octet_length(to_jsonb(source)::text)) OVER () bytes,count(*) OVER () count,row_number() OVER () ordinal FROM source) SELECT CASE WHEN bytes<=2097152 AND count<=10000 THEN id END id,CASE WHEN bytes<=2097152 AND count<=10000 THEN name END name,CASE WHEN bytes<=2097152 AND count<=10000 THEN kind END kind,CASE WHEN bytes<=2097152 AND count<=10000 THEN currency END currency,display,picker,encode(sha256(convert_to((to_jsonb(measured)-'bytes'-'count'-'ordinal')::text,'UTF8')),'hex') digest FROM measured WHERE (bytes<=2097152 AND count<=10000) OR ordinal=1 ORDER BY name,id") .bind(company.as_str()).bind(ids).bind(include_picker).fetch_all(c).await?;
    let mut display = Vec::new();
    let mut picker = Vec::new();
    let mut digests = Vec::new();
    for r in rows {
        let id: String = r.try_get::<Option<String>, _>("id")?.ok_or_else(limited)?;
        let name: String = r
            .try_get::<Option<String>, _>("name")?
            .ok_or_else(limited)?;
        let currency: String = r
            .try_get::<Option<String>, _>("currency")?
            .ok_or_else(limited)?;
        if r.try_get("display")? {
            display.push(ServiceCustomerDisplay {
                id: id.clone(),
                name: name.clone(),
                kind: r
                    .try_get::<Option<String>, _>("kind")?
                    .ok_or_else(limited)?,
            });
        }
        if r.try_get("picker")? {
            picker.push(ServiceCustomerReference { id, name, currency });
        }
        digests.push(r.try_get("digest")?);
    }
    Ok(Image {
        data: ServiceWorkspaceCustomers { display, picker },
        digests,
    })
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("FINANCE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_SERVICE_CUSTOMERS_CHANGED",
        "The Service customer source changed.",
    )
    .into()
}
fn limited() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FIELD_SERVICE_CUSTOMERS_LIMIT",
        "The Service customer workspace exceeds its bounded read.",
    )
    .into()
}
