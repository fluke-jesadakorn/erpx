//! Benefits owner. The qualified native slice reads the Company-scoped
//! plans, enrollment windows, current worker's dependents and reviewer/self
//! enrollments used by the retained Benefits workspace, and owns native plan,
//! window, dependent and election commands with current actor policy,
//! idempotency, optimistic versions, money minor units, verified dependent and
//! enrollment-window facts, persisted readback and the payroll-input handoff
//! row. Enrollment detail history/payroll consumption, attachments, Learning
//! eligibility consumption, package settlement, external provider delivery and
//! browser acceptance remain in the retained TypeScript owner or composition
//! until their native slices are implemented.
mod adapters;
mod application;
mod domain;
pub mod public;
