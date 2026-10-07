import type {WorkItem} from '@/lib/work-item';
import type {Row} from '../../commerce/public/commerce';
import type {resolveFlow} from '../../workflow/public/flow-routing';
export async function salesOrderWorkItems(rows:Row[],actorId:string,resolve:(id:string)=>Promise<Awaited<ReturnType<typeof resolveFlow>>>):Promise<WorkItem[]>{return Promise.all(rows.map(async row => {
    const flow = await resolve(String(row.id)), own = row.created_by === actorId;
    return { id: String(row.id), type: "order" as const, title: `${row.number} · ${row.customer_name}`, detail: flow.current?.action ?? "Review sales document", href: `/commerce/orders/${row.id}?view=${row.kind}`, status: String(row.state), updatedAt: Number(row.updated_at), dueAt: row.due_date ? Date.parse(String(row.due_date) + "T00:00:00Z") : null, queue: flow.canAct ? "todo" as const : "waiting" as const, canAct: flow.canAct, nextAction: flow.current?.action ?? "Review sales document", responsibleQueue: flow.current?.teamName, blocker: flow.blocker, relationships: [...(own ? ["requested" as const] : []), ...(flow.canAct ? ["assigned" as const] : ["team" as const])] };
  }));
}
