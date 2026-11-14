import { describe, expect, it } from "vitest";
import { commercialPackages, purchasedCapability, purchasedCapabilityPurpose, validateGrantChange, visiblePurchasedProducts, type CompanyPackageGrant, type CompanyPackageHistoryRight } from "@erpx/server-commercial/public/packages";
import { effectiveCapabilityDecision, capabilityRevisionSource } from "@erpx/server-commercial/domain/capability-decision";
import { readPackageObligations, packageObligationModules, type PackageObligationReader } from "@erpx/server-commercial/public/package-obligations";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { validatePackageDependencies } from "@erpx/server-commercial/domain/package-dependencies";
const now = 1_000;
const grant = (packageId: string, extra: Partial<CompanyPackageGrant> = {}): CompanyPackageGrant => ({ companyId: "c1", packageId, packageVersion: 1, state: "active", startsAt: 0, endsAt: null, version: 1, reason: "Fixture", evidence: "test:approved", ...extra });
const retained = (packageId: string, extra: Partial<CompanyPackageHistoryRight> = {}): CompanyPackageHistoryRight => ({ companyId: "c1", packageId, packageVersion: 1, sourceGrantVersion: 1, sourceEventId: "reviewed-purchase", retainedAt: 0, ...extra });
const decision = (grants: CompanyPackageGrant[], capability: string, purpose: "new-work" | "settle-existing" | "history" = "new-work", history: CompanyPackageHistoryRight[] = []) => purchasedCapability(grants, "c1", capability, purpose, now, history);
it("maps immutable Work purchases to Apps without inventing a grant or changing lifecycle rights", () => {
  expect(commercialPackages.filter(item => item.productId === "apps").map(item => [item.id, item.dependencies])).toEqual([
    ["work", []], ["work-social", ["work"]], ["work-messages", ["work"]], ["work-routines", ["work"]],
  ]);
  expect(commercialPackages.some(item => item.id === "apps")).toBe(false);
  const active = [grant("work"), grant("work-social")];
  expect(visiblePurchasedProducts(active, "c1", now)).toEqual(["apps"]);
  expect(decision(active, "work.social").allowed).toBe(true);
  const draining = active.map(item => ({ ...item, state: "draining" as const }));
  expect(visiblePurchasedProducts(draining, "c1", now)).toEqual([]);
  expect(visiblePurchasedProducts(draining, "c1", now, "history")).toEqual(["apps"]);
  expect(decision(draining, "work.social").code).toBe("PACKAGE_DRAINING");
  expect(decision(draining, "work.social", "settle-existing").allowed).toBe(true);
  const history = [retained("work"), retained("work-social")];
  expect(visiblePurchasedProducts([], "c1", now, "history", history)).toEqual(["apps"]);
  expect(decision([], "work.social", "history", history).allowed).toBe(true);
  expect(decision([], "work.social", "new-work", history).allowed).toBe(false);
  expect(visiblePurchasedProducts(active, "another-company", now, "history", history)).toEqual([]);
});
it("separates readable product entry from permission to start new work", () => {
  const draining=[grant('support',{state:'draining'})];
  expect(visiblePurchasedProducts(draining,'c1',now)).toEqual([]);expect(visiblePurchasedProducts(draining,'c1',now,'history')).toEqual(['support']);
  expect(purchasedCapabilityPurpose({grants:draining,history:[]},'c1','support.tickets',now)).toBe('settle-existing');
  const future=[grant('support',{startsAt:now+1})],history=[retained('support')];
  expect(visiblePurchasedProducts(future,'c1',now,'history',history)).toEqual(['support']);
  expect(purchasedCapabilityPurpose({grants:future,history},'c1','support.tickets',now)).toBe('history');
  expect(purchasedCapabilityPurpose({grants:future,history:[]},'c1','support.tickets',now)).toBeNull();
  expect(purchasedCapability(future,'c1','support.tickets','new-work',now,history).allowed).toBe(false);
});
it("intersects every purchase/readiness/authority/record gate without giving dependencies management authority", () => {
  for (let bits = 0; bits < 16; bits++) {
    const result = effectiveCapabilityDecision({ companyId: "c1", capability: "pos.sales", purpose: "new-work", now,
      grants: bits & 1 ? [grant("pos")] : [], release: { allowed: !!(bits & 2), revision: "release-1" },
      authority: { allowed: !!(bits & 4), revision: "authority-1" }, record: { allowed: !!(bits & 8), revision: "record-1" } });
    expect(result.allowed, String(bits)).toBe(bits === 15);
    expect(result.revisions).toEqual({ release: "release-1", authority: "authority-1", record: "record-1" });
  }
  const gate = { allowed: true, revision: "1" };
  expect(effectiveCapabilityDecision({ companyId: "c1", capability: "finance.accounting", purpose: "new-work", now,
    grants: [grant("pos")], release: gate, authority: gate, record: gate }).allowed).toBe(false);
});
it("changes capability revisions at starts, expiry and cancellation while isolating Company and row order", () => {
  const grants = [grant("pos", { endsAt: now }), grant("work", { startsAt: now })];
  const current = capabilityRevisionSource("c1", grants, now);
  expect(capabilityRevisionSource("c1", [...grants].reverse(), now)).toBe(current);
  expect(capabilityRevisionSource("c1", [...grants, grant("legal", { companyId: "c2" })], now)).toBe(current);
  expect(capabilityRevisionSource("c1", grants, now - 1)).not.toBe(current);
  expect(capabilityRevisionSource("c1", grants, now + 1)).toBe(current);
  expect(capabilityRevisionSource("c1", grants.map(grant => ({ ...grant, state: "draining", version: 2 })), now)).not.toBe(current);
});
it("requires current, scoped evidence from every settlement owner and preserves the supplied transaction", async () => {
  const tx = {} as SqlDatabase, input = { companyId: "c1", packageId: "pos", packageVersion: 1, now };
  const readers: Record<string, PackageObligationReader> = Object.fromEntries(packageObligationModules("pos", 1).map(moduleId => [moduleId, async (database, request) => {
    expect(database).toBe(tx);
    return { companyId: request.companyId, packageId: request.packageId, moduleId, observedAt: request.now, revision: "owner-1", outstanding: moduleId === "finance" ? 2 : 0 };
  }]));
  expect((await readPackageObligations(tx, input, readers)).total).toBe(2);
  await expect(readPackageObligations(tx, input, {})).rejects.toThrow("PACKAGE_OBLIGATION_READER_REQUIRED:pos");
  for (const mismatch of [{ companyId: "c2" }, { packageId: "pms" }, { observedAt: now - 1 }, { revision: "" }, { outstanding: NaN }, { outstanding: -1 }]) {
    await expect(readPackageObligations(tx, input, { ...readers, pos: async () => ({ companyId: "c1", packageId: "pos", moduleId: "pos", observedAt: now, revision: "1", outstanding: 0, ...mismatch }) })).rejects.toThrow("INVALID_PACKAGE_OBLIGATION_EVIDENCE:pos");
  }
});
it("rejects unrecognized lifecycle states and unsafe versions even when a caller bypasses request parsing", () => {
  expect(() => validateGrantChange(null, grant("work", { state: "unknown" as CompanyPackageGrant["state"] }), 0, 0)).toThrow("INVALID_PACKAGE_STATE");
  expect(() => validateGrantChange(grant("work", { version: Number.MAX_SAFE_INTEGER }), grant("work", { version: Number.MAX_SAFE_INTEGER + 1 }), Number.MAX_SAFE_INTEGER, 0)).toThrow("STALE_PACKAGE_GRANT");
});
it("requires zero owner obligations before moving an active or draining start forward", () => {
  for(const state of ["active","draining"] as const) {
    const before=grant("support",{state});
    expect(()=>validateGrantChange(before,{...before,startsAt:now+60000,version:2},1,1)).toThrow("PACKAGE_OBLIGATIONS_REMAIN");
    expect(()=>validateGrantChange(before,{...before,startsAt:now+60000,version:2},1,0)).not.toThrow();
  }
});
it("protects a future period that can become effective before its replacement commits", () => {
  const before=grant("support",{startsAt:now+1});
  for(const outstanding of [1,-1,NaN,Infinity])expect(()=>validateGrantChange(before,{...before,startsAt:now+60000,version:2},1,outstanding)).toThrow("PACKAGE_OBLIGATIONS_REMAIN");
  expect(()=>validateGrantChange(before,{...before,startsAt:now+60000,version:2},1,0)).not.toThrow();
});
it("keeps existing settlement through unchanged/earlier starts, expiry and ordinary draining with live work", () => {
  const before=grant("support",{startsAt:500});
  for(const patch of [{endsAt:501},{startsAt:0},{state:"draining" as const}])expect(()=>validateGrantChange(before,{...before,...patch,version:2},1,3)).not.toThrow();
  expect(()=>validateGrantChange({...before,state:"inactive"},{...before,startsAt:now+60000,version:2},1,3)).not.toThrow();
});
it("does not withdraw a base while a draining add-on still depends on its settlement period", () => {
  const base=grant("support"),addon=grant("support-portal",{state:"draining"});
  expect(()=>validatePackageDependencies({...base,state:"inactive"},[base,addon],now)).toThrow("PACKAGE_DEPENDENCY_IN_USE");
  expect(()=>validatePackageDependencies({...base,startsAt:now+1},[base,addon],now)).toThrow("PACKAGE_DEPENDENCY_PERIOD_CONFLICT");
  expect(()=>validatePackageDependencies({...base,state:"draining"},[base,addon],now)).not.toThrow();
  expect(()=>validatePackageDependencies({...base,state:"inactive"},[base,{...addon,state:"inactive"}],now)).not.toThrow();
});
it("keeps a draining add-on's dependency started and non-inactive while allowing expiry settlement", () => {
  const addon=grant("support-portal",{state:"draining"});
  for(const base of [grant("support",{state:"inactive"}),grant("support",{startsAt:now+1})])expect(()=>validatePackageDependencies(addon,[base,addon],now)).toThrow("PACKAGE_DEPENDENCY_REQUIRED");
  expect(()=>validatePackageDependencies(addon,[grant("support",{state:"draining",endsAt:1}),addon],now)).not.toThrow();
});
describe("commercial packages", () => {
  it("includes shared foundations without selling Social or full management dependencies", () => {
    expect(decision([grant("pos")], "foundation.notifications").allowed).toBe(true);
    expect(decision([grant("pos")], "work.social").allowed).toBe(false);
    expect(decision([grant("pos")], "finance.pos-posting").allowed).toBe(true);
    expect(decision([grant("pos")], "finance.accounting").allowed).toBe(false);
    expect(visiblePurchasedProducts([grant("pos")], "c1", now)).toEqual(["pos"]);
  });
  it("supports complete base-package handoffs without buying the dependency management product", () => {
    for (const [packageId, capability] of [["sales", "sales.orders"], ["sales", "inventory.stock-core"], ["supply", "finance.inventory-valuation"], ["assets", "finance.asset-disposal"], ["projects", "people.worker-reference"], ["learn", "people.learner-reference"], ["service", "support.source-ticket"], ["service", "finance.service-invoice"], ["manufacturing", "quality.production"], ["manufacturing", "planning.basic"]]) expect(decision([grant(packageId)], capability).allowed, `${packageId}/${capability}`).toBe(true);
    expect(decision([grant("projects")], "people.directory").allowed).toBe(false);
  });
  it("requires a current same-Company base grant for add-ons", () => {
    expect(decision([grant("people-payroll")], "people.payroll").code).toBe("DEPENDENCY_REQUIRED");
    expect(decision([grant("people-payroll"), grant("people", { companyId: "c2" })], "people.payroll").allowed).toBe(false);
    expect(decision([grant("people-payroll"), grant("people")], "people.payroll").allowed).toBe(true);
    expect(decision([grant("people-payroll"), grant("people", { endsAt: now })], "people.payroll").allowed).toBe(false);
    expect(decision([grant("people-payroll"), grant("people", { startsAt: now + 1 })], "people.payroll").allowed).toBe(false);
  });
  it("drains new work at exact expiry while keeping existing settlement and history", () => {
    const expired = grant("pms", { endsAt: now });
    expect(decision([expired], "pms.guest-accounts").code).toBe("PACKAGE_DRAINING");
    expect(decision([expired], "pms.guest-accounts", "settle-existing").allowed).toBe(true);
    expect(decision([grant("pms", { state: "inactive" })], "pms.guest-accounts", "settle-existing").allowed).toBe(false);
    expect(decision([grant("pms", { state: "inactive" })], "pms.guest-accounts", "history", [retained("pms")]).allowed).toBe(true);
    expect(decision([], "pms.guest-accounts", "history").allowed).toBe(false);
  });
  it("requires evidence, expected versions, draining and an owner obligation check", () => {
    const current = grant("assets");
    expect(() => validateGrantChange(current, { ...current, version: 2, state: "inactive" }, 1, 0)).toThrow("DRAIN_PACKAGE_FIRST");
    const draining = { ...current, state: "draining" as const };
    expect(() => validateGrantChange(draining, { ...current, version: 2, state: "inactive" }, 1, 1)).toThrow("PACKAGE_OBLIGATIONS_REMAIN");
    expect(() => validateGrantChange(draining, { ...current, version: 2, state: "inactive" }, 1, 0)).not.toThrow();
    expect(() => validateGrantChange(current, { ...current, version: 2 }, 0, 0)).toThrow("STALE_PACKAGE_GRANT");
    expect(() => validateGrantChange(null, { ...current, reason: " " }, 0, 0)).toThrow("GRANT_EVIDENCE_REQUIRED");
  });
  it("has no commercial capability that creates provider or identity authority", () => {
    for (const definition of commercialPackages) expect([...definition.visibleCapabilities, ...definition.includedCapabilities].some(capability => /^(?:platform|manage_erpx|iam\.manage)/.test(capability))).toBe(false);
  });
});

it("retains only historical purposes across future renewal while keeping current owner gates", () => {
  const future = [grant("support", { startsAt: now + 1 })], history = [retained("support")], gate = { allowed: true, revision: "owner" };
  expect(decision(future, "support.tickets", "history", history).allowed).toBe(true);
  expect(decision(future, "support.tickets", "new-work", history).allowed).toBe(false);
  expect(decision(future, "support.tickets", "settle-existing", history).allowed).toBe(false);
  expect(effectiveCapabilityDecision({ companyId: "c1", grants: future, history, capability: "support.tickets", purpose: "history", now,
    release: gate, authority: { allowed: false, revision: "revoked" }, record: gate }).code).toBe("AUTHORITY_REQUIRED");
  expect(effectiveCapabilityDecision({ companyId: "c1", grants: future, history, capability: "support.tickets", purpose: "history", now,
    release: gate, authority: gate, record: { allowed: false, revision: "record-revoked" } }).code).toBe("RECORD_DENIED");
});

it("requires scoped, started history evidence and retained dependencies without granting management", () => {
  expect(decision([grant("support", { state: "inactive" })], "support.tickets", "history").allowed).toBe(false);
  for (const invalid of [{ companyId: "other" }, { retainedAt: now + 1 }, { retainedAt: -1 }, { sourceGrantVersion: 0 }, { sourceEventId: "" }, { packageVersion: 999 }])
    expect(decision([], "support.tickets", "history", [retained("support", invalid)]).allowed).toBe(false);
  expect(decision([], "support.portal", "history", [retained("support-portal")]).code).toBe("DEPENDENCY_REQUIRED");
  expect(decision([], "support.portal", "history", [retained("support-portal"), retained("support")]).allowed).toBe(true);
  expect(decision([], "finance.accounting", "history", [retained("service")]).allowed).toBe(false);
});

it("binds history evidence to the capability revision while preserving scope and order stability", () => {
  const history = [retained("support"), retained("service", { sourceEventId: "service-purchase" })], current = capabilityRevisionSource("c1", [], now, history);
  expect(capabilityRevisionSource("c1", [], now, [...history].reverse())).toBe(current);
  expect(capabilityRevisionSource("c1", [], now, [...history, retained("pos", { companyId: "other" })])).toBe(current);
  expect(capabilityRevisionSource("c1", [], now, history.map(right => ({ ...right, sourceEventId: "different-source" })))).not.toBe(current);
  expect(capabilityRevisionSource("c1", [], now - 1, [retained("support", { retainedAt: now })])).not.toBe(capabilityRevisionSource("c1", [], now, [retained("support", { retainedAt: now })]));
});
