//! PostgreSQL Work Projection card-availability source.
//!
//! Composes only the owner facts this crate can currently read on the caller's
//! retained Holding transaction: the native Workflow approval queue, the
//! capability probes and the already-authorized navigation facts. It performs
//! no HTTP, session or command work and grants no page authority.
use crate::adapters::work_capability_probes::{
    InventoryPreparation, WorkCapabilityScope, read_work_capabilities,
};
use crate::adapters::work_snapshot_sources::approval_items;
use crate::domain::navigation_destinations::{NavigationShortcutFacts, shortcut_destination_ids};
use crate::domain::work_card_availability::{WorkSnapshotFact, available_work_card_ids};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use erpx_server_workflow::public::approval_oversight::{
    ApprovalOversightActor, PgApprovalOversight, approval_queue,
};
use sqlx::PgConnection;
use std::collections::BTreeSet;

type Error = PgTransactionError<BusinessError>;

/// Already-authorized facts for one selected Holding/Company work context.
pub struct WorkCardAvailabilityRequest<'a> {
    pub approval_actor: &'a ApprovalOversightActor,
    pub branch_id: Option<&'a str>,
    pub department_id: Option<&'a str>,
    pub beta: bool,
    pub permissions: &'a [String],
    pub modules: &'a [String],
    pub inventory: InventoryPreparation,
    pub expense_available: bool,
}

fn has(request: &WorkCardAvailabilityRequest<'_>, permission: &str) -> bool {
    request.permissions.iter().any(|value| value == permission)
}

/// Reads the current native availability for the exact selected context.
pub async fn read_available_work_cards(
    connection: &mut PgConnection,
    request: &WorkCardAvailabilityRequest<'_>,
    now: i64,
) -> Result<BTreeSet<String>, Error> {
    // The oversight queue is the only approval source this crate owns today.
    // It requires the exact holding-scope oversight permission; other approval
    // readers must be ported before they can contribute cards.
    let approvals = if request.approval_actor.has("organization.manage")
        && request.approval_actor.scope_kind == "holding"
        && request.approval_actor.branch_id.is_none()
        && request.approval_actor.department_id.is_none()
    {
        let mut repository = PgApprovalOversight::new(connection);
        approval_queue(&mut repository, request.approval_actor, now)
            .await?
            .approvals
    } else {
        Vec::new()
    };
    let snapshot = WorkSnapshotFact {
        items: approval_items(&approvals),
    };
    let capability_scope = WorkCapabilityScope {
        company_id: request.approval_actor.company_id.as_str(),
        user_id: request.approval_actor.user_id.as_str(),
        branch_id: request.branch_id,
        department_id: request.department_id,
        beta: request.beta,
    };
    let capabilities = read_work_capabilities(
        connection,
        &capability_scope,
        now,
        request.inventory,
        request.expense_available,
    )
    .await?;
    let navigation = NavigationShortcutFacts {
        beta: request.beta,
        scope_is_department: request.department_id.is_some(),
        executive_visible: has(request, "executive.access"),
        commerce_read: has(request, "commerce.read"),
        inventory_read: has(request, "inventory.read"),
        finance_read: has(request, "finance.read"),
        support_read: has(request, "support.read"),
        quality_read: has(request, "quality.read"),
        records_read: has(request, "records.read"),
        active_modules: request.modules,
    };
    Ok(available_work_card_ids(
        &snapshot,
        &capabilities,
        &shortcut_destination_ids(&navigation),
    ))
}
