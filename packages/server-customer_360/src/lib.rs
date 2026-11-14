//! Native Customer 360 owner: bounded federated timeline and Executive activity
//! reads, customer notes, customer directory read/pagination, and the customer
//! portal projection/acknowledgement transaction. Every source owner remains
//! authoritative; portal sessions, purchases, private files, notifications and
//! workflow receipts are consumed through composition ports.
mod adapters;
mod application;
mod domain;
pub mod public;
