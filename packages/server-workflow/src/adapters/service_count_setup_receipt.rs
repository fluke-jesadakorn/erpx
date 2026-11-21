//! Workflow-owned exact Service setup receipt facts for Count creation.
//! This witness is data, not purchase, mapping, or actor authority.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
fn invalid() -> Error {
    PersistenceError::new("SERVICE_COUNT_SETUP_RECEIPT_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED",
        "The original Service setup receipt changed.",
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
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceCountSetupReceipt {
    pub command_id: String,
    pub request_hash: String,
    pub proof: String,
}
#[derive(Clone, Eq, PartialEq)]
struct Image {
    data: Option<ServiceCountSetupReceipt>,
    digest: Option<String>,
}
pub struct CurrentServiceCountSetupReceipt {
    company: String,
    command: String,
    mapping: Option<String>,
    unit: Unit,
    image: Image,
}
impl CurrentServiceCountSetupReceipt {
    pub fn facts(&self) -> Option<&ServiceCountSetupReceipt> {
        self.image.data.as_ref()
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture(c, &self.company, &self.command, self.mapping.as_deref()).await?
                != self.image
        {
            return Err(changed());
        }
        Ok(())
    }
}
/// Caller takes Workflow receipt lock before its first snapshot.
pub async fn read_service_count_setup_receipt(
    c: &mut PgConnection,
    company: &str,
    command: &str,
    mapping: &str,
) -> Result<CurrentServiceCountSetupReceipt, Error> {
    if company.is_empty() || command.is_empty() || mapping.len() > 2 * 1024 * 1024 {
        return Err(invalid());
    }
    let tx = unit(c).await?;
    let image = capture(c, company, command, Some(mapping)).await?;
    Ok(CurrentServiceCountSetupReceipt {
        company: company.into(),
        command: command.into(),
        mapping: Some(mapping.into()),
        unit: tx,
        image,
    })
}
/// Historical lineage uses its captured mapping, not today's settings.
pub async fn read_service_count_setup_history(
    c: &mut PgConnection,
    company: &str,
    command: &str,
) -> Result<CurrentServiceCountSetupReceipt, Error> {
    if company.is_empty() || command.is_empty() {
        return Err(invalid());
    }
    let tx = unit(c).await?;
    let image = capture(c, company, command, None).await?;
    Ok(CurrentServiceCountSetupReceipt {
        company: company.into(),
        command: command.into(),
        mapping: None,
        unit: tx,
        image,
    })
}
async fn capture(
    c: &mut PgConnection,
    company: &str,
    command: &str,
    mapping: Option<&str>,
) -> Result<Image, Error> {
    let row=sqlx::query("SELECT CASE WHEN octet_length(to_jsonb(r)::text)<=2097152 THEN request_hash END request_hash,CASE WHEN octet_length(to_jsonb(r)::text)<=2097152 THEN (result_json::jsonb->'_servicePartsPostingSetup')::text END proof,encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex') digest,octet_length(to_jsonb(r)::text)<=2097152 AND record_id=$1 AND authority_json::jsonb->>'companyId'=$1 AND operation IN ('commerce.service-parts-posting-setup.initialize','commerce.service-parts-posting-setup.confirm-existing') AND result_json::jsonb->>'id'=$1 AND result_json::jsonb->>'version'='1' AND result_json::jsonb->>'status'=CASE operation WHEN 'commerce.service-parts-posting-setup.initialize' THEN 'configured' ELSE 'confirmed-existing' END AND result_json::jsonb->'_servicePartsPostingSetup'->>'kind'='service-parts-posting-setup' AND result_json::jsonb->'_servicePartsPostingSetup'->>'companyId'=$1 AND result_json::jsonb->'_servicePartsPostingSetup'->>'commandId'=$2 AND result_json::jsonb->'_servicePartsPostingSetup'->'mapping'->>'company_id'=$1 AND ($3::text IS NULL OR result_json::jsonb->'_servicePartsPostingSetup'->'mapping'=$3::jsonb) valid FROM command_receipts r WHERE command_id=$2").bind(company).bind(command).bind(mapping).fetch_optional(c).await?;
    let Some(r) = row else {
        return Ok(Image {
            data: None,
            digest: None,
        });
    };
    if !r.try_get::<Option<bool>, _>("valid")?.unwrap_or(false) {
        return Err(changed());
    }
    Ok(Image {
        data: Some(ServiceCountSetupReceipt {
            command_id: command.into(),
            request_hash: r
                .try_get::<Option<String>, _>("request_hash")?
                .ok_or_else(invalid)?,
            proof: r
                .try_get::<Option<String>, _>("proof")?
                .ok_or_else(invalid)?,
        }),
        digest: Some(r.try_get("digest")?),
    })
}
