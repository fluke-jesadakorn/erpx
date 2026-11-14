//! Native Talent owner.
//!
//! Qualified native slices:
//!
//! - requisition draft/approve/reject/open/close/cancel plus workspace/detail;
//! - candidate creation with consent/retention evidence and privacy
//!   hold/release/withdraw/erase plus the privacy workspace read;
//! - application interview/select/reject/withdraw/reassign and proposal
//!   submission/accept/reject with pinned requisition/candidate versions and
//!   owner eligibility;
//! - offer draft/approve/send/accept/decline/withdraw with the
//!   monthly-compensation Money snapshot port and the accepted-offer hire
//!   handoff contract consumed by People intake.
//!
//! Interview material, attachments, package obligations, the talent workspace
//! merge, offer documents and the People hire intake itself remain with their
//! current owners. The Finance Money snapshot producer is not public yet, so
//! offer creation fails closed with
//! `TALENT_OFFER_MONEY_SNAPSHOT_UNAVAILABLE` until it is composed.
mod adapters;
mod application;
mod domain;
pub mod public;
