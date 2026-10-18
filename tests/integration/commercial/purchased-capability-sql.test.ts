import { beforeAll, expect, it } from "vitest";
import { env } from "../../support/runtime-env";
import { commercialPackages, purchasedCapability, type CompanyPackageGrant, type CompanyPackageHistoryRight, type CapabilityPurpose } from "@/server/modules/commercial/public/packages";
import { purchasedCapabilityPredicate } from "@/server/modules/commercial/public/purchased-capability-sql";

const db = env.H001_DB, now = 10000;
const grant = (packageId: string, changes: Partial<CompanyPackageGrant> = {}): CompanyPackageGrant => ({ companyId: "company-a", packageId,
  packageVersion: 1, state: "active", startsAt: 0, endsAt: null, version: 1, reason: "Fictional comparison", evidence: "test:sql-parity", ...changes });
// These are read-only CTE facts, never installed purchase rows or SQL grants.
const retained = (packageId: string, changes: Partial<CompanyPackageHistoryRight> = {}): CompanyPackageHistoryRight => ({ companyId: "company-a", packageId,
  packageVersion: 1, sourceGrantVersion: 1, sourceEventId: "declared-started-purchase", retainedAt: 0, ...changes });
const cases: [string, CompanyPackageGrant[], CompanyPackageHistoryRight[]?][] = [
  ["no purchase", []], ["active", [grant("support")]], ["draining", [grant("support", { state: "draining" })]],
  ["inactive after a started purchase", [grant("support", { state: "inactive" })], [retained("support")]], ["future start", [grant("support", { startsAt: now + 1 })]],
  ["exact start", [grant("support", { startsAt: now })]], ["exact expiry", [grant("support", { endsAt: now })]],
  ["before expiry", [grant("support", { endsAt: now + 1 })]], ["other Company", [grant("support", { companyId: "company-b" })]],
  ["unknown version", [grant("support", { packageVersion: 999 })]], ["source-only inclusion", [grant("service")]],
  ["missing dependency", [grant("support-portal")]], ["active dependency", [grant("support-portal"), grant("support")]],
  ["draining dependency", [grant("support-portal"), grant("support", { state: "draining" })]],
  ["inactive dependency", [grant("support-portal"), grant("support", { state: "inactive" })], [retained("support")]],
  ["future dependency", [grant("support-portal"), grant("support", { startsAt: now + 1 })]],
  ["other Company dependency", [grant("support-portal"), grant("support", { companyId: "company-b" })]],
  ["unknown dependency version", [grant("support-portal"), grant("support", { packageVersion: 999 })]],
  ["all packages", commercialPackages.map(definition => grant(definition.id))],
  ["future renewal retains history", [grant("support", { startsAt: now + 1 })], [retained("support")]],
  ["inactive without a started purchase", [grant("support", { state: "inactive" })]],
  ["retained add-on and dependency", [], [retained("support-portal"), retained("support")]],
  ["history in another Company", [], [retained("support", { companyId: "company-b" })]],
  ["future history observation", [], [retained("support", { retainedAt: now + 1 })]],
];
beforeAll(async () => { await db.prepare("SELECT 1").first(); });
it.each(cases)("matches the domain decision for %s across every known capability and purpose", async (_label, grants, history = []) => {
  const capabilities = [...new Set(commercialPackages.flatMap(definition => [...definition.visibleCapabilities, ...definition.includedCapabilities])), "unknown.capability"];
  for (const purpose of ["new-work", "settle-existing", "history"] as CapabilityPurpose[]) {
    // Each result evaluates the production predicate on one PostgreSQL snapshot.
    const predicates = capabilities.map((capability, index) => `SELECT ${index} index,${purchasedCapabilityPredicate("scope.company_id", capability, purpose, "scope.now")} allowed FROM scope`);
    const result = await db.prepare(`WITH company_package_grants AS (SELECT * FROM jsonb_to_recordset(?::jsonb)
      AS g(company_id text,package_id text,package_version integer,state text,starts_at bigint,ends_at bigint)),
      company_package_history_rights AS (SELECT * FROM jsonb_to_recordset(?::jsonb)
        AS h(company_id text,package_id text,package_version integer,source_grant_version integer,source_event_id text,retained_at bigint)),
      scope AS (SELECT ?::text company_id,?::bigint now) ${predicates.join(" UNION ALL ")}`)
      .bind(JSON.stringify(grants.map(g => ({ company_id: g.companyId, package_id: g.packageId, package_version: g.packageVersion,
        state: g.state, starts_at: g.startsAt, ends_at: g.endsAt }))), JSON.stringify(history.map(h => ({ company_id: h.companyId,
        package_id: h.packageId, package_version: h.packageVersion, source_grant_version: h.sourceGrantVersion, source_event_id: h.sourceEventId, retained_at: h.retainedAt }))), "company-a", now).all<{ index: number; allowed: boolean }>();
    expect(result.results.map(row => row.allowed)).toEqual(capabilities.map(capability => purchasedCapability(grants, "company-a", capability, purpose, now, history).allowed));
  }
});

it("rejects dynamic SQL expressions instead of accepting request-provided fragments", () => {
  expect(() => purchasedCapabilityPredicate("company_id;DROP TABLE support_tickets", "support.tickets", "history", "scope.now")).toThrow("CAPABILITY_SQL_IDENTIFIER_REQUIRED");
  expect(() => purchasedCapabilityPredicate("scope.company_id", "support.tickets", "history", "now OR true")).toThrow("CAPABILITY_SQL_IDENTIFIER_REQUIRED");
});
