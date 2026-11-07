//! Native Talent owner.
//!
//! The qualified slice is the requisition lifecycle: a branch-scoped draft
//! requisition command, independent approve/reject, open/close/cancel
//! transitions, and bounded workspace/detail reads over the existing Talent
//! requisition tables. Candidate, application, interview, proposal, offer,
//! hire handoff, privacy, attachment and package-obligation workflows remain in
//! the retained TypeScript owner until their native slices are implemented.
mod adapters;
mod application;
mod domain;
pub mod public;
