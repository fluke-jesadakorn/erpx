export type WorkItem = {
  actionKind?: "work" | "approval" | "override";
  canAct?: boolean;
  category?: "operational" | "setup";
  setupReason?: string | null;
  flow?: import("./flow").FlowView;
  id: string;
  type:
    | "customer_identity"
    | "customer_duplicate"
    | "customer_access"
    | "support_incident"
    | "support_policy"
    | "support_sla_alert"
    | "support_ticket"
    | "support_session"
    | "maintenance_work"
    | "field_service"
    | "collaboration_action"
    | "learning_assignment"
    | "benefit_enrollment"
    | "talent_application"
    | "marketing_candidate"
    | "manufacturing_order"
    | "crm_lead"
    | "crm_activity"
    | "customer_followup"
    | "supplier_promise"
    | "procurement_rfq"
    | "supplier_bill"
    | "landed_cost"
    | "inventory_transit"
    | "quality_hold"
    | "quality_alert"
    | "supplier_batch"
    | "refund"
    | "legal"
    | "pms"
    | "pos"
    | "organization_change"
    | "company_business_acceptance"
    | "people_lifecycle"
    | "overtime"
    | "leave"
    | "expense"
    | "stock_count"
    | "transfer"
    | "staff_task"
    | "service_request"
    | "equipment_request"
    | "purchase_request"
    | "order" | "invoice" | "journal" | "payroll";
  title: string;
  detail: string;
  href: string;
  status: string;
  updatedAt: number;
  queue: "todo" | "waiting" | "completed";
  responsibleQueue?: string;
  nextAction?: string;
  dueAt?: number | null;
  blocker?: string | null;
  relationships?: ("requested" | "assigned" | "approval" | "team")[];
};
export const workTypes = [
  ["customer_identity", "Customer identity changes"],
  ["customer_duplicate", "Customer duplicate reviews"],
  ["customer_access", "Customer access reviews"],
  ["support_incident", "Support incidents"],
  ["support_policy", "Support SLA policies"],
  ["support_sla_alert", "Support target escalations"],
  ["support_ticket", "Support cases"],
  ["support_session", "Temporary support access"],
  ["maintenance_work", "Maintenance work"],
  ["field_service", "Field service"],
  ["collaboration_action", "Cross-record actions"],
  ["learning_assignment", "Learning assignments"],
  ["benefit_enrollment", "Benefit enrollments"],
  ["talent_application", "Talent applications"],
  ["marketing_candidate", "Marketing lead candidates"],
  ["manufacturing_order", "Manufacturing orders"],
  ["crm_lead", "CRM leads & opportunities"],
  ["crm_activity", "CRM activities"],
  ["customer_followup", "Customer follow-ups"],
  ["legal", "Legal"],
  ["pms", "Hotel operations"],
  ["all", "All types"],
  ["pos", "POS"],
  ["organization_change", "Organization changes"],
  ["company_business_acceptance", "Company setup acceptance"],
  ["people_lifecycle", "People & IT preparation"],
  ["order", "Sales / purchase orders"],
  ["invoice", "Invoices and settlement"],
  ["refund", "Return refunds"],
  ["supplier_batch", "Supplier payment batches"],
  ["quality_hold", "Quality holds"],
  ["quality_alert", "Quality escalations"],
  ["inventory_transit", "Stock in transit"],
  ["supplier_promise", "Supplier delivery confirmations"],
  ["procurement_rfq", "Supplier quotation reviews"],
  ["supplier_bill", "Supplier invoice matching"],
  ["landed_cost", "Inventory landed costs"],
  ["journal", "General ledger"],
  ["payroll", "Payroll"],
  ["staff_task", "Tasks"],
  ["overtime", "Overtime"],
  ["leave", "Leave requests"],
  ["expense", "Expense claims"],
  ["stock_count", "Stock counts"],
  ["transfer", "Warehouse transfers"],
  ["service_request", "Service requests"],
  ["equipment_request", "Equipment requests"],
  ["purchase_request", "Purchase requests"],
] as const;
export const workResponsibilities = [
  ["all", "All responsibilities"],
  ["requested", "My requests"],
  ["assigned", "Assigned to me"],
  ["approval", "Approvals"],
  ["team", "Team queue"],
] as const;
export function filterWork(
  items: WorkItem[],
  queue: string,
  type: string,
  q: string,
  responsibility = "all",
) {
  return items.filter(
    (item) =>
      (queue === "all" || item.queue === queue) &&
      (type === "all" || item.type === type) &&
      (responsibility === "all" ||
        item.relationships?.some((r) => r === responsibility)) &&
      `${item.title} ${item.detail} ${item.status}`
        .toLowerCase()
        .includes(q.toLowerCase()),
  );
}

export const workViews = [
  ["actions", "My actions"], ["waiting", "Waiting"], ["requests", "My requests"],
  ["team", "Team"], ["completed", "Completed"], ["all", "All work"],
  ["routing", "Routing attention"],
] as const;
export type WorkView = typeof workViews[number][0];
/** Source adapters decide authority. Legacy queues remain valid deep links. */
export function isMyAction(item: WorkItem) {
  return item.queue === "todo" && (item.canAct ?? true);
}
export function compareWork(a: WorkItem, b: WorkItem) {
  return (a.dueAt ?? Infinity) - (b.dueAt ?? Infinity) ||
    b.updatedAt - a.updatedAt || `${a.type}:${a.id}:${a.actionKind ?? "work"}`.localeCompare(`${b.type}:${b.id}:${b.actionKind ?? "work"}`);
}
export function workMatchesView(item: WorkItem, view: string) {
  return view === "routing" ? !!(item.flow?.canManage && item.flow.current && item.flow.blocker)
    : view === "actions" ? isMyAction(item)
    : view === "waiting" ? item.queue === "waiting" && !!item.relationships?.some(r => r !== "team")
    : view === "requests" ? item.relationships?.includes("requested")
    : view === "team" ? item.queue !== "completed" && (item.category === "setup" || item.relationships?.includes("team"))
    : view === "completed" ? item.queue === "completed" : true;
}
export function workInView(items: WorkItem[], view: string) {
  return items.filter(item => workMatchesView(item, view)).sort(compareWork);
}
export function workItemHref(base: string, item: WorkItem, returnTo: string) {
  const url = new URL(`${base}${item.href}`, "https://work.invalid");
  url.searchParams.set("return", returnTo);
  return `${url.pathname}${url.search}${url.hash}`;
}
