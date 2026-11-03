//! Current Organization locations for the Service workspace; no editor grant.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
/// Report labels do not grant access to a node or Field record. Historical
/// rows retain their current name even if a node is no longer active.
pub async fn read_service_report_node_names(c:&mut PgConnection,company:&CompanyId,ids:&[String])->Result<Vec<(String,String)>,Error>{
    if !c.is_in_transaction()||ids.len()>1000{return Err(PersistenceError::new("SERVICE_REPORT_NODE_SCOPE_REQUIRED").into());}
    let rows:Vec<(String,Option<String>)>=sqlx::query_as("SELECT id,CASE WHEN octet_length(display_name)<=3000 THEN display_name END FROM organization_nodes WHERE company_id=$1 AND id=ANY($2::text[]) ORDER BY id").bind(company.as_str()).bind(ids).fetch_all(c).await?;
    rows.into_iter().map(|(id,name)|name.map(|name|(id,name)).ok_or_else(||BusinessError::new(FailureKind::Unavailable,"SERVICE_REPORT_LABEL_LIMIT","A report label exceeds this bounded read.").into())).collect()
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceLocationReference {
    pub id: String,
    pub kind: String,
    pub display_name: String,
    pub branch_id: String,
}
#[derive(Clone, PartialEq, Eq)]
struct Image {
    data: Vec<ServiceLocationReference>,
    digests: Vec<String>,
}
pub struct CurrentServiceWorkspaceLocations {
    company: CompanyId,
    branch: Option<String>,
    department: Option<String>,
    include_locations: bool,
    unit: Unit,
    image: Image,
}
impl CurrentServiceWorkspaceLocations {
    pub fn data(&self) -> &Vec<ServiceLocationReference> {
        &self.image.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture(
                c,
                &self.company,
                self.branch.as_deref(),
                self.department.as_deref(),
                self.include_locations,
            )
            .await?
                != self.image
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn lock_service_workspace_locations(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("ORGANIZATION_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE organization_nodes IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_service_workspace_locations(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&str>,
    department: Option<&str>,
    include_locations: bool,
) -> Result<CurrentServiceWorkspaceLocations, Error> {
    let unit = unit(c).await?;
    let image = capture(c, company, branch, department, include_locations).await?;
    Ok(CurrentServiceWorkspaceLocations {
        company: company.clone(),
        branch: branch.map(str::to_owned),
        department: department.map(str::to_owned),
        include_locations,
        unit,
        image,
    })
}
async fn capture(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&str>,
    department: Option<&str>,
    include_locations: bool,
) -> Result<Image, Error> {
    let rows=sqlx::query("WITH source AS MATERIALIZED (SELECT n.*,CASE WHEN n.kind='department' THEN n.branch_id ELSE n.id END service_branch FROM organization_nodes n WHERE $4 AND n.company_id=$1 AND n.status='active' AND n.kind IN ('branch','headquarters','department') AND (n.kind<>'department' OR EXISTS(SELECT 1 FROM organization_nodes b WHERE b.id=n.branch_id AND b.company_id=n.company_id AND b.kind IN ('branch','headquarters') AND b.status='active')) AND ($2::text IS NULL OR CASE WHEN n.kind='department' THEN n.branch_id ELSE n.id END=$2) AND ($3::text IS NULL OR n.kind='department' AND n.id=$3 OR n.kind IN ('branch','headquarters') AND EXISTS(SELECT 1 FROM organization_nodes d WHERE d.id=$3 AND d.company_id=n.company_id AND d.kind='department' AND d.status='active' AND d.branch_id=n.id))), measured AS (SELECT *,sum(octet_length(to_jsonb(source)::text)) OVER () bytes,count(*) OVER () count,row_number() OVER () ordinal FROM source) SELECT CASE WHEN bytes<=2097152 AND count<=10000 THEN id END id,CASE WHEN bytes<=2097152 AND count<=10000 THEN kind END kind,CASE WHEN bytes<=2097152 AND count<=10000 THEN display_name END display_name,CASE WHEN bytes<=2097152 AND count<=10000 THEN service_branch END service_branch,encode(sha256(convert_to((to_jsonb(measured)-'bytes'-'count'-'ordinal')::text,'UTF8')),'hex') digest FROM measured WHERE (bytes<=2097152 AND count<=10000) OR ordinal=1 ORDER BY kind,display_name,id").bind(company.as_str()).bind(branch).bind(department).bind(include_locations).fetch_all(c).await?;
    let mut data = Vec::new();
    let mut digests = Vec::new();
    for r in rows {
        data.push(ServiceLocationReference {
            id: r.try_get::<Option<String>, _>("id")?.ok_or_else(limited)?,
            kind: r
                .try_get::<Option<String>, _>("kind")?
                .ok_or_else(limited)?,
            display_name: r
                .try_get::<Option<String>, _>("display_name")?
                .ok_or_else(limited)?,
            branch_id: r
                .try_get::<Option<String>, _>("service_branch")?
                .ok_or_else(limited)?,
        });
        digests.push(r.try_get("digest")?);
    }
    Ok(Image { data, digests })
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("ORGANIZATION_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_SERVICE_LOCATIONS_CHANGED",
        "The Service Organization source changed.",
    )
    .into()
}
fn limited() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FIELD_SERVICE_LOCATIONS_LIMIT",
        "The Service Organization workspace exceeds its bounded read.",
    )
    .into()
}

/// The Service stock picker uses active branches only, ordered by physical ID.
/// It intentionally does not inherit the broader workspace's headquarters or
/// department predicates.
pub struct CurrentServiceStockBranches {
    company: CompanyId,
    branch: Option<String>,
    unit: Unit,
    image: Vec<(ServiceLocationReference, String)>,
}
impl CurrentServiceStockBranches {
    pub fn branches(&self) -> impl Iterator<Item = &ServiceLocationReference> {
        self.image.iter().map(|(row, _)| row)
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || stock_branches(c, &self.company, self.branch.as_deref()).await? != self.image
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_service_stock_branches(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&str>,
) -> Result<CurrentServiceStockBranches, Error> {
    let unit = unit(c).await?;
    let image = stock_branches(c, company, branch).await?;
    Ok(CurrentServiceStockBranches {
        company: company.clone(),
        branch: branch.map(str::to_owned),
        unit,
        image,
    })
}
async fn stock_branches(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&str>,
) -> Result<Vec<(ServiceLocationReference, String)>, Error> {
    let rows=sqlx::query("WITH source AS MATERIALIZED (SELECT * FROM organization_nodes WHERE company_id=$1 AND kind='branch' AND status='active' AND ($2::text IS NULL OR id=$2)), measured AS (SELECT *,sum(octet_length(to_jsonb(source)::text)) OVER () bytes,count(*) OVER () count,row_number() OVER () ordinal FROM source) SELECT CASE WHEN bytes<=2097152 AND count<=10000 THEN id END id,CASE WHEN bytes<=2097152 AND count<=10000 THEN display_name END display_name,encode(sha256(convert_to((to_jsonb(measured)-'bytes'-'count'-'ordinal')::text,'UTF8')),'hex') digest FROM measured WHERE (bytes<=2097152 AND count<=10000) OR ordinal=1 ORDER BY id").bind(company.as_str()).bind(branch).fetch_all(c).await?;
    let mut result = Vec::new();
    for r in rows {
        let id = r.try_get::<Option<String>, _>("id")?.ok_or_else(limited)?;
        let name = r
            .try_get::<Option<String>, _>("display_name")?
            .ok_or_else(limited)?;
        let digest = r.try_get("digest")?;
        result.push((
            ServiceLocationReference {
                branch_id: id.clone(),
                id,
                kind: "branch".into(),
                display_name: name,
            },
            digest,
        ));
    }
    Ok(result)
}

/// Raw retained Company identities for Service package settlement. Inactive
/// references remain visible here: a picker must not redefine a historical
/// settlement duty. The outer consumer supplies Commercial/IAM authority.
#[derive(Clone,Debug,PartialEq,Eq,serde::Deserialize)]
pub struct ServiceSettlementLocation {
    pub id: String,
    pub branch_id: Option<String>,
    pub kind: String,
    pub status: String,
}
pub struct CurrentServiceSettlementLocations {
    company:CompanyId,
    ids:Vec<String>,
    unit:Unit,
    data:Vec<ServiceSettlementLocation>,
    digest:String,
}
impl CurrentServiceSettlementLocations {
    pub fn data(&self)->&[ServiceSettlementLocation]{&self.data}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        if unit(c).await?!=self.unit{return Err(changed());}
        let(data,digest)=capture_service_settlement_locations(c,&self.company,&self.ids).await?;
        if data!=self.data||digest!=self.digest{return Err(changed());}
        Ok(())
    }
}
async fn capture_service_settlement_locations(c:&mut PgConnection,company:&CompanyId,ids:&[String])->Result<(Vec<ServiceSettlementLocation>,String),Error>{
    let row=sqlx::query("WITH source AS MATERIALIZED(SELECT * FROM organization_nodes WHERE company_id=$1 AND id=ANY($2::text[])),snapshot AS(SELECT coalesce(jsonb_agg(jsonb_build_object('id',s.id,'branch_id',s.branch_id,'kind',s.kind,'status',s.status) ORDER BY s.id),'[]'::jsonb) value,encode(sha256(convert_to(coalesce(string_agg(encode(sha256(convert_to(to_jsonb(s)::text,'UTF8')),'hex'),'' ORDER BY s.id),''),'UTF8')),'hex') digest FROM source s)SELECT CASE WHEN octet_length(value::text)<=2097152 THEN value::text END payload,digest FROM snapshot").bind(company.as_str()).bind(ids).fetch_one(c).await?;
    let payload:Option<String>=row.try_get("payload")?;
    let data=serde_json::from_str(&payload.ok_or_else(limited)?).map_err(|_|limited())?;
    Ok((data,row.try_get("digest")?))
}
pub async fn read_service_settlement_locations(c:&mut PgConnection,company:&CompanyId,ids:&[String])->Result<CurrentServiceSettlementLocations,Error>{
    let unit=unit(c).await?;
    let(data,digest)=capture_service_settlement_locations(c,company,ids).await?;
    Ok(CurrentServiceSettlementLocations{company:company.clone(),ids:ids.to_vec(),unit,data,digest})
}
