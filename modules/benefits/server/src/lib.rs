//! Benefits owner. The qualified native slice reads the Company-scoped
//! plans, enrollment windows, current worker's dependents, reviewer/self
//! enrollments, one election detail with immutable event history and its
//! released payroll input, plus released payroll-source facts for People
//! payroll. It owns native plan, window, dependent and election commands with
//! current actor policy, idempotency, optimistic versions, money minor units,
//! verified dependent and enrollment-window facts, persisted readback and the
//! payroll-input handoff row. Records/Files evidence binding and Learning
//! eligibility consumption are owner ports: the retained entrypoints fail
//! closed when they are absent, and composition mounts the concrete adapters.
//! External provider delivery and browser acceptance remain deferred.
mod adapters;
mod application;
mod domain;
pub mod public;
