//! Native Manufacturing production-lot posting use cases.
//!
//! The caller owns the serializable Holding transaction, the current
//! Manufacturing authority and the command receipt. This layer reads the
//! current Inventory lot facts through the owner port, applies the native
//! policy and asks the adapter to persist the derived rows in transaction
//! order. It never opens a transaction or writes another owner's row itself.
use crate::domain::lots::{
    MaterialAllocation, MaterialIssueRow, MaterialLotFacts, MaterialLotPlan, OutputAllocation,
    OutputLotInput, OutputLotPlan, TrackingMode, plan_material_lots, plan_output_lots,
    validate_material_allocations,
};
use erpx_server_shared::public::BusinessError;
use std::future::Future;
use std::pin::Pin;

pub type LotFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub struct MaterialLotRequest {
    pub tracking_mode: TrackingMode,
    pub required_milli: i64,
    pub allocations: Vec<MaterialAllocation>,
}

pub struct OutputLotRequest {
    pub tracking_mode: TrackingMode,
    pub output_milli: i64,
    pub total_minor: i64,
    pub allocations: Vec<OutputAllocation>,
}

/// Transaction context the order command supplies to the lot posting.
pub struct LotPosting<'a> {
    pub key: &'a str,
    pub order_id: &'a str,
    pub order_version: i64,
    pub warehouse_id: &'a str,
    pub product_id: &'a str,
    pub entry_date: &'a str,
    pub company_id: &'a str,
    pub user_id: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterialLotPosting {
    pub value_minor: i64,
    pub issued_lots: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputLotPosting {
    pub lots: usize,
    pub genealogy_rows: usize,
}

pub trait ManufacturingLotRepository: Send {
    type Error: From<BusinessError> + Send;

    fn material_lot<'a>(
        &'a mut self,
        company_id: &'a str,
        product_id: &'a str,
        warehouse_id: &'a str,
        lot_id: &'a str,
    ) -> LotFuture<'a, Option<MaterialLotFacts>, Self::Error>;

    fn material_lot_issues<'a>(
        &'a mut self,
        order_id: &'a str,
    ) -> LotFuture<'a, Vec<MaterialIssueRow>, Self::Error>;

    fn commit_material_lots<'a>(
        &'a mut self,
        plan: &'a MaterialLotPlan,
        posting: &'a LotPosting<'a>,
    ) -> LotFuture<'a, (), Self::Error>;

    fn commit_output_lots<'a>(
        &'a mut self,
        plan: &'a OutputLotPlan,
        posting: &'a LotPosting<'a>,
    ) -> LotFuture<'a, (), Self::Error>;
}

/// Post the material lot issue for one issued order line.
///
/// Returns `None` when the material is not lot tracked. Otherwise the owner
/// writes the lot consumption, depletion, retained issue evidence and Inventory
/// movement in the caller's transaction and returns the issued value.
pub async fn post_material_lots<R: ManufacturingLotRepository>(
    repository: &mut R,
    posting: LotPosting<'_>,
    request: MaterialLotRequest,
) -> Result<Option<MaterialLotPosting>, R::Error> {
    let allocations: Vec<MaterialAllocation> = request
        .allocations
        .into_iter()
        .filter(|allocation| allocation.product_id == posting.product_id)
        .collect();
    validate_material_allocations(request.tracking_mode, request.required_milli, &allocations)?;
    if request.tracking_mode == TrackingMode::None {
        return Ok(None);
    }
    let mut facts = Vec::with_capacity(allocations.len());
    for allocation in &allocations {
        facts.push(
            repository
                .material_lot(
                    posting.company_id,
                    posting.product_id,
                    posting.warehouse_id,
                    &allocation.lot_id,
                )
                .await?,
        );
    }
    let plan = plan_material_lots(
        request.tracking_mode,
        request.required_milli,
        posting.entry_date,
        &allocations,
        &facts,
    )?;
    let plan = match plan {
        Some(plan) => plan,
        None => return Ok(None),
    };
    repository.commit_material_lots(&plan, &posting).await?;
    Ok(Some(MaterialLotPosting {
        value_minor: plan.value_minor,
        issued_lots: plan.issues.len(),
    }))
}

/// Post accepted output lots and lot genealogy for one completed order.
pub async fn post_output_lots<R: ManufacturingLotRepository>(
    repository: &mut R,
    posting: LotPosting<'_>,
    request: OutputLotRequest,
) -> Result<Option<OutputLotPosting>, R::Error> {
    let input = OutputLotInput {
        key: posting.key,
        order_id: posting.order_id,
        order_version: posting.order_version,
        tracking_mode: request.tracking_mode,
        output_milli: request.output_milli,
        total_minor: request.total_minor,
        entry_date: posting.entry_date,
    };
    if request.tracking_mode == TrackingMode::None {
        // Untracked output writes no lot, movement or genealogy row.
        plan_output_lots(&input, &request.allocations, &[])?;
        return Ok(None);
    }
    let issues = repository.material_lot_issues(posting.order_id).await?;
    let plan = plan_output_lots(&input, &request.allocations, &issues)?;
    let plan = match plan {
        Some(plan) => plan,
        None => return Ok(None),
    };
    repository
        .commit_output_lots(&plan, &posting)
        .await?;
    Ok(Some(OutputLotPosting {
        lots: plan.lots.len(),
        genealogy_rows: plan.genealogy.len(),
    }))
}
