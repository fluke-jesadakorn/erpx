import { requestActions } from "@/lib/work_projection/request-actions";
/** Discovery categories are independent of module grants and source ownership. */
export const workplaceCategories = [
  ["people-organization", "People & organization", "people"],
  ["customers-sales", "Customers & sales", "parties"],
  ["purchasing", "Purchasing", "purchasing"],
  ["inventory-quality", "Inventory & quality", "warehouse"],
  ["projects-routines", "Projects & routines", "projects"],
  ["services", "Services", "feedback"],
  ["finance", "Finance", "finance"],
  ["pos", "POS", "commerce"],
  ["pms", "Hotel PMS", "pms"],
  ["production", "Production", "product"],
  ["knowledge", "Knowledge", "learn"],
  ["insights", "Insights", "ledger"],
  ["administration", "Administration", "settings"],
  ["erpx-management", "ERPX Management", "technology"],
] as const;
export type WorkplaceCategoryId = typeof workplaceCategories[number][0];
export type AiSupport = "guide" | "read" | "prepare";
export type WorkplaceModuleDestination = {
  categoryId: WorkplaceCategoryId | "daily";
  objective: string;
  path: string;
  aiSupport: AiSupport;
};

export const workplaceStartActions = [
  requestActions.purchase_request,
  requestActions.expense,
  requestActions.leave,
  { id: "overtime", label: "Request overtime", path: "/overtime", owner: "people_payroll", objective: "Request extra work or submit actual hours for review." },
  { id: "task", label: "Create a task", path: "/staff-tasks/new", owner: "operations", objective: "Assign a piece of work with an owner and next step." },
  { id: "post", label: "Post an update", path: "/home#compose", owner: "collaboration", objective: "Share an update, question, blocker or thanks with a chosen audience." },
  { id: "handover", label: "Pass on work", path: "/home#handover", owner: "collaboration", objective: "Hand over context while keeping the linked work's own status." },
  requestActions.service_request,
  { id: "plan", label: "Plan my day", path: "/home#plan", owner: "operations", objective: "Arrange your day around commitments and priorities." },
] as const;

/** All 33 catalogue modules have a home; this never enables a feature. */
export const workplaceModuleDestinations: Record<string, WorkplaceModuleDestination> = {
  brand: { categoryId: "administration", objective: "Draft, protect and publish the company's approved identity.", path: "settings?section=brand&tab=identity", aiSupport: "guide" },
  commercial: { categoryId: "administration", objective: "Review this company's usage and subscription.", path: "settings?section=usage", aiSupport: "guide" },
  identity_access: { categoryId: "administration", objective: "Give the right people access for the right scope and dates.", path: "technology?view=organization", aiSupport: "guide" },
  organization: { categoryId: "people-organization", objective: "Find people, positions and the team responsible for work.", path: "people", aiSupport: "guide" },
  platform_administration: { categoryId: "erpx-management", objective: "Manage ERPX provisioning and platform administration.", path: "manage-erpx", aiSupport: "guide" },
  customer_360: { categoryId: "customers-sales", objective: "Understand a customer's relationships and related sales and service history.", path: "customers", aiSupport: "guide" },
  customer_authority: { categoryId: "customers-sales", objective: "Review customer identity, duplicates and access relationships.", path: "customer-identity", aiSupport: "guide" },
  support: { categoryId: "customers-sales", objective: "Resolve customer problems with a responsible owner and service target.", path: "support", aiSupport: "guide" },
  finance: { categoryId: "finance", objective: "Review evidence, post accounting and settle approved obligations.", path: "finance", aiSupport: "read" },
  pos: { categoryId: "pos", objective: "Take orders, serve, collect payment and reconcile the shift.", path: "pos", aiSupport: "guide" },
  sales: { categoryId: "customers-sales", objective: "Turn demand into quotations, orders and verified fulfilment.", path: "commerce", aiSupport: "guide" },
  assets: { categoryId: "services", objective: "Track asset custody, location, condition and return.", path: "assets", aiSupport: "guide" },
  inventory: { categoryId: "inventory-quality", objective: "Receive, issue, reserve, transfer and count stock.", path: "commerce?view=stock", aiSupport: "guide" },
  maintenance: { categoryId: "services", objective: "Repair equipment and verify the result before closing work.", path: "assets?view=maintenance", aiSupport: "guide" },
  operations: { categoryId: "projects-routines", objective: "Review exceptions, assign work and independently verify resolutions.", path: "operations", aiSupport: "guide" },
  procurement: { categoryId: "purchasing", objective: "Compare offers, order, track deliveries and match supplier invoices.", path: "commerce?view=purchase", aiSupport: "guide" },
  quality: { categoryId: "inventory-quality", objective: "Inspect, hold and independently release or dispose of stock.", path: "quality", aiSupport: "guide" },
  legal: { categoryId: "services", objective: "Request advice and review contracts within the matter's access boundary.", path: "legal", aiSupport: "guide" },
  people_payroll: { categoryId: "people-organization", objective: "Prepare staffing, approved time and payroll for accounting and payment.", path: "people/payroll", aiSupport: "guide" },
  crm: { categoryId: "customers-sales", objective: "Know whom to contact and move opportunities toward a sale.", path: "crm", aiSupport: "guide" },
  marketing: { categoryId: "customers-sales", objective: "Publish campaigns and pass leads to sales with their source context.", path: "marketing", aiSupport: "guide" },
  manufacturing: { categoryId: "production", objective: "Issue materials, record output and loss, inspect and hand off costs.", path: "manufacturing", aiSupport: "guide" },
  planning: { categoryId: "production", objective: "Review demand, capacity and shortages before releasing a production plan.", path: "planning", aiSupport: "guide" },
  projects: { categoryId: "projects-routines", objective: "Coordinate related tasks, dependencies and handoffs.", path: "projects", aiSupport: "guide" },
  ecommerce: { categoryId: "customers-sales", objective: "Manage channels and pass orders, deliveries and returns to their owners.", path: "ecommerce", aiSupport: "guide" },
  benefits: { categoryId: "people-organization", objective: "Manage plans, eligibility, enrollment and reviewed benefit elections.", path: "benefits", aiSupport: "guide" },
  learning: { categoryId: "knowledge", objective: "Complete assigned courses and keep training results separate from reading guidance.", path: "knowledge?view=learning&section=courses", aiSupport: "guide" },
  talent: { categoryId: "people-organization", objective: "Recruit, review offers and hand selected candidates to onboarding.", path: "talent", aiSupport: "guide" },
  field_service: { categoryId: "services", objective: "Dispatch technicians, verify service evidence and release billing.", path: "field-service", aiSupport: "guide" },
  agent: { categoryId: "administration", objective: "Review AI providers, approved agents and invocation evidence.", path: "agents", aiSupport: "guide" },
  collaboration: { categoryId: "daily", objective: "Share updates, discuss blockers and hand over linked work.", path: "home", aiSupport: "read" },
  pms: { categoryId: "pms", objective: "Coordinate reservations, room readiness, stays, guest accounts and the daily close.", path: "pms/dashboard", aiSupport: "guide" },
  manage_erpx: { categoryId: "erpx-management", objective: "Manage ERPX customers, billing, provisioning and platform authority.", path: "manage-erpx", aiSupport: "guide" },
};

/** Stable destination IDs supported by the existing ordered shortcut preferences. */
export const shortcutDestinationIds = [
  "customer-followups", "crm-pipeline", "customer-timeline", "operations-quotations", "operations-sales",
  "operations-stock", "operations-purchasing", "procurement-rfqs", "operations-transit",
  "finance-payments", "finance-ledger", "finance-bank", "finance-payables", "finance-receivables",
  "projects-list", "projects-routines", "support-cases", "quality-holds", "assets", "maintenance", "field-service",
  "learning", "benefits", "talent", "marketing", "planning", "manufacturing", "executive-canvas",
] as const;

/** Translate effective rollout into display destinations without changing grants. */
export function availableNavigationAppIds(entries: readonly { id: string; state: string; permitted?: boolean }[]) {
  const enabled = new Set(entries.filter(entry => entry.permitted !== false &&
    ["Available", "Pilot", "Setup required"].includes(entry.state)).map(entry => entry.id));
  const apps = [...enabled];
  if (["organization", "identity_access", "people_payroll"].some(id => enabled.has(id))) apps.push("people");
  if (enabled.has("identity_access")) apps.push("technology");
  return apps;
}
