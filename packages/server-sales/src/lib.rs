//! Sales owner. Qualified slices are the bounded quotation and sales-order
//! document list read plus the Sales-owned price-list draft/activation
//! lifecycle that gates quotation preparation. Quote/order document write
//! commands remain Commerce-owned; fulfillment, cancellation and revenue
//! handoff remain in the retained TypeScript owner until their native slices
//! and cross-owner guards exist.
mod adapters;
mod application;
mod domain;
pub mod public;
