import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type {StaffPerson} from "@erpx/operations-server/public/staff-policy";
import type {CalendarNode} from "@erpx/work-projection-server/public/rules/calendar-domain";
import type {CalendarPurchaseControls} from '@erpx/work-projection-ui/lib/calendar-purchases';
export type CalendarAccess = {
  actor: WorkContext;
  staff: StaffPerson | null;
  nodes: CalendarNode[];
  scopeIds: string[];
  rosterIds: string[];
  peopleIds: string[];
  availability: boolean;
  rosterReader: boolean;
  lifecycleReader: boolean;
  teamDefault: boolean;
  manualPurchase?:CalendarPurchaseControls;
};
export function descendants(nodes: CalendarNode[], root: string) {
  const found = new Set<string>([root]);
  for (let i = 0; i < nodes.length; i++) {
    const before = found.size;
    for (const n of nodes)
      if (n.parentId && found.has(n.parentId)) found.add(n.id);
    if (before === found.size) break;
  }
  return [...found];
}
export function withinScope(
  access: CalendarAccess,
  branch: string | null,
  department: string | null,
) {
  return access.scopeIds.includes(
    department ?? branch ?? access.actor.companyId,
  );
}
