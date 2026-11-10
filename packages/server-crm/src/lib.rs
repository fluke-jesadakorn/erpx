//! CRM owner. The qualified slice is the bounded lead/opportunity pipeline
//! read, the native lead reassign/disqualify/reopen and opportunity
//! link-quote/negotiate/lose/reopen/win commands, plus owner-local HTTP
//! ingress for account create / Finance link, activity create/complete/cancel/
//! reassign and lead create/qualify. The account, activity and lead intake
//! ingress still need their Finance, Commerce, Sales-recipient, purchase,
//! notification and Workflow composition mounts; PMS outcomes and the Sales
//! handoff remain retained until then.
mod adapters;
mod application;
mod domain;
pub mod public;
