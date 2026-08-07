//! Domain-grouped integration test harness. Individual cases remain in their original files.

#[path = "finance_banking_import_jobs.rs"]
mod finance_banking_import_jobs;
#[path = "finance_banking_import_worker.rs"]
mod finance_banking_import_worker;
#[path = "finance_candidate_activation.rs"]
mod finance_candidate_activation;
#[path = "finance_chart_projection.rs"]
mod finance_chart_projection;
#[path = "operations_completion_authoritative.rs"]
mod operations_completion_authoritative;
#[path = "operations_projection.rs"]
mod operations_projection;
