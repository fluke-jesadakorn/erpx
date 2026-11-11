//! Sales owner. Qualified slices are the bounded quotation and sales-order
//! document list read, the Sales-owned price-list draft/activation lifecycle
//! that gates quotation preparation, and the native confirmed sales-order
//! cancellation command with its Inventory reservation-release port. Quote and
//! draft order write commands remain Commerce-owned; fulfillment, returns and
//! revenue handoff remain in the retained TypeScript owner until their native
//! slices and cross-owner guards exist.
mod adapters;
mod application;
mod domain;
pub mod public;
