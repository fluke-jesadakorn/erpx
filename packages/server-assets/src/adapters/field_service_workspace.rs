//! Scoped Service asset display and picker; this supplies no Asset grant.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceAssetDisplay {
    pub id: String,
    pub code: String,
    pub branch_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceAssetReference {
    pub id: String,
    pub code: String,
    pub name: String,
    pub version: i64,
    pub branch_id: String,
    pub department_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceWorkspaceAssets {
    pub display: Vec<ServiceAssetDisplay>,
    pub picker: Vec<ServiceAssetReference>,
}
#[derive(Clone, PartialEq, Eq)]
struct Image {
    data: ServiceWorkspaceAssets,
    digests: Vec<String>,
}
pub struct CurrentServiceWorkspaceAssets {
    company: CompanyId,
    branch: Option<String>,
    department: Option<String>,
    include_picker: bool,
    ids: Vec<String>,
    unit: Unit,
    image: Image,
}
impl CurrentServiceWorkspaceAssets {
    pub fn data(&self) -> &ServiceWorkspaceAssets {
        &self.image.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture(
                c,
                &self.company,
                self.branch.as_deref(),
                self.department.as_deref(),
                self.include_picker,
                &self.ids,
            )
            .await?
                != self.image
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn lock_service_workspace_assets(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("ASSETS_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE operational_assets IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_service_workspace_assets(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&str>,
    department: Option<&str>,
    include_picker: bool,
    ids: &[String],
) -> Result<CurrentServiceWorkspaceAssets, Error> {
    let unit = unit(c).await?;
    let image = capture(c, company, branch, department, include_picker, ids).await?;
    Ok(CurrentServiceWorkspaceAssets {
        company: company.clone(),
        branch: branch.map(str::to_owned),
        department: department.map(str::to_owned),
        include_picker,
        ids: ids.to_vec(),
        unit,
        image,
    })
}
async fn capture(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&str>,
    department: Option<&str>,
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
    let rows=sqlx::query("WITH source AS MATERIALIZED (SELECT a.*,a.id=ANY($2::text[]) AS display,($3 AND a.lifecycle_status NOT IN ('retired','retirement_pending') AND ($4::text IS NULL OR a.branch_id=$4) AND ($5::text IS NULL OR a.department_id=$5)) AS picker FROM operational_assets a WHERE a.company_id=$1 AND (a.id=ANY($2::text[]) OR ($3 AND a.lifecycle_status NOT IN ('retired','retirement_pending') AND ($4::text IS NULL OR a.branch_id=$4) AND ($5::text IS NULL OR a.department_id=$5)))), measured AS (SELECT *,sum(octet_length(to_jsonb(source)::text)) OVER () bytes,count(*) OVER () count,row_number() OVER () ordinal FROM source) SELECT CASE WHEN bytes<=2097152 AND count<=10000 THEN id END id,CASE WHEN bytes<=2097152 AND count<=10000 THEN code END code,CASE WHEN bytes<=2097152 AND count<=10000 THEN name END name,CASE WHEN bytes<=2097152 AND count<=10000 THEN version END version,CASE WHEN bytes<=2097152 AND count<=10000 THEN branch_id END branch_id,CASE WHEN bytes<=2097152 AND count<=10000 THEN department_id END department_id,display,picker,encode(sha256(convert_to((to_jsonb(measured)-'bytes'-'count'-'ordinal')::text,'UTF8')),'hex') digest FROM measured WHERE (bytes<=2097152 AND count<=10000) OR ordinal=1 ORDER BY code,id").bind(company.as_str()).bind(ids).bind(include_picker).bind(branch).bind(department).fetch_all(c).await?;
    let mut display = Vec::new();
    let mut picker = Vec::new();
    let mut digests = Vec::new();
    for r in rows {
        let id: String = r.try_get::<Option<String>, _>("id")?.ok_or_else(limited)?;
        let code: String = r
            .try_get::<Option<String>, _>("code")?
            .ok_or_else(limited)?;
        if r.try_get("display")? {
            display.push(ServiceAssetDisplay {
                id: id.clone(),
                code: code.clone(),
                branch_id: r
                    .try_get::<Option<String>, _>("branch_id")?
                    .ok_or_else(limited)?,
            });
        }
        if r.try_get("picker")? {
            picker.push(ServiceAssetReference {
                id,
                code,
                name: r
                    .try_get::<Option<String>, _>("name")?
                    .ok_or_else(limited)?,
                version: r
                    .try_get::<Option<i64>, _>("version")?
                    .ok_or_else(limited)?,
                branch_id: r
                    .try_get::<Option<String>, _>("branch_id")?
                    .ok_or_else(limited)?,
                department_id: r.try_get("department_id")?,
            });
        }
        digests.push(r.try_get("digest")?);
    }
    Ok(Image {
        data: ServiceWorkspaceAssets { display, picker },
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
        return Err(PersistenceError::new("ASSETS_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_SERVICE_ASSETS_CHANGED",
        "The Service Asset source changed.",
    )
    .into()
}
fn limited() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FIELD_SERVICE_ASSETS_LIMIT",
        "The Service Asset workspace exceeds its bounded read.",
    )
    .into()
}

/// Field-owned reference supplied by the native Commercial composition. Assets
/// validates its original retained identity, without requiring disposal or an
/// active picker entry when software is withdrawn.
#[derive(Clone,Debug,PartialEq,Eq,serde::Serialize)]
pub struct ServiceAssetSettlementSource {
    pub id:String,
    pub company_id:String,
    pub branch_id:String,
    pub department_id:Option<String>,
    pub asset_id:String,
}
#[derive(Clone,Debug,PartialEq,Eq,serde::Serialize,serde::Deserialize)]
pub struct ServiceAssetSettlementFact {pub id:String,pub fact:String}
pub struct CurrentServiceAssetSettlement {
    company:CompanyId,
    sources:Vec<ServiceAssetSettlementSource>,
    unit:Unit,
    facts:Vec<ServiceAssetSettlementFact>,
    digest:String,
}
pub async fn lock_service_asset_settlement(c:&mut PgConnection)->Result<(),Error>{
    if !c.is_in_transaction(){return Err(PersistenceError::new("ASSET_TRANSACTION_REQUIRED").into());}
    sqlx::query("LOCK TABLE operational_assets,operational_asset_events IN SHARE MODE").execute(c).await?;
    Ok(())
}
async fn capture_service_asset_settlement(c:&mut PgConnection,company:&CompanyId,sources:&[ServiceAssetSettlementSource])->Result<(Vec<ServiceAssetSettlementFact>,String),Error>{
    if sources.iter().any(|s|s.company_id!=company.as_str()){return Err(changed());}
    let input=serde_json::to_value(sources).map_err(|_|limited())?;
    let row=sqlx::query("WITH refs AS MATERIALIZED(SELECT * FROM jsonb_to_recordset($2::jsonb) AS r(id text,company_id text,branch_id text,department_id text,asset_id text)),facts AS(SELECT source.id,to_jsonb(source)::text fact FROM refs source LEFT JOIN operational_assets asset ON asset.id=source.asset_id AND asset.company_id=source.company_id AND asset.branch_id=source.branch_id AND (source.department_id IS NULL OR asset.department_id=source.department_id) WHERE asset.id IS NULL),snapshot AS(SELECT coalesce(jsonb_agg(to_jsonb(facts) ORDER BY id,fact),'[]'::jsonb) value FROM facts) SELECT CASE WHEN octet_length(value::text)<=2097152 THEN value::text END payload,(SELECT encode(sha256(convert_to(coalesce(string_agg(encode(sha256(convert_to(to_jsonb(a)::text,'UTF8')),'hex'),'' ORDER BY a.id),''),'UTF8')),'hex') FROM operational_assets a WHERE a.company_id=$1 AND EXISTS(SELECT 1 FROM refs r WHERE r.asset_id=a.id)) digest FROM snapshot").bind(company.as_str()).bind(input).fetch_one(c).await?;
    let payload:Option<String>=row.try_get("payload")?;
    let facts=serde_json::from_str(&payload.ok_or_else(limited)?).map_err(|_|limited())?;
    Ok((facts,row.try_get("digest")?))
}
impl CurrentServiceAssetSettlement {
    pub fn facts(&self)->&[ServiceAssetSettlementFact]{&self.facts}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        if unit(c).await?!=self.unit{return Err(changed());}
        let(facts,digest)=capture_service_asset_settlement(c,&self.company,&self.sources).await?;
        if facts!=self.facts||digest!=self.digest{return Err(changed());}
        Ok(())
    }
}
pub async fn read_service_asset_settlement(c:&mut PgConnection,company:&CompanyId,sources:&[ServiceAssetSettlementSource])->Result<CurrentServiceAssetSettlement,Error>{
    let unit=unit(c).await?;
    let(facts,digest)=capture_service_asset_settlement(c,company,sources).await?;
    Ok(CurrentServiceAssetSettlement{company:company.clone(),sources:sources.to_vec(),unit,facts,digest})
}
