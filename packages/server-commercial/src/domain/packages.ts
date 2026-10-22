import { productIds, type ProductId } from "@erpx/contracts/product-registry";

export type CommercialPackageDefinition = {
  id: string;
  version: number;
  productId: ProductId;
  kind: "base" | "addon";
  visibleCapabilities: readonly string[];
  includedCapabilities: readonly string[];
  dependencies: readonly string[];
};
const foundation = ["foundation.identity", "foundation.organization", "foundation.files", "foundation.audit", "foundation.approvals", "foundation.notifications"];
const base: Record<ProductId, { visible: string[]; included?: string[] }> = {
  work: { visible: ["work.today", "work.actions", "work.calendar", "work.handbook"] },
  pos: { visible: ["pos.sales", "pos.payments", "pos.refunds", "pos.shifts"], included: ["inventory.stock-core", "finance.pos-posting"] },
  pms: { visible: ["pms.reservations", "pms.front-desk", "pms.rooms", "pms.housekeeping", "pms.guest-accounts", "pms.night-audit"], included: ["inventory.stock-core", "finance.hotel-posting"] },
  finance: { visible: ["finance.accounting", "finance.receivables", "finance.payables", "finance.banking", "finance.tax", "finance.close"] },
  people: { visible: ["people.directory", "people.lifecycle"] },
  sales: { visible: ["sales.crm", "sales.customers", "sales.quotes", "sales.orders"], included: ["inventory.stock-core", "finance.sales-posting"] },
  supply: { visible: ["supply.purchasing", "supply.receiving", "supply.stock", "supply.transfers", "supply.counts"], included: ["finance.inventory-valuation"] },
  assets: { visible: ["assets.register", "assets.custody", "assets.maintenance", "assets.downtime"], included: ["finance.asset-recognition", "finance.asset-disposal"] },
  projects: { visible: ["projects.wbs", "projects.tasks", "projects.board", "projects.time", "projects.handover"], included: ["people.worker-reference"] },
  recruit: { visible: ["recruit.positions", "recruit.candidates", "recruit.applications", "recruit.offers"] },
  learn: { visible: ["learn.courses", "learn.assignments", "learn.results", "learn.certificates"], included: ["people.learner-reference"] },
  support: { visible: ["support.tickets", "support.sla", "support.incidents", "support.evidence"] },
  service: { visible: ["service.agreements", "service.orders", "service.dispatch", "service.visits", "service.billing"], included: ["support.source-ticket", "finance.service-invoice"] },
  legal: { visible: ["legal.contracts", "legal.documents", "legal.matters", "legal.obligations", "legal.access"] },
  manufacturing: { visible: ["manufacturing.bom", "manufacturing.materials", "manufacturing.execution", "manufacturing.quality", "manufacturing.costs"], included: ["planning.basic", "quality.production", "inventory.stock-core", "finance.production-costing"] },
};
const addons: readonly [string, ProductId, readonly string[]][] = [
  ["work-social", "work", ["work.social"]], ["work-messages", "work", ["work.messages"]], ["work-routines", "work", ["work.advanced-routines"]],
  ["pos-restaurant", "pos", ["pos.restaurant", "pos.kitchen"]], ["pos-hotel-charge", "pos", ["pos.hotel-charge"]],
  ["pms-minibar-linen", "pms", ["pms.minibar", "pms.linen"]], ["finance-advanced", "finance", ["finance.advanced"]],
  ["people-workforce", "people", ["people.workforce"]], ["people-payroll", "people", ["people.payroll"]], ["people-expenses", "people", ["people.expenses"]],
  ["people-performance", "people", ["people.performance"]], ["people-benefits", "people", ["people.benefits"]],
  ["sales-marketing", "sales", ["sales.marketing"]], ["sales-ecommerce", "sales", ["sales.ecommerce"]], ["sales-portal", "sales", ["sales.portal"]],
  ["supply-quality", "supply", ["supply.quality"]], ["supply-advanced", "supply", ["supply.advanced-inventory"]],
  ["assets-preventive", "assets", ["assets.preventive"]], ["assets-parts-costs", "assets", ["assets.parts-costs"]],
  ["projects-costs", "projects", ["projects.costs"]], ["support-portal", "support", ["support.portal"]],
  ["service-parts-assets", "service", ["service.parts-assets"]], ["manufacturing-mrp", "manufacturing", ["planning.advanced-mrp"]],
];
export const commercialPackages: readonly CommercialPackageDefinition[] = [
  ...productIds.map(productId => ({ id: productId, version: 1, productId, kind: "base" as const, visibleCapabilities: base[productId].visible, includedCapabilities: [...foundation, ...(base[productId].included ?? [])], dependencies: [] })),
  ...addons.map(([id, productId, visibleCapabilities]) => ({ id, version: 1, productId, kind: "addon" as const, visibleCapabilities, includedCapabilities: [], dependencies: [productId] })),
];
export function packageDefinition(id: string, version: number) { return commercialPackages.find(entry => entry.id === id && entry.version === version); }
export type CompanyPackageGrant = {
  companyId: string;
  packageId: string;
  packageVersion: number;
  state: "active" | "draining" | "inactive";
  startsAt: number;
  endsAt: number | null;
  version: number;
  reason: string;
  evidence: string;
};
export function canonicalPackageGrant(grant: CompanyPackageGrant): CompanyPackageGrant {
  return { companyId: grant.companyId, packageId: grant.packageId, packageVersion: grant.packageVersion, state: grant.state,
    startsAt: grant.startsAt, endsAt: grant.endsAt, version: grant.version, reason: grant.reason, evidence: grant.evidence };
}
export type CapabilityPurpose = "new-work" | "settle-existing" | "history";
export type CompanyPackageHistoryRight = {
  companyId: string;
  packageId: string;
  packageVersion: number;
  sourceGrantVersion: number;
  sourceEventId: string;
  retainedAt: number;
};
export type CompanyPurchaseFacts = { grants: CompanyPackageGrant[]; history: CompanyPackageHistoryRight[] };
export type PackageDecision = { allowed: boolean; code: "GRANTED" | "PACKAGE_REQUIRED" | "PACKAGE_DRAINING" | "PACKAGE_INACTIVE" | "DEPENDENCY_REQUIRED" };

/** This decision never grants identity, record authority or release readiness. */
export function purchasedCapability(grants: readonly CompanyPackageGrant[], companyId: string, capability: string, purpose: CapabilityPurpose, now: number, history: readonly CompanyPackageHistoryRight[] = []): PackageDecision {
  const scoped = grants.filter(grant => grant.companyId === companyId && grant.startsAt <= now);
  if (purpose === "history") {
    // Current non-inactive periods can already have started without a further
    // grant write. Inactive/current-future periods alone prove no past right.
    const retained = history.filter(right => right.companyId === companyId && right.retainedAt <= now &&
      Number.isSafeInteger(right.retainedAt) && right.retainedAt >= 0 && Number.isSafeInteger(right.sourceGrantVersion) && right.sourceGrantVersion > 0 && right.sourceEventId);
    const sources = [...scoped.filter(grant => grant.state !== "inactive"), ...retained];
    let denial: PackageDecision["code"] = "PACKAGE_REQUIRED";
    for (const source of sources) {
      const definition = packageDefinition(source.packageId, source.packageVersion);
      if (!definition || ![...definition.visibleCapabilities, ...definition.includedCapabilities].includes(capability)) continue;
      if (definition.dependencies.some(id => !sources.some(parent => parent.packageId === id && packageDefinition(parent.packageId, parent.packageVersion)))) { denial = "DEPENDENCY_REQUIRED"; continue; }
      return { allowed: true, code: "GRANTED" };
    }
    return { allowed: false, code: denial };
  }
  const usable = (grant: CompanyPackageGrant) => grant.state === "active" && (grant.endsAt === null || grant.endsAt > now);
  const candidates = scoped.filter(grant => {
    const definition = packageDefinition(grant.packageId, grant.packageVersion);
    return definition && [...definition.visibleCapabilities, ...definition.includedCapabilities].includes(capability);
  });
  let denial: PackageDecision["code"] = "PACKAGE_REQUIRED";
  for (const grant of candidates) {
    const definition = packageDefinition(grant.packageId, grant.packageVersion)!;
    const available = usable(grant);
    // Expiry behaves as draining; final inactivation is an explicit owner-checked command.
    if (!available && purpose === "new-work") { denial = grant.state === "inactive" ? "PACKAGE_INACTIVE" : "PACKAGE_DRAINING"; continue; }
    if (grant.state === "inactive") { denial = "PACKAGE_INACTIVE"; continue; }
    if (definition.dependencies.some(id => !scoped.some(parent => parent.packageId === id && packageDefinition(parent.packageId, parent.packageVersion) && (purpose === "new-work" ? usable(parent) : parent.state !== "inactive")))) {
      denial = "DEPENDENCY_REQUIRED"; continue;
    }
    return { allowed: true, code: "GRANTED" };
  }
  return { allowed: false, code: denial };
}

export function visiblePurchasedProducts(grants: readonly CompanyPackageGrant[], companyId: string, now: number,
  purpose: CapabilityPurpose = "new-work", history: readonly CompanyPackageHistoryRight[] = []): ProductId[] {
  return productIds.filter(id => base[id].visible.some(capability => purchasedCapability(grants, companyId, capability, purpose, now, history).allowed));
}

/** Entry presentation only. Owners still choose the command purpose and must
 * intersect release, current personal authority and the actual record policy. */
export function purchasedCapabilityPurpose(facts: CompanyPurchaseFacts, companyId: string, capability: string, now: number): CapabilityPurpose | null {
  return (["new-work","settle-existing","history"] as const).find(purpose=>purchasedCapability(facts.grants,companyId,capability,purpose,now,facts.history).allowed) ?? null;
}

export function purchasedCapabilities(grants: readonly CompanyPackageGrant[], companyId: string, purpose: CapabilityPurpose, now: number, history: readonly CompanyPackageHistoryRight[] = []): string[] {
  const known = new Set(commercialPackages.flatMap(definition => [...definition.visibleCapabilities, ...definition.includedCapabilities]));
  return [...known].filter(capability => purchasedCapability(grants, companyId, capability, purpose, now, history).allowed);
}

/** Moving even an unstarted period forward can withdraw settlement by COMMIT:
 * its old start may pass while the reviewed transaction is waiting. Expiry and
 * draining retain settlement, so they do not require an empty owner backlog. */
export function requiresPackageSettlementReview(current: CompanyPackageGrant | null, next: CompanyPackageGrant) {
  if (next.state === "inactive") return true;
  if (!current || current.state === "inactive") return false;
  if (next.startsAt > current.startsAt) return true;
  const before = packageDefinition(current.packageId,current.packageVersion), after = packageDefinition(next.packageId,next.packageVersion);
  if (!before || !after) return true;
  const capabilities = new Set([...after.visibleCapabilities,...after.includedCapabilities]);
  return [...before.visibleCapabilities,...before.includedCapabilities].some(capability=>!capabilities.has(capability));
}

export function validateGrantChange(current: CompanyPackageGrant | null, next: CompanyPackageGrant, expectedVersion: number, outstandingObligations: number) {
  if (!packageDefinition(next.packageId, next.packageVersion)) throw new Error("UNKNOWN_PACKAGE_VERSION");
  if (!["active", "draining", "inactive"].includes(next.state)) throw new Error("INVALID_PACKAGE_STATE");
  if (!next.companyId || !next.reason.trim() || !next.evidence.trim()) throw new Error("GRANT_EVIDENCE_REQUIRED");
  if (!Number.isSafeInteger(next.startsAt) || next.startsAt < 0 || next.endsAt !== null && (!Number.isSafeInteger(next.endsAt) || next.endsAt <= next.startsAt)) throw new Error("INVALID_GRANT_PERIOD");
  if (!Number.isSafeInteger(expectedVersion) || expectedVersion < 0 || !Number.isSafeInteger(next.version) || (current?.version ?? 0) !== expectedVersion || next.version !== expectedVersion + 1) throw new Error("STALE_PACKAGE_GRANT");
  if (current && (current.companyId !== next.companyId || current.packageId !== next.packageId)) throw new Error("PACKAGE_GRANT_IDENTITY_MISMATCH");
  if (!current && next.state !== "active" || current?.state === "active" && next.state === "inactive") throw new Error("DRAIN_PACKAGE_FIRST");
  if (requiresPackageSettlementReview(current,next) && (!Number.isSafeInteger(outstandingObligations) || outstandingObligations !== 0)) throw new Error("PACKAGE_OBLIGATIONS_REMAIN");
}
