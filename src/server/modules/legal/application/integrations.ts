import type {WorkContext} from "../../identity_access/public/authorization";
import type {Clock} from "../../shared/application/ports";
import type {WorkItem} from "@/lib/work-item";
import type {CalendarItem,CalendarQuery} from "@/server/modules/work_projection/public/rules/calendar-domain";
import type {LegalItem} from "../domain/model";
export type LegalAssignedTask={id:string;item_id:string;title:string;record_title:string;state:string;updated_at:number};
export function createLegalIntegrations(ports:{canRead():boolean;canEdit():boolean;canApprove():boolean;clock:Clock;featureCategories:readonly string[];repository:{records(now:number):Promise<{results:LegalItem[]}>;tasks(category:string):Promise<{results:LegalAssignedTask[]}>;approvals():Promise<{results:LegalItem[]}>;calendar(q:CalendarQuery,from:string,to:string,take:number):Promise<{results:(LegalItem&{item_id:string})[]}>}}){
function legalOwnerLabel(ownerId: string, actor: WorkContext) {
  return ownerId === actor.userId ? "Me" : "Legal owner";
}

async function legalWork(
  a: WorkContext,
): Promise<WorkItem[]> {
  if (!ports.canRead()) return [];
  const records = (
    await ports.repository.records(ports.clock.now())
  ).results.map((i) => ({
    id: i.id,
    type: "legal",
    title: i.title,
    detail: `${i.kind} · ${i.state}`,
    href: `/legal?record=${encodeURIComponent(i.id)}`,
    status: i.state,
    updatedAt: i.updated_at,
    queue: i.owner_id === a.userId ? "todo" : "waiting",
    canAct: i.owner_id === a.userId && ports.canEdit(),
    responsibleQueue: legalOwnerLabel(i.owner_id, a),
    relationships: [i.owner_id === a.userId ? "assigned" : "requested"],
  })) as WorkItem[];
  const tasks: WorkItem[] = [];
  for (const category of ports.featureCategories) {
    const rows = (
      await ports.repository.tasks(category)
    ).results;
    tasks.push(
      ...rows.map((r) => ({
        id: `legal-task:${r.id}`,
        type: "legal" as const,
        title: r.title,
        detail: `${r.record_title} · ${r.state}`,
        href: `/legal?record=${encodeURIComponent(r.item_id)}`,
        status: r.state,
        updatedAt: r.updated_at,
        queue: "todo" as const,
        canAct: ports.canEdit(),
        responsibleQueue: "Me",
        relationships: ["assigned" as const],
      })),
    );
  }
  const approvals = (
    await ports.repository.approvals()
  ).results;
  for (const i of approvals)
    tasks.push({
      id: `legal-approval:${i.id}`,
      type: "legal",
      title: i.title,
      detail: "Legal approval assigned to you",
      href: `/legal?record=${encodeURIComponent(i.id)}`,
      status: "approval",
      updatedAt: i.updated_at,
      queue: "todo",
      canAct: ports.canApprove(),
      responsibleQueue: "Me",
      relationships: ["assigned"],
    });
  return [...records, ...tasks];
}
async function legalCalendar(
  a: WorkContext,
  q: CalendarQuery,
  from: string,
  to: string,
  take: number,
): Promise<CalendarItem[]> {
  if (!ports.canRead()) return [];
  return (
    await ports.repository.calendar(q,from,to,take)
  ).results.map((i) => ({
    id: `legal:${i.id}`,
    source: "legal",
    sourceId: i.item_id,
    kind: "task",
    title: i.title,
    description: i.description,
    allDay: true,
    startDate: i.due_date,
    endDate: i.due_date,
    startsAt: null,
    endsAt: null,
    timezone: i.timezone,
    departmentId: i.department_id,
    branchId: i.branch_id,
    participants: [],
    owner: legalOwnerLabel(i.owner_id, a),
    status: i.state,
    version: i.version,
    permittedActions: [],
    href: `/legal?record=${encodeURIComponent(i.item_id)}`,
    warnings: [],
    history: [],
  }));
}

return {legalWork,legalCalendar};}
