//! Projects owner. Qualified native slices cover the project detail and bounded
//! WBS milestone/dependency read, the project graph read (tasks and package
//! actuals), and owner-managed milestone create/revise/transition commands.
//! Project/handoff commands and Operations-owned staff task commands remain in
//! their owning retained paths until their native slices are implemented.
mod adapters;
mod application;
mod domain;
pub mod public;
