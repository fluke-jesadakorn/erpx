import { beforeAll, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { legacyCompanyModuleAccess } from "@/server/modules/identity_access/public/legacy-module-access";
import { legacyPackagePreview } from "@/server/modules/commercial/domain/legacy-packages";
import { createProductWorkspace } from "@/server/modules/identity_access/application/product-workspace";
import { productWorkspaceTarget } from "@/server/modules/identity_access/domain/product-workspace";
import { workspaceGrantsForAccount, type AccountAccessContext } from "@/server/modules/identity_access/public/authorization";
import { accountWorkModuleAccess } from "@/server/modules/identity_access/public/work-module-access";
import { readCompanyPurchaseFacts } from "@/server/modules/commercial/public/package-grants";
import { ApiError } from "@/server/modules/runtime/public/http";
vi.mock("@/server/modules/runtime/public/runtime", () => ({ holdingResources: async () => ({ write: env.H001_DB }), runtimeEnv: async () => env }));
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

it("projects the published commerce/iam/people legacy witness into current Sales workspace entry without writing grants", async () => {
  const f = await seedExpenseCompany(db, env.H001_FILES);
  await db.prepare("UPDATE assignments SET modules_json=? WHERE id=?")
    .bind(JSON.stringify(["commerce", "iam", "people"]), f.owner.assignmentId).run();

  // This snapshot is taken after shaping the fixture as a genuine legacy assignment.
  const assignmentsBefore = (await db.prepare("SELECT id,user_id,role,company_id,modules_json,version,revoked_at,scope_kind,scope_node_id,valid_from,valid_until FROM assignments WHERE company_id=? ORDER BY id")
    .bind(f.company).all()).results;
  const grantsBefore = (await db.prepare("SELECT * FROM company_package_grants WHERE company_id=? ORDER BY package_id")
    .bind(f.company).all()).results;

  const facts = await legacyCompanyModuleAccess(db, f.company);
  const witness = facts.witnesses.find(row => row.assignmentId === f.owner.assignmentId)!;
  expect(witness.modules).toEqual(["commerce", "iam", "people", "people_payroll"]);
  expect(witness.compatibility).toEqual(expect.arrayContaining([
    { label: "commerce", modules: ["sales", "procurement", "inventory"], source: "published-permission-policy/v1" },
    { label: "iam", modules: ["identity_access"], source: "published-permission-policy/v1" },
    { label: "people", modules: ["people_payroll"], source: "published-permission-policy/v1" },
  ]));

  const now = Date.now();
  const preview = legacyPackagePreview(facts, [], {
    startsAt: now,
    now,
    reason: "Review legacy compatibility for direct Sales entry",
    evidence: "test:legacy-direct-sales-entry",
  });
  const ownerCoverage = preview.roles.find(row => row.assignmentId === f.owner.assignmentId)!;
  expect(ownerCoverage.compatibilityLabels).toEqual(["commerce", "iam", "people"]);
  expect(ownerCoverage.purchaseCoverageModules).toEqual(["identity_access", "inventory", "people_payroll", "procurement", "sales"]);
  expect(ownerCoverage.missingPurchasedCoverage).toEqual([]);
  expect(preview.requiredPackages).toEqual(expect.arrayContaining(["sales", "supply", "people-payroll"]));

  const sessionId = crypto.randomUUID(), expiresAt = Date.now() + 600_000;
  const scope = { kind: "holding" as const, holdingId: "h001" as const, holdingSlug: "test", holdingName: "Fixture", companyId: f.company };
  const access = async (): Promise<AccountAccessContext> => {
    const { rows, roles } = await workspaceGrantsForAccount(db, f.company, f.owner.userId);
    return {
      scope, sessionId, userId: f.owner.userId, expiresAt, name: "Fictional legacy owner", email: "legacy-owner@example.test", requestHeaders: new Headers(),
      assignments: rows.filter(row => roles.get(row.role)?.scopes.includes(row.scope_kind)).map(row => ({
        id: row.id, role: row.role, companyId: row.company_id, branchId: row.branch_id, scopeKind: row.scope_kind,
        scopeNodeId: row.scope_node_id, modules: JSON.parse(row.modules_json) as string[], version: row.version,
        validUntil: row.valid_until, label: roles.get(row.role)?.name,
      })),
    };
  };
  const input = productWorkspaceTarget("sales", f.company, "/holdings/test/customers-sales/sales/today?legacy=compatibility#entry")!;
  const service = createProductWorkspace({
    access,
    modules: accountWorkModuleAccess,
    // Only the in-memory projection is supplied to the current host; no grant store or write path is used.
    purchaseFacts: async account => {
      const actual = await readCompanyPurchaseFacts(db, account.scope.companyId);
      return { ...actual, grants: preview.after.map(grant => ({ ...grant })) };
    },
    ready: async () => true,
    selection: async (account, applicable) => {
      const owner = applicable.find(assignment => assignment.id === f.owner.assignmentId);
      if (!owner) return null;
      return { session_id: account.sessionId, user_id: account.userId, company_id: f.company,
        assignment_id: owner.id, assignment_version: owner.version!, revision: 1, expires_at: account.expiresAt };
    },
    select: async () => { throw new Error("UNEXPECTED_WORK_SELECTION_WRITE"); },
    now: Date.now,
    error: (status, code, message) => new ApiError(status, code, message),
  });
  const state = await service.read("sales", input);
  expect(state.options.find(option => option.assignmentId === f.owner.assignmentId)).toMatchObject({
    assignmentVersion: witness.assignmentVersion,
    target: input.target,
    purchasePurpose: "new-work",
  });
  expect(state.target).toBe(input.target);

  expect((await db.prepare("SELECT id,user_id,role,company_id,modules_json,version,revoked_at,scope_kind,scope_node_id,valid_from,valid_until FROM assignments WHERE company_id=? ORDER BY id")
    .bind(f.company).all()).results).toEqual(assignmentsBefore);
  expect((await db.prepare("SELECT * FROM company_package_grants WHERE company_id=? ORDER BY package_id")
    .bind(f.company).all()).results).toEqual(grantsBefore);
});
