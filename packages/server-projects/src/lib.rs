//! Projects owner. The qualified slice is the project detail and bounded WBS
//! milestone/dependency read; project, task and handoff commands remain in the
//! retained TypeScript owner until their native slices are implemented.
mod adapters;
mod application;
mod domain;
pub mod public;
