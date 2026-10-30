//! Workflow-owned original Service Count receipt witness. It validates receipt
//! facts only; Inventory, Commerce, Finance, Identity and purchase have separate
//! same-unit witnesses. The ledger remains the sole receipt writer/replay path.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
fn invalid() -> Error {
    PersistenceError::new("SERVICE_INITIAL_COUNT_RECEIPT_UNIT_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED",
        "The original Count receipt changed.",
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
pub struct ServiceInitialCountReceiptFacts {
    pub command_id: String,
    pub record_id: String,
    pub actor_id: String,
    pub company_id: String,
    pub request_hash: String,
    pub proof_json: String,
    pub result_json: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct Image {
    facts: Option<ServiceInitialCountReceiptFacts>,
    digest: Option<String>,
}
pub struct CurrentServiceInitialCountReceipt {
    command: String,
    company: String,
    count: String,
    unit: Unit,
    image: Image,
}
impl CurrentServiceInitialCountReceipt {
    pub fn facts(&self) -> Option<&ServiceInitialCountReceiptFacts> {
        self.image.facts.as_ref()
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture(c, &self.command, &self.company, &self.count).await? != self.image
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn lock_service_initial_count_receipts(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_service_initial_count_receipt(
    c: &mut PgConnection,
    command: &str,
    company: &str,
    count: &str,
) -> Result<CurrentServiceInitialCountReceipt, Error> {
    if command.is_empty() || company.is_empty() || count != format!("count-{command}") {
        return Err(invalid());
    }
    let tx = unit(c).await?;
    let image = capture(c, command, company, count).await?;
    Ok(CurrentServiceInitialCountReceipt {
        command: command.into(),
        company: company.into(),
        count: count.into(),
        unit: tx,
        image,
    })
}
async fn capture(
    c: &mut PgConnection,
    command: &str,
    company: &str,
    count: &str,
) -> Result<Image, Error> {
    let row=sqlx::query("SELECT CASE WHEN octet_length(to_jsonb(r)::text)<=2097152 THEN record_id END record_id,CASE WHEN octet_length(to_jsonb(r)::text)<=2097152 THEN actor_id END actor_id,CASE WHEN octet_length(to_jsonb(r)::text)<=2097152 THEN request_hash END request_hash,CASE WHEN octet_length(to_jsonb(r)::text)<=2097152 THEN (result_json::jsonb->'_serviceInitialPhysicalStockCount')::text END proof,CASE WHEN octet_length(to_jsonb(r)::text)<=2097152 THEN result_json END result_json,encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex') digest,octet_length(to_jsonb(r)::text)<=2097152 AND operation='inventory.count.create' AND record_id=$2 AND authority_json::jsonb->>'companyId'=$3 AND result_json::jsonb->>'id'=$2 AND result_json::jsonb->>'status'='draft' AND result_json::jsonb->>'version'='1' AND NOT jsonb_exists(result_json::jsonb,'_initialPhysicalStockCount') AND NOT jsonb_exists(result_json::jsonb,'_manufacturingInitialPhysicalStockCount') AND NOT jsonb_exists(result_json::jsonb,'_posInitialPhysicalStockCount') valid FROM command_receipts r WHERE command_id=$1").bind(command).bind(count).bind(company).fetch_optional(c).await?;
    let Some(r) = row else {
        return Ok(Image {
            facts: None,
            digest: None,
        });
    };
    let proof: Option<String> = r.try_get("proof")?;
    if !r.try_get::<Option<bool>, _>("valid")?.unwrap_or(false) {
        return Err(changed());
    }
    let proof = proof.ok_or_else(changed)?;
    let value: serde_json::Value = serde_json::from_str(&proof).map_err(|_| changed())?;
    if value.get("kind").and_then(|v| v.as_str()) != Some("service-initial-physical-stock-count")
        || value.get("countId").and_then(|v| v.as_str()) != Some(count)
        || value.get("companyId").and_then(|v| v.as_str()) != Some(company)
        || value.get("countCommandId").and_then(|v| v.as_str()) != Some(command)
    {
        return Err(changed());
    }
    Ok(Image {
        facts: Some(ServiceInitialCountReceiptFacts {
            command_id: command.into(),
            record_id: count.into(),
            actor_id: r.try_get("actor_id")?,
            company_id: company.into(),
            request_hash: r.try_get("request_hash")?,
            proof_json: proof,
            result_json: r.try_get("result_json")?,
        }),
        digest: Some(r.try_get("digest")?),
    })
}

/// Bounded factual audit projection for an already validated original Count.
/// People display names and current actor authority belong to outer composition.
pub async fn read_service_count_generic_audit(
    c: &mut PgConnection,
    receipt: &CurrentServiceInitialCountReceipt,
) -> Result<serde_json::Value, Error> {
    receipt.require_current(c).await?;
    let facts = receipt.facts().ok_or_else(changed)?;
    let rows: Vec<(String,String,i64,String)> = sqlx::query_as("SELECT a.id,a.action,a.created_at,a.actor_id FROM audit_events a JOIN command_receipts r ON r.command_id=a.command_id AND r.record_id=a.record_id AND r.operation=a.action AND r.actor_id=a.actor_id AND r.authority_json::jsonb->>'assignmentId'=a.assignment_id WHERE a.record_id=$1 AND r.authority_json::jsonb->>'companyId'=$2 AND a.record_type IN ('inventory_count','flow_assign') ORDER BY a.created_at DESC LIMIT 501")
        .bind(&facts.record_id).bind(&facts.company_id).fetch_all(&mut *c).await?;
    if rows.len() > 500 {
        return Err(invalid());
    }
    let value = serde_json::Value::Array(rows.into_iter().map(|(id,action,created_at,actor_id)| serde_json::json!({"id":id,"action":action,"created_at":created_at,"actor_id":actor_id})).collect());
    if value.to_string().len() > 2 * 1024 * 1024 {
        return Err(invalid());
    }
    receipt.require_current(c).await?;
    Ok(value)
}
