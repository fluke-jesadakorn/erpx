//! Native Work Projection navigation read.
//!
//! Composes the existing native snapshot and capability reads on the caller's
//! retained serializable Holding transaction. The Home unread count and the
//! routing-attention flow fact are owner facts supplied by the caller; this
//! module opens no transaction, reads no HTTP request and grants no authority.
use crate::adapters::work_capability_probes::{
    InventoryPreparation, WorkCapabilityScope, read_work_capabilities,
};
use crate::adapters::work_snapshot_read::{NativeWorkSnapshot, read_native_work_snapshot};
use crate::domain::work_card_availability::WorkSnapshotFact;
use crate::domain::work_navigation::{WorkNavigationFact, WorkNavigationFacts, work_navigation};
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, TimestampMillis};
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

/// Already-authorized facts for one selected Work context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeWorkNavigationRequest {
    pub beta: bool,
    pub inventory: InventoryPreparation,
    pub expense_available: bool,
    /// `unreadHomeCount` from the Collaboration Home cursor owner read.
    pub home_unread: i64,
    pub include_attention: bool,
    /// `work.items.some(item => workMatchesView(item, "routing"))` from the
    /// owner flow enrichment; the availability snapshot carries no flow.
    pub routing_attention: bool,
}

/// The exact `navigationSnapshot` shape without page authority.
pub async fn read_native_work_navigation(
    connection: &mut PgConnection,
    current: &AuthorizedWorkContext,
    request: NativeWorkNavigationRequest,
    now: TimestampMillis,
) -> Result<WorkNavigationFact, Error> {
    let snapshot = read_native_work_snapshot(connection, current, now).await?;
    let NativeWorkSnapshot {
        items,
        unavailable_sources,
    } = snapshot;
    let workspace = current.selection().workspace();
    let scope = WorkCapabilityScope {
        company_id: workspace.company_id.as_str(),
        user_id: current.user_id().as_str(),
        branch_id: workspace.branch_id.as_ref().map(|id| id.as_str()),
        department_id: workspace.department_id.as_ref().map(|id| id.as_str()),
        beta: request.beta,
    };
    let capabilities = read_work_capabilities(
        connection,
        &scope,
        now.get(),
        request.inventory,
        request.expense_available,
    )
    .await?;
    let facts = WorkSnapshotFact { items };
    Ok(work_navigation(WorkNavigationFacts {
        snapshot: &facts,
        unavailable_sources: &unavailable_sources,
        capabilities: &capabilities,
        home_unread: request.home_unread,
        include_attention: request.include_attention,
        routing_attention: request.routing_attention,
    }))
}
