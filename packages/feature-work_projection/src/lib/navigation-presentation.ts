import { canonicalUiHref, destinationForPath, sourcePath, workspacePathParts, type Destination } from "./destination-registry";
import { destinationAvailable } from "./destination-access";
import { workplaceCategories, type WorkplaceCategoryId } from "./workplace-catalogue";
import type { NavigationContext, NavigationIconId, WorkplaceNavigationModel } from "./workplace-navigation";

type WorkEntry = { id: string; categoryId: WorkplaceCategoryId | "daily"; en: string; th: string; icon: NavigationIconId; landing: string; mode?: "index" };
const entry = (categoryId: WorkEntry["categoryId"], id: string, en: string, th: string, icon: NavigationIconId, landing: string, mode?: "index"): WorkEntry => ({categoryId, id, en, th, icon, landing, mode});

/** Presentation only. Destinations and owner readers continue to decide access.
 * A landing is a preference, never permission to discover or open that page. */
export const workEntries: readonly WorkEntry[] = [
  entry("people-organization", "people-directory", "Employees", "พนักงาน", "people", "people-organization/people/directory/current"),
  entry("people-organization", "people-roster", "Roster", "ตารางเวร", "calendar", "people-organization/people/roster"),
  entry("people-organization", "people-payroll", "Payroll", "เงินเดือน", "payroll", "people-organization/people/payroll/dashboard"),
  entry("people-organization", "people-evaluation", "Skills & performance", "ทักษะและประเมินผล", "people", "people-organization/people/evaluation/table"),
  entry("people-organization", "talent", "Recruitment", "สรรหา", "people", "people-organization/talent"),
  entry("people-organization", "benefits", "Benefits", "สวัสดิการ", "people", "people-organization/benefits/plans"),
  entry("people-organization", "people-external", "External collaborators", "ผู้ร่วมงานภายนอก", "people", "people-organization/people/external/users"),
  entry("people-organization", "people-unclassified", "Unclassified", "ยังไม่จัดประเภท", "people", "people-organization/people/unclassified"),
  entry("administration", "technology-accounts", "Accounts", "บัญชีผู้ใช้", "access", "administration/technology/resources/accounts"),
  entry("people-organization", "people-settings", "HR settings", "ตั้งค่า HR", "settings", "people-organization/people/hr-settings", "index"),
  entry("customers-sales", "sales-hub", "Sales work", "งานขาย", "commerce", "customers-sales/sales/today"),
  entry("customers-sales", "crm-pipeline", "Opportunities", "โอกาสการขาย", "quotation", "customers-sales/sales/pipeline"),
  entry("customers-sales", "customers-hub", "Customers & contacts", "ลูกค้าและผู้ติดต่อ", "parties", "customers-sales/customers/directory"),
  entry("customers-sales", "sales-documents", "Sales documents", "เอกสารการขาย", "invoice", "customers-sales/sales/documents/sales"),
  entry("customers-sales", "marketing", "Marketing", "การตลาด", "feed", "customers-sales/marketing"),
  entry("customers-sales", "ecommerce", "Sales channels", "ช่องทางขาย", "commerce", "customers-sales/ecommerce"),
  entry("purchasing", "operations-purchasing", "Purchase orders", "ใบสั่งซื้อ", "purchasing", "purchasing/orders"),
  entry("purchasing", "procurement-rfqs", "Quotation requests", "ขอใบเสนอราคา", "quotation", "purchasing/rfqs"),
  entry("purchasing", "procurement-suppliers", "Suppliers", "ผู้ขาย", "parties", "purchasing/suppliers"),
  entry("inventory-quality", "operations-stock", "Stock & movements", "สต็อกและความเคลื่อนไหว", "stock", "inventory-quality/stock"),
  entry("inventory-quality", "operations-counts", "Stock counts", "ตรวจนับ", "stock-count", "inventory-quality/counts"),
  entry("inventory-quality", "operations-transfers", "Transfers & receiving", "โอนและรับสินค้า", "stock-transfer", "inventory-quality/transfers"),
  entry("inventory-quality", "operations-products", "Products", "สินค้า", "product", "inventory-quality/products"),
  entry("inventory-quality", "operations-warehouses", "Warehouses", "คลังสินค้า", "warehouse", "inventory-quality/warehouses"),
  entry("inventory-quality", "quality-holds", "Quality", "คุณภาพ", "stock-count", "inventory-quality/quality"),
  entry("projects-routines", "projects-list", "Projects", "โครงการ", "projects", "projects-routines/projects"),
  entry("projects-routines", "projects-routines", "Routines", "งานประจำ", "routine", "projects-routines/routines"),
  entry("projects-routines", "operations", "Exception tracking", "ติดตามข้อยกเว้น", "projects", "projects-routines/operations"),
  entry("services", "service-desk", "Service center", "ศูนย์บริการ", "feedback", "services/desk"),
  entry("services", "field-service", "Field service", "บริการภาคสนาม", "technology", "services/field-service"),
  entry("services", "assets", "Assets & maintenance", "ทรัพย์สินและซ่อมบำรุง", "technology", "services/assets/register"),
  entry("services", "legal", "Legal", "กฎหมาย", "handbook", "services/legal/mine"),
  entry("finance", "finance-overview", "Finance work", "งานการเงิน", "home", "finance/today/actions"),
  entry("finance", "finance-sales", "Finance documents", "เอกสารการเงิน", "invoice", "finance/documents/sales/invoices"),
  entry("finance", "finance-payments", "Cash & banking", "เงินสดและธนาคาร", "bank", "finance/cash/payments"),
  entry("finance", "finance-ledger", "Accounting & tax", "บัญชีและภาษี", "ledger", "finance/accounting/journals"),
  entry("finance", "finance-reports", "Reports", "รายงาน", "ledger", "finance/reports/company", "index"),
  entry("finance", "finance-setup", "Finance settings", "ตั้งค่าการเงิน", "settings", "finance/setup", "index"),
  entry("pos", "pos-front", "Open POS", "เปิดหน้าขาย", "commerce", "pos"),
  entry("pos", "pos-approvals", "Approvals", "อนุมัติ", "admin-approvals", "pos/approvals"),
  entry("pos", "pos-reports", "Reports", "รายงาน", "ledger", "pos/reports"),
  entry("pos", "pos-settings", "Settings", "ตั้งค่า", "settings", "pos/settings", "index"),
  entry("pms", "pms-front", "Open hotel work", "เปิดงานโรงแรม", "pms", "pms/dashboard"),
  entry("pms", "pms-approvals", "Approvals", "อนุมัติ", "admin-approvals", "pms/approvals"),
  entry("pms", "pms-reports", "Reports", "รายงาน", "ledger", "pms/reports"),
  entry("pms", "pms-settings", "Settings", "ตั้งค่า", "settings", "pms/settings", "index"),
  entry("production", "planning", "Production planning", "วางแผนการผลิต", "calendar", "production/planning"),
  entry("production", "manufacturing", "Production work", "งานผลิต", "product", "production/manufacturing"),
  entry("knowledge", "handbook", "Company handbook", "คู่มือบริษัท", "handbook", "knowledge/handbook/published"),
  entry("knowledge", "learning", "Learning", "การเรียนรู้", "education", "knowledge/learning/courses"),
  entry("knowledge", "learn", "Help & guides", "วิธีใช้และช่วยเหลือ", "learn", "knowledge/guides", "index"),
  entry("insights", "executive-canvas", "Overview & analytics", "ภาพรวมและวิเคราะห์", "ledger", "insights/executive"),
  entry("administration", "technology-organization", "IT accounts & resources", "บัญชีและทรัพยากร IT", "technology", "administration/technology/organization"),
  entry("administration", "technology-directory", "Directory", "Directory", "admin-iam", "administration/technology/directory/objects"),
  entry("administration", "technology-settings", "Access & security", "สิทธิ์และความปลอดภัย", "access", "administration/technology/settings/roles", "index"),
  entry("administration", "administration-approvals", "Approval routing", "เส้นทางอนุมัติ", "admin-approvals", "administration/approvals/templates", "index"),
  entry("administration", "administration-ai", "AI & agents", "AI และ Agents", "admin-ai", "administration/ai", "index"),
  entry("administration", "administration-usage", "Usage & billing", "การใช้งานและค่าบริการ", "admin-usage", "administration/usage"),
  entry("administration", "administration-brand", "Company settings", "ตั้งค่าบริษัท", "settings", "administration/brand/identity", "index"),
  entry("administration", "technology-business-readiness", "Readiness & rollout", "ความพร้อมและการเปิดใช้", "stock-count", "administration/business-readiness", "index"),
  entry("erpx-management", "erpx-management", "Overview", "ภาพรวม", "technology", "erpx-management"),
  entry("erpx-management", "manage-erpx-billing", "Customer billing", "ค่าบริการลูกค้า", "finance", "erpx-management/billing/target"),
  entry("erpx-management", "manage-erpx-usage", "Customer usage", "การใช้งานลูกค้า", "admin-usage", "erpx-management/usage"),
  entry("erpx-management", "manage-erpx-iam", "Platform IAM", "Platform IAM", "access", "erpx-management/iam/overview", "index"),
  entry("erpx-management", "manage-erpx-executive", "Platform Executive", "Platform Executive", "projects", "erpx-management/executive"),
];

/** Exact owner sources, independent of neighboring URL shapes. */
const sourceEntries: Record<string, string> = {
  home: "home", feed: "home", posts: "home", work: "work", "work/routing": "work", "work/reports": "work", "staff-tasks": "work", calendar: "calendar", inbox: "inbox",
  "people/organization": "people-directory", "people/move": "people-directory", "people/clearance": "people-directory", "people/hire-handoffs": "people-directory", "people/roster": "people-roster", "people/settings": "people-settings", "people/evaluation": "people-evaluation", "people/payroll": "people-payroll", "people/payslips": "people-payroll", talent: "talent", benefits: "benefits",
  crm: "crm-pipeline", "customer-followups": "sales-hub", customers: "customers-hub", "customer-identity": "customers-hub", "customer-duplicates": "customers-hub", "customer-access": "customers-hub", marketing: "marketing", ecommerce: "ecommerce",
  "procurement-rfqs": "procurement-rfqs", "supplier-bills": "operations-purchasing", "landed-costs": "operations-purchasing", "stock-counts": "operations-counts", "stock-transfers": "operations-transfers", "stock-transit": "operations-transfers", quality: "quality-holds",
  projects: "projects-list", "routine-templates": "projects-routines", operations: "operations", requests: "service-desk", support: "service-desk", "field-service": "field-service", assets: "assets", maintenance: "assets", legal: "legal",
  finance: "finance-overview", "finance/sales": "finance-sales", "finance/invoices": "finance-sales", expenses: "finance-sales", "finance/contracts": "finance-sales", "finance/due-drafts": "finance-sales", "finance/payments": "finance-payments", "finance/bank": "finance-payments", "finance/supplier-batches": "finance-payments", "finance/refunds": "finance-payments", "finance/collections": "finance-payments", "finance/accounting": "finance-ledger", "finance/journals": "finance-ledger", "finance/tax": "finance-ledger", "finance/close": "finance-ledger", "finance/hotels": "finance-ledger", "finance/hotel-accounts": "finance-ledger", "finance/hotel-documents": "finance-ledger", "finance/restaurants": "finance-ledger", "finance/engines": "finance-ledger", "finance/assets": "finance-ledger", "finance/instruments": "finance-ledger", "finance/book-adjustments": "finance-ledger", "finance/reports": "finance-reports", "finance/group": "finance-reports", "finance/group-editions": "finance-reports", "finance/statement-packages": "finance-reports", "finance/setup": "finance-setup", "finance/hospitality-settings": "finance-setup", "finance/hotel-import": "finance-setup", "finance/asset-policies": "finance-setup", "finance/thai-tax": "finance-setup", "finance/thai-documents": "finance-setup",
  planning: "planning", manufacturing: "manufacturing", handbook: "handbook", learning: "learning", learn: "learn", "future-features": "learn", executive: "executive-canvas",
  agents: "administration-ai", usage: "administration-usage", "settings/business-readiness": "technology-business-readiness", "settings/pilot-features": "technology-business-readiness", "settings/notifications": "inbox", "settings/stripe": "administration-brand",
  "manage-erpx": "erpx-management", "manage-erpx/billing": "manage-erpx-billing", "manage-erpx/billing/usage": "manage-erpx-usage", "manage-erpx/iam": "manage-erpx-iam", "manage-erpx/executive": "manage-erpx-executive",
};

export function destinationWorkEntry({ source, selection }: Pick<Destination, "source" | "selection">): string | undefined {
  if (source === "people") return selection.view === "external" ? "people-external" : selection.view === "unclassified" ? "people-unclassified" : selection.view === "users" ? "technology-accounts" : "people-directory";
  if (source === "sales") return selection.view === "pipeline" ? "crm-pipeline" : selection.view === "documents" || selection.view === "settings" ? "sales-documents" : "sales-hub";
  if (source === "commerce") return ({purchase: "operations-purchasing", parties: selection.kind === "supplier" ? "procurement-suppliers" : "customers-hub", stock: "operations-stock", products: "operations-products", warehouses: "operations-warehouses", pricing: "sales-hub", quote: "sales-documents", sales: "sales-documents"} as Record<string,string>)[selection.view ?? "sales"];
  if (source === "technology" && selection.view === "resources" && selection.section === "accounts") return "technology-accounts";
  if (source === "technology") return ({directory: "technology-directory", settings: "technology-settings"} as Record<string,string>)[selection.view] ?? "technology-organization";
  if (source === "settings") return ({approvals: "administration-approvals", brand: "administration-brand", ai: "administration-ai", usage: "administration-usage", feedback: "learn", people: "technology-organization", organization: "administration-brand"} as Record<string,string>)[selection.section];
  if (source === "knowledge") return selection.view === "handbook" ? "handbook" : "learning";
  if (source === "pos") return selection.tab === "settings" ? "pos-settings" : selection.tab === "reports" ? "pos-reports" : selection.queue === "approval" ? "pos-approvals" : "pos-front";
  if (source === "support" || source.startsWith("support/")) return "customers-hub";
  if (source === "pms/guest-accounts" && selection.queue === "approval") return "pms-approvals";
  if (source === "pms/reports") return "pms-reports";
  if (["pms/settings", "pms/import", "pms/setup", "hospitality/setup"].includes(source)) return "pms-settings";
  if (source === "pms" || source.startsWith("pms/")) return "pms-front";
  const owner = Object.keys(sourceEntries).filter(key => source === key || source.startsWith(key + "/")).sort((a,b) => b.length-a.length)[0];
  return owner ? sourceEntries[owner] : undefined;
}

export function workEntryForHref(href: string, base: string): string | undefined {
  const url = new URL(canonicalUiHref(href), "https://navigation.invalid"), parts = workspacePathParts(url.pathname);
  if (!parts || parts.base !== base) return undefined;
  const destination = destinationForPath(parts.path);
  if (destination) return destinationWorkEntry(destination);
  const root = workEntries.find(item => item.landing === parts.path);
  if (root) return root.id;
  return destinationWorkEntry({source: sourcePath(parts.path), selection: Object.fromEntries(url.searchParams)});
}

const categoryThai: Partial<Record<WorkplaceCategoryId,string>> = {"people-organization":"บุคลากรและองค์กร", "customers-sales":"ลูกค้าและการขาย", purchasing:"จัดซื้อ", "inventory-quality":"คลังสินค้าและคุณภาพ", "projects-routines":"โครงการและงานประจำ", services:"บริการ", finance:"การเงิน", pos:"ขายหน้าร้าน", pms:"โรงแรม", production:"การผลิต", knowledge:"ความรู้", insights:"ภาพรวมและวิเคราะห์", administration:"การดูแลระบบ"};

export function presentNavigation(model: WorkplaceNavigationModel, context: NavigationContext): WorkplaceNavigationModel {
  if (context.scopeKind === "platform" || context.beta) return model;
  const base = context.workspaceBase;
  const seen = new Set<string>();
  const search = model.search.filter(item => {
    const href = canonicalUiHref(item.href), path = workspacePathParts(new URL(href,"https://navigation.invalid").pathname)?.path;
    const destination = path ? destinationForPath(path) : undefined;
    if (destination && !destinationAvailable(context, destination)) return false;
    if (destination?.source === "settings" && destination.selection.section === "approvals" && !context.canManageApprovals) return false;
    if (destination?.source.startsWith("manage-erpx") && (base!=="/holdings/erpx" || (context.scopeKind??"holding")!=="holding")) return false;
    // Parent aliases add no authority: only registered eligible children become topics.
    if (!destination && !model.primary.some(root => root.id === item.id) && !item.id.startsWith("start-") && !["pos-front", "pms-front", "branch-usage"].includes(item.id)) return false;
    if (seen.has(href)) return false;
    seen.add(href); return true;
  }).map(item => ({...item, href: canonicalUiHref(item.href), workEntryId: item.id.startsWith("start-") ? "work" : ["pos-front","pms-front"].includes(item.id) ? item.id : workEntryForHref(item.href, base)}));
  const roots = workEntries.flatMap(definition => {
    const paths = new Set<string>();
    const children = search.filter(item => {
      if (item.workEntryId !== definition.id || item.id.startsWith("start-")) return false;
      const path = new URL(item.href,"https://navigation.invalid").pathname;
      if (paths.has(path)) return false;
      paths.add(path);return true;
    });
    if (!children.length) return [];
    const preferred = children.find(item => workspacePathParts(new URL(item.href,"https://navigation.invalid").pathname)?.path === definition.landing) ?? children[0];
    const href = definition.mode === "index" && !destinationForPath(definition.landing) ? `${base}/${definition.landing}` : preferred.href;
    return [{...preferred, href, id:definition.id, label:definition[context.language === "th" ? "th" : "en"], icon:definition.icon, categoryId:definition.categoryId, workEntryId:definition.id, submodules:children}];
  });
  const categories = workplaceCategories.flatMap(([id,label,icon]) => {
    const apps = roots.filter(item => item.categoryId === id);
    return apps.length ? [{id, label:context.language === "th" ? categoryThai[id] ?? label : label, icon, apps}] : [];
  });
  const primary = model.primary.map(item=>({...item, submodules:search.filter(child=>child.workEntryId===item.id && child.id!==item.id && !child.id.startsWith("start-") && child.href!==`${base}/projects-routines/tasks`)}));
  const aliases = roots.filter(item=>!search.some(child=>child.id===item.id)).map(item=>({...item,submodules:undefined}));
  return {...model, primary, categories, search:[...search,...aliases]};
}

/** Explicit area roots also power old bookmarks and index landings. */
export const workEntryRoots: Readonly<Record<string,string>> = {
  "people-organization/people":"people-directory", "people-organization/people/directory":"people-directory", "people-organization/people/payroll":"people-payroll", "people-organization/people/hr-settings":"people-settings", "people-organization/people/evaluation":"people-evaluation", "people-organization/people/external":"people-external", "people-organization/people/users-access":"technology-accounts",
  "customers-sales/sales":"sales-hub", "customers-sales/sales/documents":"sales-documents", "customers-sales/customers":"customers-hub", "customers-sales/customers/reviews":"customers-hub", "customers-sales/customers/service":"customers-hub",
  "services/legal":"legal", "services/assets":"assets", "knowledge/handbook":"handbook", "knowledge/learning":"learning", "finance/today":"finance-overview", "finance/documents":"finance-sales", "finance/documents/sales":"finance-sales", "finance/cash":"finance-payments", "finance/accounting":"finance-ledger", "finance/reports":"finance-reports", "finance/setup":"finance-setup", "administration/technology/resources":"technology-organization", "administration/technology/directory":"technology-directory", "administration/technology/settings":"technology-settings", "administration/approvals":"administration-approvals", "administration/brand":"administration-brand", "erpx-management/billing":"manage-erpx-billing", "erpx-management/iam":"manage-erpx-iam",
};

export function navigationWorkEntry(model: WorkplaceNavigationModel, base: string, href: string) {
  const path = workspacePathParts(new URL(canonicalUiHref(href),"https://navigation.invalid").pathname)?.path;
  const id = path && workEntryRoots[path] || workEntryForHref(href,base);
  return [...model.primary,...model.categories.flatMap(category => category.apps)].find(item => item.id === id);
}

/** Local scope context survives topic changes; record/filter/pagination state
 * belongs to the collection being left and is intentionally not inherited. */
export function navigationTopicHref(href:string, query:Pick<URLSearchParams,"getAll">) {
  const target = new URL(canonicalUiHref(href), "https://topics.invalid");
  for (const key of ["company", "companyId", "branch", "department", "property", "chart", "chartId", "book", "periodId", "asOf"]) {
    if (!target.searchParams.has(key)) for (const value of query.getAll(key)) target.searchParams.append(key,value);
  }
  const path = workspacePathParts(target.pathname)?.path;
  const destination = path ? destinationForPath(path) : undefined;
  const chart = query.getAll("chart")[0] ?? query.getAll("chartId")[0];
  if (chart && destination?.source.startsWith("finance")) {
    target.searchParams.delete("chart");target.searchParams.delete("chartId");
    target.searchParams.set(destination.source==="finance/invoices" ? "chartId" : "chart",chart);
  }
  return target.pathname + target.search + target.hash;
}

