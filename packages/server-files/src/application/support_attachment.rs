//! Ports for the native Support evidence attachment intake.
//!
//! Files owns the set/link/source ledger, the private quarantine object and the
//! pending scan attempt. The authoritative Support ticket/version/audience rules
//! and the purchased `support.evidence` capability remain composition ports. The
//! ports are always called with the caller's open SERIALIZABLE unit.
use erpx_server_runtime::public::postgres::PgWorkFuture;
use erpx_server_shared::public::BusinessError;
use sqlx::PgConnection;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupportAttachmentStage {
    Read,
    Write,
}

/// Exact Support target facts presented to the owner port. The port must reject
/// a Company, branch, department, ticket version or responsibility that the
/// current selected work context cannot reach.
pub struct SupportAttachmentTarget<'a> {
    pub company_id: &'a str,
    pub ticket_id: &'a str,
    pub purpose_key: &'a str,
    pub component_kind: &'a str,
    pub component_ref: Option<&'a str>,
    /// `None` for read/replay paths; the exact set version for a write.
    pub expected_target_version: Option<i64>,
    pub stage: SupportAttachmentStage,
}

/// Ticket image returned by the Support port. It is data, not a grant: the
/// caller compares it across the Files row lock.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupportAttachmentTicketFacts {
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub version: i64,
}

/// Composition-owned Support authority. `authorize` is invoked before the Files
/// row lock and again while the set/link rows are locked; the second call is the
/// stale-authority fence. `require_purchase` checks `support.evidence` inside
/// the same connection and transaction.
pub trait SupportAttachmentAuthority: Sync {
    fn authorize<'a>(
        &'a self,
        c: &'a mut PgConnection,
        target: &'a SupportAttachmentTarget<'a>,
    ) -> PgWorkFuture<'a, SupportAttachmentTicketFacts, BusinessError>;

    fn require_purchase<'a>(
        &'a self,
        c: &'a mut PgConnection,
        company_id: &'a str,
        purpose_key: &'a str,
    ) -> PgWorkFuture<'a, (), BusinessError>;
}
