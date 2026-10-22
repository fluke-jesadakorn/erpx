import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import { packageGrantStore, packageGrantRevision, readCompanyPackageGrants, readCompanyPurchaseFacts } from "@erpx/server-commercial/public/package-grants";
import { ownerPackageObligations, withOwnerPackageObligations } from "@erpx/server-commercial/public/owner-package-obligations";
import { purchasedCapability, type CompanyPackageGrant } from "@erpx/server-commercial/public/packages";
import { readPurchasedAccess } from "@erpx/server-commercial/public/purchase-command-boundary";
import { purchasedCapabilityPredicate } from "@erpx/server-commercial/public/purchased-capability-sql";
import { readCommandCapability, readEffectiveCapability } from "@erpx/server-commercial/public/capability-decision";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

let f: Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => { f = await expenseFixture(); });
afterEach(() => { vi.restoreAllMocks(); });
const store = () => packageGrantStore(db, {
  authorize: async () => ({ userId: "declared-native-history-reviewer" }),
  obligations: (tx, companyId, packageId) => ownerPackageObligations(db, tx, companyId, packageId),
});
function value(version: number, state: CompanyPackageGrant["state"], startsAt = 0): CompanyPackageGrant {
  return { companyId: f.company, packageId: "support", packageVersion: 1, version, state, startsAt, endsAt: null,
    reason: "Reviewed native history period", evidence: "test:normal-commercial-history" };
}
async function applyIn(tx: SqlDatabase, grant: CompanyPackageGrant) {
  const grants = await readCompanyPackageGrants(tx, f.company);
  return store().batch.applyIn(tx, { companyId: f.company, expectedRevision: packageGrantRevision(f.company, grants), idempotencyKey: key(),
    changes: [{ grant, expectedVersion: grant.version - 1 }] }, async () => {});
}
const apply = (grant: CompanyPackageGrant) => store().apply({ grant, expectedVersion: grant.version - 1, idempotencyKey: key() });
const access = () => readPurchasedAccess(db, "enforced", { companyId: f.company, capability: "support.tickets" });
const pauseUntil = async (instant: number) => { const left = instant - Date.now(); if (left > 0) await new Promise(resolve => setTimeout(resolve, left)); };
const gate = { allowed: true, revision: "declared-current-owner-gate" };

it("retains an audited started purchase through normal draining, inactivation and a future renewal", async () => {
  await apply(value(1, "active")); await apply(value(2, "draining"));
  await withOwnerPackageObligations(db, { companyId: f.company, packageId: "support", packageVersion: 1 }, tx => applyIn(tx, value(3, "inactive")));
  await apply(value(4, "active", Date.now() + 60000));
  expect(await access()).toMatchObject({ history: true, newWork: false, settleExisting: false });
  const facts = await readCompanyPurchaseFacts(db, f.company);
  expect(facts.history).toHaveLength(1); expect(facts.history[0]).toMatchObject({ packageId: "support", sourceGrantVersion: 1 });
  const audit = await db.prepare("SELECT after_json FROM company_package_grant_events WHERE id=?").bind(facts.history[0].sourceEventId).first<{ after_json: CompanyPackageGrant | string }>();
  expect(typeof audit!.after_json === "string" ? JSON.parse(audit!.after_json) : audit!.after_json).toMatchObject({ companyId: f.company, packageId: "support", state: "active", version: 1 });
  const query = { companyId: f.company, capability: "support.tickets", purpose: "history" as const, now: Date.now(), release: gate, authority: gate, record: gate };
  expect((await readEffectiveCapability(db, query)).allowed).toBe(true);
  expect((await readEffectiveCapability(db, { ...query, authority: { allowed: false, revision: "revoked" } })).code).toBe("AUTHORITY_REQUIRED");
  expect((await readEffectiveCapability(db, { ...query, record: { allowed: false, revision: "private-record" } })).code).toBe("RECORD_DENIED");
  await db.transaction(async tx => {
    expect((await readCommandCapability(db, tx, query, async () => ({ release: gate, authority: gate, record: gate }))).allowed).toBe(true);
  });
});

it("does not turn a first future purchase or its pre-start cancellation into history", async () => {
  const start = Date.now() + 60000;
  await apply(value(1, "active", start)); expect(await access()).toMatchObject({ history: false, newWork: false, settleExisting: false });
  await apply(value(2, "draining", start));
  await withOwnerPackageObligations(db, { companyId: f.company, packageId: "support", packageVersion: 1 }, tx => applyIn(tx, value(3, "inactive", start)));
  const facts = await readCompanyPurchaseFacts(db, f.company); expect(facts.history).toEqual([]);
  expect(purchasedCapability(facts.grants, f.company, "support.tickets", "history", start + 1, facts.history).allowed).toBe(false);
  const row = await db.prepare(`WITH scope AS (SELECT ?::text company_id,?::bigint now)
    SELECT ${purchasedCapabilityPredicate("scope.company_id", "support.tickets", "history", "scope.now")} allowed FROM scope`)
    .bind(f.company, start + 1).first<{ allowed: boolean }>();
  expect(row?.allowed).toBe(false);
});

it("observes a committed prior period at the end of a delayed renewal transaction", async () => {
  const start = Date.now() + 1000;
  await apply(value(1, "active", start)); expect((await readCompanyPurchaseFacts(db, f.company)).history).toEqual([]);
  await withOwnerPackageObligations(db, {companyId:f.company,packageId:"support",packageVersion:1}, async tx => {
    await applyIn(tx, value(2, "active", start + 60000));
    expect((await readCompanyPurchaseFacts(tx, f.company)).history).toEqual([]);
    await pauseUntil(start + 30);
  });
  const facts = await readCompanyPurchaseFacts(db, f.company);
  expect(facts.history).toHaveLength(1); expect(facts.history[0]).toMatchObject({ sourceGrantVersion: 1 });
  expect(await access()).toMatchObject({ history: true, newWork: false, settleExisting: false });
});

it("does not fabricate a started period from intermediate future changes in one delayed transaction", async () => {
  const start = Date.now() + 1000;
  await withOwnerPackageObligations(db, { companyId: f.company, packageId: "support", packageVersion: 1 }, async tx => {
    await applyIn(tx, value(1, "active", start)); await applyIn(tx, value(2, "draining", start)); await applyIn(tx, value(3, "inactive", start));
    await pauseUntil(start + 30);
  });
  expect((await readCompanyPurchaseFacts(db, f.company)).history).toEqual([]);
  expect(await access()).toMatchObject({ history: false, newWork: false, settleExisting: false });
});

it("rolls history, grant and audit back together after a late rejection", async () => {
  await expect(db.transaction(async tx => { await applyIn(tx, value(1, "active")); throw new Error("DECLARED_LATE_REJECTION"); })).rejects.toThrow("DECLARED_LATE_REJECTION");
  expect(await readCompanyPurchaseFacts(db, f.company)).toEqual({ grants: [], history: [] });
  expect(await db.prepare("SELECT count(*) total FROM company_package_grant_events WHERE company_id=?").bind(f.company).first()).toEqual({ total: 0 });
  await apply(value(1, "active", Date.now() + 60000));
  expect(await access()).toMatchObject({ history: false, newWork: false });
});

it("starts current history at its deadline without an extra grant mutation and captures it on the next change", async () => {
  const start = Date.now() + 1000;
  await apply(value(1, "active", start)); expect((await access()).history).toBe(false);
  const before = await db.prepare("SELECT count(*) total FROM company_package_grant_events WHERE company_id=?").bind(f.company).first();
  await pauseUntil(start + 20);
  expect((await access()).history).toBe(true);
  expect(await db.prepare("SELECT count(*) total FROM company_package_grant_events WHERE company_id=?").bind(f.company).first()).toEqual(before);
  await apply(value(2, "draining", start)); expect((await readCompanyPurchaseFacts(db, f.company)).history).toHaveLength(1);
});

it("keeps retained facts immutable and Company-scoped", async () => {
  await apply(value(1, "active"));
  await expect(db.prepare("UPDATE company_package_history_rights SET retained_at=0 WHERE company_id=?").bind(f.company).run()).rejects.toThrow("COMMERCIAL_PACKAGE_AUDIT_IMMUTABLE");
  await expect(db.prepare("DELETE FROM company_package_history_rights WHERE company_id=?").bind(f.company).run()).rejects.toThrow("COMMERCIAL_PACKAGE_AUDIT_IMMUTABLE");
  expect((await readCompanyPurchaseFacts(db, "different-company")).history).toEqual([]);
  expect((await access()).history).toBe(true);
});

it("does not retain an uncommitted intermediate period when later mutations cross its start", async () => {
  const start = Date.now() + 1000;
  await withOwnerPackageObligations(db, { companyId: f.company, packageId: "support", packageVersion: 1 }, async tx => {
    await applyIn(tx, value(1, "active", start));
    await pauseUntil(start + 30);
    await applyIn(tx, value(2, "draining", start)); await applyIn(tx, value(3, "inactive", start));
  });
  expect((await readCompanyPurchaseFacts(db, f.company)).history).toEqual([]);
  expect(await access()).toMatchObject({ history: false, newWork: false, settleExisting: false });
});
