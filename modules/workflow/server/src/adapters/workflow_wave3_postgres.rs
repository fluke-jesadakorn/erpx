//! Postgres adapter for the native Workflow command-ledger slice. SQL is owned
//! by this crate; the caller executes the statement in its own serializable
//! unit together with owner effects, receipt/audit/outbox and assertions.
use crate::{
    application::workflow_wave3::{WorkflowWave3Command, plan_workflow_wave3},
    domain::workflow_wave3::WorkflowWave3Statement,
};
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::BusinessError;
use sqlx::PgConnection;

pub async fn workflow_wave3_statement(
    connection: &mut PgConnection,
    command: &WorkflowWave3Command,
) -> Result<WorkflowWave3Statement, PgTransactionError<BusinessError>> {
    let _ = connection;
    plan_workflow_wave3(command).map_err(PgTransactionError::Business)
}
