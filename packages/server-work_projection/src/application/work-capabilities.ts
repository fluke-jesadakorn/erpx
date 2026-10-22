import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import {requestActions} from "@erpx/feature-work_projection/lib/request-actions";import {selfServiceHref} from "@erpx/feature-work_projection/lib/self-service";import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";import type {WorkContext,publicScope} from "@erpx/server-identity_access/public/authorization";import type {StaffPerson} from "@erpx/server-operations/public/staff-policy";import type {WorkCapabilities,WorkCreation} from "@erpx/feature-work_projection/lib/work-cards";
export type WorkCapabilitiesUnit={coveredTeam(actor:WorkContext):Promise<unknown>;activeStaff(actor:WorkContext):Promise<StaffPerson|null>};
export function createWorkCapabilities(ports:{errorStatus(error:unknown):number|undefined;canPrepareCounts(actor:WorkContext):boolean;canPrepareTransfers(actor:WorkContext):boolean;publicScope:typeof publicScope}){
async function hasCoveredTeam(db: WorkCapabilitiesUnit, actor: WorkContext) {
  return !!(await db.coveredTeam(actor));
}
async function getWorkCapabilities(
  db: WorkCapabilitiesUnit,
  actor: WorkContext,
): Promise<WorkCapabilities> {
  const base = workspaceBase(ports.publicScope(actor));
  const checks = await Promise.allSettled([
    db.activeStaff(actor).catch((e) => {
      if (ports.errorStatus(e)===409) return null;
      throw e;
    }),
    hasCoveredTeam(db, actor),
  ]);
  const unavailable: string[] = [];
  if (checks[0].status === "rejected") unavailable.push("Employee setup");
  if (checks[1].status === "rejected") unavailable.push("Team coverage");
  const staff = checks[0].status === "fulfilled" && !!checks[0].value;
  const creation: WorkCreation[] = [
    ["expense", requestActions.expense.label],
    ["service_request", requestActions.service_request.label],
    ["equipment_request", "Equipment request"],
    ["purchase_request", requestActions.purchase_request.label],
    ...(!actor.beta ? [["leave", requestActions.leave.label]] : []),
    ...(actor.beta
      ? [["conversation", "Start from a conversation"]]
      : staff
        ? [
            ["task", "Task"],
            ["overtime", "Overtime"],
          ]
        : []),
    ...(!actor.beta && ports.canPrepareCounts(actor)
      ? [["stock_count", "Stock count"]]
      : []),
    ...(!actor.beta && ports.canPrepareTransfers(actor)
      ? [["transfer", "Warehouse transfer"]]
      : []),
  ].map(([kind, label]) => ({
    kind: kind as WorkCreation["kind"],
    label,
    href: selfServiceHref(base,kind) ?? (kind === "conversation" ? canonicalUiHref(`${base}/home`) : canonicalUiHref(`${base}/work?new=${kind}`)),
  }));
  return {
    creation,
    teamQueue: checks[1].status === "fulfilled" && checks[1].value,
    rosterAccess: !actor.beta && checks[0].status==="fulfilled" && checks[0].value?.roster_required===1,
    unavailable,
  };
}

return {hasCoveredTeam,getWorkCapabilities};}
