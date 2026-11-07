//! Native Customer Authority owner.
//!
//! The qualified slices are the source-bound customer access request/decision
//! command and its bounded workspace/detail read, plus the customer duplicate
//! review request and its bounded workspace/detail read. Customer identity
//! changes, duplicate review decision/reopen/merge, portal invitations/
//! enrollment, portal support threads, attachments and every other Customer
//! Authority workflow remain in the retained TypeScript owner until their
//! native slices are implemented.
mod adapters;
mod application;
mod domain;
pub mod public;
