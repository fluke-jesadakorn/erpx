import { expect, it } from "vitest";
import { cp, mkdir, mkdtemp, readdir, rm } from "node:fs/promises";
import { join, resolve } from "node:path";
import { randomUUID } from "node:crypto";
import { env } from "../../support/runtime-env";
import { migrateDatabase } from "@erpx/server-runtime/public/runtime/migrations";
import { packageGrantStore, packageGrantRevision, readCompanyPackageGrants, readCompanyPurchaseFacts } from "@erpx/server-commercial/public/package-grants";
import { ownerPackageObligations, withOwnerPackageObligations } from "@erpx/server-commercial/public/owner-package-obligations";
import { purchasedCapability, type CompanyPackageGrant } from "@erpx/server-commercial/public/packages";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

const db = env.H002_DB;
const store = packageGrantStore(db, { authorize: async () => ({ userId: "declared-old-writer-reviewer" }),
  obligations: (tx, companyId, packageId) => ownerPackageObligations(db, tx, companyId, packageId) });
const grant = (companyId: string, version: number, state: CompanyPackageGrant["state"], startsAt = 0): CompanyPackageGrant => ({
  companyId, packageId: "support", packageVersion: 1, version, state, startsAt, endsAt: null,
  reason: "Populated compatibility rehearsal", evidence: "test:audited-pre-1047-writer" });
const apply = (value: CompanyPackageGrant) => store.apply({ grant: value, expectedVersion: value.version - 1, idempotencyKey: randomUUID() });
async function inactivate(companyId: string, version: number, startsAt: number) {
  return reviewedChange(grant(companyId,version,"inactive",startsAt));
}
async function reviewedChange(value: CompanyPackageGrant) {
  const {companyId}=value;
  await withOwnerPackageObligations(db, { companyId, packageId: "support", packageVersion: 1 }, async tx => {
    const current = await readCompanyPackageGrants(tx, companyId);
    await store.batch.applyIn(tx, { companyId, expectedRevision: packageGrantRevision(companyId, current), idempotencyKey: randomUUID(),
      changes: [{ expectedVersion: value.version - 1, grant: value }] }, async () => {});
  });
}
async function digests(database: SqlDatabase, names?: string[]) {
  const tables = names ?? (await database.prepare("SELECT tablename FROM pg_tables WHERE schemaname='public' AND tablename<>'erpx_migrations' ORDER BY tablename").all<{ tablename: string }>()).results.map(row => row.tablename);
  const result: { table: string; rows: number; digest: string }[] = [];
  for (const table of tables) {
    if (!/^[a-z][a-z0-9_]*$/.test(table)) throw new Error("DECLARED_SCHEMA_TABLE_REQUIRED");
    const newCacheSource = table === "work_cache_revision_baselines" ? " WHERE table_name<>'company_package_history_rights'" : "";
    const row = await database.prepare(`SELECT count(*) rows,md5(coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]'::jsonb)::text) digest FROM "${table}" t${newCacheSource}`).first<{ rows: number; digest: string }>();
    result.push({ table, ...row! });
  }
  return result;
}

it("upgrades populated audited periods, preserves every prior row, registers the new cache source and keeps the normal old write shape compatible", async () => {
  const oldRoot = await mkdtemp(resolve(".runtime/tests/package-history-before-")), currentRoot = await mkdtemp(resolve(".runtime/tests/package-history-current-")), sourceRoot = resolve("migrations/postgres");
  try {
    await mkdir(join(oldRoot, "holding-write"), { recursive: true });
    await mkdir(join(currentRoot, "holding-write"), { recursive: true });
    // This receipt verifies the 1047 migration, independently of later additive schema.
    for (const file of (await readdir(join(sourceRoot, "holding-write"))).filter(file => file.endsWith(".sql") && Number(file.slice(0, 4)) <= 1047))
      await cp(join(sourceRoot, "holding-write", file), join(currentRoot, "holding-write", file));
    for (const file of (await readdir(join(currentRoot, "holding-write"))).filter(file => file.endsWith(".sql") && Number(file.slice(0, 4)) <= 1046))
      await cp(join(currentRoot, "holding-write", file), join(oldRoot, "holding-write", file));
    await migrateDatabase(db, "holding-write", oldRoot);
    const oldCompany = randomUUID(), cancelled = randomUUID(), rescheduled = randomUUID(), future = Date.now() + 60000;
    await apply(grant(oldCompany, 1, "active")); await apply(grant(oldCompany, 2, "draining")); await inactivate(oldCompany, 3, 0);
    await apply(grant(oldCompany, 4, "active", future));
    await apply(grant(cancelled, 1, "active", future)); await apply(grant(cancelled, 2, "draining", future)); await inactivate(cancelled, 3, future);
    await apply(grant(rescheduled, 1, "active", future)); await reviewedChange(grant(rescheduled, 2, "active", future + 60000));
    const before = await digests(db), receipts = (await db.prepare("SELECT name,checksum FROM erpx_migrations ORDER BY name").all()).results;
    await migrateDatabase(db, "holding-write", currentRoot);
    expect(await digests(db, before.map(row => row.table))).toEqual(before);
    expect(await db.prepare("SELECT table_name,revision FROM work_cache_revision_baselines WHERE table_name='company_package_history_rights'").first())
      .toEqual({ table_name: "company_package_history_rights", revision: 2 });
    expect((await db.prepare("SELECT name,checksum FROM erpx_migrations WHERE name NOT LIKE '1047_%' ORDER BY name").all()).results).toEqual(receipts);
    const facts = await readCompanyPurchaseFacts(db, oldCompany);
    expect(facts.history).toHaveLength(1); expect(facts.history[0]).toMatchObject({ sourceGrantVersion: 1 });
    expect(purchasedCapability(facts.grants, oldCompany, "support.tickets", "history", Date.now(), facts.history).allowed).toBe(true);
    for (const companyId of [cancelled, rescheduled]) {
      const facts = await readCompanyPurchaseFacts(db, companyId);
      expect(facts.history).toEqual([]);
      expect(purchasedCapability(facts.grants, companyId, "support.tickets", "history", future + 1, facts.history).allowed).toBe(false);
    }
    const retained = await db.prepare("SELECT * FROM company_package_history_rights ORDER BY company_id,package_id,package_version").all();
    await migrateDatabase(db, "holding-write", currentRoot);
    expect((await db.prepare("SELECT * FROM company_package_history_rights ORDER BY company_id,package_id,package_version").all()).results).toEqual(retained.results);
    // Selecting the older migration directory is compatible and additive: it
    // does not remove the newer history schema or mutate old checksums.
    await migrateDatabase(db, "holding-write", oldRoot);
    await apply(grant(oldCompany, 5, "active"));
    expect((await readCompanyPurchaseFacts(db, oldCompany)).history).toEqual(facts.history);
    expect((await readCompanyPackageGrants(db, oldCompany))[0].version).toBe(5);
  } finally { await rm(oldRoot, { recursive: true, force: true }); await rm(currentRoot, { recursive: true, force: true }); }
}, 60000);
