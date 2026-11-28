//! Application use case and port for the native Customer Authority write slice.
use crate::domain::customer_authority_wave3::{CustomerAuthorityWave3Statement, plan, supported};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct CustomerAuthorityWave3Command {
    pub kind: String,
    pub values: Vec<Value>,
    pub now: i64,
}

pub trait CustomerAuthorityWave3Port: Send + Sync {
    fn statement<'a>(
        &'a self,
        command: &'a CustomerAuthorityWave3Command,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<CustomerAuthorityWave3Statement, BusinessError>>
                + Send
                + 'a,
        >,
    >;
}

pub fn plan_customer_authority_wave3(
    command: &CustomerAuthorityWave3Command,
) -> Result<CustomerAuthorityWave3Statement, BusinessError> {
    if !supported(&command.kind) {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "CUSTOMER_AUTHORITY_WAVE3_WRITE_INVALID",
            "Review the Customer Authority owner write request.",
        ));
    }
    plan(&command.kind, command.values.clone())
}
