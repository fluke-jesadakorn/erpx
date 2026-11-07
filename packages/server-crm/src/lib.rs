//! CRM owner. The qualified slice is the bounded lead/opportunity pipeline
//! read, the native lead reassign/disqualify/reopen and opportunity
//! link-quote/negotiate/lose/reopen/win commands with owner-local HTTP
//! ingress. Lead create/qualify, PMS outcomes, activities and the Sales
//! handoff remain in the retained TypeScript owner until their cross-owner
//! ports are composed.
mod adapters;
mod application;
mod domain;
pub mod public;
