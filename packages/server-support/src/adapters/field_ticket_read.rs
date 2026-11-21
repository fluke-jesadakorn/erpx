//! Support-owned Field case readback. Field owns all canonical transitions for
//! these cases; this port never introduces a generic Support writer.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::Value;
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
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
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SOURCE_RECORD_CHANGED",
        "The Support case changed. Reload its owning workspace.",
    )
    .into()
}
async fn image(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
    branch: Option<&str>,
    department: Option<&str>,
    field_only: bool,
) -> Result<Option<String>, Error> {
    let raw:Option<Option<String>>=sqlx::query_scalar("WITH ticket AS MATERIALIZED (SELECT * FROM support_tickets WHERE id=$1 AND company_id=$2 AND (NOT $5::boolean OR source_kind='field_service') AND ($3::text IS NULL OR branch_id=$3) AND ($4::text IS NULL OR department_id=$4) FOR SHARE),history AS (SELECT coalesce(jsonb_agg(to_jsonb(e) ORDER BY version,id),'[]'::jsonb) events FROM support_ticket_events e WHERE ticket_id=$1 AND EXISTS(SELECT 1 FROM ticket)),payload AS (SELECT jsonb_build_object('row',to_jsonb(ticket),'history',history.events)::text value FROM ticket CROSS JOIN history) SELECT CASE WHEN octet_length(value)<=2097152 THEN value END FROM payload").bind(id).bind(company.as_str()).bind(branch).bind(department).bind(field_only).fetch_optional(c).await?;
    match raw {
        None => Ok(None),
        Some(Some(v)) => Ok(Some(v)),
        Some(None) => Err(BusinessError::new(
            FailureKind::Unavailable,
            "SUPPORT_READ_LIMIT",
            "The Support case exceeds this bounded read.",
        )
        .into()),
    }
}
pub struct CurrentSupportTicketRead {
    company: CompanyId,
    id: String,
    branch: Option<String>,
    department: Option<String>,
    field_only: bool,
    unit: Unit,
    image: String,
    data: Value,
}
impl CurrentSupportTicketRead {
    pub fn data(&self) -> &Value {
        &self.data
    }
    pub fn record(
        &self,
    ) -> Result<erpx_server_shared::public::record_source_contract::SourceRecord, BusinessError>
    {
        use erpx_server_shared::public::record_source_contract::{SourceKind, SourceRecord};
        let r = &self.data["row"];
        let text = |k: &str| {
            r[k].as_str().map(str::to_owned).ok_or_else(|| {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "SUPPORT_SOURCE_INVALID",
                    "The Support source is unavailable.",
                )
            })
        };
        Ok(SourceRecord {
            kind: SourceKind::SupportTicket,
            id: text("id")?,
            company_id: text("company_id")?,
            branch_id: r["branch_id"].as_str().map(str::to_owned),
            department_id: r["department_id"].as_str().map(str::to_owned),
            version: r["version"]
                .as_i64()
                .filter(|v| *v > 0 && *v <= 9_007_199_254_740_991)
                .ok_or_else(|| {
                    BusinessError::invalid("SUPPORT_SOURCE_INVALID", "Invalid source version.")
                })?,
            title: text("title")?,
            status: text("status")?,
        })
    }
    pub fn origin(
        &self,
    ) -> Result<
        Option<erpx_server_shared::public::record_source_contract::SourceReference>,
        BusinessError,
    > {
        use erpx_server_shared::public::record_source_contract::{SourceKind, SourceReference};
        let r = &self.data["row"];
        let Some(raw) = r["source_record_json"].as_str() else {
            return Ok(None);
        };
        let invalid = || {
            BusinessError::new(
                FailureKind::Unavailable,
                "SUPPORT_SOURCE_INVALID",
                "The Support origin is unavailable.",
            )
        };
        let v: Value = serde_json::from_str(raw).map_err(|_| invalid())?;
        let kind = SourceKind::parse(v["kind"].as_str().ok_or_else(invalid)?)?;
        if !matches!(
            kind,
            SourceKind::PosSale | SourceKind::PmsRecord | SourceKind::Asset
        ) || r["source_kind"].as_str() != Some(kind.as_str())
        {
            return Err(invalid());
        }
        // Stored origin version is historical evidence; legacy reads/guards use
        // the current parent version, then retain that exact owner witness.
        Ok(Some(SourceReference {
            kind,
            id: v["id"].as_str().ok_or_else(invalid)?.into(),
            expected_version: None,
        }))
    }
    /// The selected authorizer has already checked the exact current assignment,
    /// role, Company and selected scope. Preserve the original own-report rule.
    pub fn require_reader(
        &self,
        user: &str,
        support_read: bool,
        records_read: bool,
    ) -> Result<(), BusinessError> {
        if support_read
            || records_read
                && self.data["row"]["source_kind"].as_str() != Some("sales_order")
                && self.data["row"]["created_by"].as_str() == Some(user)
        {
            Ok(())
        } else {
            Err(BusinessError::new(
                FailureKind::Forbidden,
                "SUPPORT_TICKET_NOT_FOUND",
                "Ticket not found in this scope.",
            ))
        }
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || image(
                c,
                &self.company,
                &self.id,
                self.branch.as_deref(),
                self.department.as_deref(),
                self.field_only,
            )
            .await?
            .as_deref()
                != Some(self.image.as_str())
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_field_ticket(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
    branch: Option<&str>,
    department: Option<&str>,
) -> Result<Option<CurrentSupportTicketRead>, Error> {
    read_ticket(c, company, id, branch, department, true).await
}
pub async fn read_support_record_ticket(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
    branch: Option<&str>,
    department: Option<&str>,
) -> Result<Option<CurrentSupportTicketRead>, Error> {
    read_ticket(c, company, id, branch, department, false).await
}
async fn read_ticket(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
    branch: Option<&str>,
    department: Option<&str>,
    field_only: bool,
) -> Result<Option<CurrentSupportTicketRead>, Error> {
    let unit = unit(c).await?;
    let Some(raw) = image(c, company, id, branch, department, field_only).await? else {
        return Ok(None);
    };
    let data = serde_json::from_str(&raw).map_err(|_| changed())?;
    Ok(Some(CurrentSupportTicketRead {
        company: company.clone(),
        id: id.into(),
        branch: branch.map(Into::into),
        department: department.map(Into::into),
        unit,
        field_only,
        image: raw,
        data,
    }))
}
