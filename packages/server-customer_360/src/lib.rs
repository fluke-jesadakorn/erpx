//! Native Customer 360 owner. The qualified slice is the bounded federated
//! customer timeline read and its owner-local HTTP ingress. Customer portal,
//! portal attachments, customer notes, customer authority and every source
//! owner command remain in the retained TypeScript owners until their native
//! slices are implemented.
mod adapters;
mod application;
mod domain;
pub mod public;
