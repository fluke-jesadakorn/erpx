//! Native Manufacturing production-lot posting contract.
//!
//! Material issue and accepted-output lot posting for one production order, to
//! be called by the native Manufacturing order command inside its serializable
//! Holding transaction. The retained TypeScript production command still owns
//! the order lifecycle, so this port has no HTTP ingress yet:
//!
//! * `post_material_lots` — validate the selected material lots against the
//!   current Inventory balance facts, then post the lot consumption, depletion,
//!   retained `manufacturing_material_lot_issues` evidence and Inventory
//!   `manufacturing_issue` movement. Returns the issued value used by the
//!   order's material cost.
//! * `post_output_lots` — validate accepted output allocations, then post the
//!   new `inventory_lots`, balance, retained `manufacturing_output_lots`
//!   evidence, Inventory `manufacturing_output` movement and lot genealogy.
//!
//! The adapter requires the caller's serializable transaction. It writes no
//! command receipt, audit or outbox row; the order command keeps that
//! responsibility. Non-lot-tracked materials and outputs return `None` and
//! write nothing.
pub use crate::adapters::lots::PgManufacturingLots;
pub use crate::application::lots::{
    LotFuture, LotPosting, ManufacturingLotRepository, MaterialLotPosting, MaterialLotRequest,
    OutputLotPosting, OutputLotRequest, post_material_lots, post_output_lots,
};
pub use crate::domain::lots::{
    Genealogy, MaterialAllocation, MaterialIssueRow, MaterialLotFacts, MaterialLotIssue,
    MaterialLotPlan, OutputAllocation, OutputLot, OutputLotInput, OutputLotPlan, TrackingMode,
    parse_material_allocation, parse_output_allocation, plan_material_lots, plan_output_lots, validate_material_allocations,
    scaled_amount,
};
