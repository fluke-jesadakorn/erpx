//! Receipt Quality effect. The coordinator must retain selected authority,
//! purchase, Inventory prepared-allocation, Workflow and notification proofs.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};
use std::future::Future;

type Error = PgTransactionError<BusinessError>;
type Unit = (String,i32,String,String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GoodsReceiptHoldFacts {
    pub command_key: String,
    pub fulfillment_id: String,
    pub line_index: usize,
    pub allocation_index: usize,
    pub company_id: String,
    pub branch_id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub lot_id: String,
    pub quantity_milli: i64,
    pub reason: String,
    pub due_date: String,
    pub actor_id: String,
    pub now: i64,
}
impl GoodsReceiptHoldFacts {
    pub fn hold_id(&self) -> String { format!("quality-{}-{}-{}",self.command_key,self.line_index,self.allocation_index) }
    pub fn source_id(&self) -> String { format!("{}-{}-{}",self.fulfillment_id,self.line_index,self.allocation_index) }
    pub fn notification_key(&self) -> String { format!("{}-{}-{}",self.command_key,self.line_index,self.allocation_index) }
}
/// The actual Inventory prepared-allocation witness supplies these facts and
/// revalidates them in this connection. It is not actor/purchase authority.
pub trait GoodsReceiptHoldSource: Sync {
    fn facts(&self) -> &GoodsReceiptHoldFacts;
    fn require_before(&self, c:&mut PgConnection) -> impl Future<Output=Result<(),Error>> + Send;
    /// Includes only the explicitly expected Quality hold-trigger effects.
    fn require_after(&self, c:&mut PgConnection) -> impl Future<Output=Result<(),Error>> + Send;
}
fn changed() -> Error { BusinessError::new(FailureKind::Conflict,"QUALITY_RECEIPT_SOURCE_CHANGED","The original receipt hold source changed.").into() }
fn invalid() -> Error { PersistenceError::new("QUALITY_RECEIPT_TRANSACTION_REQUIRED").into() }
async fn unit(c:&mut PgConnection)->Result<Unit,Error> {
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" { return Err(invalid()); }
    let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND relation='inventory_quality_holds'::regclass AND mode='ShareRowExclusiveLock') AND EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND relation='inventory_quality_specs'::regclass AND mode='ShareLock')").fetch_one(&mut *c).await?;
    if !held { return Err(invalid()); }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
/// Acquire before the first data snapshot; held-lock checks cannot prove order.
pub async fn lock_goods_receipt_hold_sources(c:&mut PgConnection)->Result<(),Error> {
    if !c.is_in_transaction() { return Err(invalid()); }
    sqlx::query("LOCK TABLE inventory_quality_holds IN SHARE ROW EXCLUSIVE MODE").execute(&mut *c).await?;
    sqlx::query("LOCK TABLE inventory_quality_specs IN SHARE MODE").execute(c).await?;
    Ok(())
}
async fn active_spec(c:&mut PgConnection,f:&GoodsReceiptHoldFacts)->Result<Option<(String,i64)>,Error> {
    Ok(sqlx::query_as("SELECT id,version FROM inventory_quality_specs WHERE company_id=$1 AND product_id=$2 AND status='active'").bind(&f.company_id).bind(&f.product_id).fetch_optional(c).await?)
}
async fn hold_image(c:&mut PgConnection,id:&str)->Result<Option<String>,Error> {
    Ok(sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(h)::text,'UTF8')),'hex') FROM inventory_quality_holds h WHERE id=$1").bind(id).fetch_optional(c).await?)
}
pub struct PreparedGoodsReceiptHold<'a,S:GoodsReceiptHoldSource> { source:&'a S, facts:GoodsReceiptHoldFacts, unit:Unit, spec:Option<(String,i64)> }
pub async fn prepare_goods_receipt_hold<'a,S:GoodsReceiptHoldSource>(c:&mut PgConnection,source:&'a S)->Result<PreparedGoodsReceiptHold<'a,S>,Error> {
    let unit=unit(c).await?;
    let facts=source.facts().clone();
    source.require_before(c).await?;
    if source.facts()!=&facts { return Err(changed()); }
    if facts.command_key.is_empty() || facts.fulfillment_id.is_empty() || facts.company_id.is_empty() || facts.branch_id.is_empty() || facts.warehouse_id.is_empty() || facts.product_id.is_empty() || facts.lot_id.is_empty() || facts.actor_id.is_empty() || facts.quantity_milli<=0 { return Err(invalid()); }
    let spec=active_spec(c,&facts).await?;
    if source.facts()!=&facts { return Err(changed()); }
    Ok(PreparedGoodsReceiptHold {source,facts,unit,spec})
}
impl<'a,S:GoodsReceiptHoldSource> PreparedGoodsReceiptHold<'a,S> {
    pub async fn apply(self,c:&mut PgConnection)->Result<RetainedGoodsReceiptHold<'a,S>,Error> {
        if unit(c).await?!=self.unit || self.source.facts()!=&self.facts { return Err(changed()); }
        self.source.require_before(c).await?;
        if self.source.facts()!=&self.facts || active_spec(c,&self.facts).await?!=self.spec { return Err(changed()); }
        let mut savepoint=c.begin().await?;
        let result:Result<String,Error>=async {
            let f=&self.facts;
            sqlx::query("INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,created_by,created_at,updated_at,lot_id,held_quantity_milli,source_type,source_id,spec_id,spec_version,quantity_scope) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$9,$10,$11,'goods_receipt',$12,$13,$14,'partial')")
                .bind(f.hold_id()).bind(&f.company_id).bind(&f.branch_id).bind(&f.warehouse_id).bind(&f.product_id).bind(&f.reason).bind(&f.due_date).bind(&f.actor_id).bind(f.now).bind(&f.lot_id).bind(f.quantity_milli).bind(f.source_id()).bind(self.spec.as_ref().map(|v|v.0.as_str())).bind(self.spec.as_ref().map(|v|v.1)).execute(&mut *savepoint).await?;
            let exact:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inventory_quality_holds h WHERE h.id=$1 AND h.company_id=$2 AND h.branch_id=$3 AND h.warehouse_id=$4 AND h.product_id=$5 AND h.reason=$6 AND h.due_date=$7 AND h.created_by=$8 AND h.created_at=$9 AND h.updated_at=$9 AND h.lot_id=$10 AND h.held_quantity_milli=$11 AND h.source_type='goods_receipt' AND h.source_id=$12 AND h.spec_id IS NOT DISTINCT FROM $13 AND h.spec_version IS NOT DISTINCT FROM $14 AND h.quantity_scope='partial' AND h.status='held' AND h.version=1 AND h.last_inspector IS NULL AND h.last_result IS NULL AND h.released_by IS NULL AND h.release_reason IS NULL AND h.released_at IS NULL AND h.cancel_requested_by IS NULL AND h.cancel_reason IS NULL AND h.cancelled_by IS NULL AND h.cancelled_at IS NULL AND h.disposition_kind IS NULL AND h.disposition_reason IS NULL AND h.disposition_requested_by IS NULL AND h.disposition_count_id IS NULL AND h.disposed_by IS NULL AND h.disposed_at IS NULL)")
                .bind(f.hold_id()).bind(&f.company_id).bind(&f.branch_id).bind(&f.warehouse_id).bind(&f.product_id).bind(&f.reason).bind(&f.due_date).bind(&f.actor_id).bind(f.now).bind(&f.lot_id).bind(f.quantity_milli).bind(f.source_id()).bind(self.spec.as_ref().map(|v|v.0.as_str())).bind(self.spec.as_ref().map(|v|v.1)).fetch_one(&mut *savepoint).await?;
            if !exact { return Err(changed()); }
            self.source.require_after(&mut savepoint).await?;
            if self.source.facts()!=f || active_spec(&mut savepoint,f).await?!=self.spec { return Err(changed()); }
            hold_image(&mut savepoint,&f.hold_id()).await?.ok_or_else(changed)
        }.await;
        match result { Ok(image)=>{savepoint.commit().await?;Ok(RetainedGoodsReceiptHold {source:self.source,facts:self.facts,unit:self.unit,spec:self.spec,image})},Err(error)=>{savepoint.rollback().await?;Err(error)} }
    }
}
pub struct RetainedGoodsReceiptHold<'a,S:GoodsReceiptHoldSource> { source:&'a S, facts:GoodsReceiptHoldFacts, unit:Unit, spec:Option<(String,i64)>, image:String }
impl<S:GoodsReceiptHoldSource> RetainedGoodsReceiptHold<'_,S> {
    pub fn hold_id(&self)->String { self.facts.hold_id() }
    pub fn notification_key(&self)->String { self.facts.notification_key() }
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error> {
        if unit(c).await?!=self.unit || self.source.facts()!=&self.facts { return Err(changed()); }
        self.source.require_after(c).await?;
        if self.source.facts()!=&self.facts || active_spec(c,&self.facts).await?!=self.spec || hold_image(c,&self.facts.hold_id()).await?.as_deref()!=Some(self.image.as_str()) { return Err(changed()); }
        Ok(())
    }
}
