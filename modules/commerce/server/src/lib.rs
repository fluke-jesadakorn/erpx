//! Native Commerce owner.
//!
//! The qualified slice reads Commerce-owned quotation and sales-order headers
//! and lines and performs the safe owner-local lifecycle transitions that do
//! not move Inventory, post Finance or mutate Quality. Sales-order approval
//! still requires the retained Finance credit guard and confirmed sales-order
//! cancellation still requires the retained Inventory reversal guard; both
//! fail closed. Purchase orders, fulfillment, invoicing, returns and every
//! other Commerce workflow remain in the retained TypeScript owner until their
//! native slices are implemented.
mod adapters;
mod application;
mod domain;
pub mod public;
