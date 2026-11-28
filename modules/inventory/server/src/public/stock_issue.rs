//! Sales-delivery source/guard/effect seam for the Commerce-owned fulfillment
//! command. Inventory applies stock, lot/serial balances and Sales reservation
//! consumption inside the caller's transaction; it does not own the
//! fulfillment, Finance journal or command receipt.
pub use crate::adapters::stock_issue::{
    apply_stock_issue, lock_stock_issue_sources, read_stock_issue_source,
};
pub use crate::domain::stock_issue::{
    MAX_ALLOCATIONS, MAX_LINES, MAX_LOTS_PER_LINE, MAX_RESERVATIONS_PER_LINE,
    StockIssueAllocationInput, StockIssueAllocationPlan, StockIssueAppliedAllocation,
    StockIssueAppliedLine, StockIssueAppliedReceipt, StockIssueInput, StockIssueLine,
    StockIssueLineInput, StockIssueLinePlan, StockIssueLot, StockIssueOrder, StockIssuePlan,
    StockIssueReservation, StockIssueReservationPlan, StockIssueSource, StockIssueStock,
    prepare_stock_issue, validate_stock_issue_plan,
};
