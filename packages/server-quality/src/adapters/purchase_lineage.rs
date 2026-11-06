//! Quality-owned remaining-hold ancestry; Count identity is an injected
//! Inventory witness. This is not actor authority or a purchase decision.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};
use std::collections::VecDeque;
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

#[derive(Clone, Debug)]
pub struct CountLineageRequest {
    pub company_id: CompanyId,
    pub branch_id: String,
    pub warehouse_id: String,
    pub count_id: String,
    pub hold_id: String,
}
/// Outer composition supplies the actual Inventory owner witness; neither
/// client input nor a caller-provided source label is evidence of a Count.
pub trait QualityLineageCountPort {
    type Witness: Send;
    fn inspect<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        request: CountLineageRequest,
    ) -> PgWorkFuture<'a, Option<Self::Witness>, BusinessError>;
    fn require_current<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        witness: &'a Self::Witness,
    ) -> PgWorkFuture<'a, (), BusinessError>;
}
#[derive(Clone)]
pub struct QualityLineageRequest {
    pub company_id: CompanyId,
    pub branch_id: Option<String>,
    pub hold_id: String,
}
struct Step<W> {
    hold: Value,
    count: W,
}
pub struct CurrentQualityPurchaseLineage<W> {
    request: QualityLineageRequest,
    start: Value,
    root: Value,
    steps: Vec<Step<W>>,
    unit: Unit,
}
impl<W: Send> CurrentQualityPurchaseLineage<W> {
    pub fn hold(&self) -> &Value {
        &self.start
    }
    pub fn root(&self) -> &Value {
        &self.root
    }
    pub async fn require_current<P: QualityLineageCountPort<Witness = W>>(
        &self,
        c: &mut PgConnection,
        counts: &mut P,
    ) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        for step in &self.steps {
            counts.require_current(&mut *c, &step.count).await?;
        }
        let current = walk(c, counts, &self.request).await?;
        if current.0 != self.start
            || current.1 != self.root
            || current.2.iter().map(|s| &s.hold).collect::<Vec<_>>()
                != self.steps.iter().map(|s| &s.hold).collect::<Vec<_>>()
        {
            return Err(changed());
        }
        Ok(())
    }
}
/// Acquire alongside Inventory and Commerce source locks before any snapshot.
pub async fn lock_quality_purchase_lineage(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query(
        "LOCK TABLE inventory_quality_holds,inventory_quality_disposition_events IN SHARE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}
pub async fn inspect_quality_purchase_lineage<P: QualityLineageCountPort>(
    c: &mut PgConnection,
    counts: &mut P,
    request: QualityLineageRequest,
) -> Result<CurrentQualityPurchaseLineage<P::Witness>, Error> {
    let unit = unit(c).await?;
    let (start, root, steps) = walk(c, counts, &request).await?;
    Ok(CurrentQualityPurchaseLineage {
        request,
        start,
        root,
        steps,
        unit,
    })
}

async fn walk<P: QualityLineageCountPort>(
    c: &mut PgConnection,
    counts: &mut P,
    request: &QualityLineageRequest,
) -> Result<(Value, Value, Vec<Step<P::Witness>>), Error> {
    let start: Option<Value> = sqlx::query_scalar("SELECT jsonb_build_object('id',h.id,'company_id',h.company_id,'branch_id',h.branch_id,'warehouse_id',h.warehouse_id,'product_id',h.product_id,'lot_id',h.lot_id,'source_type',h.source_type,'source_id',h.source_id,'spec_id',h.spec_id,'spec_version',h.spec_version,'created_by',h.created_by,'held_quantity_milli',h.held_quantity_milli) FROM inventory_quality_holds h WHERE id=$1 AND company_id=$2 AND ($3::text IS NULL OR branch_id=$3)")
        .bind(&request.hold_id).bind(request.company_id.as_str()).bind(request.branch_id.as_deref()).fetch_optional(&mut *c).await?;
    let start = start.ok_or_else(not_found)?;
    let mut root = start.clone();
    let mut max_depth = 0usize;
    let mut queue = VecDeque::from([(request.hold_id.clone(), 0usize)]);
    let mut steps = Vec::new();
    while let Some((child, depth)) = queue.pop_front() {
        if depth == 64 {
            continue;
        }
        // Quality-owned conditions exactly match the retained TS parent/event
        // predicate; Inventory independently proves each posted Count.
        let parents = sqlx::query("SELECT DISTINCT jsonb_build_object('id',parent.id,'company_id',parent.company_id,'branch_id',parent.branch_id,'warehouse_id',parent.warehouse_id,'product_id',parent.product_id,'lot_id',parent.lot_id,'source_type',parent.source_type,'source_id',parent.source_id,'spec_id',parent.spec_id,'spec_version',parent.spec_version,'created_by',parent.created_by,'held_quantity_milli',parent.held_quantity_milli) parent,parent.disposition_count_id count_id FROM inventory_quality_holds child JOIN inventory_quality_holds parent ON parent.company_id=child.company_id AND parent.branch_id=child.branch_id AND parent.warehouse_id=child.warehouse_id AND parent.product_id=child.product_id AND parent.lot_id IS NOT DISTINCT FROM child.lot_id AND parent.created_by=child.created_by AND parent.spec_id IS NOT DISTINCT FROM child.spec_id AND parent.spec_version IS NOT DISTINCT FROM child.spec_version AND parent.source_type IS NOT DISTINCT FROM child.source_type AND parent.status='disposed' JOIN inventory_quality_disposition_events e ON e.hold_id=parent.id AND e.count_id=parent.disposition_count_id AND e.action='post' WHERE child.id=$1 AND child.company_id=$2 AND child.id=parent.id||'-remaining-'||parent.disposition_count_id AND child.source_id=parent.source_id||':remaining:'||parent.disposition_count_id AND child.held_quantity_milli=parent.held_quantity_milli-parent.disposition_quantity_milli AND child.held_quantity_milli>0 ORDER BY parent LIMIT 2")
            .bind(&child).bind(request.company_id.as_str()).fetch_all(&mut *c).await?;
        // The ID delimiter (-remaining-) and source delimiter (:remaining:)
        // make two distinct parent/count decompositions impossible. Reject
        // rather than choose an origin if stored data ever violates that.
        if parents.len() > 1 {
            return Err(PersistenceError::new("QUALITY_LINEAGE_AMBIGUOUS").into());
        }
        for row in parents {
            let parent: Value = row.try_get("parent")?;
            let count_id: String = row.try_get("count_id")?;
            let text = |key: &str| -> Result<String, Error> {
                parent[key].as_str().map(str::to_owned).ok_or_else(invalid)
            };
            let id = text("id")?;
            let count = counts
                .inspect(
                    &mut *c,
                    CountLineageRequest {
                        company_id: request.company_id.clone(),
                        branch_id: text("branch_id")?,
                        warehouse_id: text("warehouse_id")?,
                        count_id,
                        hold_id: id.clone(),
                    },
                )
                .await?;
            if let Some(count) = count {
                let next = depth + 1;
                if next > max_depth {
                    root = parent.clone();
                    max_depth = next;
                }
                queue.push_back((id, next));
                steps.push(Step {
                    hold: parent,
                    count,
                });
            }
        }
    }
    Ok((start, root, steps))
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
    PersistenceError::new("QUALITY_LINEAGE_TRANSACTION_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "QUALITY_SOURCE_CHANGED",
        "The original Quality lineage changed.",
    )
    .into()
}
fn not_found() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "QUALITY_HOLD_NOT_FOUND",
        "Quality hold not found in the current workspace.",
    )
    .into()
}
