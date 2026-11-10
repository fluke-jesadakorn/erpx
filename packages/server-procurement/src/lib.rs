//! Procurement owner. The qualified slices are the bounded supplier promise
//! history/open-delivery read for one purchase order, the native supplier
//! promise command (with its owner-local HTTP ingress and bounded detail read)
//! and Finance supplier bill matching. The promise route mount, RFQ, bill
//! capture, payment and requisition paths remain lead integration or retained
//! TypeScript until their own packets are certified.
mod adapters;
mod application;
mod domain;
pub mod public;
