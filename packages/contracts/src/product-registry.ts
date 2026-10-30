import {socialPostSourceEntry} from './social-post-entry.ts';
import {workforceSelfServiceEntry} from './workforce-self-service-entry.ts';
/** Product presentation is independent of module identity and commercial authority. */
export const productIds = ["work", "pos", "pms", "finance", "people", "sales", "supply", "assets", "projects", "recruit", "learn", "support", "service", "legal", "manufacturing"] as const;
export type ProductId = typeof productIds[number];
export const sharedHostKeys = ["apps", "account", "admin", "booking", "portal", "legal-portal", "forms"] as const;
export type ProductHostKey = ProductId | typeof sharedHostKeys[number];
export type ProductDefinition = {
  id: ProductId;
  label: { en: string; th: string };
  icon: string;
  shell: "office" | "front";
  landing: string;
  destinations: readonly string[];
  modules: readonly string[];
};

export const products: readonly ProductDefinition[] = [
  { id: "work", label: { en: "Work", th: "งาน" }, icon: "BriefcaseBusiness", shell: "office", landing: "home", destinations: ["work", "calendar", "knowledge", "home", "inbox", "insights/executive", "projects-routines/tasks", "projects-routines/routines", "projects-routines/operations"], modules: ["operations", "collaboration"] },
  { id: "pos", label: { en: "POS", th: "ขายหน้าร้าน" }, icon: "ShoppingCart", shell: "front", landing: "sale", destinations: ["pos", "sale", "kitchen", "history", "shifts"], modules: ["pos"] },
  { id: "pms", label: { en: "Hotel", th: "โรงแรม" }, icon: "Hotel", shell: "front", landing: "front-desk", destinations: ["pms", "front-desk", "reservations", "rooms", "housekeeping", "guest-accounts", "cashier", "night-audit"], modules: ["pms"] },
  { id: "finance", label: { en: "Finance", th: "การเงิน" }, icon: "Landmark", shell: "office", landing: "finance/documents", destinations: ["finance"], modules: ["finance"] },
  { id: "people", label: { en: "People", th: "บุคลากร" }, icon: "Users", shell: "office", landing: "people-organization/people/directory/current", destinations: ["people-organization/people", "people-organization/benefits"], modules: ["people_payroll", "benefits"] },
  { id: "sales", label: { en: "Sales", th: "การขาย" }, icon: "Handshake", shell: "office", landing: "customers-sales/sales/pipeline", destinations: ["customers-sales/sales", "customers-sales/customers/directory", "customers-sales/customers/contacts", "customers-sales/customers/prospects", "customers-sales/customers/reviews", "customers-sales/marketing", "customers-sales/ecommerce"], modules: ["crm", "sales", "marketing", "ecommerce", "customer_360", "customer_authority"] },
  { id: "supply", label: { en: "Supply", th: "จัดซื้อและคลัง" }, icon: "Package", shell: "office", landing: "purchasing/orders", destinations: ["purchasing", "inventory-quality"], modules: ["procurement", "inventory", "quality"] },
  { id: "assets", label: { en: "Assets & Maintenance", th: "ทรัพย์สินและซ่อมบำรุง" }, icon: "Wrench", shell: "office", landing: "services/assets/register", destinations: ["services/assets"], modules: ["assets", "maintenance"] },
  { id: "projects", label: { en: "Projects", th: "โครงการ" }, icon: "Network", shell: "office", landing: "projects-routines/projects", destinations: ["projects-routines"], modules: ["projects"] },
  { id: "recruit", label: { en: "Recruitment", th: "สรรหาบุคลากร" }, icon: "UserPlus", shell: "office", landing: "people-organization/talent", destinations: ["people-organization/talent"], modules: ["talent"] },
  { id: "learn", label: { en: "Learning", th: "การเรียนรู้" }, icon: "GraduationCap", shell: "office", landing: "knowledge/learning", destinations: ["knowledge/learning"], modules: ["learning"] },
  { id: "support", label: { en: "Helpdesk", th: "ศูนย์ช่วยเหลือ" }, icon: "LifeBuoy", shell: "office", landing: "customers-sales/customers/service/cases", destinations: ["customers-sales/customers/service", "services/desk"], modules: ["support"] },
  { id: "service", label: { en: "Field Service", th: "บริการภาคสนาม" }, icon: "Route", shell: "office", landing: "services/field-service", destinations: ["services/field-service"], modules: ["field_service"] },
  { id: "legal", label: { en: "Legal", th: "กฎหมาย" }, icon: "Scale", shell: "office", landing: "services/legal/mine", destinations: ["services/legal"], modules: ["legal"] },
  { id: "manufacturing", label: { en: "Manufacturing", th: "การผลิต" }, icon: "Factory", shell: "office", landing: "production/manufacturing", destinations: ["production"], modules: ["manufacturing", "planning"] },
];

/** Every original module has exactly one commercial/presentation classification. */
export const modulePlacement = {
  brand: "admin", commercial: "account", identity_access: "admin", organization: "admin",
  platform_administration: "provider", manage_erpx: "provider", customer_360: "shared", customer_authority: "shared",
  support: "support", finance: "finance", pos: "pos", pms: "pms", sales: "sales", crm: "sales",
  assets: "assets", maintenance: "assets", inventory: "supply", procurement: "supply", quality: "supply",
  operations: "work", collaboration: "work", legal: "legal", people_payroll: "people", benefits: "people",
  marketing: "sales", ecommerce: "sales", manufacturing: "manufacturing", planning: "manufacturing",
  projects: "projects", learning: "learn", talent: "recruit", field_service: "service", agent: "shared",
} as const;

export function product(id: string) { return products.find(entry => entry.id === id); }
export function isProductHostKey(value: string): value is ProductHostKey {
  return [...productIds, ...sharedHostKeys].includes(value as ProductHostKey);
}
/** One-level names use the owned zone's existing Universal SSL wildcard. */
export function developmentProductOrigin(key: ProductHostKey) { return `https://${key}-dev-erpx.jesadakorn.com`; }
/** Planning metadata only. Never feed this into an origin allowlist. */
export function futureProductOrigin(key: ProductHostKey, environment: "development" | "production") {
  return `https://${environment === "development" ? "dev-" : ""}${key}.erpx.com`;
}

export function destinationProduct(destination: string): ProductId | undefined {
  if(socialPostSourceEntry(destination)||workforceSelfServiceEntry(destination))return "work";
  const matches = products.flatMap(entry => entry.destinations
    .filter(prefix => destination === prefix || destination.startsWith(prefix + "/"))
    .map(prefix => ({ id: entry.id, length: prefix.length })));
  matches.sort((a, b) => b.length - a.length);
  return matches[0]?.id;
}

export function validateProductRegistry(moduleIds: readonly string[]) {
  const problems: string[] = [];
  const checkUnique = (values: string[], kind: string) => {
    const seen = new Set<string>();
    for (const value of values) { if (seen.has(value)) problems.push(`${kind}:${value}`); seen.add(value); }
  };
  checkUnique(products.map(entry => entry.id), "duplicate-product");
  checkUnique([...productIds, ...sharedHostKeys].map(developmentProductOrigin), "duplicate-host");
  checkUnique(products.flatMap(entry => [...entry.destinations]), "duplicate-destination");
  for (const id of moduleIds) if (!(id in modulePlacement)) problems.push(`unmapped-module:${id}`);
  for (const id of Object.keys(modulePlacement)) if (!moduleIds.includes(id)) problems.push(`unknown-module:${id}`);
  for (const entry of products) if (!entry.destinations.some(prefix => entry.landing === prefix || entry.landing.startsWith(prefix + "/"))) problems.push(`invalid-landing:${entry.id}`);
  return problems;
}

/** Purpose of a purchased capability in presentation and owner policy. */
export type CapabilityPurpose = "new-work" | "settle-existing" | "history";
