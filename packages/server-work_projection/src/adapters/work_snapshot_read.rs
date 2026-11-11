//! Native Work Projection snapshot composition.
//!
//! This adapter reads only owner public contracts on the caller's retained
//! serializable Holding transaction. Sources without a native owner read are
//! reported as unavailable; they are never replaced by an empty list.
use crate::adapters::work_snapshot_sources::{
    approval_items, maintenance_items, organization_workspace_change_items, preventive_plan_items,
};
use crate::domain::work_card_availability::{WorkItemFact, merge_work_items};
use erpx_server_identity_access::public::command_metadata::project_command_iam;
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_identity_access::public::work_context::WorkScopeKind;
use erpx_server_maintenance::public::read::{read_maintenance_plans, read_maintenance_workspace};
use erpx_server_organization::public::organization_nodes::{
    ChangeFilters, OrganizationAuthority, OrganizationCommand, OrganizationCommandActor,
    PgOrganizationNodes, read_workspace,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, TimestampMillis};
use erpx_server_workflow::public::approval_oversight::{
    ApprovalOversightActor, PgApprovalOversight, approval_queue,
};
use erpx_server_workflow::public::commands::CommandJson;
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

/// Sources whose TypeScript work readers have no native owner public read yet.
/// They are explicit gaps, not empty results.
const UNAVAILABLE_SOURCES: &[&str] = &[
    "fieldFinancialWork",
    "financeExpansionWork",
    "legalWork",
    "companyBusinessAcceptanceWork",
    "posWork",
    "crmWork",
    "supportAlertWork",
    "incidentWork",
    "supportPolicyWork",
    "supportWork",
    "supportSessionWork",
    "projectHandoffWork",
    "fieldServiceWork",
    "collaborationActionWork",
    "operationsExceptionWork",
    "learningWork",
    "benefitWork",
    "talentWork",
    "marketingWork",
    "manufacturingWork",
    "customerAccessWork",
    "customerDuplicateWork",
    "customerIdentityWork",
    "customerFollowupWork",
    "supplierBillWork",
    "landedCostWork",
    "rfqWork",
    "supplierPromiseWork",
    "transitWork",
    "qualityWork",
    "qualityAlertWork",
    "supplierBatchWork",
    "refundWork",
    "overtimeWork",
    "pmsWork",
    "employeeRequestWork",
    "departmentOverrideWork",
    "retirementWork",
    "peopleLifecycleWork",
    "otherFlowWork",
];

/// One native snapshot plus the sources that could not be read natively.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeWorkSnapshot {
    pub items: Vec<WorkItemFact>,
    pub unavailable_sources: Vec<&'static str>,
}

fn approval_actor(current: &AuthorizedWorkContext) -> ApprovalOversightActor {
    let workspace = current.selection().workspace();
    let assignment = current.selection().assignment();
    ApprovalOversightActor {
        user_id: current.user_id().clone(),
        holding_id: current.holding_id().clone(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        assignment_id: assignment.id.clone(),
        scope_kind: match assignment.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .to_owned(),
        permissions: current
            .command_iam_metadata()
            .permissions
            .iter()
            .cloned()
            .collect(),
    }
}

/// Compose the native snapshot from current owner public reads.
///
/// The caller must already hold the retained serializable Holding transaction.
/// The function opens no transaction, reads no HTTP request and grants no
/// authority; each owner read rechecks the selected context and its own scope.
pub async fn read_native_work_snapshot(
    connection: &mut PgConnection,
    current: &AuthorizedWorkContext,
    now: TimestampMillis,
) -> Result<NativeWorkSnapshot, Error> {
    let actor = approval_actor(current);
    let approvals = if actor.has("organization.manage")
        && actor.scope_kind == "holding"
        && actor.branch_id.is_none()
        && actor.department_id.is_none()
    {
        let mut repository = PgApprovalOversight::new(connection);
        approval_queue(&mut repository, &actor, now.get())
            .await?
            .approvals
    } else {
        Vec::new()
    };
    let mut items = approval_items(&approvals);

    if current
        .modules()
        .iter()
        .any(|module| module == "maintenance")
        && current.has_permission("records.read")
    {
        let workspace = read_maintenance_workspace(&mut *connection, current, now).await?;
        items.extend(maintenance_items(
            &workspace.items,
            current.user_id().as_str(),
            current.has_permission("records.review"),
        ));
        let plans = read_maintenance_plans(&mut *connection, current, now).await?;
        items.extend(preventive_plan_items(
            &plans,
            current.user_id().as_str(),
            current.has_permission("records.write"),
            current.has_permission("records.review"),
            now.get(),
        ));
    }

    if current.has_permission("organization.manage")
        && current.selection().assignment().scope_kind == WorkScopeKind::Holding
        && current.selection().workspace().branch_id.is_none()
        && current.selection().workspace().department_id.is_none()
    {
        let selected = current.selection();
        let workspace = selected.workspace();
        let assignment = selected.assignment();
        let actor = OrganizationCommandActor {
            holding_id: current.holding_id().clone(),
            command: OrganizationCommand {
                user_id: current.user_id().clone(),
                assignment_id: assignment.id.clone(),
                role: assignment.role.clone(),
                company_id: workspace.company_id.clone(),
                branch_id: workspace
                    .branch_id
                    .as_ref()
                    .map(|id| id.as_str().to_owned()),
                department_id: workspace
                    .department_id
                    .as_ref()
                    .map(|id| id.as_str().to_owned()),
                authority: Some(OrganizationAuthority {
                    session_id: current.session_id().as_str().to_owned(),
                    revision: selected.revision(),
                    assignment_version: assignment.version,
                }),
                iam_json: CommandJson::from_serializable(&project_command_iam(current)?)
                    .map_err(PgTransactionError::Business)?
                    .as_str()
                    .to_owned(),
                assignment_scope_kind: match assignment.scope_kind {
                    WorkScopeKind::Holding => "holding",
                    WorkScopeKind::Branch => "branch",
                    WorkScopeKind::Department => "department",
                }
                .to_owned(),
                assignment_scope_node_id: assignment
                    .scope_node_id
                    .as_ref()
                    .map(|id| id.as_str().to_owned()),
            },
            modules: current.modules().to_vec(),
            permissions: current.command_iam_metadata().permissions.clone(),
        };
        let mut repository = PgOrganizationNodes::new(&mut *connection);
        let workspace = read_workspace(&mut repository, &actor, &ChangeFilters::default())
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => PgTransactionError::Business(error),
                PgTransactionError::Persistence(error) => PgTransactionError::Persistence(error),
            })?;
        items.extend(organization_workspace_change_items(
            &workspace.node_changes,
            current.user_id().as_str(),
            current.selection().workspace().company_id.as_str(),
        ));
    }

    let mut unavailable_sources = UNAVAILABLE_SOURCES.to_vec();
    unavailable_sources.retain(|source| {
        !matches!(
            *source,
            "maintenanceWork" | "preventivePlanWork" | "organizationChangeWork"
        )
    });
    // The retained snapshot returns `mergeWorkItems(rows)`; compose the same
    // duplicate/quality-alert merge before handing facts to any consumer.
    Ok(NativeWorkSnapshot {
        items: merge_work_items(&items),
        unavailable_sources,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The retained TypeScript work reader union. Composed sources are removed
    /// from the unavailable list; every other source must stay explicit.
    const ALL_SOURCES: &[&str] = &[
        "fieldFinancialWork",
        "financeExpansionWork",
        "legalWork",
        "organizationChangeWork",
        "companyBusinessAcceptanceWork",
        "posWork",
        "crmWork",
        "supportAlertWork",
        "incidentWork",
        "supportPolicyWork",
        "supportWork",
        "supportSessionWork",
        "maintenanceWork",
        "preventivePlanWork",
        "projectHandoffWork",
        "fieldServiceWork",
        "collaborationActionWork",
        "operationsExceptionWork",
        "learningWork",
        "benefitWork",
        "talentWork",
        "marketingWork",
        "manufacturingWork",
        "customerAccessWork",
        "customerDuplicateWork",
        "customerIdentityWork",
        "customerFollowupWork",
        "supplierBillWork",
        "landedCostWork",
        "rfqWork",
        "supplierPromiseWork",
        "transitWork",
        "qualityWork",
        "qualityAlertWork",
        "supplierBatchWork",
        "refundWork",
        "overtimeWork",
        "pmsWork",
        "employeeRequestWork",
        "departmentOverrideWork",
        "retirementWork",
        "peopleLifecycleWork",
        "otherFlowWork",
    ];

    #[test]
    fn unavailable_sources_are_exactly_the_non_composed_reader_union() {
        let expected = ALL_SOURCES
            .iter()
            .copied()
            .filter(|source| {
                !matches!(
                    *source,
                    "maintenanceWork" | "preventivePlanWork" | "organizationChangeWork"
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(UNAVAILABLE_SOURCES, expected.as_slice());
        let mut unique = std::collections::BTreeSet::new();
        assert!(
            UNAVAILABLE_SOURCES
                .iter()
                .all(|source| unique.insert(*source))
        );
    }
}
