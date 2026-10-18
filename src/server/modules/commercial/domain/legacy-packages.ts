import type { LegacyCompanyModuleAccess } from "../../identity_access/public/rules/legacy-module-access";
import { commercialPackages, packageDefinition, purchasedCapability, type CompanyPackageGrant } from "./packages";

/** Explicit old-module coverage. Dependencies include foundations, never user/provider authority. */
export const legacyModulePackages: Readonly<Record<string, readonly string[]>> = {
  brand: [], commercial: [], identity_access: [], organization: [], platform_administration: [], manage_erpx: [],
  customer_360: ["sales"], customer_authority: ["sales", "sales-portal"],
  operations: ["work", "work-routines"], collaboration: ["work", "work-social", "work-messages"], agent: ["work"],
  pos: ["pos", "pos-restaurant", "pos-hotel-charge"], pms: ["pms", "pms-minibar-linen"],
  finance: ["finance", "finance-advanced"], people_payroll: ["people", "people-workforce", "people-payroll", "people-expenses", "people-performance"], benefits: ["people", "people-benefits"],
  crm: ["sales"], sales: ["sales"], marketing: ["sales", "sales-marketing"], ecommerce: ["sales", "sales-ecommerce"],
  procurement: ["supply"], inventory: ["supply", "supply-advanced"], quality: ["supply", "supply-quality"],
  assets: ["assets"], maintenance: ["assets", "assets-preventive", "assets-parts-costs"], projects: ["projects", "projects-costs"],
  talent: ["recruit"], learning: ["learn"], support: ["support", "support-portal"], field_service: ["service", "service-parts-assets"],
  legal: ["legal"], manufacturing: ["manufacturing"], planning: ["manufacturing", "manufacturing-mrp"],
};

// These old assignment labels are not additional Module IDs. Coverage needs
// either independent Company module witnesses or current published module and
// business permissions evidenced by IAM. The original assignment stays intact.
const redundantLabels = new Map<string, readonly string[]>([
  ["iam", ["identity_access"]], ["people", ["people_payroll"]], ["commerce", ["sales", "procurement", "inventory"]],
]);

export function legacyPackagePreview(facts: LegacyCompanyModuleAccess, existing: readonly CompanyPackageGrant[], input: { startsAt: number; now: number; reason: string; evidence: string }) {
  if (!facts.companyId || !input.reason.trim() || !input.evidence.trim()) throw new Error("GRANT_EVIDENCE_REQUIRED");
  if (!Number.isSafeInteger(input.startsAt) || input.startsAt < 0 || input.startsAt > input.now || facts.observedAt > input.now || input.now - facts.observedAt > 30_000) throw new Error("STALE_LEGACY_PREVIEW");
  const observedModules = new Set(facts.witnesses.flatMap(witness => witness.modules));
  const proven = facts.witnesses.map(witness => witness.compatibility.filter(proof =>
    proof.source === "published-permission-policy/v1" && witness.modules.includes(proof.label) && redundantLabels.has(proof.label)));
  const coverage = new Set([...observedModules, ...proven.flatMap(proofs => proofs.flatMap(proof =>
    proof.modules.filter(moduleId => redundantLabels.get(proof.label)!.includes(moduleId))))]);
  const normalized = facts.witnesses.map(witness => witness.modules.filter(moduleId =>
    !redundantLabels.get(moduleId)?.every(owner => coverage.has(owner))));
  const resolvedLegacyLabels = [...new Set(facts.witnesses.flatMap((witness, index) => witness.modules.filter(moduleId => !normalized[index].includes(moduleId))))].sort();
  const redundantLegacyLabels = resolvedLegacyLabels.filter(label => redundantLabels.get(label)!.every(owner => observedModules.has(owner)));
  const modules = [...new Set([...normalized.flat(), ...resolvedLegacyLabels.flatMap(label => redundantLabels.get(label)!)])].sort();
  const unknown = modules.filter(id => !Object.hasOwn(legacyModulePackages, id));
  if (unknown.length) throw new Error(`UNMAPPED_LEGACY_MODULE:${unknown.join(",")}`);
  // Home Social and current-account Messages were existing Company work without
  // a collaboration Module or HR profile. Their original current account,
  // audience, membership and release policies still authorize every use.
  // Beta's own allowlist remains authoritative; it is not an enterprise license.
  const required = new Set<string>(facts.witnesses.length ? ["work", "work-messages", "work-social"] : []);
  const routineCoverage=facts.witnesses.map(witness=>!facts.beta&&witness.routineAccess?.source==='operations/original-routines/v1'&&
    (witness.routineAccess.activeStaff||witness.routineAccess.retainedTemplates>0));
  if(routineCoverage.some(Boolean))required.add('work-routines');
  if (facts.beta && facts.witnesses.length) for (const id of ["work-social", "work-messages", "people", "people-expenses"]) required.add(id);
  else for (const id of modules) for (const packageId of legacyModulePackages[id]) required.add(packageId);
  for (const id of [...required]) for (const dependency of packageDefinition(id, 1)!.dependencies) required.add(dependency);
  const before = existing.filter(grant => grant.companyId === facts.companyId).slice().sort((a, b) => a.packageId.localeCompare(b.packageId));
  const after = new Map(before.map(grant => [grant.packageId, grant]));
  const changes: { expectedVersion: number; grant: CompanyPackageGrant }[] = [];
  const blockers: { packageId: string; code: string }[] = [];
  for (const id of [...required].sort()) {
    const current = after.get(id);
    if (current) {
      if (!packageDefinition(current.packageId, current.packageVersion) || current.state !== "active" || current.startsAt > input.now || current.endsAt !== null && current.endsAt <= input.now)
        blockers.push({ packageId: id, code: "EXISTING_GRANT_REQUIRES_REVIEW" });
      continue;
    }
    const dependencyEnds = packageDefinition(id, 1)!.dependencies.map(dependency => before.find(grant => grant.packageId === dependency)?.endsAt)
      .filter((end): end is number => typeof end === "number");
    const grant: CompanyPackageGrant = { companyId: facts.companyId, packageId: id, packageVersion: 1, state: "active", startsAt: input.startsAt,
      endsAt: dependencyEnds.length ? Math.min(...dependencyEnds) : null, version: 1, reason: input.reason, evidence: input.evidence };
    changes.push({ expectedVersion: 0, grant }); after.set(id, grant);
  }
  const all = [...after.values()].sort((a, b) => a.packageId.localeCompare(b.packageId));
  const roles = facts.witnesses.map((witness, index) => {
    const compatibilityLabels = witness.modules.filter(moduleId => !normalized[index].includes(moduleId));
    const purchaseCoverageModules = [...new Set([...normalized[index], ...compatibilityLabels.flatMap(label => redundantLabels.get(label) ?? [])])].sort();
    const ownPackages = facts.beta ? [...required] : [...new Set(["work", "work-messages", "work-social",...(routineCoverage[index]?['work-routines']:[]), ...purchaseCoverageModules.flatMap(moduleId => legacyModulePackages[moduleId])])];
    const capabilities = commercialPackages.filter(definition => ownPackages.includes(definition.id)).flatMap(definition => [...definition.visibleCapabilities, ...definition.includedCapabilities]);
    return { assignmentId: witness.assignmentId, assignmentVersion: witness.assignmentVersion, roleId: witness.roleId, roleRevision: witness.roleRevision,
      scopeKind: witness.scopeKind, scopeNodeId: witness.scopeNodeId, modulesBefore: witness.modules, modulesAfter: witness.modules,
      compatibilityLabels, purchaseCoverageModules,
      compatibilityEvidence: proven[index],
      missingPurchasedCoverage: [...new Set(capabilities)].filter(capability => !purchasedCapability(all, facts.companyId, capability, "new-work", input.now).allowed) };
  });
  return { companyId: facts.companyId, authorityRevision: facts.revision, legacyModules: modules, redundantLegacyLabels, resolvedLegacyLabels, beta: facts.beta, requiredPackages: [...required].sort(),
    before, after: all, changes, blockers, roles, authorityChanges: [] as never[], rolloutChanges: [] as never[], pricingChanges: [] as never[],
    acceptance: "purchase-gate-coverage-only; owner workflow/shadow acceptance remains required" };
}
