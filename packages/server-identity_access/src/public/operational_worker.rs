//! Actual scoped IAM/People reference, never selected Work or owner permission.
pub use crate::adapters::operational_worker::{
    CurrentOperationalWorker, OperationalWorkerPermission, OperationalWorkerRequest,
    RawOperationalWorkerRequest, inspect_operational_worker, inspect_operational_worker_raw,
    lock_operational_worker_sources,
};
