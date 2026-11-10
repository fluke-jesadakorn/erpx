//! Native Customer Authority owner.
//!
//! The qualified slices are the source-bound customer access request/decision
//! command and its bounded workspace/detail read, the customer duplicate
//! review request/decision/reopen/merge commands with their HTTP router, and
//! the customer identity request/change workspace with its HTTP router. Portal
//! invitations/enrollment, portal support threads, attachments and every other
//! Customer Authority workflow remain in the retained TypeScript owner until
//! their native slices are implemented.
mod adapters;
mod application;
mod domain;
pub mod public;
