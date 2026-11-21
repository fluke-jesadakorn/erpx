//! Projects owner. Qualified native slices cover the project detail and bounded
//! WBS milestone/dependency read, the project graph read (tasks and package
//! actuals), owner-managed milestone create/revise/transition commands, native
//! task binding/handoff lifecycle commands, native time/cost actual commands
//! (Finance remains the posting owner) and the project attachment target policy
//! consumed by the Files port. Operations-owned staff task commands remain in
//! their owning retained paths until their native slices are implemented.
mod adapters;
mod application;
mod domain;
pub mod public;
