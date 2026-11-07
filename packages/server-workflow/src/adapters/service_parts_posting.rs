//! Historical Workflow receipt discovery for Service posting setup. This is
//! provenance data only; it is never current authorization or replay admission.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServicePostingReceiptFacts {
    pub command_id: String,
    pub proof_json: String,
}
#[derive(Clone, Eq, PartialEq)]
struct Image {
    facts: Option<ServicePostingReceiptFacts>,
    digest: Option<String>,
}
/// Retains absence as well as presence, for same-unit late revalidation.
pub struct CurrentServicePostingReceipt {
    company: CompanyId,
    unit: Unit,
    image: Image,
}
impl CurrentServicePostingReceipt {
    pub fn facts(&self) -> Option<&ServicePostingReceiptFacts> {
        self.image.facts.as_ref()
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || capture(c, &self.company).await? != self.image {
            return Err(changed());
        }
        Ok(())
    }
}
/// Utility lock only; must run with every other owner lock before the first
/// SERIALIZABLE snapshot, at the retained setup command's original strength.
pub async fn lock_service_posting_receipts(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_service_posting_receipt(
    c: &mut PgConnection,
    company: &CompanyId,
) -> Result<CurrentServicePostingReceipt, Error> {
    let unit = unit(c).await?;
    let image = capture(c, company).await?;
    Ok(CurrentServicePostingReceipt {
        company: company.clone(),
        unit,
        image,
    })
}
/// Keyed discovery supplies the original physical record ID to replay. These
/// facts do not grant authority; the command ledger still validates the entire
/// actor, operation, Company and request fingerprint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServicePrerequisiteReceiptFacts {
    pub record_id: String,
    pub operation: String,
    pub actor_id: String,
    pub company_id: Option<String>,
}
#[derive(Clone, Eq, PartialEq)]
struct KeyedImage {
    facts: Option<ServicePrerequisiteReceiptFacts>,
    digest: Option<String>,
}
pub struct CurrentServicePrerequisiteReceipt {
    key: String,
    unit: Unit,
    image: KeyedImage,
}
impl CurrentServicePrerequisiteReceipt {
    pub fn facts(&self) -> Option<&ServicePrerequisiteReceiptFacts> {
        self.image.facts.as_ref()
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || capture_key(c, &self.key).await? != self.image {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_service_prerequisite_receipt(
    c: &mut PgConnection,
    key: &str,
) -> Result<CurrentServicePrerequisiteReceipt, Error> {
    let unit = unit(c).await?;
    let image = capture_key(c, key).await?;
    Ok(CurrentServicePrerequisiteReceipt {
        key: key.into(),
        unit,
        image,
    })
}
async fn capture_key(c: &mut PgConnection, key: &str) -> Result<KeyedImage, Error> {
    let row=sqlx::query("SELECT CASE WHEN octet_length(to_jsonb(r)::text)<=2097152 THEN record_id END record_id,CASE WHEN octet_length(to_jsonb(r)::text)<=2097152 THEN operation END operation,CASE WHEN octet_length(to_jsonb(r)::text)<=2097152 THEN actor_id END actor_id,CASE WHEN octet_length(to_jsonb(r)::text)<=2097152 THEN authority_json::jsonb->>'companyId' END company_id,encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex') digest FROM command_receipts r WHERE command_id=$1").bind(key).fetch_optional(c).await?;
    match row {
        None => Ok(KeyedImage {
            facts: None,
            digest: None,
        }),
        Some(row) => Ok(KeyedImage {
            facts: Some(ServicePrerequisiteReceiptFacts {
                record_id: row
                    .try_get::<Option<String>, _>("record_id")?
                    .ok_or_else(invalid)?,
                operation: row
                    .try_get::<Option<String>, _>("operation")?
                    .ok_or_else(invalid)?,
                actor_id: row
                    .try_get::<Option<String>, _>("actor_id")?
                    .ok_or_else(invalid)?,
                company_id: row.try_get("company_id")?,
            }),
            digest: Some(row.try_get("digest")?),
        }),
    }
}
async fn capture(c: &mut PgConnection, company: &CompanyId) -> Result<Image, Error> {
    let row=sqlx::query("WITH source AS MATERIALIZED (SELECT r.* FROM command_receipts r WHERE r.record_id=$1 AND r.operation IN ('commerce.service-parts-posting-setup.initialize','commerce.service-parts-posting-setup.confirm-existing') AND r.authority_json::jsonb->>'companyId'=r.record_id AND r.result_json::jsonb->>'id'=r.record_id AND r.result_json::jsonb->>'version'='1' AND r.result_json::jsonb->>'status'=CASE r.operation WHEN 'commerce.service-parts-posting-setup.initialize' THEN 'configured' ELSE 'confirmed-existing' END AND r.result_json::jsonb->'_servicePartsPostingSetup'->>'kind'='service-parts-posting-setup' AND r.result_json::jsonb->'_servicePartsPostingSetup'->>'companyId'=r.record_id AND r.result_json::jsonb->'_servicePartsPostingSetup'->>'commandId'=r.command_id AND r.result_json::jsonb->'_servicePartsPostingSetup'->'mapping'->>'company_id'=r.record_id ORDER BY r.command_id LIMIT 1) SELECT CASE WHEN octet_length(to_jsonb(source)::text)<=2097152 THEN command_id END command_id,CASE WHEN octet_length(to_jsonb(source)::text)<=2097152 THEN (result_json::jsonb->'_servicePartsPostingSetup')::text END proof,encode(sha256(convert_to(to_jsonb(source)::text,'UTF8')),'hex') digest FROM source").bind(company.as_str()).fetch_optional(c).await?;
    match row {
        None => Ok(Image {
            facts: None,
            digest: None,
        }),
        Some(r) => {
            let command_id = r
                .try_get::<Option<String>, _>("command_id")?
                .ok_or_else(invalid)?;
            let proof_json = r
                .try_get::<Option<String>, _>("proof")?
                .ok_or_else(invalid)?;
            Ok(Image {
                facts: Some(ServicePostingReceiptFacts {
                    command_id,
                    proof_json,
                }),
                digest: Some(r.try_get("digest")?),
            })
        }
    }
}
/// Mapping-qualified discovery for the retained Service Stock source. The
/// mapping is supplied by Commerce and is evidence, never an authorization.
pub struct CurrentServiceStockPostingReceipt {
    company: CompanyId,
    mapping: String,
    unit: Unit,
    image: Image,
}
impl CurrentServiceStockPostingReceipt {
    pub fn facts(&self) -> Option<&ServicePostingReceiptFacts> {
        self.image.facts.as_ref()
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture_for_mapping(c, &self.company, &self.mapping).await? != self.image
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_service_stock_posting_receipt(
    c: &mut PgConnection,
    company: &CompanyId,
    mapping: &str,
) -> Result<CurrentServiceStockPostingReceipt, Error> {
    if mapping.len() > 2 * 1024 * 1024 {
        return Err(invalid());
    }
    let unit = unit(c).await?;
    let image = capture_for_mapping(c, company, mapping).await?;
    Ok(CurrentServiceStockPostingReceipt {
        company: company.clone(),
        mapping: mapping.into(),
        unit,
        image,
    })
}
async fn capture_for_mapping(
    c: &mut PgConnection,
    company: &CompanyId,
    mapping: &str,
) -> Result<Image, Error> {
    let row=sqlx::query("WITH source AS MATERIALIZED (SELECT r.* FROM command_receipts r WHERE r.record_id=$1 AND r.operation IN ('commerce.service-parts-posting-setup.initialize','commerce.service-parts-posting-setup.confirm-existing') AND r.authority_json::jsonb->>'companyId'=r.record_id AND r.result_json::jsonb->>'id'=r.record_id AND r.result_json::jsonb->>'version'='1' AND r.result_json::jsonb->>'status'=CASE r.operation WHEN 'commerce.service-parts-posting-setup.initialize' THEN 'configured' ELSE 'confirmed-existing' END AND r.result_json::jsonb->'_servicePartsPostingSetup'->>'kind'='service-parts-posting-setup' AND r.result_json::jsonb->'_servicePartsPostingSetup'->>'companyId'=r.record_id AND r.result_json::jsonb->'_servicePartsPostingSetup'->>'commandId'=r.command_id AND r.result_json::jsonb->'_servicePartsPostingSetup'->'mapping'->>'company_id'=r.record_id AND r.result_json::jsonb->'_servicePartsPostingSetup'->'mapping'=$2::jsonb ORDER BY r.command_id LIMIT 1) SELECT CASE WHEN octet_length(to_jsonb(source)::text)<=2097152 THEN command_id END command_id,CASE WHEN octet_length(to_jsonb(source)::text)<=2097152 THEN (result_json::jsonb->'_servicePartsPostingSetup')::text END proof,encode(sha256(convert_to(to_jsonb(source)::text,'UTF8')),'hex') digest FROM source").bind(company.as_str()).bind(mapping).fetch_optional(c).await?;
    match row {
        None => Ok(Image {
            facts: None,
            digest: None,
        }),
        Some(r) => {
            let command_id = r
                .try_get::<Option<String>, _>("command_id")?
                .ok_or_else(invalid)?;
            let proof_json = r
                .try_get::<Option<String>, _>("proof")?
                .ok_or_else(invalid)?;
            Ok(Image {
                facts: Some(ServicePostingReceiptFacts {
                    command_id,
                    proof_json,
                }),
                digest: Some(r.try_get("digest")?),
            })
        }
    }
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
    PersistenceError::new("WORKFLOW_SERVICE_POSTING_RECEIPT_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_PARTS_POSTING_SOURCE_CHANGED",
        "The original setup receipt changed.",
    )
    .into()
}
