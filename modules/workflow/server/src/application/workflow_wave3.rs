//! Application use case and port for the native Workflow command-ledger slice.
use crate::domain::workflow_wave3::{WorkflowWave3Statement, plan, supported};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct WorkflowWave3Command {
    pub kind: String,
    pub values: Vec<Value>,
    pub now: i64,
}

pub trait WorkflowWave3Port: Send + Sync {
    fn statement<'a>(
        &'a self,
        command: &'a WorkflowWave3Command,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<WorkflowWave3Statement, BusinessError>>
                + Send
                + 'a,
        >,
    >;
}

pub fn plan_workflow_wave3(
    command: &WorkflowWave3Command,
) -> Result<WorkflowWave3Statement, BusinessError> {
    if !supported(&command.kind) {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "WORKFLOW_WAVE3_WRITE_INVALID",
            "Review the Workflow owner write request.",
        ));
    }
    plan(&command.kind, command.values.clone())
}
