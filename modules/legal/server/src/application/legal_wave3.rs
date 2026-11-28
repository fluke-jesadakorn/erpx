//! Application use case and port for the native Legal write slice.
//!
//! The use case is IO-free: it validates the typed command and asks the pure
//! domain planner for the authoritative statement. The port is implemented by
//! the owner Postgres adapter on the caller's already-open serializable unit.
//! Receipt/audit/outbox effects stay in that caller unit; this use case neither
//! opens a transaction nor mints authority.
use crate::domain::legal_wave3::{LegalWave3Statement, plan, supported};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct LegalWave3Command {
    pub kind: String,
    pub values: Vec<Value>,
    pub now: i64,
}

/// Composition-owned current-actor/Company resolution and serializable unit.
/// The HTTP layer never resolves a session or grants authority itself.
pub trait LegalWave3Port: Send + Sync {
    fn statement<'a>(
        &'a self,
        command: &'a LegalWave3Command,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<LegalWave3Statement, BusinessError>>
                + Send
                + 'a,
        >,
    >;
}

/// Pure use case: typed kind + bounded values in, owner-owned statement out.
pub fn plan_legal_wave3(command: &LegalWave3Command) -> Result<LegalWave3Statement, BusinessError> {
    if !supported(&command.kind) {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "LEGAL_WAVE3_WRITE_INVALID",
            "Review the Legal owner write request.",
        ));
    }
    plan(&command.kind, command.values.clone())
}
