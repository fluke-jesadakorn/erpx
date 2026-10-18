import { beforeAll, expect, it } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { legacyCompanyModuleAccess } from "@/server/modules/identity_access/public/legacy-module-access";
import { legacyPackagePreview } from "@/server/modules/commercial/domain/legacy-packages";
const db=env.H001_DB;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
it("reads actual published module ceilings and HR restrictions without modifying Company grants or assignments", async()=>{
  const f=await seedExpenseCompany(db,env.H001_FILES),finance=await f.member("legacy-finance","finance_preparer"),hr=await f.member("legacy-hr","hr_admin");
  await db.prepare("UPDATE assignments SET modules_json=? WHERE id=?").bind(JSON.stringify(["finance","pos"]),finance.assignmentId).run();
  const before=await db.prepare("SELECT id,version,modules_json,revoked_at FROM assignments WHERE company_id=? ORDER BY id").bind(f.company).all();
  const facts=await legacyCompanyModuleAccess(db,f.company);
  expect(facts.witnesses.find(row=>row.assignmentId===finance.assignmentId)?.modules).toEqual(["finance"]);
  expect(facts.witnesses.find(row=>row.assignmentId===hr.assignmentId)?.modules).not.toContain("people_payroll");
  expect((await db.prepare("SELECT id,version,modules_json,revoked_at FROM assignments WHERE company_id=? ORDER BY id").bind(f.company).all()).results).toEqual(before.results);
  expect(await db.prepare("SELECT 1 FROM company_package_grants WHERE company_id=?").bind(f.company).first()).toBeNull();
  expect(JSON.stringify(facts)).not.toContain('"userId"');expect(JSON.stringify(facts)).not.toContain('"permissions"');
});

it('locks original routine sources before the license snapshot and expires employment evidence at its UTC date boundary',async()=>{
 const {withLegacyModuleAccessLease}=await import('@/server/modules/identity_access/public/legacy-module-access'),f=await seedExpenseCompany(db,env.H001_FILES);let now=Date.now();const boundary=(Math.floor(now/86400000)+1)*86400000;
 await withLegacyModuleAccessLease(db,f.company,async(_database,read)=>{
  const client=await db.pool.connect();try{await client.query('BEGIN');await client.query("SET LOCAL lock_timeout='50ms'");await expect(client.query('DELETE FROM routine_templates WHERE false')).rejects.toMatchObject({code:'55P03'});await client.query('ROLLBACK');}finally{client.release();}
  const lease=await read();expect(lease.facts.witnesses.some(row=>row.routineAccess?.activeStaff)).toBe(true);now=boundary;await expect(lease.assertCurrent()).rejects.toMatchObject({code:'STALE_LEGACY_PREVIEW'});
 },()=>now);
});
it("binds legacy review to current custom policy and revocation instead of stale assignment modules",async()=>{
  const f=await seedExpenseCompany(db,env.H001_FILES),role="custom_legacy_"+crypto.randomUUID().replaceAll("-","");
  await db.prepare("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES(?,?,?,false,1,'active')").bind(f.company,role,"Legacy fixture").run();
  await db.prepare("INSERT INTO iam_role_revisions VALUES(?,?,1,?::jsonb,'[]'::jsonb,'[\"holding\"]'::jsonb,?,?)").bind(f.company,role,JSON.stringify(["module.finance.read"]),f.owner.userId,Date.now()).run();
  const actor=await f.member("legacy-custom",role);
  const first=await legacyCompanyModuleAccess(db,f.company);
  expect(first.witnesses.find(row=>row.assignmentId===actor.assignmentId)?.modules).toEqual(["finance"]);
  await db.prepare("INSERT INTO iam_role_revisions VALUES(?,?,2,?::jsonb,'[]'::jsonb,'[\"holding\"]'::jsonb,?,?)").bind(f.company,role,JSON.stringify(["module.pos.read"]),f.owner.userId,Date.now()).run();
  await db.prepare("UPDATE iam_roles SET published_revision=2 WHERE company_id=? AND id=?").bind(f.company,role).run();
  const second=await legacyCompanyModuleAccess(db,f.company);
  expect(second.revision).not.toBe(first.revision);expect(second.witnesses.find(row=>row.assignmentId===actor.assignmentId)?.modules).toEqual(["pos"]);
  await db.prepare("UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?").bind(Date.now(),actor.assignmentId).run();
  const revoked=await legacyCompanyModuleAccess(db,f.company);
  expect(revoked.revision).not.toBe(second.revision);expect(revoked.witnesses.some(row=>row.assignmentId===actor.assignmentId)).toBe(false);
});
it("proves old Company group labels from current published permissions while preserving assignment records", async () => {
  const f = await seedExpenseCompany(db, env.H001_FILES);
  const modules = ["commerce", "iam", "people", "pms", "pos", "finance", "organization"];
  await db.prepare("UPDATE assignments SET modules_json=? WHERE id=?").bind(JSON.stringify(modules), f.owner.assignmentId).run();
  const before = await db.prepare("SELECT modules_json,version FROM assignments WHERE id=?").bind(f.owner.assignmentId).first();
  const facts = await legacyCompanyModuleAccess(db, f.company), witness = facts.witnesses.find(row => row.assignmentId === f.owner.assignmentId)!;
  expect(witness.modules.filter(id => ["commerce", "iam", "people"].includes(id))).toEqual(["commerce", "iam", "people"]);
  expect(witness.compatibility.find(proof => proof.label === "commerce")?.modules).toEqual(["sales", "procurement", "inventory"]);
  expect(witness.compatibility.find(proof => proof.label === "iam")?.modules).toEqual(["identity_access"]);
  const now = Date.now(), plan = legacyPackagePreview(facts, [], { startsAt: now, now, reason: "Review actual published owner access", evidence: "test:historical-groups" });
  expect(plan.requiredPackages).toContain("sales"); expect(plan.requiredPackages).toContain("supply");
  expect(plan.requiredPackages).not.toContain("legal"); expect(plan.requiredPackages).not.toContain("manufacturing");
  expect(plan.roles.every(role => role.missingPurchasedCoverage.length === 0)).toBe(true);
  expect(await db.prepare("SELECT modules_json,version FROM assignments WHERE id=?").bind(f.owner.assignmentId).first()).toEqual(before);
  expect(await db.prepare("SELECT 1 FROM company_package_grants WHERE company_id=?").bind(f.company).first()).toBeNull();
});
