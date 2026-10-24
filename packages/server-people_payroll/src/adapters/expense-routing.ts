import { lastAuditOwnerProjectionSources as organizationLastAuditProjectionSources } from "@erpx/server-organization/public/last-audit-owner-projection-sources";
import { hospitalityOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as operationsProjectionSources } from "@erpx/server-operations/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/hospitality-owner-projection-sources";
import { createExpenseRoute, createLegacyExpenseRoute, createSpendingUsers, createExpensePoolCandidates, createSpendingStepCandidates, type ExpenseRoutePlan, type ExpenseRoutingPorts } from "@erpx/server-people_payroll/application/expense-routing";
/** Native Expense adapter; preserves legacy persistence and command semantics. */
import {policyRead} from "@erpx/server-identity_access/public/iam-read-snapshot";
import {personIdentityIds} from "@erpx/server-identity_access/public/iam-person";
import { reportingRosterFacts, activeDepartmentFact } from "@erpx/server-organization/public/profile-facts";
import { workflowStaffRoleUsers } from "@erpx/server-identity_access/public/workflow-role-facts";
import { activeStaffLevelRanks } from "@erpx/server-operations/public/staff-facts";
import {spendingTemplate} from '@erpx/server-people_payroll/public/spending-template-read';
import type { SpendingKind } from "@erpx/feature-shared/lib/spending";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";
import {
  activeStaff,
  staffPeople,
  reportingChain,
  type ApprovalRoute,
  type RecordScope,
} from "@erpx/server-operations/public/staff-policy";
import type {ApprovalInstance,ApprovalStep} from '@erpx/server-workflow/public/approvals';
import {assertion} from "@erpx/server-workflow/public/command-assertions";

// Capture before eligibility reads, then compare inside the committing batch.
// This includes level/routing changes and additions as well as role revocations.
const authoritySnapshotSql = `SELECT jsonb_build_object(
 'organization',(SELECT erpx_json_array_agg(jsonb_build_object('id',id,'status',status,'parent',parent_id,'branch',branch_id)) FROM (SELECT * FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE company_id=? ORDER BY id)),
 'people',(SELECT erpx_json_array_agg(jsonb_build_object('id',id,'user',user_id,'status',status,'merged',merged_into_id)) FROM (SELECT * FROM hr_people WHERE company_id=? ORDER BY id)),
 'engagements',(SELECT erpx_json_array_agg(jsonb_build_object('id',id,'person',person_id,'version',version,'status',status,'branch',branch_id,'department',department_id,'level',level_id,'manager',manager_person_id,'start',start_date,'end',end_date,'position',position_id)) FROM (SELECT * FROM hr_engagements WHERE company_id=? ORDER BY id)),
 'levels',(SELECT erpx_json_array_agg(jsonb_build_object('id',id,'rank',rank,'number',level_number,'active',active)) FROM (SELECT * FROM (${operationsProjectionSources.staff_levels}) staff_levels WHERE company_id=? ORDER BY id)),
 'positions',(SELECT erpx_json_array_agg(to_jsonb(p)) FROM (SELECT * FROM (${organizationProjectionSources.organization_positions}) organization_positions WHERE company_id=? ORDER BY id) p),
 'template',(SELECT to_jsonb(t) FROM (${organizationLastAuditProjectionSources.spending_approval_templates}) t WHERE company_id=?),
 'assignments',(SELECT erpx_json_array_agg(jsonb_build_object('id',id,'user',user_id,'role',role,'scope',scope_kind,'node',scope_node_id)) FROM (SELECT * FROM (${identity_accessProjectionSources.assignments}) assignments WHERE company_id=? AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) ORDER BY id))
) snapshot`;
export async function expenseAuthoritySnapshot(
  db: SqlDatabase,
  companyId: string,
) {
  const args = Array(7).fill(companyId) as string[];
  const row = await policyRead(db, JSON.stringify(["expense-authority-snapshot",companyId]), () => db
    .prepare(authoritySnapshotSql)
    .bind(...args)
    .first<{ snapshot: string }>());
  const check = {
    query: `(${authoritySnapshotSql})=?`,
    args: [...args, row!.snapshot],
  };
  return {
    ...check,
    guard: (key: string, label = "expense-authority-snapshot") =>
      assertion(db, key, label, check.query, check.args),
  };
}

export {
  expenseStages,
  expenseStageLabels,
  type ExpenseStage,
} from "@erpx/feature-people_payroll/lib/expense-workflow";

export async function expenseRoleUsers(db: SqlDatabase, scope: RecordScope, role: string | null) {
  return workflowStaffRoleUsers(db, scope, role);
}

/** Compose typed owner facts; SQL and concrete policy-error mapping stay native. */
export function nativeExpenseRoutingPorts(db: SqlDatabase): ExpenseRoutingPorts {
  return {
    error: violation => new ApiError(violation.status, violation.code, violation.message),
    rejection: error => error instanceof ApiError ? error.message : null,
    authority: async companyId => ({ native: await expenseAuthoritySnapshot(db, companyId) }),
    staff: (companyId, userId) => activeStaff(db, companyId, userId),
    activeDepartment: (companyId, departmentId) => activeDepartmentFact(db, companyId, departmentId),
    levels: companyId => activeStaffLevelRanks(db, companyId),
    staffPeople: companyId => staffPeople(db, companyId),
    reportingChain,
    engagementGuard: staff => ({ native: { query: "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=? AND status='active')", args: [staff.engagement_id, staff.engagement_version] } }),
    roleUsers: (scope, role) => expenseRoleUsers(db, scope, role),
    identities: (companyId, users) => personIdentityIds(db, companyId, users),
    roster: companyId => reportingRosterFacts(db, companyId),
    today: () => new Date().toISOString().slice(0, 10),
    template: companyId => spendingTemplate(db, companyId),
    async department(companyId, userId, excluded) {
      const native = await (await import("@erpx/server-organization/public/department-authority-reads")).departmentRoute(db, companyId, userId, excluded);
      return { scope: native.scope, steps: native.steps, blockedReason: native.blockedReason ?? null, guards: native.checks?.map(check => ({ native: check })) };
    },
    departmentEligible: async (companyId, userId, snapshot) => (await import("@erpx/server-organization/public/department-authority-reads")).departmentStepEligible(db, companyId, userId, snapshot),
    spendingUsers: (scope, role) => spendingUsers(db, scope, role),
    claim: instance => db.prepare("SELECT prepared_by,claimant_user_id,accounting_json FROM finance_expenses WHERE id=? AND company_id=? AND workflow_version=2").bind(instance.record_id, instance.company_id).first<{ prepared_by: string; claimant_user_id: string; accounting_json: string }>(),
  };
}
function nativeExpenseRoute(plan: ExpenseRoutePlan): ApprovalRoute {
  return { scope: plan.scope, steps: plan.steps, blockedReason: plan.blockedReason, ...(plan.guards === undefined ? {} : { checks: plan.guards.map(guard => guard.native as NonNullable<ApprovalRoute["checks"]>[number]) }) };
}
export async function legacyExpenseRoute(db: SqlDatabase, companyId: string, userId: string, excluded: string[] = [], departmentOnly = false): Promise<ApprovalRoute> {
  return nativeExpenseRoute(await createLegacyExpenseRoute(nativeExpenseRoutingPorts(db))(companyId, userId, excluded, departmentOnly));
}
export async function expensePoolCandidates(db: SqlDatabase, instance: ApprovalInstance, step: ApprovalStep, steps: ApprovalStep[]) {
  return createExpensePoolCandidates(nativeExpenseRoutingPorts(db))(instance, step, steps);
}
/** Employee resource requests reuse department eligibility, without expense finance stages. */
export function resourceRequestRoute(db: SqlDatabase, companyId: string, requester: string, preparer: string) {
  return legacyExpenseRoute(db, companyId, requester, [preparer], true);
}
/** Preserve the operation snapshot cache around the injected profile policy. */
export async function spendingUsers(db: SqlDatabase, scope: RecordScope, role: string | null) {
  return (await policyRead(db, JSON.stringify(["spending-users", scope.company_id, scope.branch_id, scope.department_id, role]), () => createSpendingUsers(nativeExpenseRoutingPorts(db))(scope, role))).slice();
}
export async function expenseRoute(db: SqlDatabase, companyId: string, userId: string, excluded: string[] = [], departmentOnly = false, kind: SpendingKind = "expense"): Promise<ApprovalRoute> {
  return nativeExpenseRoute(await createExpenseRoute(nativeExpenseRoutingPorts(db))(companyId, userId, excluded, departmentOnly, kind));
}
/** Revalidate saved participants; never resolve a new reporting chain behind an existing decision. */
export async function spendingStepCandidates(db: SqlDatabase, scope: RecordScope, requester: string, preparers: string[], step: { stage?: string; requiredRole?: string; candidateUserIds?: string[]; snapshot?: Record<string, unknown> }, decisions: string[]) {
  return createSpendingStepCandidates(nativeExpenseRoutingPorts(db))(scope, requester, preparers, step, decisions);
}
