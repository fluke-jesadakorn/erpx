//! Sales owner. The qualified slice is the bounded quotation and sales-order
//! document list read. Quote/order commands, fulfillment, cancellation and
//! revenue handoff remain in the retained TypeScript owner until their native
//! slices are implemented.
mod adapters;
mod application;
mod domain;
pub mod public;
