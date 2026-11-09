//! CRM owner. The qualified slice is the bounded lead/opportunity pipeline
//! read, the native lead reassign/disqualify/reopen and opportunity
//! link-quote/negotiate/lose/reopen/win commands with owner-local HTTP
//! ingress. Owner-local account create, Finance customer link, lead
//! create/qualify effects and staged activity create/complete/cancel/reassign
//! exist but still need their Finance, Commerce, Sales-recipient, purchase,
//! notification and Workflow composition mounts; PMS outcomes and the Sales
//! handoff remain retained until then.
mod adapters;
mod application;
mod domain;
pub mod public;
