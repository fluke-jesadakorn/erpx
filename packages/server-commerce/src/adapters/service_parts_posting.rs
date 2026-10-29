//! Commerce-owned mapping and history facts for Service parts preparation.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServicePartsPostingFacts {
    pub schema_available: bool,
    pub mapping_json: Option<String>,
    /// A Commerce row is not itself authority: Workflow must prove its receipt.
    pub source_json: Option<String>,
    pub has_commerce_history: bool,
    /// PostgreSQL JSON text for the original ordered Commerce orders image.
    /// Composition may combine owner images, but this value grants no access.
    pub commerce_orders_json: String,
    /// Actual Company warehouse identities for Inventory's same-unit witness.
    pub warehouse_ids: Vec<String>,
}
#[derive(Clone, Eq, PartialEq)]
struct Image {
    facts: ServicePartsPostingFacts,
    history_digest: String,
}
pub struct CurrentServicePartsPosting {
    company: CompanyId,
    unit: Unit,
    image: Image,
}
impl CurrentServicePartsPosting {
    pub fn facts(&self) -> &ServicePartsPostingFacts {
        &self.image.facts
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || capture(c, &self.company).await? != self.image {
            return Err(changed());
        }
        Ok(())
    }
    /// Mapping/source are intentionally excluded because the owner command may
    /// insert them; Commerce history must still match its original witness.
    pub async fn require_history_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        let current = capture(c, &self.company).await?;
        if unit(c).await? != self.unit
            || current.history_digest != self.image.history_digest
            || current.facts.warehouse_ids != self.image.facts.warehouse_ids
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn lock_service_parts_posting_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE commerce_settings IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *c)
        .await?;
    sqlx::query("LOCK TABLE commerce_orders,commerce_order_lines,commerce_fulfillments,commerce_fulfillment_lines,commerce_goods_valuations,commerce_warehouses,commerce_products IN SHARE MODE")
        .execute(&mut *c)
        .await?;
    // Older schemas legitimately lack this relation. Probe with utility
    // statements only: SELECT to_regclass here would fix a SERIALIZABLE
    // snapshot before a blocked source lock has been acquired.
    sqlx::query("SAVEPOINT service_parts_posting_schema_lock")
        .execute(&mut *c)
        .await?;
    let locked = sqlx::query(
        "LOCK TABLE commerce_service_parts_posting_sources IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(&mut *c)
    .await;
    if let Err(error) = locked {
        sqlx::query("ROLLBACK TO SAVEPOINT service_parts_posting_schema_lock")
            .execute(&mut *c)
            .await?;
        sqlx::query("RELEASE SAVEPOINT service_parts_posting_schema_lock")
            .execute(&mut *c)
            .await?;
        if error.as_database_error().and_then(|e| e.code()).as_deref() != Some("42P01") {
            return Err(error.into());
        }
        return Ok(());
    }
    sqlx::query("RELEASE SAVEPOINT service_parts_posting_schema_lock")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_service_parts_posting(
    c: &mut PgConnection,
    company: &CompanyId,
) -> Result<CurrentServicePartsPosting, Error> {
    let unit = unit(c).await?;
    let image = capture(c, company).await?;
    Ok(CurrentServicePartsPosting {
        company: company.clone(),
        unit,
        image,
    })
}
async fn schema(c: &mut PgConnection) -> Result<bool, Error> {
    Ok(sqlx::query_scalar(
        "SELECT to_regclass('commerce_service_parts_posting_sources') IS NOT NULL",
    )
    .fetch_one(c)
    .await?)
}
/// Caller takes the utility source locks before this first owner snapshot.
pub async fn service_posting_schema_available(c: &mut PgConnection) -> Result<bool, Error> {
    unit(c).await?;
    schema(c).await
}
async fn capture(c: &mut PgConnection, company: &CompanyId) -> Result<Image, Error> {
    let available = schema(c).await?;
    let mapping: Option<String> = sqlx::query_scalar("SELECT CASE WHEN octet_length(to_jsonb(s)::text) <= 2097152 THEN to_jsonb(s)::text ELSE NULL END FROM commerce_settings s WHERE s.company_id=$1").bind(company.as_str()).fetch_optional(&mut *c).await?.flatten();
    // Reject oversized existing values rather than confusing them with absence.
    if mapping.is_none()
        && sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM commerce_settings WHERE company_id=$1)",
        )
        .bind(company.as_str())
        .fetch_one(&mut *c)
        .await?
    {
        return Err(invalid());
    }
    let source: Option<String> = if available {
        sqlx::query_scalar("SELECT CASE WHEN octet_length(to_jsonb(s)::text)<=2097152 THEN to_jsonb(s)::text END FROM commerce_service_parts_posting_sources s WHERE company_id=$1").bind(company.as_str()).fetch_optional(&mut *c).await?.flatten()
    } else {
        None
    };
    if available && source.is_none() && sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM commerce_service_parts_posting_sources WHERE company_id=$1)").bind(company.as_str()).fetch_one(&mut *c).await? { return Err(invalid()); }
    let row=sqlx::query("WITH image AS (SELECT (SELECT jsonb_agg(to_jsonb(o) ORDER BY o.id) FROM commerce_orders o WHERE o.company_id=$1)::text value) SELECT EXISTS(SELECT 1 FROM commerce_orders WHERE company_id=$1) history, CASE WHEN octet_length(coalesce(value,'null'))<=2097152 THEN coalesce(value,'null') END orders_json, encode(sha256(convert_to(coalesce(value,'null'),'UTF8')),'hex') digest FROM image").bind(company.as_str()).fetch_one(&mut *c).await?;
    let orders_json = row
        .try_get::<Option<String>, _>("orders_json")?
        .ok_or_else(invalid)?;
    let warehouse_ids: Option<Vec<String>> = sqlx::query_scalar("SELECT CASE WHEN coalesce(sum(octet_length(id)),0)<=2097152 AND count(*)<=10000 THEN coalesce(array_agg(id ORDER BY id),ARRAY[]::text[]) END FROM commerce_warehouses WHERE company_id=$1").bind(company.as_str()).fetch_one(c).await?;
    let warehouse_ids = warehouse_ids.ok_or_else(invalid)?;
    Ok(Image {
        facts: ServicePartsPostingFacts {
            schema_available: available,
            mapping_json: mapping,
            source_json: source,
            has_commerce_history: row.try_get("history")?,
            commerce_orders_json: orders_json,
            warehouse_ids,
        },
        history_digest: row.try_get("digest")?,
    })
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
    PersistenceError::new("COMMERCE_SERVICE_POSTING_SOURCE_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_PARTS_POSTING_SOURCE_CHANGED",
        "The original Commerce mapping or history changed.",
    )
    .into()
}

/// Actual Finance/Inventory/Field/IAM/Commercial/Workflow proofs are assembled
/// by the caller on this transaction. This interface carries no authority by
/// itself; callbacks must verify the original owner witnesses before and after.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServicePartsPostingMapping {
    pub chart_id: String,
    pub currency: String,
    pub receivable_account_id: String,
    pub payable_account_id: String,
    pub revenue_account_id: String,
    pub inventory_account_id: String,
    pub clearing_account_id: String,
    pub cogs_account_id: String,
    pub input_tax_account_id: String,
    pub output_tax_account_id: String,
    pub cash_account_id: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServicePartsPostingIntent {
    pub company: CompanyId,
    pub command_id: String,
    pub actor_id: String,
    pub created_at: i64,
    pub mode: crate::domain::service_parts_posting::PostingMode,
    pub mapping: ServicePartsPostingMapping,
}
pub trait PgServicePartsPostingSource: Sync {
    fn intent(&self) -> &ServicePartsPostingIntent;
    fn require_before(
        &self,
        c: &mut PgConnection,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send;
    fn require_after(
        &self,
        c: &mut PgConnection,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send;
}
pub struct PreparedServicePartsPosting {
    original: CurrentServicePartsPosting,
    intent: ServicePartsPostingIntent,
    expected_mapping: String,
    expected_proof: String,
    expected_source: String,
}
impl PreparedServicePartsPosting {
    /// Exact immutable Workflow receipt marker, not a receipt or authorization.
    pub fn receipt_marker_json(&self) -> &str {
        &self.expected_proof
    }
    pub async fn prepare<S: PgServicePartsPostingSource>(
        c: &mut PgConnection,
        source: &S,
    ) -> Result<Self, Error> {
        source.require_before(c).await?;
        let intent = source.intent().clone();
        let mapping = &intent.mapping;
        let values = [
            intent.company.as_str(),
            &intent.command_id,
            &intent.actor_id,
            &mapping.chart_id,
            &mapping.currency,
            &mapping.receivable_account_id,
            &mapping.payable_account_id,
            &mapping.revenue_account_id,
            &mapping.inventory_account_id,
            &mapping.clearing_account_id,
            &mapping.cogs_account_id,
            &mapping.input_tax_account_id,
            &mapping.output_tax_account_id,
            &mapping.cash_account_id,
        ];
        if values.iter().any(|value| value.is_empty())
            || values
                .iter()
                .try_fold(0usize, |total, value| total.checked_add(value.len()))
                .is_none_or(|bytes| bytes > 2 * 1024 * 1024)
        {
            return Err(invalid());
        }
        let original = read_service_parts_posting(c, &intent.company).await?;
        if !original.facts().schema_available {
            return Err(invalid());
        }
        let facts = original.facts();
        let admission = crate::domain::service_parts_posting::posting_admission(
            intent.mode,
            facts.mapping_json.is_some(),
            facts.source_json.is_some(),
            facts.has_commerce_history,
        );
        if admission != crate::domain::service_parts_posting::PostingAdmission::Allowed {
            return Err(changed());
        }
        let expected_mapping = mapping_image(c, &intent).await?;
        if intent.mode == crate::domain::service_parts_posting::PostingMode::ConfirmExisting
            && facts.mapping_json.as_deref() != Some(expected_mapping.as_str())
        {
            return Err(changed());
        }
        let expected_source = source_image(c, &intent, &expected_mapping).await?;
        let expected_proof = proof_image(c, &intent, &expected_mapping).await?;
        Ok(Self {
            original,
            intent,
            expected_mapping,
            expected_proof,
            expected_source,
        })
    }
    /// Called by the real Workflow effect only after the original command
    /// receipt exists. Deferred database constraints validate that linkage.
    pub async fn apply<S: PgServicePartsPostingSource>(
        &self,
        c: &mut PgConnection,
        source: &S,
    ) -> Result<(), Error> {
        if source.intent() != &self.intent {
            return Err(changed());
        }
        source.require_before(c).await?;
        self.original.require_current(c).await?;
        if self.intent.mode == crate::domain::service_parts_posting::PostingMode::Initialize {
            let m = &self.intent.mapping;
            sqlx::query("INSERT INTO commerce_settings(company_id,chart_id,currency,receivable_account_id,payable_account_id,revenue_account_id,inventory_account_id,clearing_account_id,cogs_account_id,input_tax_account_id,output_tax_account_id,cash_account_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
                .bind(self.intent.company.as_str()).bind(&m.chart_id).bind(&m.currency).bind(&m.receivable_account_id).bind(&m.payable_account_id).bind(&m.revenue_account_id).bind(&m.inventory_account_id).bind(&m.clearing_account_id).bind(&m.cogs_account_id).bind(&m.input_tax_account_id).bind(&m.output_tax_account_id).bind(&m.cash_account_id).execute(&mut *c).await?;
        }
        let mode = match self.intent.mode {
            crate::domain::service_parts_posting::PostingMode::Initialize => "initialize",
            crate::domain::service_parts_posting::PostingMode::ConfirmExisting => {
                "confirm-existing"
            }
        };
        sqlx::query("INSERT INTO commerce_service_parts_posting_sources(company_id,command_id,mode,mapping_json,proof_json,created_by,created_at) VALUES($1,$2,$3,$4::jsonb,jsonb_build_object('kind','service-parts-posting-setup','companyId',$1::text,'commandId',$2::text,'mode',$3::text,'mapping',$4::jsonb),$5,$6)")
            .bind(self.intent.company.as_str()).bind(&self.intent.command_id).bind(mode).bind(&self.expected_mapping).bind(&self.intent.actor_id).bind(self.intent.created_at).execute(&mut *c).await?;
        Ok(())
    }
    pub async fn require_current<S: PgServicePartsPostingSource>(
        &self,
        c: &mut PgConnection,
        source: &S,
    ) -> Result<(), Error> {
        if source.intent() != &self.intent {
            return Err(changed());
        }
        source.require_after(c).await?;
        self.original.require_history_current(c).await?;
        let now = capture(c, &self.intent.company).await?;
        if now.facts.mapping_json.as_deref() != Some(&self.expected_mapping)
            || now.facts.source_json.as_deref() != Some(&self.expected_source)
        {
            return Err(changed());
        }
        Ok(())
    }
}
async fn proof_image(
    c: &mut PgConnection,
    i: &ServicePartsPostingIntent,
    mapping: &str,
) -> Result<String, Error> {
    let mode = match i.mode {
        crate::domain::service_parts_posting::PostingMode::Initialize => "initialize",
        crate::domain::service_parts_posting::PostingMode::ConfirmExisting => "confirm-existing",
    };
    Ok(sqlx::query_scalar("SELECT jsonb_build_object('kind','service-parts-posting-setup','companyId',$1::text,'commandId',$2::text,'mode',$3::text,'mapping',$4::jsonb)::text").bind(i.company.as_str()).bind(&i.command_id).bind(mode).bind(mapping).fetch_one(c).await?)
}
async fn mapping_image(
    c: &mut PgConnection,
    i: &ServicePartsPostingIntent,
) -> Result<String, Error> {
    let m = &i.mapping;
    Ok(sqlx::query_scalar("SELECT jsonb_build_object('company_id',$1::text,'chart_id',$2::text,'currency',$3::text,'receivable_account_id',$4::text,'payable_account_id',$5::text,'revenue_account_id',$6::text,'inventory_account_id',$7::text,'clearing_account_id',$8::text,'cogs_account_id',$9::text,'input_tax_account_id',$10::text,'output_tax_account_id',$11::text,'cash_account_id',$12::text)::text")
       .bind(i.company.as_str()).bind(&m.chart_id).bind(&m.currency).bind(&m.receivable_account_id).bind(&m.payable_account_id).bind(&m.revenue_account_id).bind(&m.inventory_account_id).bind(&m.clearing_account_id).bind(&m.cogs_account_id).bind(&m.input_tax_account_id).bind(&m.output_tax_account_id).bind(&m.cash_account_id).fetch_one(c).await?)
}
async fn source_image(
    c: &mut PgConnection,
    i: &ServicePartsPostingIntent,
    mapping: &str,
) -> Result<String, Error> {
    let mode = match i.mode {
        crate::domain::service_parts_posting::PostingMode::Initialize => "initialize",
        crate::domain::service_parts_posting::PostingMode::ConfirmExisting => "confirm-existing",
    };
    Ok(sqlx::query_scalar("SELECT jsonb_build_object('company_id',$1::text,'command_id',$2::text,'mode',$3::text,'mapping_json',$4::jsonb,'proof_json',jsonb_build_object('kind','service-parts-posting-setup','companyId',$1::text,'commandId',$2::text,'mode',$3::text,'mapping',$4::jsonb),'created_by',$5::text,'created_at',$6::bigint)::text").bind(i.company.as_str()).bind(&i.command_id).bind(mode).bind(mapping).bind(&i.actor_id).bind(i.created_at).fetch_one(c).await?)
}
