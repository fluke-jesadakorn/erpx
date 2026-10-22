import { commercialPackages } from "@erpx/server-commercial/public/packages";
import { readCompanyPackageGrants } from "@erpx/server-commercial/public/package-grants";
import { beforeAll, expect, it } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import { packageGrantStore } from "@erpx/server-commercial/adapters/package-grants";
import type { CompanyPackageGrant } from "@erpx/server-commercial/public/packages";
import { randomUUID } from "node:crypto";
import { readCommandCapability, readEffectiveCapability } from "@erpx/server-commercial/public/capability-decision";
import { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
const db = env.H001_DB;
let authorized = true, obligations = 0;
const store = packageGrantStore(db, { authorize: async () => { if (!authorized) throw new Error("FORBIDDEN"); return { userId: "provider-fixture" }; }, obligations: async () => obligations, now: () => 1000 });
const change = (companyId: string = randomUUID(), packageId = "work") => ({ idempotencyKey: randomUUID(), expectedVersion: 0, grant: { companyId, packageId, packageVersion: 1, state: "active", startsAt: 0, endsAt: null, version: 1, reason: "Fixture administrator decision", evidence: "test:reviewed" } as CompanyPackageGrant });
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
it("reads the capability on the actual owner transaction, refuses a pool and rechecks expiry/owner gates", async () => {
  const input = change(randomUUID(), "pos"); await store.apply(input);
  const query = { companyId: input.grant.companyId, capability: "pos.sales", purpose: "new-work" as const, now: 1000 };
  const gate = { allowed: true, revision: "owner-1" };
  const gates = async () => ({ release: gate, authority: gate, record: gate });
  await expect(readCommandCapability(db, db, query, gates)).rejects.toThrow("TRANSACTION_NOT_ACTIVE_OR_OWNED");
  const first = await db.transaction(async tx => readCommandCapability(db, tx, query, async database => {
    expect(database).toBe(tx); return gates();
  }));
  expect(first.allowed).toBe(true);
  const draining = { expectedVersion: 1, idempotencyKey: randomUUID(), grant: { ...input.grant, version: 2, state: "draining" as const } };
  await store.apply(draining);
  const second = await db.transaction(tx => readCommandCapability(db, tx, query, gates));
  expect(second.code).toBe("PACKAGE_DRAINING"); expect(second.revision).not.toBe(first.revision);
  expect((await readEffectiveCapability(db, { ...query, purpose: "settle-existing", ...await gates() })).allowed).toBe(true);
  expect((await readEffectiveCapability(db, { ...query, purpose: "history", ...await gates(), record: { allowed: false, revision: "owner-2" } })).code).toBe("RECORD_DENIED");
});
it("uses a fresh server time after the lock and owner gates even when the caller carries a pre-expiry timestamp", async () => {
  const input = change(randomUUID(), "pos");
  await store.apply({ ...input, grant: { ...input.grant, endsAt: Date.now() - 1000 } });
  const query = { companyId: input.grant.companyId, capability: "pos.sales", purpose: "new-work" as const, now: 1000 };
  const gate = { allowed: true, revision: "owner-1" };
  const result = await db.transaction(tx => readCommandCapability(db, tx, query, async () => ({ release: gate, authority: gate, record: gate })));
  expect(result.code).toBe("PACKAGE_DRAINING"); expect(result.allowed).toBe(false);
});
it("retries a pre-existing serializable snapshot instead of allowing a command after concurrent draining", async () => {
  const input = change(randomUUID(), "pos"); await store.apply(input);
  let snapshotReady!: () => void, resume!: () => void, attempts = 0;
  const ready = new Promise<void>(resolve => { snapshotReady = resolve; });
  const resumeOwner = new Promise<void>(resolve => { resume = resolve; });
  const gate = { allowed: true, revision: "owner-1" };
  const pending = db.transaction(async tx => {
    attempts++;
    if (attempts === 1) {
      expect((await tx.prepare("SELECT state FROM company_package_grants WHERE company_id=?").bind(input.grant.companyId).first<{ state: string }>())!.state).toBe("active");
      snapshotReady(); await resumeOwner;
    }
    return readCommandCapability(db, tx, { companyId: input.grant.companyId, capability: "pos.sales", purpose: "new-work" }, async () => ({ release: gate, authority: gate, record: gate }));
  });
  await ready;
  try { await store.apply({ expectedVersion: 1, idempotencyKey: randomUUID(), grant: { ...input.grant, version: 2, state: "draining" } }); }
  finally { resume(); }
  const result = await pending;
  expect(result.allowed).toBe(false); expect(result.code).toBe("PACKAGE_DRAINING"); expect(attempts).toBeGreaterThan(1);
});
it("previews without writes and atomically applies a grant with one immutable audit receipt", async () => {
  const input = change();
  expect((await store.preview(input)).before).toBeNull();
  expect(await db.prepare("SELECT 1 FROM company_package_grants WHERE company_id=?").bind(input.grant.companyId).first()).toBeNull();
  expect(await store.apply(input)).toEqual(input.grant);
  expect(await store.apply(input)).toEqual(input.grant);
  expect((await db.prepare("SELECT count(*) AS total FROM company_package_grant_events WHERE id=?").bind(input.idempotencyKey).first<{ total: number }>())!.total).toBe(1);
  await expect(store.apply({ ...input, grant: { ...input.grant, reason: "Different" } })).rejects.toThrow("IDEMPOTENCY_CONFLICT");
});
it("rechecks current authority even on an idempotent replay", async () => {
  const input = change();
  await store.apply(input);
  authorized = false;
  try { await expect(store.apply(input)).rejects.toThrow("FORBIDDEN"); } finally { authorized = true; }
});
it("supplies the exact target Company to authority on preview, apply and replay", async () => {
  const input = change(), denied = change();
  const seen: string[] = [];
  const scoped = packageGrantStore(db, { authorize: async (_database, companyId) => {
    seen.push(companyId);
    if (companyId !== input.grant.companyId) throw new Error("FORBIDDEN_COMPANY");
    return { userId: "scoped-provider-fixture" };
  }, obligations: async () => 0 });
  await scoped.preview(input); await scoped.apply(input); await scoped.apply(input);
  expect(seen.every(companyId => companyId === input.grant.companyId)).toBe(true);
  await expect(scoped.preview(denied)).rejects.toThrow("FORBIDDEN_COMPANY");
  await expect(scoped.apply(denied)).rejects.toThrow("FORBIDDEN_COMPANY");
});
it("rejects concurrent stale writers and keeps the losing write out of audit", async () => {
  const first = change(), second = { ...first, idempotencyKey: randomUUID() };
  const results = await Promise.allSettled([store.apply(first), store.apply(second)]);
  expect(results.filter(result => result.status === "fulfilled")).toHaveLength(1);
  expect(results.filter(result => result.status === "rejected")).toHaveLength(1);
  expect((await db.prepare("SELECT count(*) AS total FROM company_package_grant_events WHERE company_id=?").bind(first.grant.companyId).first<{ total: number }>())!.total).toBe(1);
});
it("requires a same-Company base covering the complete add-on period", async () => {
  const input = change(randomUUID(), "people-payroll");
  await expect(store.apply(input)).rejects.toThrow("PACKAGE_DEPENDENCY_REQUIRED");
  const base = change(input.grant.companyId, "people");
  await store.apply({ ...base, grant: { ...base.grant, endsAt: 2000 } });
  await expect(store.apply(input)).rejects.toThrow("PACKAGE_DEPENDENCY_REQUIRED");
  await store.apply({ ...input, grant: { ...input.grant, endsAt: 2000 } });
});
it("requires draining and a fresh owner obligation check before final inactivation", async () => {
  const input = change(); await store.apply(input);
  const draining = { expectedVersion: 1, idempotencyKey: randomUUID(), grant: { ...input.grant, version: 2, state: "draining" as const } };
  await store.apply(draining);
  const inactive = { expectedVersion: 2, idempotencyKey: randomUUID(), grant: { ...input.grant, version: 3, state: "inactive" as const } };
  obligations = 1;
  try { await expect(store.apply(inactive)).rejects.toThrow("PACKAGE_OBLIGATIONS_REMAIN"); } finally { obligations = 0; }
  expect(await store.apply(inactive)).toEqual(inactive.grant);
});
it("keeps a draining add-on settled until a reviewed atomic base/add-on inactivation", async () => {
  const companyId=randomUUID(),{readCompanyPackageGrants,packageGrantRevision}=await import("@erpx/server-commercial/public/package-grants");
  for(const packageId of ["support","support-portal"])await store.apply(change(companyId,packageId));
  for(const packageId of ["support","support-portal"]){const before=(await readCompanyPackageGrants(db,companyId)).find(row=>row.packageId===packageId)!;await store.apply({expectedVersion:1,idempotencyKey:randomUUID(),grant:{...before,state:"draining",version:2}});}
  const before=await readCompanyPackageGrants(db,companyId),base=before.find(row=>row.packageId==='support')!;
  const inactive={expectedVersion:2,idempotencyKey:randomUUID(),grant:{...base,state:"inactive" as const,version:3}};
  await expect(store.preview(inactive)).rejects.toThrow("PACKAGE_DEPENDENCY_IN_USE");
  await expect(store.apply(inactive)).rejects.toThrow("PACKAGE_DEPENDENCY_IN_USE");
  expect(await readCompanyPackageGrants(db,companyId)).toEqual(before);
  const request={companyId,expectedRevision:packageGrantRevision(companyId,before),idempotencyKey:randomUUID(),changes:before.map(grant=>({expectedVersion:2,grant:{...grant,state:"inactive" as const,version:3}}))};
  await store.batch.preview(request);await store.batch.apply(request,async()=>{});
  expect((await readCompanyPackageGrants(db,companyId)).map(row=>row.state)).toEqual(["inactive","inactive"]);
});
it("refuses to move a base beyond a draining add-on even when the declared owner backlog is empty", async () => {
  const companyId=randomUUID(),{readCompanyPackageGrants}=await import("@erpx/server-commercial/public/package-grants");
  for(const packageId of ["support","support-portal"])await store.apply(change(companyId,packageId));
  const addon=(await readCompanyPackageGrants(db,companyId)).find(row=>row.packageId==='support-portal')!;
  await store.apply({expectedVersion:1,idempotencyKey:randomUUID(),grant:{...addon,state:"draining",version:2}});
  const base=(await readCompanyPackageGrants(db,companyId)).find(row=>row.packageId==='support')!,next={expectedVersion:1,idempotencyKey:randomUUID(),grant:{...base,startsAt:2000,version:2}};
  await expect(store.preview(next)).rejects.toThrow("PACKAGE_DEPENDENCY_PERIOD_CONFLICT");await expect(store.apply(next)).rejects.toThrow("PACKAGE_DEPENDENCY_PERIOD_CONFLICT");
  expect((await readCompanyPackageGrants(db,companyId)).find(row=>row.packageId==='support')).toEqual(base);
});
it("previews and commits a reviewed base/add-on change atomically with one immutable batch receipt", async () => {
  const companyId = randomUUID(), { packageGrantRevision } = await import("@erpx/server-commercial/public/package-grants");
  const input = { companyId, expectedRevision: packageGrantRevision(companyId, []), idempotencyKey: randomUUID(), changes: [change(companyId,"people-payroll"),change(companyId,"people")].map(({grant,expectedVersion})=>({grant,expectedVersion})) };
  const preview = await store.batch.preview(input);
  expect(preview.before).toEqual([]); expect(preview.after).toHaveLength(2);
  expect(await db.prepare("SELECT 1 FROM company_package_grants WHERE company_id=?").bind(companyId).first()).toBeNull();
  let reviews=0;
  expect(await store.batch.apply(input, async reviewed => { reviews++;expect(reviewed).toEqual(preview); })).toEqual(preview);
  expect(await store.batch.apply(input, async()=>{throw new Error("PREVIEW_EXPIRED");})).toEqual(preview); expect(reviews).toBe(1);
  expect((await db.prepare("SELECT count(*) AS total FROM company_package_grant_events WHERE company_id=?").bind(companyId).first<{total:number}>())!.total).toBe(2);
  expect((await db.prepare("SELECT count(*) AS total FROM company_package_change_sets WHERE company_id=?").bind(companyId).first<{total:number}>())!.total).toBe(1);
  authorized=false;try{await expect(store.batch.apply(input,async()=>{})).rejects.toThrow("FORBIDDEN");}finally{authorized=true;}
  await expect(store.apply({...change(companyId,"work"),idempotencyKey:input.idempotencyKey})).rejects.toThrow("IDEMPOTENCY_CONFLICT");
});
it("rolls back every grant and audit if authority disappears after the first staged grant", async () => {
  const companyId=randomUUID(),{packageGrantRevision}=await import("@erpx/server-commercial/public/package-grants");
  const scoped=packageGrantStore(db,{authorize:async database=>{
    const count=(await database.prepare("SELECT count(*) AS total FROM company_package_grants WHERE company_id=?").bind(companyId).first<{total:number}>())!.total;
    if(count>0)throw new Error("FORBIDDEN");return{userId:"provider-fixture"};
  },obligations:async()=>0,now:()=>1000});
  const input={companyId,expectedRevision:packageGrantRevision(companyId,[]),idempotencyKey:randomUUID(),changes:[change(companyId,"people"),change(companyId,"people-payroll")].map(({grant,expectedVersion})=>({grant,expectedVersion}))};
  await expect(scoped.batch.apply(input,async()=>{})).rejects.toThrow("FORBIDDEN");
  for(const table of ["company_package_grants","company_package_grant_events","company_package_change_sets"])expect((await db.prepare(`SELECT count(*) AS total FROM ${table} WHERE company_id=?`).bind(companyId).first<{total:number}>())!.total).toBe(0);
});
it("validates the complete prospective periods before atomically shortening a base and its add-on", async () => {
  const companyId=randomUUID(),{packageGrantRevision,readCompanyPackageGrants}=await import("@erpx/server-commercial/public/package-grants");
  for(const id of ["people","people-payroll"]){const input=change(companyId,id);await store.apply({...input,grant:{...input.grant,endsAt:5000}});}
  const before=await readCompanyPackageGrants(db,companyId),input={companyId,expectedRevision:packageGrantRevision(companyId,before),idempotencyKey:randomUUID(),changes:before.map(grant=>({expectedVersion:grant.version,grant:{...grant,endsAt:3000,version:grant.version+1}}))};
  await store.batch.preview(input);await store.batch.apply(input,async()=>{});
  expect((await readCompanyPackageGrants(db,companyId)).map(grant=>grant.endsAt)).toEqual([3000,3000]);
});
it("rejects a changed Company revision, expired review and duplicate package targets without any writes", async () => {
  const companyId=randomUUID(),{packageGrantRevision}=await import("@erpx/server-commercial/public/package-grants");
  const entry=change(companyId),input={companyId,expectedRevision:packageGrantRevision(companyId,[]),idempotencyKey:randomUUID(),changes:[{grant:entry.grant,expectedVersion:0}]};
  await expect(store.batch.apply(input,async()=>{throw new Error("PREVIEW_EXPIRED");})).rejects.toThrow("PREVIEW_EXPIRED");
  expect(await db.prepare("SELECT 1 FROM company_package_grants WHERE company_id=?").bind(companyId).first()).toBeNull();
  await expect(store.batch.preview({...input,changes:[...input.changes,...input.changes]})).rejects.toThrow("INVALID_PACKAGE_BATCH");
  await store.apply(change(companyId,"finance")); await expect(store.batch.apply(input,async()=>{})).rejects.toThrow("STALE_PACKAGE_BATCH");
});
it("rejects one of two concurrent whole-Company reviews even when they target different new packages",async()=>{
  const companyId=randomUUID(),{packageGrantRevision}=await import("@erpx/server-commercial/public/package-grants");
  const inputs=["work","finance"].map(packageId=>{const entry=change(companyId,packageId);return{companyId,expectedRevision:packageGrantRevision(companyId,[]),idempotencyKey:randomUUID(),changes:[{grant:entry.grant,expectedVersion:0}]};});
  const results=await Promise.allSettled(inputs.map(input=>store.batch.apply(input,async()=>{})));
  expect(results.filter(result=>result.status==="fulfilled")).toHaveLength(1);
  expect(results.filter(result=>result.status==="rejected")).toHaveLength(1);
  expect((await db.prepare("SELECT count(*) AS total FROM company_package_grants WHERE company_id=?").bind(companyId).first<{total:number}>())!.total).toBe(1);
});
it("keeps both per-grant and reviewed batch audit rows immutable",async()=>{
  const companyId=randomUUID(),{packageGrantRevision}=await import("@erpx/server-commercial/public/package-grants"),entry=change(companyId);
  const input={companyId,expectedRevision:packageGrantRevision(companyId,[]),idempotencyKey:randomUUID(),changes:[{grant:entry.grant,expectedVersion:0}]};
  await store.batch.apply(input,async()=>{});
  for(const table of ["company_package_change_sets","company_package_grant_events"]){
    await expect(db.prepare(`UPDATE ${table} SET actor_id='other' WHERE company_id=?`).bind(companyId).run()).rejects.toThrow("COMMERCIAL_PACKAGE_AUDIT_IMMUTABLE");
    await expect(db.prepare(`DELETE FROM ${table} WHERE company_id=?`).bind(companyId).run()).rejects.toThrow("COMMERCIAL_PACKAGE_AUDIT_IMMUTABLE");
  }
});
it("requires the live owner's transaction for batch apply and rolls back grants, audits and receipts when the owner rejects afterward", async () => {
  const companyId = randomUUID(), { packageGrantRevision } = await import("@erpx/server-commercial/public/package-grants"), entry = change(companyId);
  const input = { companyId, expectedRevision: packageGrantRevision(companyId, []), idempotencyKey: randomUUID(), changes: [{ grant: entry.grant, expectedVersion: 0 }] };
  expect(() => store.batch.applyIn(db, input, async () => {})).toThrow("TRANSACTION_NOT_ACTIVE_OR_OWNED");
  const other = new PostgresDatabase({ ...db.pool.options, max: 1 });
  try { await expect(other.transaction(tx => store.batch.applyIn(tx, input, async () => {}))).rejects.toThrow("TRANSACTION_NOT_ACTIVE_OR_OWNED"); }
  finally { await other.close(); }
  let escaped!: SqlDatabase;
  await expect(db.transaction(async tx => {
    escaped = tx;
    expect((await store.batch.applyIn(tx, input, async () => {})).after).toEqual([entry.grant]);
    throw new Error("SOURCE_CHANGED_AFTER_RECEIPT");
  })).rejects.toThrow("SOURCE_CHANGED_AFTER_RECEIPT");
  expect(() => store.batch.applyIn(escaped, input, async () => {})).toThrow("TRANSACTION_NOT_ACTIVE_OR_OWNED");
  for (const table of ["company_package_grants", "company_package_grant_events", "company_package_change_sets", "company_package_revisions"])
    expect(await db.prepare(`SELECT 1 FROM ${table} WHERE company_id=?`).bind(companyId).first()).toBeNull();
});


// This reaches each actual source unit, rather than accepting a package merely
// because its ID is listed. Nonzero original owner work is valid evidence here.
it.each(commercialPackages.map(({ id, version }) => ({ packageId: id, packageVersion: version })))
  ("opens the real declared $packageId v$packageVersion withdrawal source unit without changing Company authority or grants", async definition => {
    const { expenseFixture } = await import("../../support/helpers/expense-fixture");
    const { withOwnerPackageObligations, readOwnerPackageObligations } = await import("@erpx/server-commercial/public/owner-package-obligations");
    const { packageObligationModules } = await import("@erpx/server-commercial/public/package-obligations");
    const fixture = await expenseFixture(false, false);
    const input = { companyId: fixture.company, ...definition, now: Date.now() };
    const authorityImage = async () => (await db.prepare("SELECT to_jsonb(a)::text image FROM assignments a WHERE company_id=? ORDER BY id").bind(fixture.company).all()).results;
    const authority = await authorityImage(), grants = await readCompanyPackageGrants(db, fixture.company);
    const evidence = await withOwnerPackageObligations(db, input, transaction => readOwnerPackageObligations(db, transaction, input));
    const modules = packageObligationModules(input.packageId, input.packageVersion);
    expect(evidence.observations.map(row => row.moduleId)).toEqual(modules);
    expect(new Set(modules).size).toBe(modules.length);
    expect(modules.length).toBeGreaterThan(0);
    for (const row of evidence.observations) {
      expect(row).toMatchObject({ companyId: fixture.company, packageId: input.packageId, observedAt: input.now });
      expect(row.revision.length).toBeGreaterThan(0);
      expect(Number.isSafeInteger(row.outstanding)).toBe(true);
      expect(row.outstanding).toBeGreaterThanOrEqual(0);
    }
    expect(evidence.total).toBe(evidence.observations.reduce((total, row) => total + row.outstanding, 0));
    expect(await authorityImage()).toEqual(authority);
    expect(await readCompanyPackageGrants(db, fixture.company)).toEqual(grants);
  });


it("composes base Hotel/POS and their three add-on source units in one owned Company review without replacing individual evidence", async () => {
  const { expenseFixture } = await import("../../support/helpers/expense-fixture");
  const { withOwnerPackageObligations, readOwnerPackageObligations } = await import("@erpx/server-commercial/public/owner-package-obligations");
  const fixture = await expenseFixture(false, false), now = Date.now();
  const inputs = ["pos", "pms", "pos-restaurant", "pos-hotel-charge", "pms-minibar-linen"].map(packageId => ({ companyId: fixture.company, packageId, packageVersion: 1, now }));
  const expected = [];
  for (const input of inputs) expected.push(await withOwnerPackageObligations(db, input, transaction => readOwnerPackageObligations(db, transaction, input)));
  const combined = await withOwnerPackageObligations(db, inputs, async transaction => {
    const observations = [];
    for (const input of inputs) observations.push(await readOwnerPackageObligations(db, transaction, input));
    return observations;
  });
  expect(combined).toEqual(expected);
});
