import "server-only";
import type { WorkContext } from "./authorization";
import type { Row } from "./commerce";
import type { WorkItem } from "@/lib/work-item";
import { hasPermission } from "@/lib/permissions";
import { salesCustomerAccess, type HubQuery, queryValue } from "@/lib/sales-customers";
import { availableNavigationAppIds } from "@/lib/workplace-catalogue";
import { companyFeatureCatalogue } from "./hospitality-rollout";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { readListPage } from "./list-page";
import { resolveFlow } from "./flow-routing";
import { crmWork } from "./crm";
import { customerFollowupWorkspace, customerFollowupWork } from "./customer-followups";
import { customerTimeline } from "./customer-timeline";
import { customerNotesForCustomer } from "./customer-notes";
import { supportTicketReadPredicate } from "./support";
import { ApiError } from "./http";

export async function salesCustomersAccess(actor: WorkContext) {
  const apps = process.env.NODE_ENV === "production" ? availableNavigationAppIds(await companyFeatureCatalogue(actor)) : undefined;
  return salesCustomerAccess(actor, apps);
}
function orderScope(actor: WorkContext, alias = "o") {
  return { sql: `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, args: [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId] };
}
export type CustomerSummary = {
  nextFollowup: { id: string; dueDate: string; action: string; orderId: string } | null;
  openOrders: number;
  openOpportunities: number | null;
  openSupport: number | null;
};
export type CustomerDirectoryRow = Row & { id: string; page_key: string; summary: CustomerSummary };
/** Master visibility is company-wide; related evidence keeps each owner's narrower scope. */
export async function customerSummaries(actor: WorkContext, ids: string[]): Promise<Map<string, CustomerSummary>> {
  requirePermission(actor, "commerce.read");
  const access = await salesCustomersAccess(actor), result = new Map(ids.map(id => [id, { nextFollowup: null, openOrders: 0, openOpportunities: access.pipeline ? 0 : null, openSupport: access.support && hasPermission(actor, "support.read") ? 0 : null } as CustomerSummary]));
  if (!ids.length) return result;
  const { write } = await holdingResources(actor.holdingId), s = orderScope(actor), placeholders = ids.map(() => "?").join(",");
  const [orders, followups] = await Promise.all([
    write.prepare(`SELECT o.party_id,COUNT(*) count FROM commerce_orders o WHERE ${s.sql} AND o.party_id IN (${placeholders}) AND o.kind IN ('quote','sales') AND o.state NOT IN ('fulfilled','converted','cancelled') GROUP BY o.party_id`).bind(...s.args, ...ids).all<{ party_id: string; count: number }>(),
    write.prepare(`SELECT DISTINCT ON (o.party_id) o.party_id,c.id,c.due_date,c.next_action,c.order_id FROM customer_followups c JOIN commerce_orders o ON o.id=c.order_id AND o.company_id=c.company_id WHERE ${s.sql} AND o.party_id IN (${placeholders}) AND c.status='open' ORDER BY o.party_id,c.due_date,c.id`).bind(...s.args, ...ids).all<{ party_id: string; id: string; due_date: string; next_action: string; order_id: string }>(),
  ]);
  for (const row of orders.results) result.get(row.party_id)!.openOrders = Number(row.count);
  for (const row of followups.results) result.get(row.party_id)!.nextFollowup = { id: row.id, dueDate: row.due_date, action: row.next_action, orderId: row.order_id };
  if (access.pipeline) {
    const lead = orderScope(actor, "l");
    const opportunities = await write.prepare(`SELECT v.customer_id,COUNT(*) count FROM crm_opportunities v JOIN crm_leads l ON l.id=v.lead_id WHERE ${lead.sql} AND v.customer_id IN (${placeholders}) AND l.status='qualified' AND v.stage NOT IN ('won','lost') GROUP BY v.customer_id`).bind(...lead.args, ...ids).all<{ customer_id: string; count: number }>();
    for (const row of opportunities.results) result.get(row.customer_id)!.openOpportunities = Number(row.count);
  }
  if (access.support && hasPermission(actor, "support.read")) {
    const t = orderScope(actor, "t"), policy = supportTicketReadPredicate(actor);
    const support = await write.prepare(`SELECT t.customer_id,COUNT(*) count FROM support_tickets t JOIN commerce_orders o ON o.id=t.order_id AND o.company_id=t.company_id AND o.party_id=t.customer_id WHERE ${t.sql} AND ${s.sql} AND ${policy.sql} AND t.customer_id IN (${placeholders}) AND t.status NOT IN ('closed','cancelled') GROUP BY t.customer_id`).bind(...t.args, ...s.args, ...policy.bindings, ...ids).all<{ customer_id: string; count: number }>();
    for (const row of support.results) result.get(row.customer_id)!.openSupport = Number(row.count);
  }
  return result;
}
export async function customerWorkspaceDirectory(actor: WorkContext, query: HubQuery) {
  requirePermission(actor, "commerce.read");
  const { write } = await holdingResources(actor.holdingId), search = queryValue(query, "q").trim().slice(0, 200), status = queryValue(query, "status");
  if (status && !["active", "blocked"].includes(status)) throw new ApiError(422, "CUSTOMER_STATUS_INVALID", "Choose an available customer status.");
  const page = await readListPage<Row & { id: string; page_key: string }>(write, {
    sql: "SELECT p.*,p.name page_key FROM finance_parties p WHERE p.company_id=? AND p.kind='customer' AND p.person_id IS NULL AND (?='' OR strpos(lower(p.code||' '||p.name),lower(?))>0) AND (?='' OR p.status=?)",
    args: [actor.companyId, search, search, status, status], cursor: queryValue(query, "cursor") || undefined,
    // Retain compatibility with old customer-master pagination when no status is selected.
    scope: [actor.companyId, actor.branchId, "parties", "parties", "customer", search, ...(status ? [status] : [])],
  });
  const summaries = await customerSummaries(actor, page.items.map(row => row.id));
  return { ...page, items: page.items.map(row => ({ ...row, summary: summaries.get(row.id)! } as CustomerDirectoryRow)) };
}
export async function customerWorkspaceProfile(actor: WorkContext, id: string, query: HubQuery = {}) {
  requirePermission(actor, "commerce.read");
  const { write } = await holdingResources(actor.holdingId);
  const customer = await write.prepare("SELECT * FROM finance_parties WHERE id=? AND company_id=? AND kind='customer' AND person_id IS NULL").bind(id, actor.companyId).first<Row>();
  if (!customer) throw new ApiError(404, "CUSTOMER_NOT_FOUND", "Customer is not available in this workspace.");
  const summaries = await customerSummaries(actor, [id]), s = orderScope(actor), access = await salesCustomersAccess(actor);
  const orders = (await write.prepare(`SELECT o.id,o.number,o.kind,o.state,o.due_date,o.currency,(o.subtotal_minor+o.tax_minor) total_minor,o.version FROM commerce_orders o WHERE ${s.sql} AND o.party_id=? AND o.kind IN ('quote','sales') AND o.state NOT IN ('fulfilled','converted','cancelled') ORDER BY o.due_date,o.id LIMIT 30`).bind(...s.args, id).all<Row>()).results;
  const followups = (await write.prepare(`SELECT c.id,c.next_action,c.due_date,c.status,c.order_id,o.number order_number FROM customer_followups c JOIN commerce_orders o ON o.id=c.order_id AND o.company_id=c.company_id WHERE ${s.sql} AND o.party_id=? AND c.status='open' ORDER BY c.due_date,c.id LIMIT 30`).bind(...s.args, id).all<Row>()).results;
  const lead = orderScope(actor, "l");
  const opportunities = access.pipeline ? (await write.prepare(`SELECT l.id,l.title,l.due_date,v.stage,v.close_date,v.expected_minor,v.currency FROM crm_leads l JOIN crm_opportunities v ON v.lead_id=l.id WHERE ${lead.sql} AND v.customer_id=? AND l.status='qualified' AND v.stage NOT IN ('won','lost') ORDER BY v.close_date,l.id LIMIT 30`).bind(...lead.args, id).all<Row>()).results : null;
  let timeline: Awaited<ReturnType<typeof customerTimeline>> | null = null;
  let notes: Awaited<ReturnType<typeof customerNotesForCustomer>> | null = null;
  // The existing timeline/notes APIs keep their source-visibility contract.
  if (access.directory) {
    try { timeline = await customerTimeline({ ...actor, pageRequest: undefined }, id, { cursor: queryValue(query, "cursor") || undefined }); }
    catch (error) { if (!(error instanceof ApiError && error.code === "CUSTOMER_TIMELINE_NOT_FOUND")) throw error; }
    if (timeline) notes = await customerNotesForCustomer(actor, id);
  }
  let cases: Row[] | null = null;
  if (access.support && hasPermission(actor, "support.read")) {
    const t = orderScope(actor, "t"), policy = supportTicketReadPredicate(actor);
    cases = (await write.prepare(`SELECT t.id,t.title,t.status,t.due_date FROM support_tickets t JOIN commerce_orders o ON o.id=t.order_id AND o.company_id=t.company_id AND o.party_id=t.customer_id WHERE ${t.sql} AND ${s.sql} AND ${policy.sql} AND t.customer_id=? AND t.status NOT IN ('closed','cancelled') ORDER BY t.updated_at DESC,t.id LIMIT 30`).bind(...t.args, ...s.args, ...policy.bindings, id).all<Row>()).results;
  }
  const supportSource = access.support && hasPermission(actor, "support.create") ? await write.prepare(`SELECT o.id FROM commerce_orders o WHERE ${s.sql} AND o.party_id=? AND o.kind IN ('quote','sales') ORDER BY o.updated_at DESC LIMIT 1`).bind(...s.args, id).first<{ id: string }>() : null;
  return { customer, summary: summaries.get(id)!, orders, followups, opportunities, cases, timeline, notes, supportSource };
}
export async function customerReviewCounts(actor: WorkContext) {
  const access = await salesCustomersAccess(actor);
  if (!access.reviews) return null;
  const { write } = await holdingResources(actor.holdingId);
  const s = orderScope(actor);
  const rows = (await write.prepare(`SELECT 'identity' kind,COUNT(*) count FROM customer_identity_changes WHERE company_id=? AND status='pending' UNION ALL SELECT 'duplicates',COUNT(*) FROM customer_duplicate_reviews WHERE company_id=? AND status='pending' UNION ALL SELECT 'access',COUNT(*) FROM customer_access_bindings b JOIN commerce_orders o ON o.id=b.order_id AND o.company_id=b.company_id WHERE ${s.sql} AND b.status='pending'`).bind(actor.companyId, actor.companyId, ...s.args).all<{ kind: string; count: number }>()).results;
  return { identity: Number(rows.find(row => row.kind === "identity")?.count ?? 0), duplicates: Number(rows.find(row => row.kind === "duplicates")?.count ?? 0), access: Number(rows.find(row => row.kind === "access")?.count ?? 0) };
}
async function salesOrderWork(actor: WorkContext): Promise<WorkItem[]> {
  const { write } = await holdingResources(actor.holdingId), s = orderScope(actor);
  const rows = (await write.prepare(`SELECT o.*,p.name customer_name FROM commerce_orders o JOIN finance_parties p ON p.id=o.party_id AND p.company_id=o.company_id WHERE ${s.sql} AND o.kind IN ('quote','sales') AND o.state NOT IN ('fulfilled','converted','cancelled') ORDER BY o.due_date,o.id`).bind(...s.args).all<Row>()).results;
  return Promise.all(rows.map(async row => {
    const flow = await resolveFlow(write, actor, "order", String(row.id)), own = row.created_by === actor.userId;
    return { id: String(row.id), type: "order" as const, title: `${row.number} · ${row.customer_name}`, detail: flow.current?.action ?? "Review sales document", href: `/commerce/orders/${row.id}?view=${row.kind}`, status: String(row.state), updatedAt: Number(row.updated_at), dueAt: row.due_date ? Date.parse(String(row.due_date) + "T00:00:00Z") : null, queue: flow.canAct ? "todo" as const : "waiting" as const, canAct: flow.canAct, nextAction: flow.current?.action ?? "Review sales document", responsibleQueue: flow.current?.teamName, blocker: flow.blocker, relationships: [...(own ? ["requested" as const] : []), ...(flow.canAct ? ["assigned" as const] : ["team" as const])] };
  }));
}
export async function salesToday(actor: WorkContext, query: HubQuery) {
  requirePermission(actor, "commerce.read");
  const access = await salesCustomersAccess(actor), kind = queryValue(query, "kind", "all"), jobs: { label: string; load: () => Promise<WorkItem[]> }[] = [];
  if (!["all", "crm", "followups", "documents"].includes(kind)) throw new ApiError(422, "SALES_TASK_KIND_INVALID", "Choose an available task type.");
  if ((["crm", "followups"].includes(kind) && !access.pipeline) || (kind === "documents" && !access.documents)) throw new ApiError(403, "SALES_TASK_ACCESS_DENIED", "This task type is not available in your workspace.");
  const reader = { ...actor, pageRequest: undefined };
  if (access.pipeline && ["all", "crm"].includes(kind)) jobs.push({ label: "CRM activities", load: () => crmWork(reader) });
  if (access.pipeline && ["all", "followups"].includes(kind)) jobs.push({ label: "Customer follow-ups", load: () => customerFollowupWork(reader) });
  if (access.documents && ["all", "documents"].includes(kind)) jobs.push({ label: "Sales documents", load: () => salesOrderWork(reader) });
  const sources = await Promise.allSettled(jobs.map(job => job.load())), unavailable: string[] = [];
  const items: WorkItem[] = [];
  sources.forEach((source, index) => { if (source.status === "fulfilled") items.push(...source.value); else unavailable.push(jobs[index].label); });
  const team = queryValue(query, "owner") === "team", all = queryValue(query, "status") === "all", search = queryValue(query, "q").trim().toLowerCase();
  const orderId = queryValue(query, "orderId");
  const allowedFollowups = orderId ? new Set((await customerFollowupWorkspace(reader, orderId)).items.map(row => String(row.id))) : null;
  const rows = items.filter(item => (!allowedFollowups || item.type !== "customer_followup" || allowedFollowups.has(item.id)) && (all || item.queue !== "completed") && (team || item.relationships?.some(relation => ["requested", "assigned", "approval"].includes(relation))) && (!search || `${item.title} ${item.nextAction} ${item.responsibleQueue}`.toLowerCase().includes(search)));
  rows.sort((a, b) => (a.dueAt ?? Infinity) - (b.dueAt ?? Infinity) || b.updatedAt - a.updatedAt || a.id.localeCompare(b.id));
  return { items: rows, unavailable, observedAt: Date.now(), canCreateFollowup: access.pipeline && hasPermission(actor, "commerce.sales"), canReadCrm: access.pipeline, canReadDocuments: access.documents };
}
