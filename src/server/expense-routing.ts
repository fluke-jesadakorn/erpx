import type { SqlDatabase } from "./runtime/types";
import { ApiError } from "./http";
import {
  activeStaffSelect,
  activeStaff,
  staffPeople,
  reportingChain,
  type ApprovalRoute,
  type RecordScope,
} from "./staff-policy";
import type { ApprovalInstance, ApprovalStep } from "./approvals";
import { assertion } from "./commands";

// Capture before eligibility reads, then compare inside the committing batch.
// This includes level/routing changes and additions as well as role revocations.
const authoritySnapshotSql = `SELECT jsonb_build_object(
 'organization',(SELECT erpx_json_array_agg(jsonb_build_object('id',id,'status',status,'parent',parent_id,'branch',branch_id)) FROM (SELECT * FROM organization_nodes WHERE company_id=? ORDER BY id)),
 'people',(SELECT erpx_json_array_agg(jsonb_build_object('id',id,'user',user_id,'status',status,'merged',merged_into_id)) FROM (SELECT * FROM hr_people WHERE company_id=? ORDER BY id)),
 'engagements',(SELECT erpx_json_array_agg(jsonb_build_object('id',id,'person',person_id,'version',version,'status',status,'branch',branch_id,'department',department_id,'level',level_id,'manager',manager_person_id,'start',start_date,'end',end_date)) FROM (SELECT * FROM hr_engagements WHERE company_id=? ORDER BY id)),
 'levels',(SELECT erpx_json_array_agg(jsonb_build_object('id',id,'rank',rank,'active',active)) FROM (SELECT * FROM staff_levels WHERE company_id=? ORDER BY id)),
 'assignments',(SELECT erpx_json_array_agg(jsonb_build_object('id',id,'user',user_id,'role',role,'scope',scope_kind,'node',scope_node_id)) FROM (SELECT * FROM assignments WHERE company_id=? AND revoked_at IS NULL AND valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) ORDER BY id))
) snapshot`;
export async function expenseAuthoritySnapshot(
  db: SqlDatabase,
  companyId: string,
) {
  const args = [companyId, companyId, companyId, companyId, companyId];
  const row = await db
    .prepare(authoritySnapshotSql)
    .bind(...args)
    .first<{ snapshot: string }>();
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
} from "@/lib/expense-workflow";

export async function expenseRoleUsers(
  db: SqlDatabase,
  scope: RecordScope,
  role: string | null,
) {
  const now = Date.now();
  const rows = await db
    .prepare(
      `SELECT staff.user_id FROM (${activeStaffSelect}) staff WHERE EXISTS (
    SELECT 1 FROM assignments a WHERE a.user_id=staff.user_id AND a.company_id=? AND (?::text IS NULL OR a.role=?) AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>?
    AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=?) OR (a.scope_kind='department' AND a.scope_node_id=?)))
    AND (?::text IS NULL OR EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=staff.company_id AND kind='branch' AND status='active'))
    AND (?::text IS NULL OR EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=staff.company_id AND kind='department' AND status='active'))
    GROUP BY staff.user_id HAVING COUNT(*)=1 ORDER BY staff.user_id`,
    )
    .bind(
      scope.company_id,
      now,
      now,
      scope.company_id,
      role,
      role,
      now,
      now,
      scope.branch_id,
      scope.department_id,
      scope.branch_id,
      scope.branch_id,
      scope.department_id,
      scope.department_id,
    )
    .all<{ user_id: string }>();
  return rows.results.map((row) => row.user_id);
}

export async function expenseRoute(
  db: SqlDatabase,
  companyId: string,
  userId: string,
  excluded: string[] = [],
  departmentOnly = false,
): Promise<ApprovalRoute> {
  let scope: RecordScope = {
    company_id: companyId,
    branch_id: null,
    department_id: null,
  };
  try {
    const authoritySnapshot = await expenseAuthoritySnapshot(db, companyId);
    const claimant = await activeStaff(db, companyId, userId);
    scope = claimant;
    if (!claimant.department_id)
      throw new ApiError(
        409,
        "DEPARTMENT_REQUIRED",
        "Set the claimant's department in People before submitting.",
      );
    if (
      !(await db
        .prepare(
          "SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='department' AND status='active'",
        )
        .bind(claimant.department_id, companyId)
        .first())
    )
      throw new ApiError(
        409,
        "DEPARTMENT_REQUIRED",
        "Select an active department in People before submitting.",
      );
    const levels = (
      await db
        .prepare(
          "SELECT id,rank FROM staff_levels WHERE company_id=? AND active=1",
        )
        .bind(companyId)
        .all<{ id: string; rank: number }>()
    ).results;
    const ranks = new Map(levels.map((l) => [l.id, l.rank]));
    const membersInScope = new Set(await expenseRoleUsers(db, scope, null));
    if (!membersInScope.has(userId))
      throw new ApiError(
        409,
        "CLAIMANT_SCOPE_REQUIRED",
        "The claimant's access must cover their active branch and department. Review their People Access panel.",
      );
    if (!claimant.level_id || !ranks.has(claimant.level_id))
      throw new ApiError(
        409,
        "LEVEL_REQUIRED",
        "Set the claimant's active staff level in People before submitting.",
      );
    const roster = await staffPeople(db, companyId),
      people = roster.filter(
        (p) =>
          roster.filter((other) => other.user_id === p.user_id).length === 1,
      ),
      chain = reportingChain(people, claimant.person_id),
      forbidden = new Set([userId, ...excluded]);
    const department = people.filter(
      (p) =>
        p.department_id === claimant.department_id &&
        p.level_id &&
        ranks.has(p.level_id),
    );
    const highest = Math.max(...department.map((p) => ranks.get(p.level_id!)!));
    let heads = department
      .filter(
        (p) =>
          ranks.get(p.level_id!) === highest &&
          !forbidden.has(p.user_id) &&
          membersInScope.has(p.user_id),
      )
      .map((p) => p.user_id);
    let authority: "named" | "department" | "holding_owner" = "department";
    if (!heads.length) {
      const superior = chain.find(
        (p) => !forbidden.has(p.user_id) && membersInScope.has(p.user_id),
      );
      if (superior) {
        heads = [superior.user_id];
        authority = "named";
      } else {
        heads = (await expenseRoleUsers(db, scope, "holding_owner")).filter(
          (id) => !forbidden.has(id),
        );
        authority = "holding_owner";
      }
    }
    if (!heads.length)
      throw new ApiError(
        409,
        "INDEPENDENT_HEAD_REQUIRED",
        "Configure an independent superior or company owner for this top-level claim.",
      );
    if (departmentOnly) return {
      scope, blockedReason: null,
      steps: [{ userId: heads[0], authority, candidateUserIds: heads, stage: "department_approval",
        snapshot: { departmentId: claimant.department_id, highestRank: highest, excluded } }],
      checks: [authoritySnapshot],
    };
    const [preparers, managers, payers] = await Promise.all(
      ["finance_preparer", "finance_approver", "treasury"].map(async (role) =>
        (await expenseRoleUsers(db, scope, role)).filter(
          (id) => !forbidden.has(id),
        ),
      ),
    );
    if (!preparers.length || !managers.length || !payers.length)
      throw new ApiError(
        409,
        "EXPENSE_TEAM_REQUIRED",
        "Configure Accounts reviewers, Accounts managers, and Treasury for this company or branch.",
      );
    heads = heads.filter((h) =>
      preparers.some(
        (p) =>
          p !== h &&
          managers.some(
            (m) =>
              m !== h && m !== p && payers.some((t) => ![h, p, m].includes(t)),
          ),
      ),
    );
    if (!heads.length)
      throw new ApiError(
        409,
        "EXPENSE_SEPARATION_REQUIRED",
        "Four different people must be available for department approval, Accounts review, Accounts manager approval, and payment.",
      );
    return {
      scope,
      blockedReason: null,
      steps: [
        {
          userId: heads[0],
          authority,
          candidateUserIds: heads,
          stage: "department_approval",
          snapshot: {
            departmentId: claimant.department_id,
            highestRank: highest,
            excluded,
          },
        },
        {
          userId: preparers[0],
          authority: "named",
          candidateUserIds: preparers,
          requiredRole: "finance_preparer",
          stage: "accounts_review",
        },
        {
          userId: managers[0],
          authority: "named",
          candidateUserIds: managers,
          requiredRole: "finance_approver",
          stage: "accounts_approval",
        },
      ],
      checks: [
        authoritySnapshot,
        {
          query:
            "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=? AND status='active')",
          args: [claimant.engagement_id, claimant.engagement_version],
        },
      ],
    };
  } catch (error) {
    if (!(error instanceof ApiError)) throw error;
    return { scope, steps: [], blockedReason: error.message };
  }
}

export async function expensePoolCandidates(
  db: SqlDatabase,
  instance: ApprovalInstance,
  step: ApprovalStep,
  steps: ApprovalStep[],
) {
  const claim = await db
    .prepare(
      "SELECT prepared_by,claimant_user_id,accounting_json FROM finance_expenses WHERE id=? AND company_id=? AND workflow_version=2",
    )
    .bind(instance.record_id, instance.company_id)
    .first<{
      prepared_by: string;
      claimant_user_id: string;
      accounting_json: string;
    }>();
  if (!claim) return [];
  const forbidden = new Set([
    claim.claimant_user_id,
    claim.prepared_by,
    ...steps
      .filter((s) => s.ordinal < step.ordinal && s.state === "approved")
      .map((s) => s.approver_user_id),
  ]);
  if (step.stage === "accounts_approval") {
    const accounting = JSON.parse(claim.accounting_json) as {
      preparedBy?: string;
    };
    if (accounting.preparedBy) forbidden.add(accounting.preparedBy);
  }
  let current: string[] = [];
  if (step.stage === "department_approval") {
    const route = await expenseRoute(
      db,
      instance.company_id,
      claim.claimant_user_id,
      [claim.prepared_by],
    );
    current = route.steps[0]?.candidateUserIds ?? [];
  } else if (step.required_role)
    current = await expenseRoleUsers(db, instance, step.required_role);
  const saved = JSON.parse(step.approver_pool_json ?? "[]") as string[];
  const candidates = current.filter(
    (id) => saved.includes(id) && !forbidden.has(id),
  );
  if (step.stage === "department_approval") return candidates;
  const payers = (await expenseRoleUsers(db, instance, "treasury")).filter(
    (id) => !forbidden.has(id),
  );
  if (step.stage === "accounts_approval")
    return candidates.filter((id) => payers.some((p) => p !== id));
  const managers = (
    await expenseRoleUsers(db, instance, "finance_approver")
  ).filter((id) => !forbidden.has(id));
  return candidates.filter((id) =>
    managers.some((m) => m !== id && payers.some((p) => p !== id && p !== m)),
  );
}

/** Employee resource requests reuse department eligibility, without expense finance stages. */
export function resourceRequestRoute(db: SqlDatabase, companyId: string, requester: string, preparer: string) {
  return expenseRoute(db, companyId, requester, [preparer], true);
}
