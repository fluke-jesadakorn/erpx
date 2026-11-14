//! Commerce-owned stock images for the actual Service reservation-release set.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionStockGroup {
    pub warehouse_id: String,
    pub product_id: String,
    pub count: i64,
}
pub trait PgCompletionStockSource: Sync {
    fn groups(&self) -> Vec<CompletionStockGroup>;
    fn require_before(
        &self,
        c: &mut PgConnection,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send;
    fn require_after(
        &self,
        c: &mut PgConnection,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send;
}
pub struct PreparedCompletionStock {
    unit: Unit,
    groups: Vec<CompletionStockGroup>,
    before: String,
    after: String,
}
impl PreparedCompletionStock {
    pub async fn prepare<S: PgCompletionStockSource>(
        c: &mut PgConnection,
        source: &S,
    ) -> Result<Self, Error> {
        source.require_before(c).await?;
        let unit = unit(c).await?;
        let groups = source.groups();
        let (before, after) = images(c, &groups).await?;
        Ok(Self {
            unit,
            groups,
            before,
            after,
        })
    }
    pub async fn require_before<S: PgCompletionStockSource>(
        &self,
        c: &mut PgConnection,
        source: &S,
    ) -> Result<(), Error> {
        if unit(c).await? != self.unit || source.groups() != self.groups {
            return Err(changed());
        }
        source.require_before(c).await?;
        let (before, after) = images(c, &self.groups).await?;
        if before != self.before || after != self.after {
            return Err(changed());
        }
        Ok(())
    }
    pub async fn require_after<S: PgCompletionStockSource>(
        &self,
        c: &mut PgConnection,
        source: &S,
    ) -> Result<(), Error> {
        if unit(c).await? != self.unit || source.groups() != self.groups {
            return Err(changed());
        }
        source.require_after(c).await?;
        let (actual, _) = images(c, &self.groups).await?;
        if actual != self.after {
            return Err(changed());
        }
        Ok(())
    }
}
async fn images(
    c: &mut PgConnection,
    groups: &[CompletionStockGroup],
) -> Result<(String, String), Error> {
    let w: Vec<_> = groups.iter().map(|g| g.warehouse_id.as_str()).collect();
    let p: Vec<_> = groups.iter().map(|g| g.product_id.as_str()).collect();
    let n: Vec<_> = groups.iter().map(|g| g.count).collect();
    let row=sqlx::query("WITH input AS (SELECT * FROM unnest($1::text[],$2::text[],$3::bigint[]) AS i(warehouse_id,product_id,n)), rows AS (SELECT s.*,i.n FROM input i JOIN commerce_stock s ON s.warehouse_id=i.warehouse_id AND s.product_id=i.product_id) SELECT count(*)::bigint count,encode(sha256(convert_to(coalesce(jsonb_agg(to_jsonb(r)-'n' ORDER BY warehouse_id,product_id),'[]'::jsonb)::text,'UTF8')),'hex') before,encode(sha256(convert_to(coalesce(jsonb_agg((to_jsonb(r)-'n')||jsonb_build_object('version',r.version+r.n) ORDER BY warehouse_id,product_id),'[]'::jsonb)::text,'UTF8')),'hex') after FROM rows r").bind(w).bind(p).bind(n).fetch_one(c).await?;
    if row.try_get::<i64, _>("count")? != i64::try_from(groups.len()).map_err(|_| invalid())? {
        return Err(changed());
    }
    Ok((row.try_get("before")?, row.try_get("after")?))
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
    PersistenceError::new("COMMERCE_SERVICE_COMPLETION_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_SERVICE_SOURCE_CHANGED",
        "The original stock source changed.",
    )
    .into()
}
