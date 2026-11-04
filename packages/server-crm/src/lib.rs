//! CRM owner. The qualified slice is the bounded lead/opportunity pipeline
//! read and its owner-local HTTP ingress. Lead create/qualify/reassign,
//! opportunity transitions, activities and Sales handoff remain in the
//! retained TypeScript owner until their native slices are implemented.
mod adapters;
mod application;
mod domain;
pub mod public;
