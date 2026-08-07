//! Domain-grouped integration test harness. Individual cases remain in their original files.

#[path = "g13_deployment_recovery.rs"]
mod g13_deployment_recovery;
#[path = "g14_directory_acceptance.rs"]
mod g14_directory_acceptance;
#[path = "g14_external_readiness.rs"]
mod g14_external_readiness;
#[path = "g14_local_compatibility.rs"]
mod g14_local_compatibility;
