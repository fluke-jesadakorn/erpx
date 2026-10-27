//! Actual scoped IAM/People reference, never selected Work or owner permission.
pub use crate::adapters::operational_worker::{
    CurrentOperationalWorker, OperationalWorkerPermission, OperationalWorkerRequest,
    inspect_operational_worker, lock_operational_worker_sources,
};
