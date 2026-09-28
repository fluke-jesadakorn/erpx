import { selfServiceHref } from "@/lib/self-service";
import type { SqlDatabase } from "./runtime/types";
import "server-only";
import type { WorkContext } from "./authorization";
import type { WorkCapabilities, WorkCreation } from "@/lib/work-cards";
import { activeStaff } from "./staff-policy";
import { ApiError } from "./http";
import { canPrepareCounts } from "./inventory-counts";
import { canPrepareTransfers } from "./inventory-transfers";
import { publicScope } from "./authorization";
import { workspaceBase } from "@/lib/workspace";

export async function hasCoveredTeam(db: SqlDatabase, actor: WorkContext) {
  return !!(await db
    .prepare(
      `SELECT 1 FROM service_team_members m
    JOIN service_teams t ON t.id=m.team_id AND t.company_id=m.company_id AND t.active=1
    JOIN service_team_coverage c ON c.team_id=t.id AND c.company_id=t.company_id
    JOIN organization_nodes n ON n.id=c.scope_node_id AND n.company_id=c.company_id AND n.status='active'
    LEFT JOIN organization_nodes b ON b.id=n.branch_id AND b.company_id=n.company_id AND b.kind='branch'
    WHERE m.company_id=? AND m.user_id=? AND (
      (c.scope_kind='holding' AND n.kind='company' AND n.id=m.company_id) OR
      (c.scope_kind='branch' AND n.kind='branch' AND (CAST(? AS TEXT) IS NULL OR n.id=?)) OR
      (c.scope_kind='department' AND n.kind='department' AND (n.branch_id IS NULL OR b.status='active') AND (CAST(? AS TEXT) IS NULL OR n.branch_id=?) AND (CAST(? AS TEXT) IS NULL OR n.id=?))
    ) LIMIT 1`,
    )
    .bind(
      actor.companyId,
      actor.userId,
      actor.branchId,
      actor.branchId,
      actor.branchId,
      actor.branchId,
      actor.departmentId,
      actor.departmentId,
    )
    .first());
}
export async function getWorkCapabilities(
  db: SqlDatabase,
  actor: WorkContext,
): Promise<WorkCapabilities> {
  const base = workspaceBase(publicScope(actor));
  const checks = await Promise.allSettled([
    activeStaff(db, actor.companyId, actor.userId).catch((e) => {
      if (e instanceof ApiError && e.status === 409) return null;
      throw e;
    }),
    hasCoveredTeam(db, actor),
  ]);
  const unavailable: string[] = [];
  if (checks[0].status === "rejected") unavailable.push("Employee setup");
  if (checks[1].status === "rejected") unavailable.push("Team coverage");
  const staff = checks[0].status === "fulfilled" && !!checks[0].value;
  const creation: WorkCreation[] = [
    ["expense", "Expense claim"],
    ["service_request", "Service request"],
    ["equipment_request", "Equipment request"],
    ["purchase_request", "Purchase request"],
    ...(actor.beta
      ? [["conversation", "Start from a conversation"]]
      : staff
        ? [
            ["task", "Task"],
            ["leave", "Leave request"],
            ["overtime", "Overtime"],
          ]
        : []),
    ...(!actor.beta && canPrepareCounts(actor)
      ? [["stock_count", "Stock count"]]
      : []),
    ...(!actor.beta && canPrepareTransfers(actor)
      ? [["transfer", "Warehouse transfer"]]
      : []),
  ].map(([kind, label]) => ({
    kind: kind as WorkCreation["kind"],
    label,
    href: selfServiceHref(base,kind) ?? (kind === "conversation" ? `${base}/home` : `${base}/work?new=${kind}`),
  }));
  return {
    creation,
    teamQueue: checks[1].status === "fulfilled" && checks[1].value,
    rosterAccess: !actor.beta && checks[0].status==="fulfilled" && checks[0].value?.roster_required===1,
    unavailable,
  };
}
