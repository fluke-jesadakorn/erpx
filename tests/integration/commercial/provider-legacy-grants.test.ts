import { beforeAll, expect, it, vi } from "vitest";
import { randomUUID } from "node:crypto";
import { applyMigrations, env } from "../../support/runtime-env";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { productSsoStore } from "@/server/modules/identity_access/adapters/product-sso-store";
import { withPlatformCommercialAuthority, type PlatformCommercialActor } from "@/server/modules/identity_access/public/platform-commercial-authority";
import { legacyGrantMigration } from "@/server/modules/commercial/adapters/legacy-grant-migration";
import { companyPackageLifecycle } from "@/server/modules/commercial/adapters/company-package-lifecycle";
import {withOwnerPackageObligations} from "@/server/modules/commercial/public/owner-package-obligations";
import { packageGrantRevision, readCompanyPackageGrants } from "@/server/modules/commercial/public/package-grants";
import type { PackageGrantBatch } from "@/server/modules/commercial/application/package-grant-batches";
import { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import { lockLegacyModuleAccess, type LegacyModuleAccessLease } from "@/server/modules/identity_access/public/legacy-module-access";
const control = env.CONTROL_DB, holding = env.H001_DB;
vi.mock('@/server/modules/runtime/adapters/runtime',()=>({runtimeEnv:async()=>env,holdingResources:async()=>({write:holding,files:env.H001_FILES})}));
const origins = { account: "https://account-dev-erpx.jesadakorn.com", admin: "https://admin-dev-erpx.jesadakorn.com" };
const secret = "fictional-commercial-review-secret-for-native-tests";
beforeAll(async () => { await applyMigrations(control, env.CONTROL_TEST_MIGRATIONS); await applyMigrations(holding, env.STAFF_TEST_MIGRATIONS); });
async function provider(role = "platform_admin", fresh = true, database:PostgresDatabase = control) {
  const userId = randomUUID(), parentId = randomUUID(), assignmentId = randomUUID(), instant = Date.now();
  const createdAt = new Date(instant - 60_000), expiresAt = new Date(instant + 3_600_000);
  await database.prepare('INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,1,?,?)').bind(userId, "Fictional provider", `${userId}@example.test`, instant, instant).run();
  await database.prepare('INSERT INTO session(id,token,"userId","createdAt","updatedAt","expiresAt") VALUES(?,?,?,?,?,?)').bind(parentId, randomUUID(), userId, createdAt.toISOString(), createdAt.toISOString(), expiresAt.toISOString()).run();
  const sso = productSsoStore(database, "development", origins);
  await sso.registerCentral({ id: parentId, createdAt, expiresAt });
  const browser = await sso.start("admin"), grant = await sso.authorize({ transactionId: browser.id, state: browser.state, challenge: browser.challenge, parentSessionId: parentId });
  const child = await sso.consume({ audience: "admin", code: grant.code, state: browser.state, browserCookie: browser.browserCookie });
  await database.prepare("INSERT INTO platform_assignments(id,user_id,role,modules_json,valid_from,valid_until) VALUES(?,?,?,'[]',?,?)").bind(assignmentId, userId, role, instant - 60_000, instant + 3_600_000).run();
  const policy = await database.prepare("SELECT published_revision FROM iam_roles WHERE company_id='*' AND id=?").bind(role).first<{ published_revision: number }>();
  if (fresh) {
    await database.prepare("INSERT INTO iam_mfa(user_id,secret,enabled) VALUES(?,'fictional-fixture-only',true)").bind(userId).run();
    await database.prepare("INSERT INTO iam_mfa_sessions(session_id,user_id,verified_at) VALUES(?,?,?)").bind(child.sessionId, userId, instant).run();
  }
  const actor: PlatformCommercialActor = { userId, sessionId: child.sessionId, assignmentId, role, roleRevision: Number(policy!.published_revision) };
  return { actor, parentId, sso };
}
async function fixture() { return seedExpenseCompany(holding, env.H001_FILES); }
function run<T>(actor: PlatformCommercialActor, manage: boolean, action: Parameters<typeof withPlatformCommercialAuthority<T>>[4], now = Date.now) {
  return withPlatformCommercialAuthority(control, actor, manage ? "platform.commercial.manage" : "platform.commercial.read", manage, action, now);
}
async function lifecycleRequest(companyId: string, packageId: string, state: "active" | "draining" | "inactive" = "active"): Promise<PackageGrantBatch> {
  const before = await readCompanyPackageGrants(holding, companyId), current = before.find(grant => grant.packageId === packageId);
  return { companyId, expectedRevision: packageGrantRevision(companyId, before), idempotencyKey: randomUUID(), changes: [{ expectedVersion: current?.version ?? 0,
    grant: { companyId, packageId, packageVersion: 1, state, startsAt: 0, endsAt: null, version: (current?.version ?? 0) + 1,
      reason: "Reviewed fictional Company package change", evidence: "test:provider-lifecycle" } }] };
}
it("reviews and applies ordinary packages through real Platform authority without changing Company roles", async () => {
  const p = await provider(), f = await fixture(), input = await lifecycleRequest(f.company, "finance");
  const before = (await holding.prepare("SELECT id,version,role,modules_json FROM assignments WHERE company_id=? ORDER BY id").bind(f.company).all()).results;
  const preview = await run(p.actor, false, lease => companyPackageLifecycle(holding, "h001", lease, secret).preview(input));
  expect(preview.comparison.before).toEqual([]);
  expect(await holding.prepare("SELECT 1 FROM company_package_grants WHERE company_id=?").bind(f.company).first()).toBeNull();
  expect(await holding.prepare("SELECT 1 FROM company_package_revisions WHERE company_id=?").bind(f.company).first()).toBeNull();
  const result = await run(p.actor, true, lease => companyPackageLifecycle(holding, "h001", lease, secret).apply(input, preview.review.token));
  expect(result.after).toMatchObject([{ companyId: f.company, packageId: "finance", state: "active", version: 1 }]);
  expect(await holding.prepare("SELECT comparison_json->'source'->>'kind' kind FROM company_package_review_evidence WHERE change_set_id=?").bind(input.idempotencyKey).first("kind")).toBe("package-lifecycle");
  expect((await holding.prepare("SELECT id,version,role,modules_json FROM assignments WHERE company_id=? ORDER BY id").bind(f.company).all()).results).toEqual(before);
});
it("does not let a read lease apply ordinary or legacy packages even for a manager without fresh MFA", async () => {
  const p = await provider("platform_admin", false), f = await fixture(), input = await lifecycleRequest(f.company, "finance");
  const ordinary = await run(p.actor, false, lease => companyPackageLifecycle(holding, "h001", lease, secret).preview(input));
  await expect(run(p.actor, false, lease => companyPackageLifecycle(holding, "h001", lease, secret).apply(input, ordinary.review.token))).rejects.toMatchObject({ code: "PLATFORM_COMMERCIAL_REQUIRED" });
  const legacy = await run(p.actor, false, lease => legacyGrantMigration(holding, "h001", lease, secret).preview(f.company, "Preserve fictional Company access", "test:read-only-lease"));
  await expect(run(p.actor, false, lease => legacyGrantMigration(holding, "h001", lease, secret).apply(legacy.request!, legacy.review!.token))).rejects.toMatchObject({ code: "PLATFORM_COMMERCIAL_REQUIRED" });
  expect(await holding.prepare("SELECT 1 FROM company_package_grants WHERE company_id=?").bind(f.company).first()).toBeNull();
});
it("separates ordinary package review tokens from legacy migration and recovers an expired committed review", async () => {
  const p = await provider(), f = await fixture(), input = await lifecycleRequest(f.company, "finance");
  let instant = Date.now(); const clock = () => instant;
  const preview = await run(p.actor, false, lease => companyPackageLifecycle(holding, "h001", lease, secret, clock).preview(input));
  await expect(run(p.actor, true, lease => legacyGrantMigration(holding, "h001", lease, secret, clock).apply(input, preview.review.token))).rejects.toMatchObject({ code: "PACKAGE_REVIEW_INVALID" });
  const applied = await run(p.actor, true, lease => companyPackageLifecycle(holding, "h001", lease, secret, clock).apply(input, preview.review.token));
  instant += 300_001;
  expect(await run(p.actor, true, lease => companyPackageLifecycle(holding, "h001", lease, secret, clock).apply(input, preview.review.token))).toEqual(applied);
  expect(Number(await holding.prepare("SELECT count(*) total FROM company_package_change_sets WHERE company_id=?").bind(f.company).first("total"))).toBe(1);
  expect(Number(await holding.prepare("SELECT count(*) total FROM company_package_review_evidence WHERE company_id=?").bind(f.company).first("total"))).toBe(1);
});
it("blocks actual Finance obligations and unregistered Work versions while completing concrete empty Work and Support lifecycles", async () => {
  const p = await provider(), f = await fixture();
  const change = async (packageId: string, state: "active" | "draining" | "inactive") => {
    const input = await lifecycleRequest(f.company, packageId, state);
    const preview = await run(p.actor, false, lease => companyPackageLifecycle(holding, "h001", lease, secret).preview(input));
    return run(p.actor, true, lease => companyPackageLifecycle(holding, "h001", lease, secret).apply(input, preview.review.token));
  };
  await change("finance", "active"); await change("finance", "draining");
  await expect(change("finance", "inactive")).rejects.toThrow("PACKAGE_OBLIGATIONS_REMAIN");
  expect((await readCompanyPackageGrants(holding, f.company)).find(grant => grant.packageId === "finance")).toMatchObject({ state: "draining", version: 2 });
  await change("work", "active"); await change("work", "draining");
  expect(()=>withOwnerPackageObligations(holding,{companyId:f.company,packageId:'work',packageVersion:2},async()=>{throw Error('UNREGISTERED_WORK_OWNER_MUST_NOT_RUN');})).toThrow('PACKAGE_OBLIGATION_SOURCES_REQUIRED');
  await change("work", "inactive");
  expect((await readCompanyPackageGrants(holding,f.company)).find(grant=>grant.packageId==='work')).toMatchObject({state:'inactive',version:3});
  await change("support", "active"); await change("support", "draining"); await change("support", "inactive");
  expect((await readCompanyPackageGrants(holding, f.company)).find(grant => grant.packageId === "support")).toMatchObject({ state: "inactive", version: 3 });
});
it("reviews a combined POS and Support withdrawal with all concrete owner sources under real Platform authority", async () => {
  const p=await provider(),f=await fixture();
  const batch=async(state:"active"|"draining"|"inactive")=>{
    const pos=await lifecycleRequest(f.company,"pos",state),support=await lifecycleRequest(f.company,"support",state);
    return {...pos,changes:[...pos.changes,...support.changes]};
  };
  const change=async(input:PackageGrantBatch)=>{
    const preview=await run(p.actor,false,lease=>companyPackageLifecycle(holding,"h001",lease,secret).preview(input));
    return run(p.actor,true,lease=>companyPackageLifecycle(holding,"h001",lease,secret).apply(input,preview.review.token));
  };
  await change(await batch("active"));await change(await batch("draining"));
  const inactive=await batch("inactive"),roles=(await holding.prepare("SELECT id,version,role,modules_json FROM assignments WHERE company_id=? ORDER BY id").bind(f.company).all()).results;
  const review=await run(p.actor,false,lease=>companyPackageLifecycle(holding,"h001",lease,secret).preview(inactive));
  expect(review.comparison.after.every(grant=>grant.state==='inactive')).toBe(true);
  expect((await readCompanyPackageGrants(holding,f.company)).every(grant=>grant.state==='draining')).toBe(true);
  const result=await run(p.actor,true,lease=>companyPackageLifecycle(holding,"h001",lease,secret).apply(inactive,review.review.token));
  expect(result.after).toMatchObject([{packageId:'pos',state:'inactive',version:3},{packageId:'support',state:'inactive',version:3}]);
  expect((await holding.prepare("SELECT id,version,role,modules_json FROM assignments WHERE company_id=? ORDER BY id").bind(f.company).all()).results).toEqual(roles);
  expect(await run(p.actor,true,lease=>companyPackageLifecycle(holding,"h001",lease,secret).apply(inactive,review.review.token))).toEqual(result);
});
it("reviews Hotel, POS and Support withdrawal together under real Platform authority without changing Company roles", async () => {
  const p=await provider(),f=await fixture();
  const batch=async(state:"active"|"draining"|"inactive")=>{
    const requests=await Promise.all(["pms","pos","support"].map(packageId=>lifecycleRequest(f.company,packageId,state)));
    return {...requests[0],changes:requests.flatMap(request=>request.changes)};
  };
  const change=async(input:PackageGrantBatch)=>{
    const preview=await run(p.actor,false,lease=>companyPackageLifecycle(holding,"h001",lease,secret).preview(input));
    return run(p.actor,true,lease=>companyPackageLifecycle(holding,"h001",lease,secret).apply(input,preview.review.token));
  };
  const roles=()=>holding.prepare("SELECT id,version,role,modules_json FROM assignments WHERE company_id=? ORDER BY id").bind(f.company).all(),before=(await roles()).results;
  await change(await batch("active"));await change(await batch("draining"));const inactive=await batch("inactive");
  const preview=await run(p.actor,false,lease=>companyPackageLifecycle(holding,"h001",lease,secret).preview(inactive));
  expect((await readCompanyPackageGrants(holding,f.company)).map(grant=>grant.state)).toEqual(["draining","draining","draining"]);
  const result=await run(p.actor,true,lease=>companyPackageLifecycle(holding,"h001",lease,secret).apply(inactive,preview.review.token));
  expect(result.after.map(grant=>({packageId:grant.packageId,state:grant.state,version:grant.version}))).toEqual([{packageId:"pms",state:"inactive",version:3},{packageId:"pos",state:"inactive",version:3},{packageId:"support",state:"inactive",version:3}]);
  expect((await roles()).results).toEqual(before);expect(await run(p.actor,true,lease=>companyPackageLifecycle(holding,"h001",lease,secret).apply(inactive,preview.review.token))).toEqual(result);
});
it("provides a real Platform-only preview with no Company role or package mutation", async () => {
  const p = await provider(), f = await fixture();
  const before = (await holding.prepare("SELECT id,version,role,modules_json FROM assignments WHERE company_id=? ORDER BY id").bind(f.company).all()).results;
  const preview = await run(p.actor, false, lease => legacyGrantMigration(holding, "h001", lease, secret).preview(f.company, "Preserve fictional Company access", "test:legacy-access"));
  expect(preview.request?.changes.length).toBeGreaterThan(1); expect(preview.comparison.roles.every(role => role.missingPurchasedCoverage.length === 0)).toBe(true);
  expect(await holding.prepare("SELECT 1 FROM company_package_grants WHERE company_id=?").bind(f.company).first()).toBeNull();
  expect(await holding.prepare("SELECT 1 FROM company_package_revisions WHERE company_id=?").bind(f.company).first()).toBeNull();
  expect((await holding.prepare("SELECT id,version,role,modules_json FROM assignments WHERE company_id=? ORDER BY id").bind(f.company).all()).results).toEqual(before);
});
it("previews on the existing owner transaction with a single pool connection and no revision/audit writes", async () => {
  const p = await provider(), f = await fixture();
  const single = new PostgresDatabase({ ...holding.pool.options, max: 1, connectionTimeoutMillis: 100 });
  try {
    const result = await run(p.actor, false, lease => legacyGrantMigration(single, "h001", lease, secret).preview(f.company, "Preserve fictional Company access", "test:single-connection"));
    expect(result.request?.changes.length).toBeGreaterThan(0);
    expect(await single.prepare("SELECT 1 FROM company_package_revisions WHERE company_id=?").bind(f.company).first()).toBeNull();
    expect(await single.prepare("SELECT 1 FROM company_package_change_sets WHERE company_id=?").bind(f.company).first()).toBeNull();
  } finally { await single.close(); }
});
it("atomically applies the reviewed batch through Commercial and preserves immutable review provenance on replay", async () => {
  const p = await provider(), f = await fixture();
  const preview = await run(p.actor, false, lease => legacyGrantMigration(holding, "h001", lease, secret).preview(f.company, "Preserve fictional Company access", "test:reviewed"));
  const apply = () => run(p.actor, true, lease => legacyGrantMigration(holding, "h001", lease, secret).apply(preview.request!, preview.review!.token));
  const result = await apply(); expect(result.after.length).toBe(preview.request!.changes.length);
  const audit = await holding.prepare("SELECT actor_id,assignment_id,role_revision,authority_revision FROM company_package_review_evidence WHERE change_set_id=?").bind(preview.request!.idempotencyKey).first();
  expect(audit).toMatchObject({ actor_id: p.actor.userId, assignment_id: p.actor.assignmentId, role_revision: p.actor.roleRevision, authority_revision: preview.comparison.authorityRevision });
  expect(await apply()).toEqual(result);
  expect((await holding.prepare("SELECT count(*) total FROM company_package_grant_events WHERE company_id=?").bind(f.company).first<{ total: number }>())!.total).toBe(result.changes.length);
  await expect(holding.prepare("DELETE FROM company_package_review_evidence WHERE change_set_id=?").bind(preview.request!.idempotencyKey).run()).rejects.toThrow("COMMERCIAL_PACKAGE_AUDIT_IMMUTABLE");
  await control.prepare("UPDATE platform_assignments SET revoked_at=?,version=version+1 WHERE id=?").bind(Date.now(), p.actor.assignmentId).run();
  await expect(apply()).rejects.toThrow("Platform assignment is unavailable");
});
it("rejects a stale legacy role review before any grant or evidence write", async () => {
  const p = await provider(), f = await fixture(), member = await f.member("fictional-finance", "finance_preparer");
  const preview = await run(p.actor, false, lease => legacyGrantMigration(holding, "h001", lease, secret).preview(f.company, "Preserve fictional Company access", "test:staleness"));
  await holding.prepare("UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?").bind(Date.now(), member.assignmentId).run();
  await expect(run(p.actor, true, lease => legacyGrantMigration(holding, "h001", lease, secret).apply(preview.request!, preview.review!.token))).rejects.toThrow("Legacy access changed");
  expect(await holding.prepare("SELECT 1 FROM company_package_grants WHERE company_id=?").bind(f.company).first()).toBeNull();
  expect(await holding.prepare("SELECT 1 FROM company_package_review_evidence WHERE change_set_id=?").bind(preview.request!.idempotencyKey).first()).toBeNull();
});
it("binds historical-label evidence to the current published owner policy before Apply", async () => {
  const p = await provider(), f = await fixture(), role = "legacy_policy_" + randomUUID().replaceAll("-", "");
  const permissions = ["commerce.read", "commerce.sales", "commerce.purchase", "inventory.read", "module.sales.read", "module.procurement.read", "module.inventory.read", "iam.read"];
  await holding.prepare("INSERT INTO iam_roles VALUES(?,?,'Historical group fixture',false,1,'active')").bind(f.company, role).run();
  await holding.prepare("INSERT INTO iam_role_revisions VALUES(?,?,1,?::jsonb,'[]','[\"holding\"]','fixture',1)").bind(f.company, role, JSON.stringify(permissions)).run();
  const member = await f.member("historical-owner", role);
  await holding.prepare("UPDATE assignments SET modules_json='[\"commerce\",\"iam\"]' WHERE id=?").bind(member.assignmentId).run();
  const preview = await run(p.actor, false, lease => legacyGrantMigration(holding, "h001", lease, secret).preview(f.company, "Review actual historical owner access", "test:published-compatibility"));
  const reviewed = preview.comparison.roles.find(row => row.assignmentId === member.assignmentId)!;
  expect(reviewed.modulesBefore).toEqual(["commerce", "iam"]); expect(reviewed.modulesAfter).toEqual(reviewed.modulesBefore);
  expect(reviewed.compatibilityEvidence.find(proof => proof.label === "commerce")?.modules).toEqual(["sales", "procurement", "inventory"]);
  await holding.prepare("INSERT INTO iam_role_revisions VALUES(?,?,2,?::jsonb,'[]','[\"holding\"]','fixture',2)").bind(f.company, role, JSON.stringify(permissions.filter(permission => permission !== "commerce.purchase"))).run();
  await holding.prepare("UPDATE iam_roles SET published_revision=2 WHERE company_id=? AND id=?").bind(f.company, role).run();
  await expect(run(p.actor, true, lease => legacyGrantMigration(holding, "h001", lease, secret).apply(preview.request!, preview.review!.token))).rejects.toThrow("Legacy access changed");
  expect(await holding.prepare("SELECT 1 FROM company_package_grants WHERE company_id=?").bind(f.company).first()).toBeNull();
  expect(await holding.prepare("SELECT 1 FROM company_package_review_evidence WHERE change_set_id=?").bind(preview.request!.idempotencyKey).first()).toBeNull();
});
it("requires fresh MFA before entering the Holding mutation, and read-only Platform authority cannot apply", async () => {
  const stale = await provider("platform_admin", false), auditor = await provider("platform_auditor");
  let entered = false;
  for (const actor of [stale.actor, auditor.actor]) await expect(run(actor, true, async () => { entered = true; })).rejects.toThrow();
  expect(entered).toBe(false);
  expect(await run(auditor.actor, false, async lease => lease.canManage)).toBe(false);
});
it("rechecks published custom Platform policy and permitted scope", async () => {
  const role = "platform_fixture_" + randomUUID().replaceAll("-", "");
  await control.prepare("INSERT INTO iam_roles VALUES('platform',?,'Fictional Commercial operator',false,1,'active')").bind(role).run();
  await control.prepare("INSERT INTO iam_role_revisions VALUES('platform',?,1,'[\"platform.commercial.read\",\"platform.commercial.manage\"]','[]','[\"platform\"]','fixture',1)").bind(role).run();
  const p = await provider();
  await control.prepare("UPDATE platform_assignments SET role=?,version=version+1 WHERE id=?").bind(role, p.actor.assignmentId).run();
  p.actor.role = role; p.actor.roleRevision = 1;
  expect(await run(p.actor, true, async () => "permitted")).toBe("permitted");
  await control.prepare("INSERT INTO iam_role_revisions VALUES('platform',?,2,'[\"platform.commercial.read\",\"platform.commercial.manage\"]','[]','[\"holding\"]','fixture',2)").bind(role).run();
  await control.prepare("UPDATE iam_roles SET published_revision=2 WHERE company_id='platform' AND id=?").bind(role).run();
  p.actor.roleRevision = 2;
  await expect(run(p.actor, true, async () => "must not run")).rejects.toThrow("not published for Platform");
});
it("rejects legacy cookies, another audience, expired families, parent tombstones and suspended identities", async () => {
  for (const reason of ["legacy", "audience", "expired", "tombstone", "suspended"] as const) {
    const p = await provider();
    if (reason === "legacy") await control.prepare("DELETE FROM sso_session_bindings WHERE session_id=?").bind(p.actor.sessionId).run();
    if (reason === "audience") await control.prepare("UPDATE sso_session_bindings SET audience='finance' WHERE session_id=?").bind(p.actor.sessionId).run();
    if (reason === "expired") await control.prepare('UPDATE session SET "expiresAt"=? WHERE id=?').bind(new Date(Date.now() - 1).toISOString(), p.parentId).run();
    if (reason === "tombstone") await control.prepare("INSERT INTO iam_revoked_sessions VALUES(?,'platform',?,?)").bind(p.parentId, p.actor.userId, Date.now()).run();
    if (reason === "suspended") await control.prepare("INSERT INTO iam_members(company_id,user_id,principal_id,status) VALUES('platform',?,?,'suspended')").bind(p.actor.userId, p.actor.userId).run();
    let entered = false;
    await expect(run(p.actor, true, async () => { entered = true; })).rejects.toThrow(); expect(entered).toBe(false);
  }
});
it("serializes parent deletion and tombstone insertion behind a live provider command", async () => {
  for (const reason of ["delete", "tombstone"] as const) {
    const p = await provider(); let ready!: () => void, resume!: () => void;
    const acquired = new Promise<void>(resolve => { ready = resolve; }), release = new Promise<void>(resolve => { resume = resolve; });
    const command = run(p.actor, true, async lease => { ready(); await release; await lease.assertCurrent(); return "committed-under-authority"; });
    await acquired;
    const client = await control.pool.connect();
    try {
      await client.query("BEGIN"); await client.query("SET LOCAL lock_timeout='50ms'");
      await expect(reason === "delete" ? client.query("DELETE FROM session WHERE id=$1", [p.parentId]) :
        client.query("INSERT INTO iam_revoked_sessions VALUES($1,'platform',$2,$3)", [p.actor.sessionId, p.actor.userId, Date.now()])).rejects.toMatchObject({ code: "55P03" });
      await client.query("ROLLBACK");
    } finally { client.release(); resume(); }
    expect(await command).toBe("committed-under-authority");
  }
});
it("rechecks actual expiry inside the command and rolls back Holding changes", async () => {
  const p = await provider(), f = await fixture(), instant = Date.now(); let clock = instant;
  await expect(run(p.actor, true, lease => holding.transaction(async tx => {
    await tx.prepare("INSERT INTO company_package_revisions(company_id,version) VALUES(?,0)").bind(f.company).run();
    clock += 3_600_001; await lease.assertCurrent();
  }), () => clock)).rejects.toThrow("Sign in again");
  expect(await holding.prepare("SELECT 1 FROM company_package_revisions WHERE company_id=?").bind(f.company).first()).toBeNull();
});

type TemporalBasis = "assignment" | "directory" | "external" | "cutoff" | "policy" | "provisioning" | "appointment";
async function temporalFixture(kind: TemporalBasis) {
  const f = await fixture(), member = await f.member("temporal-member", kind === "appointment" ? "hr_admin" : "finance_preparer"), id = randomUUID();
  const person = `person-${member.userId}`, engagement = `eng-${member.userId}`;
  let arm: (deadline: number) => Promise<unknown>;
  if (kind === "directory") {
    await holding.prepare("INSERT INTO iam_connections(id,company_id,name,provider,config,status,created_by,created_at) VALUES(?,?,'Fictional directory','active_directory','{}','active','fixture',1)").bind(id, f.company).run();
    await holding.prepare("INSERT INTO iam_identities(id,company_id,connection_id,external_id,username,name,enabled,observed_at,user_id) VALUES(?,?,?,?,?,'Fictional member',true,?,?)")
      .bind(id, f.company, id, id, id, Date.now(), member.userId).run();
    await holding.prepare("INSERT INTO iam_members(company_id,user_id,principal_id,person_id,login_policy,connection_id,identity_id) VALUES(?,?,?,?,'directory',?,?)")
      .bind(f.company, member.userId, member.userId, person, id, id).run();
    arm = deadline => holding.prepare("UPDATE iam_identities SET observed_at=? WHERE id=?").bind(deadline - 300_000, id).run();
  } else if (kind === "external") {
    await holding.prepare("INSERT INTO iam_access_subjects(id,company_id,kind,name,email,user_id) VALUES(?,?,'external','Fictional member','native@example.test',?)").bind(id, f.company, member.userId).run();
    arm = deadline => holding.prepare("UPDATE iam_access_subjects SET expires_at=? WHERE id=?").bind(deadline, id).run();
  } else if (kind === "cutoff") {
    arm = async deadline => {
      await holding.prepare("INSERT INTO people_lifecycle_cases(id,company_id,person_id,engagement_id,kind,status,version,effective_at,definition,created_by,updated_at) VALUES(?,?,?,?,'offboarding','scheduled',1,?,'{}','fixture',1)")
        .bind(id, f.company, person, engagement, deadline).run();
      await holding.prepare("INSERT INTO people_access_cutoffs(company_id,person_id,case_id,effective_at) VALUES(?,?,?,?)").bind(f.company, person, id, deadline).run();
    };
  } else if (kind === "policy") {
    const role = await holding.prepare("SELECT published_revision FROM iam_roles WHERE company_id='*' AND id='finance_preparer'").first<{ published_revision: number }>();
    await holding.prepare("INSERT INTO um_policies VALUES(?,?,1,?::jsonb)").bind(f.company, id, JSON.stringify({ archived: false })).run();
    await holding.prepare("INSERT INTO um_attachments VALUES(?,?,1,?::jsonb,?,?)").bind(f.company, id,
      JSON.stringify({ archived: false, policyId: id, target: "unit", targetId: f.company, exclusions: [] }), f.owner.userId, f.owner.assignmentId).run();
    arm = async deadline => {
      await holding.prepare("INSERT INTO um_candidates(id,company_id,person_id,user_id,attachment_id,attachment_version,policy_version,grant_index,definition,status,assignment_id,created_at) VALUES(?,?,?,?,?,1,1,0,?::jsonb,'active',?,1)")
        .bind(id, f.company, person, member.userId, id, JSON.stringify({ roleId: "finance_preparer", expectedRoleRevision: Number(role!.published_revision), validUntil: deadline }), member.assignmentId).run();
      await holding.prepare("UPDATE assignments SET source='policy',source_id=? WHERE id=?").bind(id, member.assignmentId).run();
    };
  } else if (kind === "provisioning") {
    await holding.prepare("INSERT INTO people_access_packages VALUES(?,?,1,1,'{}')").bind(f.company, id).run();
    await holding.prepare("INSERT INTO people_package_revisions VALUES(?,?,1,'{}','fixture',1)").bind(f.company, id).run();
    await holding.prepare("INSERT INTO people_provision_operations(id,company_id,definition,evidence,status,created_by,assignment_id,session_id,created_at,updated_at) VALUES(?,?,'{}','{}','completed','fixture',?,'fictional-session',1,1)")
      .bind(id, f.company, f.owner.assignmentId).run();
    await holding.prepare("INSERT INTO iam_reviews(id,company_id,kind,payload,created_by,created_person,created_at,status,reason) VALUES(?,?,'grant','{}','fixture','fictional-person',1,'approved','Fictional fixture')").bind(id, f.company).run();
    arm = async deadline => {
      await holding.prepare(`INSERT INTO people_applied_packages(id,company_id,person_id,engagement_id,package_id,package_revision,definition,source_kind,scope,operation_id,authorizing_assignment_id,valid_from,valid_until,created_at)
        VALUES(?,?,?,?,?,1,'{}','supplemental','{}',?,?,0,?,1)`).bind(id, f.company, person, engagement, id, id, f.owner.assignmentId, deadline).run();
      await holding.prepare("INSERT INTO people_provision_grants VALUES(?,?,?,?::jsonb,?,?)").bind(id, id, id, JSON.stringify({ userId: member.userId }), id, member.assignmentId).run();
      await holding.prepare("UPDATE assignments SET source='provisioning',source_id=? WHERE id=?").bind(id, member.assignmentId).run();
    };
  } else if (kind === "appointment") arm = deadline => holding.prepare(`INSERT INTO authority_seats(company_id,kind,scope_id,version,person_id,effective_at)
    VALUES(?,'hr',?,1,?,?) ON CONFLICT(company_id,kind,scope_id) DO UPDATE SET person_id=excluded.person_id,effective_at=excluded.effective_at,version=authority_seats.version+1`)
    .bind(f.company, f.company, person, deadline).run();
  else arm = deadline => holding.prepare("UPDATE assignments SET valid_until=? WHERE id=?").bind(deadline, member.assignmentId).run();
  return { ...f, member, id, arm };
}

async function delayReceipt(companyId: string, deadline: number) {
  const name = "temporal_" + randomUUID().replaceAll("-", "");
  if (!/^[A-Za-z0-9_-]+$/.test(companyId)) throw new Error("FICTIONAL_COMPANY_IDENTIFIER_REQUIRED");
  await holding.exec(`CREATE SEQUENCE ${name};
    CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
      IF NEW.company_id='${companyId}' THEN
        PERFORM nextval('${name}');
        PERFORM pg_sleep(GREATEST(0,(${deadline}+100-floor(extract(epoch FROM clock_timestamp())*1000))/1000.0));
      END IF; RETURN NEW;
    END $$;
    CREATE TRIGGER ${name} AFTER INSERT ON company_package_change_sets FOR EACH ROW EXECUTE FUNCTION ${name}();`);
  return {
    reached: async () => !!(await holding.prepare(`SELECT is_called FROM ${name}`).first<{ is_called: boolean }>())?.is_called,
    close: () => holding.exec(`DROP TRIGGER ${name} ON company_package_change_sets; DROP FUNCTION ${name}(); DROP SEQUENCE ${name};`),
  };
}
for (const basis of ["assignment", "directory", "external", "cutoff", "policy", "provisioning"] as const) {
  it(`rolls back all reviewed grant writes when ${basis} eligibility expires during the batch receipt`, async () => {
    const p = await provider(), f = await temporalFixture(basis), deadline = Date.now() + 4_000;
    await f.arm(deadline);
    const pause = await delayReceipt(f.company, deadline);
    try {
      const preview = await run(p.actor, false, lease => legacyGrantMigration(holding, "h001", lease, secret).preview(f.company, "Preserve only current native access", "test:temporal-source"));
      expect(preview.comparison.roles.some(role => role.assignmentId === f.member.assignmentId)).toBe(true);
      expect(Date.now()).toBeLessThan(deadline);
      await expect(run(p.actor, true, lease => legacyGrantMigration(holding, "h001", lease, secret).apply(preview.request!, preview.review!.token)))
        .rejects.toMatchObject({ code: "STALE_LEGACY_PREVIEW" });
      // A sequence survives rollback: prove this reached the final receipt after
      // actual normal-service grant/audit writes, rather than failing on setup.
      expect(await pause.reached()).toBe(true);
      for (const table of ["company_package_grants", "company_package_grant_events", "company_package_change_sets", "company_package_revisions", "company_package_review_evidence"])
        expect(await holding.prepare(`SELECT 1 FROM ${table} WHERE company_id=?`).bind(f.company).first()).toBeNull();
    } finally { await pause.close(); }
  });
}
it("rejects an HR appointment becoming effective during the receipt before committing the old module comparison", async () => {
  const p = await provider(), f = await temporalFixture("appointment"), deadline = Date.now() + 4_000;
  await f.arm(deadline);
  const pause = await delayReceipt(f.company, deadline);
  try {
    const preview = await run(p.actor, false, lease => legacyGrantMigration(holding, "h001", lease, secret).preview(f.company, "Preserve the reviewed HR ceiling", "test:future-appointment"));
    expect(preview.comparison.roles.find(role => role.assignmentId === f.member.assignmentId)!.purchaseCoverageModules).not.toContain("people_payroll");
    expect(Date.now()).toBeLessThan(deadline);
    await expect(run(p.actor, true, lease => legacyGrantMigration(holding, "h001", lease, secret).apply(preview.request!, preview.review!.token)))
      .rejects.toMatchObject({ code: "STALE_LEGACY_PREVIEW" });
    expect(await pause.reached()).toBe(true);
    expect(await holding.prepare("SELECT 1 FROM company_package_grants WHERE company_id=?").bind(f.company).first()).toBeNull();
  } finally { await pause.close(); }
});
it("requires normal directory verification instead of silently dropping a stale legacy assignment from Preview", async () => {
  const p = await provider(), f = await temporalFixture("directory");
  await f.arm(Date.now() - 1);
  await expect(run(p.actor, false, lease => legacyGrantMigration(holding, "h001", lease, secret).preview(f.company, "Preserve actual legacy access", "test:stale-directory")))
    .rejects.toMatchObject({ code: "LEGACY_DIRECTORY_REFRESH_REQUIRED" });
  expect(await holding.prepare("SELECT 1 FROM company_package_grants WHERE company_id=?").bind(f.company).first()).toBeNull();
});
it("recovers an already committed receipt under current Platform authority after a legacy witness becomes stale", async () => {
  const p = await provider(), f = await temporalFixture("directory");
  const preview = await run(p.actor, false, lease => legacyGrantMigration(holding, "h001", lease, secret).preview(f.company, "Preserve actual legacy access", "test:receipt-recovery"));
  const apply = () => run(p.actor, true, lease => legacyGrantMigration(holding, "h001", lease, secret).apply(preview.request!, preview.review!.token));
  const result = await apply();
  const events = await holding.prepare("SELECT count(*) total FROM company_package_grant_events WHERE company_id=?").bind(f.company).first();
  await f.arm(Date.now() - 1);
  expect(await apply()).toEqual(result);
  expect(await holding.prepare("SELECT count(*) total FROM company_package_grant_events WHERE company_id=?").bind(f.company).first()).toEqual(events);
});
it("freezes the complete eligibility predicates, including absent future cutoff/provisioning rows", async () => {
  let acquired!: () => void, resume!: () => void;
  const ready = new Promise<void>(resolve => { acquired = resolve; }), release = new Promise<void>(resolve => { resume = resolve; });
  const command = holding.transaction(async tx => { await lockLegacyModuleAccess(tx); acquired(); await release; });
  await ready;
  const client = await holding.pool.connect();
  try {
    for (const table of ["iam_access_subjects", "um_values", "um_candidates", "organization_positions", "people_access_cutoffs", "people_lifecycle_cases", "people_applied_packages", "people_provision_operations", "people_provision_grants"]) {
      await client.query("BEGIN"); await client.query("SET LOCAL lock_timeout='50ms'");
      await expect(client.query(`DELETE FROM ${table} WHERE false`)).rejects.toMatchObject({ code: "55P03" });
      await client.query("ROLLBACK");
    }
  } finally { client.release(); resume(); }
  await command;
});
it("confines both a legacy lease and its lazy source reader to the active owned Holding transaction", async () => {
  const { withLegacyModuleAccessLease } = await import("@/server/modules/identity_access/public/legacy-module-access");
  const f = await fixture();
  let escaped!: LegacyModuleAccessLease;
  let escapedRead!: () => Promise<LegacyModuleAccessLease>;
  const result = await withLegacyModuleAccessLease(holding, f.company, async (_database, readSource) => {
    escapedRead = readSource; escaped = await readSource(); await escaped.assertCurrent(); return escaped.facts;
  });
  expect(result.companyId).toBe(f.company);
  await expect(escaped.assertCurrent()).rejects.toThrow("TRANSACTION_NOT_ACTIVE_OR_OWNED");
  await expect(escapedRead()).rejects.toThrow("TRANSACTION_NOT_ACTIVE_OR_OWNED");
});
it('uses the actual selected Admin Provider role before applying a previously reviewed grant from a still-assigned captured administrator',async()=>{
 const p=await provider(),f=await fixture(),auditorId=randomUUID(),{providerContextStore}=await import('@/server/modules/identity_access/adapters/provider-context-store'),{providerContextSelection}=await import('@/lib/identity_access/provider-context');await control.prepare("INSERT INTO platform_assignments(id,user_id,role,modules_json,valid_from,valid_until) VALUES(?,?,'platform_auditor','[]',0,9999999999999)").bind(auditorId,p.actor.userId).run();const store=providerContextStore(control,{userId:p.actor.userId,sessionId:p.actor.sessionId,selectedCookie:p.actor.assignmentId}),before=await store.read(),input=await lifecycleRequest(f.company,'finance'),preview=await run(p.actor,false,lease=>companyPackageLifecycle(holding,'h001',lease,secret).preview(input));await store.select(providerContextSelection(before,auditorId),randomUUID());
 await expect(run(p.actor,true,lease=>companyPackageLifecycle(holding,'h001',lease,secret).apply(input,preview.review.token))).rejects.toMatchObject({code:'PROVIDER_CONTEXT_CHANGED'});expect(await holding.prepare('SELECT count(*) n FROM company_package_grants WHERE company_id=?').bind(f.company).first('n')).toBe(0);expect(await holding.prepare('SELECT count(*) n FROM company_package_change_sets WHERE id=?').bind(input.idempotencyKey).first('n')).toBe(0);expect(await holding.prepare('SELECT count(*) n FROM company_package_review_evidence WHERE change_set_id=?').bind(input.idempotencyKey).first('n')).toBe(0);expect(await control.prepare('SELECT revoked_at,version FROM platform_assignments WHERE id=?').bind(p.actor.assignmentId).first()).toEqual({revoked_at:null,version:1});
});
it('holds the actual current Provider selection through normal reviewed Holding Apply and serializes a concurrent own-role switch without retries',async()=>{
 const p=await provider(),f=await fixture(),auditorId=randomUUID(),{providerContextStore}=await import('@/server/modules/identity_access/adapters/provider-context-store'),{providerContextSelection}=await import('@/lib/identity_access/provider-context'),{measureReads}=await import('@/server/modules/runtime/public/runtime/read-metrics');await control.prepare("INSERT INTO platform_assignments(id,user_id,role,modules_json,valid_from,valid_until) VALUES(?,?,'platform_auditor','[]',0,9999999999999)").bind(auditorId,p.actor.userId).run();const store=providerContextStore(control,{userId:p.actor.userId,sessionId:p.actor.sessionId,selectedCookie:p.actor.assignmentId}),seed=await store.read(),selected=await store.select(providerContextSelection(seed,p.actor.assignmentId),randomUUID()),input=await lifecycleRequest(f.company,'finance'),preview=await run(p.actor,false,lease=>companyPackageLifecycle(holding,'h001',lease,secret).preview(input));
 let entered:()=>void=()=>{},release:()=>void=()=>{};const ready=new Promise<void>(resolve=>{entered=resolve;}),finish=new Promise<void>(resolve=>{release=resolve;});
 const observed=await measureReads(async()=>{const applying=run(p.actor,true,async lease=>{entered();await finish;return companyPackageLifecycle(holding,'h001',lease,secret).apply(input,preview.review.token);});await ready;let completed=false;const switching=store.select(providerContextSelection(selected,auditorId),randomUUID()).finally(()=>{completed=true;});
  try{let blocked=false;for(let attempt=0;attempt<100;attempt++){blocked=!!await control.prepare("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND query='LOCK TABLE session IN EXCLUSIVE MODE' AND cardinality(pg_blocking_pids(pid))>0) blocked").first('blocked');if(blocked||completed)break;await new Promise(resolve=>setTimeout(resolve,10));}expect(blocked).toBe(true);expect(completed).toBe(false);release();return {applied:await applying,switched:await switching};}finally{release();await applying.catch(()=>{});await switching.catch(()=>{});}
 });expect(observed.result.applied.after).toMatchObject([{packageId:'finance',state:'active',version:1}]);expect(observed.result.switched).toMatchObject({selectedId:auditorId,revision:2});expect(observed.metrics.deadlockFailures??0).toBe(0);expect(observed.metrics.serializationFailures??0).toBe(0);expect(observed.metrics.transactionRetries??0).toBe(0);expect(await holding.prepare('SELECT count(*) n FROM company_package_change_sets WHERE id=?').bind(input.idempotencyKey).first('n')).toBe(1);await expect(run(p.actor,true,lease=>companyPackageLifecycle(holding,'h001',lease,secret).apply(input,preview.review.token))).rejects.toMatchObject({code:'PROVIDER_CONTEXT_CHANGED'});
});
it('retains the original complete Platform Commercial authority on populated pre0036 Control without swallowing other schema or permission failures',async()=>{
 const {mkdtemp,mkdir,readdir,cp,rm}=await import('node:fs/promises'),{join,resolve}=await import('node:path'),{migrateDatabase}=await import('@/server/modules/runtime/public/runtime/migrations'),root=await mkdtemp(resolve('.runtime/tests/commercial-before-provider-')),source=resolve('migrations/postgres/control'),database=env.CONTROL_READ_DB;
 try{await mkdir(join(root,'control'),{recursive:true});for(const file of(await readdir(source)).filter(name=>name.endsWith('.sql')&&Number(name.slice(0,4))<=35))await cp(join(source,file),join(root,'control',file));await migrateDatabase(database,'control',root);const p=await provider('platform_admin',true,database);expect(await database.prepare("SELECT to_regclass('admin_provider_contexts') relation").first('relation')).toBeNull();expect(await withPlatformCommercialAuthority(database,p.actor,'platform.commercial.read',false,async lease=>{expect(lease.canManage).toBe(true);return lease.assertCurrent();})).toEqual({userId:p.actor.userId});expect(await database.prepare('SELECT count(*) n FROM account_workspace_preferences').first('n')).toBe(0);
 }finally{await rm(root,{recursive:true,force:true});}
});

it('roundtrips actual ordinary and legacy Commercial receipts through the captured Provider UI acknowledgement without losing newer drafts',async()=>{
 const {providerPackageReview,providerLegacyPackageReview,captureProviderPackageCommand,captureProviderLegacyPackageCommand,acknowledgeProviderPackageCommand}=await import('@/server/modules/commercial/public/rules/provider-package-command');
 const p=await provider(),f=await fixture(),input=await lifecycleRequest(f.company,'support'),preview=await run(p.actor,false,lease=>companyPackageLifecycle(holding,'h001',lease,secret).preview(input)),review=providerPackageReview(preview,input),capture=captureProviderPackageCommand(review,'h001','captured-provider-context',3,'original-draft'),body=capture.body;
 const apply=()=>{const stored=JSON.parse(capture.body) as {request:PackageGrantBatch;token:string};return run(p.actor,true,lease=>companyPackageLifecycle(holding,'h001',lease,secret).apply(stored.request,stored.token));};
 const receipt=await apply();expect(capture.body).toBe(body);expect(acknowledgeProviderPackageCommand({receipt},capture,{contextKey:'captured-provider-context',editVersion:4,draftImage:'later-edited-draft'}).clearDraft).toBe(false);expect(acknowledgeProviderPackageCommand({receipt:await apply()},capture,{contextKey:'captured-provider-context',editVersion:3,draftImage:'original-draft'}).clearDraft).toBe(true);expect(()=>acknowledgeProviderPackageCommand({receipt},capture,{contextKey:'new-provider-context',editVersion:3,draftImage:'original-draft'})).toThrow('acknowledgement');
 const legacy=await fixture(),source={companyId:legacy.company,reason:'Preserve actual Company access',evidence:'test:actual-provider-ui-capture'},raw=await run(p.actor,false,lease=>legacyGrantMigration(holding,'h001',lease,secret).preview(source.companyId,source.reason,source.evidence)),parsed=providerLegacyPackageReview(raw,source),captured=captureProviderLegacyPackageCommand(parsed,'h001','captured-provider-context',5,'legacy-draft');
 const stored=JSON.parse(captured.body) as {request:PackageGrantBatch;token:string},commit=()=>run(p.actor,true,lease=>legacyGrantMigration(holding,'h001',lease,secret).apply(stored.request,stored.token)),result=await commit();expect(result.changes.length).toBeGreaterThan(1);expect(acknowledgeProviderPackageCommand({receipt:result},captured,{contextKey:'captured-provider-context',editVersion:6,draftImage:'later-legacy-draft'}).clearDraft).toBe(false);expect(acknowledgeProviderPackageCommand({receipt:await commit()},captured,{contextKey:'captured-provider-context',editVersion:5,draftImage:'legacy-draft'}).clearDraft).toBe(true);
 const unchanged=await run(p.actor,false,lease=>legacyGrantMigration(holding,'h001',lease,secret).preview(source.companyId,source.reason,source.evidence));expect(providerLegacyPackageReview(unchanged,source).request).toBeNull();expect((await holding.prepare('SELECT count(*) total FROM company_package_change_sets WHERE company_id=?').bind(legacy.company).first<{total:number}>())!.total).toBe(1);
});

for(const deferred of [false,true])it(`rechecks the complete legacy source after ${deferred?'deferred':'immediate'} Company role publication at the normal batch receipt`,async()=>{
 const p=await provider(),f=await fixture(),role='custom_legacy_final_'+randomUUID().replaceAll('-',''),name='legacy_final_'+randomUUID().replaceAll('-','');
 await holding.batch([holding.prepare("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES(?,?,'Original source scope',false,1,'active')").bind(f.company,role),holding.prepare("INSERT INTO iam_role_revisions VALUES(?,?,1,'[\"module.finance.read\"]'::jsonb,'[]'::jsonb,'[\"holding\"]'::jsonb,'fixture',1)").bind(f.company,role),holding.prepare("INSERT INTO iam_role_revisions VALUES(?,?,2,'[\"module.pos.read\"]'::jsonb,'[]'::jsonb,'[\"holding\"]'::jsonb,'fixture',2)").bind(f.company,role)]);
 const actor=await f.member('actual-source-role',role),source=await run(p.actor,false,lease=>legacyGrantMigration(holding,'h001',lease,secret).preview(f.company,'Preserve the exact actual Company role scope','test:late-published-legacy-source'));
 expect(source.comparison.roles.find(row=>row.assignmentId===actor.assignmentId)).toMatchObject({roleRevision:1,modulesBefore:['finance'],modulesAfter:['finance']});expect(source.comparison.requiredPackages).toContain('finance');expect(source.comparison.requiredPackages).not.toContain('pos');
 const assignments=(await holding.prepare('SELECT id,role,modules_json,version FROM assignments WHERE company_id=? ORDER BY id').bind(f.company).all()).results;
 await holding.exec(`CREATE SEQUENCE ${name};CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.company_id='${f.company}' THEN PERFORM nextval('${name}');UPDATE iam_roles SET published_revision=2 WHERE company_id='${f.company}' AND id='${role}';END IF;RETURN NEW;END $$;CREATE ${deferred?'CONSTRAINT ':''}TRIGGER ${name} AFTER INSERT ON company_package_change_sets ${deferred?'DEFERRABLE INITIALLY DEFERRED ':''}FOR EACH ROW EXECUTE FUNCTION ${name}()`);
 try{
  await expect(run(p.actor,true,lease=>legacyGrantMigration(holding,'h001',lease,secret).apply(source.request!,source.review!.token))).rejects.toMatchObject({code:'STALE_LEGACY_PREVIEW'});
  expect(await holding.prepare(`SELECT is_called FROM ${name}`).first('is_called')).toBe(true);expect(await holding.prepare('SELECT published_revision FROM iam_roles WHERE company_id=? AND id=?').bind(f.company,role).first('published_revision')).toBe(1);
  expect((await holding.prepare('SELECT id,role,modules_json,version FROM assignments WHERE company_id=? ORDER BY id').bind(f.company).all()).results).toEqual(assignments);
  for(const table of ['company_package_grants','company_package_grant_events','company_package_change_sets','company_package_revisions','company_package_review_evidence'])expect(await holding.prepare(`SELECT 1 FROM ${table} WHERE company_id=?`).bind(f.company).first()).toBeNull();
 }finally{await holding.exec(`DROP TRIGGER ${name} ON company_package_change_sets;DROP FUNCTION ${name}();DROP SEQUENCE ${name}`);}
 const applied=await run(p.actor,true,lease=>legacyGrantMigration(holding,'h001',lease,secret).apply(source.request!,source.review!.token));expect(applied.after.some(grant=>grant.packageId==='finance')).toBe(true);expect(applied.after.some(grant=>grant.packageId==='pos')).toBe(false);expect(await run(p.actor,true,lease=>legacyGrantMigration(holding,'h001',lease,secret).apply(source.request!,source.review!.token))).toEqual(applied);
});

it('preserves actual module-free Company Branch and Department self-service through normal legacy grants, Apps discovery and exact Work source entry',async()=>{
 const {loadIamAuthority}=await import('@/server/modules/identity_access/public/iam-store'),{accountWorkModuleAccess}=await import('@/server/modules/identity_access/public/work-module-access'),{createTimeEntry,timeEntrySchema}=await import('@/server/modules/people_payroll/public/people-payroll'),{createSelfServiceLeave,runLeaveAccruals}=await import('@/server/modules/people_payroll/public/leave-self-service'),{workforceProductSelfServiceEntry}=await import('@/server/modules/people_payroll/public/self-service-product-entry'),{workforceSelfServiceEntry}=await import('@/server/modules/people_payroll/public/rules/self-service-product-entry'),{createProductLauncher}=await import('@/server/modules/work_projection/application/product-launcher'),{createProductWorkspace}=await import('@/server/modules/identity_access/application/product-workspace'),{productWorkspaceTarget}=await import('@/server/modules/identity_access/domain/product-workspace'),{initializeWorkSelection,selectWorkRole}=await import('@/server/modules/work_projection/public/work-role-store'),{readCompanyPurchaseFacts}=await import('@/server/modules/commercial/public/package-grants'),{assertProductionModuleEnabled}=await import('@/server/modules/pms/public/hospitality-rollout'),{buildWorkspaceHoldingChoice}=await import('@/lib/work_projection/workspace-choice'),{workspaceBase}=await import('@/lib/work_projection/workspace');
 type Actor=Awaited<ReturnType<Awaited<ReturnType<typeof fixture>>['member']>>;type Access=import('@/server/modules/identity_access/public/authorization').AccountAccessContext;type Scope=Access['scope'];type Assignment=Access['assignments'][number];type InputScope=import('@/lib/work_projection/workspace').HoldingWorkspaceScope;
 const p=await provider(),f=await fixture(),foreign=await fixture(),role='custom_legacy_self_'+randomUUID().replaceAll('-',''),clock=Date.now(),expiresAt=clock+3600000;
 await holding.batch([holding.prepare("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES(?,?,'Original personal requests only',false,1,'active')").bind(f.company,role),holding.prepare("INSERT INTO iam_role_revisions VALUES(?,?,1,'[\"records.read\",\"self.requests\",\"self.tasks\"]'::jsonb,'[]'::jsonb,'[\"holding\",\"branch\",\"department\"]'::jsonb,'fixture',1)").bind(f.company,role)]);
 const actors:Actor[]=[];for(const kind of ['holding','branch','department'] as const){const actor=await f.member('personal-'+kind,role);if(kind!=='holding')await holding.prepare('UPDATE assignments SET scope_kind=?,scope_node_id=?,version=version+1 WHERE id=?').bind(kind,kind==='branch'?f.branch:f.department,actor.assignmentId).run();const scope:Scope=kind==='holding'?actor.scope:kind==='branch'?{kind,holdingId:'h001',holdingSlug:'test',holdingName:'Test',companyId:f.company,branchId:f.branch,branchSlug:f.branch,branchName:'Branch'}:{kind,holdingId:'h001',holdingSlug:'test',holdingName:'Test',companyId:f.company,branchId:f.branch,branchSlug:f.branch,branchName:'Branch',departmentId:f.department,departmentSlug:f.department,departmentName:'Department'};actors.push({...actor,scope,assignmentScope:kind,branchId:kind==='holding'?null:f.branch,departmentId:kind==='department'?f.department:null,iam:await loadIamAuthority(holding,f.company,actor.userId,role)});}
 const nonEmployee=await f.member('personal-no-employment',role,1,f.department,false);
 const type='legacy-leave-type-'+randomUUID(),policy='legacy-leave-policy-'+randomUUID();await holding.batch([holding.prepare("INSERT INTO leave_types(id,company_id,code,name,tracked,effective_from,created_by,created_at,updated_at) VALUES(?,?,'annual','Original personal leave',true,'2026-01-01','fixture',1,1)").bind(type,f.company),holding.prepare("INSERT INTO leave_policy_versions(id,company_id,policy_key,version,name,status,effective_from,timezone,created_by,created_at) VALUES(?,?,'annual',1,'Original Company self-service policy','active','2026-01-01','Asia/Bangkok','fixture',1)").bind(policy,f.company),holding.prepare("INSERT INTO leave_policy_rules(policy_version_id,leave_type_id,grant_kind,grant_units) VALUES(?,?,'annual',10)").bind(policy,type),...actors.map(actor=>holding.prepare("INSERT INTO leave_policy_assignments(id,company_id,engagement_id,policy_version_id,effective_from,created_by,created_at) VALUES(?,?,?,?,'2026-01-01','fixture',1)").bind('legacy-policy-'+actor.assignmentId,f.company,'eng-'+actor.userId,policy))]);
 const snapshot=async()=>({assignments:(await holding.prepare('SELECT id,user_id,role,modules_json,scope_kind,scope_node_id,version FROM assignments WHERE company_id IN (?,?) ORDER BY id').bind(f.company,foreign.company).all()).results,roles:(await holding.prepare('SELECT id,company_id,published_revision,status FROM iam_roles WHERE company_id IN (?,?) ORDER BY company_id,id').bind(f.company,foreign.company).all()).results,rollouts:(await holding.prepare('SELECT * FROM company_feature_rollouts WHERE company_id IN (?,?) ORDER BY company_id,module_id').bind(f.company,foreign.company).all()).results}),before=await snapshot();
 const records=new Map<string,{key:string;input:ReturnType<typeof timeEntrySchema.parse>;id:string;leave:string}>();vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','disabled');
 try{
  await runLeaveAccruals(holding,'2026-09-17',f.company);
  for(const actor of actors){const key=randomUUID(),input=timeEntrySchema.parse({engagementId:'eng-'+actor.userId,workDate:'2026-09-21',regularHours:8,note:'Original personal Time before software migration'}),time=await createTimeEntry(actor,input,key),leave=await createSelfServiceLeave(actor,{leaveTypeId:type,startDate:'2026-11-02',endDate:'2026-11-02',startSegment:'full',endSegment:'full',handoverNote:'Original saved personal leave',saveOnly:true},randomUUID());records.set(actor.userId,{key,input,id:String(time.id),leave:String(leave.id)});expect(actor.modules).toEqual([]);}
  for(const actor of actors){const version=await holding.prepare('SELECT version FROM assignments WHERE id=?').bind(actor.assignmentId).first<number>('version'),candidate={companyId:f.company,branchId:actor.branchId,departmentId:actor.departmentId,userId:actor.userId,assignmentId:actor.assignmentId,assignmentVersion:version!,role};expect(await workforceProductSelfServiceEntry(holding,candidate,workforceSelfServiceEntry('leave')!)).toBeNull();expect(await workforceProductSelfServiceEntry(holding,candidate,workforceSelfServiceEntry('tasks/people_payroll/'+records.get(actor.userId)!.leave)!)).toBeNull();}
  const preview=await run(p.actor,false,lease=>legacyGrantMigration(holding,'h001',lease,secret).preview(f.company,'Preserve original scoped Company work','test:actual-role-and-self-parity'));for(const actor of actors)expect(preview.comparison.roles.find(row=>row.assignmentId===actor.assignmentId)).toMatchObject({scopeKind:actor.assignmentScope,modulesBefore:[],modulesAfter:[],missingPurchasedCoverage:[]});expect(preview.comparison.requiredPackages).toContain('people-workforce');expect(preview.comparison.roles.some(row=>row.roleId==='holding_owner'&&row.modulesBefore.includes('people_payroll'))).toBe(true);
  await run(p.actor,true,lease=>legacyGrantMigration(holding,'h001',lease,secret).apply(preview.request!,preview.review!.token));expect(await snapshot()).toEqual(before);expect(await holding.prepare('SELECT 1 FROM company_package_grants WHERE company_id=?').bind(foreign.company).first()).toBeNull();vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');
  for(const actor of actors){const prior=records.get(actor.userId)!;expect(await createTimeEntry(actor,prior.input,prior.key)).toMatchObject({id:prior.id,status:'submitted',version:1});}
  await expect((await import('@/server/modules/operations/public/staff-policy')).activeStaff(holding,f.company,nonEmployee.userId)).rejects.toMatchObject({status:409,code:'STAFF_LINK_REQUIRED'});
  expect(await workforceProductSelfServiceEntry(holding,{companyId:f.company,branchId:null,departmentId:null,userId:nonEmployee.userId,assignmentId:nonEmployee.assignmentId,assignmentVersion:1,role},workforceSelfServiceEntry('leave')!)).toBeNull();
  const nodes=(await holding.prepare("SELECT n.id,n.kind,n.slug,n.display_name label,n.company_id,n.branch_id,b.slug branch_slug FROM organization_nodes n LEFT JOIN organization_nodes b ON b.id=n.branch_id AND b.company_id=n.company_id WHERE n.company_id=? AND n.kind IN ('branch','department') AND n.status='active' ORDER BY n.id").bind(f.company).all<{id:string;kind:'branch'|'department';slug:string;label:string;company_id:string;branch_id:string|null;branch_slug:string|null}>()).results.map(row=>({id:row.id,kind:row.kind,slug:row.slug,label:row.label,companyId:row.company_id,branchId:row.branch_id,branchSlug:row.branch_slug}));
  for(const actor of actors){
   const parentId=randomUUID(),createdAt=new Date(clock-1000),expiry=new Date(expiresAt),store=productSsoStore(control,'development',{...origins,apps:'https://apps-dev-erpx.jesadakorn.com',work:'https://work-dev-erpx.jesadakorn.com'});
   await control.prepare('INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,1,?,?)').bind(actor.userId,actor.name,actor.userId+'@example.test',clock,clock).run();
   await control.prepare('INSERT INTO session(id,token,"userId","createdAt","updatedAt","expiresAt") VALUES(?,?,?,?,?,?)').bind(parentId,randomUUID(),actor.userId,createdAt.toISOString(),createdAt.toISOString(),expiry.toISOString()).run();await store.registerCentral({id:parentId,createdAt,expiresAt:expiry});
   const child=async(audience:'apps'|'work')=>{const browser=await store.start(audience),grant=await store.authorize({transactionId:browser.id,state:browser.state,challenge:browser.challenge,parentSessionId:parentId}),session=await store.consume({audience,code:grant.code,state:browser.state,browserCookie:browser.browserCookie});expect((await store.session(session.token,audience))?.user.id).toBe(actor.userId);return session;},appsSession=await child('apps'),workSession=await child('work'),sessionId=workSession.sessionId;
   const assignments=async():Promise<Assignment[]>=>{const row=await holding.prepare('SELECT id,role,company_id,modules_json,scope_kind,scope_node_id,version,valid_until FROM assignments WHERE id=? AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)').bind(actor.assignmentId).first<{id:string;role:string;company_id:string;modules_json:string[]|string;scope_kind:Assignment['scopeKind'];scope_node_id:string|null;version:number;valid_until:number}>();return row?[{id:row.id,role:row.role,companyId:row.company_id,branchId:row.scope_kind==='holding'?null:f.branch,modules:typeof row.modules_json==='string'?JSON.parse(row.modules_json):row.modules_json,scopeKind:row.scope_kind,scopeNodeId:row.scope_node_id,version:Number(row.version),validUntil:Number(row.valid_until)}]:[];},scope=(input:InputScope):Scope=>{const base={holdingId:'h001' as const,holdingSlug:'test',holdingName:'Test',companyId:f.company};if(input.kind==='holding')return {kind:'holding',...base};const branch=nodes.find(node=>node.kind==='branch'&&node.slug===input.branchSlug);if(input.kind==='branch'){if(!branch)throw Error('Actual Branch required');return {kind:'branch',...base,branchId:branch.id,branchSlug:branch.slug,branchName:branch.label};}const department=nodes.find(node=>node.kind==='department'&&node.slug===input.departmentSlug&&node.branchId===(branch?.id??null));if(!department)throw Error('Actual Department required');return {kind:'department',...base,branchId:branch?.id??null,...(branch?{branchSlug:branch.slug,branchName:branch.label}:{}),departmentId:department.id,departmentSlug:department.slug,departmentName:department.label};},access=async(input:InputScope,selectedSessionId=sessionId):Promise<Access>=>{const audience=selectedSessionId===appsSession.sessionId?'apps':'work',session=await store.session(audience==='apps'?appsSession.token:workSession.token,audience);if(!session||session.session.id!==selectedSessionId)throw Error('SESSION_EXPIRED');return {sessionId:session.session.id,expiresAt:session.session.expiresAt.getTime(),userId:session.user.id,name:session.user.name,email:session.user.email,scope:scope(input),assignments:await assignments(),requestHeaders:new Headers()};},purchaseFacts=(a:Access)=>readCompanyPurchaseFacts(holding,a.scope.companyId),ready=async(a:Access,moduleId:string)=>{await assertProductionModuleEnabled(holding,a.scope.companyId,moduleId);return true;};
   const choice=buildWorkspaceHoldingChoice('test','Test', [{companyId:f.company,companyName:'Original Company',scopeKind:actor.assignmentScope as 'holding'|'branch'|'department',scopeNodeId:actor.assignmentScope==='holding'?null:actor.assignmentScope==='branch'?f.branch:f.department}],nodes,true)!;
   const cards=await createProductLauncher({choices:async()=>[choice],access:input=>access(input,appsSession.sessionId),purchaseFacts,moduleAccess:accountWorkModuleAccess,ready,companySlug:async()=>f.company,origin:id=>`https://${id}-dev-erpx.jesadakorn.com`,denied:()=>false,now:Date.now})();expect(cards.length).toBe(actor.assignmentScope==='holding'?4:actor.assignmentScope==='branch'?3:1);expect(cards.every(card=>card.productId==='work'&&card.companyId===f.company)).toBe(true);
   const workspace=createProductWorkspace({access:input=>access(input.scope),modules:accountWorkModuleAccess,purchaseFacts,ready,selection:(a,applicable)=>initializeWorkSelection(holding,{sessionId:a.sessionId,userId:a.userId,companyId:a.scope.companyId,expiresAt:a.expiresAt},a.assignments,applicable),select:(a,option,revision)=>selectWorkRole(holding,{sessionId:a.sessionId,userId:a.userId,companyId:a.scope.companyId,expiresAt:a.expiresAt,assignmentId:option.assignmentId,assignmentVersion:option.assignmentVersion,revision}),workforceSelfServiceEntry:(a,assignment,target)=>workforceProductSelfServiceEntry(holding,{companyId:a.scope.companyId,branchId:'branchId'in a.scope?a.scope.branchId:null,departmentId:a.scope.kind==='department'?a.scope.departmentId:null,userId:a.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:a.sessionId},target),now:Date.now,error:(_status,code)=>Error(code)});
   const card=cards.find(item=>item.scope.kind===actor.scope.kind)!,handoff=new URL(new URL(card.href).searchParams.get('next')!,'https://work-dev-erpx.jesadakorn.com'),landing=productWorkspaceTarget('work',f.company,handoff.searchParams.get('next'));expect(landing).not.toBeNull();const opened=await workspace.read('work',landing!);expect(opened.target).toContain('/home');expect(opened.options.map(option=>option.assignmentId)).toEqual([actor.assignmentId]);const current=opened.selection!;actor.authority={sessionId,revision:current.revision,assignmentVersion:current.assignment_version!};
   const prior=records.get(actor.userId)!;for(const path of ['leave','leave/new','overtime','overtime/new','tasks/people_payroll/'+prior.leave+'/overview']){const target=productWorkspaceTarget('work',f.company,workspaceBase(actor.scope)+'/'+path+'?filter=mine#retained');expect(target).not.toBeNull();const result=await workspace.read('work',target!);expect(result.target).toBe(target!.target);expect(result.sourceRecord).toBe(true);}
   await expect(createTimeEntry(actor,prior.input,prior.key)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});const newKey=randomUUID(),newInput=timeEntrySchema.parse({...prior.input,workDate:'2026-09-22',note:'Original personal Time after software migration'}),created=await createTimeEntry(actor,newInput,newKey);expect(created).toMatchObject({status:'submitted',version:1});expect(await createTimeEntry(actor,newInput,newKey)).toEqual(created);expect(actor.modules).toEqual([]);const actorScope={companyId:f.company,branchId:actor.branchId,departmentId:actor.departmentId,userId:actor.userId,assignmentId:actor.assignmentId,assignmentVersion:current.assignment_version!,role,sessionId};expect(await workforceProductSelfServiceEntry(holding,actorScope,workforceSelfServiceEntry('tasks/people_payroll/'+prior.leave)!)).not.toBeNull();expect(await workforceProductSelfServiceEntry(holding,{...actorScope,assignmentVersion:actorScope.assignmentVersion+1},workforceSelfServiceEntry('leave')!)).toBeNull();expect(await workforceProductSelfServiceEntry(holding,{...actorScope,companyId:foreign.company},workforceSelfServiceEntry('leave')!)).toBeNull();const other=actors.find(item=>item.userId!==actor.userId)!;expect(await workforceProductSelfServiceEntry(holding,actorScope,workforceSelfServiceEntry('tasks/people_payroll/'+records.get(other.userId)!.leave)!)).toBeNull();await store.revokeFamily(parentId);expect(await store.session(appsSession.token,'apps')).toBeNull();expect(await store.session(workSession.token,'work')).toBeNull();await expect(workspace.read('work',landing!)).rejects.toThrow('SESSION_EXPIRED');
  }
  expect(await snapshot()).toEqual(before);expect((await holding.prepare('SELECT count(*)::int count FROM hr_time_entries WHERE company_id=?').bind(f.company).first<number>('count'))).toBe(6);
 }finally{vi.unstubAllEnvs();}
});

it('preserves original module-free current-account Messages through the normal reviewed legacy migration without granting a Company module or role',async()=>{
 const {loadIamAuthority}=await import('@/server/modules/identity_access/public/iam-store'),{createMessagingRoom,sendMessagingMessage,messagingHistory,messageSchema,manageMessaging}=await import('@/server/modules/collaboration/public/messaging');
 const p=await provider(),f=await fixture(),role='custom_legacy_messages_'+randomUUID().replaceAll('-','');
 await holding.batch([holding.prepare("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES(?,?,'Original account conversation only',false,1,'active')").bind(f.company,role),holding.prepare("INSERT INTO iam_role_revisions VALUES(?,?,1,'[\"records.read\"]'::jsonb,'[]'::jsonb,'[\"holding\"]'::jsonb,'fixture',1)").bind(f.company,role)]);
 const first=await f.member('original-account-first',role,1,f.department,false),second=await f.member('original-account-second',role,1,f.department,false);
 for(const actor of [first,second]){actor.iam=await loadIamAuthority(holding,f.company,actor.userId,role);await control.prepare('INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,1,1,1)').bind(actor.userId,actor.name,actor.userId+'@example.test').run();expect(actor.modules).toEqual([]);expect(await holding.prepare('SELECT 1 FROM hr_people WHERE company_id=? AND user_id=?').bind(f.company,actor.userId).first()).toBeNull();}
 const snapshot=async()=>({assignments:(await holding.prepare('SELECT id,user_id,role,modules_json,scope_kind,scope_node_id,version FROM assignments WHERE company_id=? ORDER BY id').bind(f.company).all()).results,roles:(await holding.prepare('SELECT id,published_revision,status FROM iam_roles WHERE company_id=? ORDER BY id').bind(f.company).all()).results,rollouts:(await holding.prepare('SELECT * FROM company_feature_rollouts WHERE company_id=? ORDER BY module_id').bind(f.company).all()).results}),before=await snapshot(),key=randomUUID(),input=messageSchema.parse({body:'Original account-only conversation before software migration'});
 vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','disabled');vi.stubEnv('ERPX_ENVIRONMENT','local');vi.stubEnv('ERPX_PRODUCT_ENVIRONMENT','development');vi.stubEnv('ERPX_PRODUCT_ROUTING','enabled');vi.stubEnv('ERPX_PRODUCT_SSO','enabled');
 try{
  const workOrigin='https://work-dev-erpx.jesadakorn.com',parentId=randomUUID(),createdAt=new Date(Date.now()-1000),expiresAt=new Date(Date.now()+3600000),store=productSsoStore(control,'development',{...origins,work:workOrigin}),{sessionFromRequest}=await import('@/server/modules/identity_access/adapters/auth');
  await control.prepare('INSERT INTO session(id,token,"userId","createdAt","updatedAt","expiresAt") VALUES(?,?,?,?,?,?)').bind(parentId,randomUUID(),first.userId,createdAt.toISOString(),createdAt.toISOString(),expiresAt.toISOString()).run();await store.registerCentral({id:parentId,createdAt,expiresAt});const browser=await store.start('work'),grant=await store.authorize({transactionId:browser.id,state:browser.state,challenge:browser.challenge,parentSessionId:parentId}),child=await store.consume({audience:'work',code:grant.code,state:browser.state,browserCookie:browser.browserCookie}),request=()=>new Request(workOrigin+'/api/v1/holdings/test/messaging/rooms',{headers:{host:new URL(workOrigin).host,cookie:'__Host-erpx-product='+child.token}});expect((await sessionFromRequest(request()))?.user.id).toBe(first.userId);
  const room=await createMessagingRoom(holding,first,{kind:'direct',name:'',members:[second.userId]},randomUUID()),sent=await sendMessagingMessage(holding,first,room.id,input,key);expect((await messagingHistory(holding,second,room.id)).messages.map(row=>row.body)).toEqual([input.body]);
  const preview=await run(p.actor,false,lease=>legacyGrantMigration(holding,'h001',lease,secret).preview(f.company,'Preserve original current-account conversations','test:actual-module-free-message-parity'));
  for(const actor of [first,second])expect(preview.comparison.roles.find(row=>row.assignmentId===actor.assignmentId)).toMatchObject({modulesBefore:[],modulesAfter:[],missingPurchasedCoverage:[]});
  await run(p.actor,true,lease=>legacyGrantMigration(holding,'h001',lease,secret).apply(preview.request!,preview.review!.token));expect(await snapshot()).toEqual(before);vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');
  expect((await messagingHistory(holding,second,room.id)).messages.map(row=>row.body)).toEqual([input.body]);expect(await sendMessagingMessage(holding,first,room.id,input,key)).toEqual(sent);
  const reply=messageSchema.parse({body:'Same original current-account authority after software migration'}),replyKey=randomUUID(),replied=await sendMessagingMessage(holding,second,room.id,reply,replyKey);expect(await sendMessagingMessage(holding,second,room.id,reply,replyKey)).toEqual(replied);expect((await messagingHistory(holding,first,room.id)).messages.map(row=>row.body)).toEqual([input.body,reply.body]);
  expect(await snapshot()).toEqual(before);expect(await holding.prepare('SELECT count(*)::int count FROM messaging_messages WHERE room_id=?').bind(room.id).first<number>('count')).toBe(2);
  const group=await createMessagingRoom(holding,first,{kind:'group',name:'Original owner-managed private group',members:[second.userId]},randomUUID()),groupKey=randomUUID(),groupInput=messageSchema.parse({body:'Original captured member reply'}),groupMessage=await sendMessagingMessage(holding,second,group.id,groupInput,groupKey);
  await manageMessaging(holding,first,group.id,false,{name:'Original owner-managed private group',members:[first.userId],expectedVersion:1},randomUUID());await expect(messagingHistory(holding,second,group.id)).rejects.toMatchObject({status:404,code:'CHAT_NOT_FOUND'});await expect(sendMessagingMessage(holding,second,group.id,groupInput,groupKey)).rejects.toMatchObject({status:404,code:'CHAT_NOT_FOUND'});
  await manageMessaging(holding,first,group.id,false,{name:'Original owner-managed private group',members:[first.userId,second.userId],expectedVersion:2},randomUUID());expect(await sendMessagingMessage(holding,second,group.id,groupInput,groupKey)).toEqual(groupMessage);expect(await snapshot()).toEqual(before);await store.revokeFamily(parentId);expect(await sessionFromRequest(request())).toBeNull();
  await holding.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),second.assignmentId).run();await expect(messagingHistory(holding,second,room.id)).rejects.toMatchObject({status:403,code:'QUEUED_AUTHORITY_CHANGED'});await expect(sendMessagingMessage(holding,second,room.id,reply,replyKey)).rejects.toMatchObject({status:403,code:'QUEUED_AUTHORITY_CHANGED'});expect(await holding.prepare('SELECT count(*)::int count FROM messaging_messages WHERE room_id=?').bind(room.id).first<number>('count')).toBe(2);
 }finally{vi.unstubAllEnvs();}
});

it('rejects an uncommitted saved pre-Messages coverage review under unchanged source and catalogue while retaining normal committed replay',async()=>{
 const {packageReviewSigner}=await import('@/server/modules/commercial/adapters/package-review'),p=await provider(),f=await fixture(),current=await run(p.actor,false,lease=>legacyGrantMigration(holding,'h001',lease,secret).preview(f.company,'Preserve actual current Company coverage','test:coverage-rules-upgrade'));
 expect(current.request!.changes.some(change=>change.grant.packageId==='work-messages')).toBe(true);
 // Reproduce the exact saved pre-fix request shape from the frozen original
 // module-only rule: this Company had no collaboration Module witness. Use
 // the unchanged real signer/source/catalogue protocol, not an invalid MAC.
 const savedRequest:PackageGrantBatch={...current.request!,idempotencyKey:randomUUID(),changes:current.request!.changes.filter(change=>change.grant.packageId!=='work-messages')},signer=packageReviewSigner(secret),claims=signer.read(current.review!.token,p.actor,'h001',current.request!),savedReview=signer.issue(p.actor,'h001',savedRequest,claims.authorityRevision),savedClaims=signer.read(savedReview.token,p.actor,'h001',savedRequest);
 expect(savedClaims.authorityRevision).toBe(claims.authorityRevision);expect(savedClaims.catalogueHash).toBe(claims.catalogueHash);expect(savedRequest.changes.length).toBe(current.request!.changes.length-1);
 const assignments=(await holding.prepare('SELECT id,role,modules_json,version FROM assignments WHERE company_id=? ORDER BY id').bind(f.company).all()).results;
 await expect(run(p.actor,true,lease=>legacyGrantMigration(holding,'h001',lease,secret).apply(savedRequest,savedReview.token))).rejects.toMatchObject({status:409,code:'STALE_LEGACY_PREVIEW'});
 for(const table of ['company_package_grants','company_package_grant_events','company_package_change_sets','company_package_revisions','company_package_review_evidence'])expect(await holding.prepare(`SELECT 1 FROM ${table} WHERE company_id=?`).bind(f.company).first()).toBeNull();expect((await holding.prepare('SELECT id,role,modules_json,version FROM assignments WHERE company_id=? ORDER BY id').bind(f.company).all()).results).toEqual(assignments);
 const applied=await run(p.actor,true,lease=>legacyGrantMigration(holding,'h001',lease,secret).apply(current.request!,current.review!.token));expect(applied.after.some(grant=>grant.packageId==='work-messages')).toBe(true);expect(await run(p.actor,true,lease=>legacyGrantMigration(holding,'h001',lease,secret).apply(current.request!,current.review!.token))).toEqual(applied);
});

it('preserves original module-free active-staff standalone Routines after normal legacy review without granting Operations or Projects authority',async()=>{
 const {loadIamAuthority}=await import('@/server/modules/identity_access/public/iam-store'),{legacyCompanyModuleAccess}=await import('@/server/modules/identity_access/public/legacy-module-access'),{activeStaff}=await import('@/server/modules/operations/public/staff-policy'),{saveTemplate,getTemplate,listTemplates,runTemplate,templateSchema}=await import('@/server/modules/operations/public/routines');
 const p=await provider(),f=await fixture(),role='custom_legacy_routines_'+randomUUID().replaceAll('-','');await holding.batch([holding.prepare("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES(?,?,'Original personal routine creator',false,1,'active')").bind(f.company,role),holding.prepare("INSERT INTO iam_role_revisions VALUES(?,?,1,'[\"records.read\"]'::jsonb,'[]'::jsonb,'[\"holding\"]'::jsonb,'fixture',1)").bind(f.company,role)]);
 const {setDepartmentHead}=await import('@/server/modules/operations/public/staff-settings');await setDepartmentHead(holding,f.owner,f.department,{personId:'person-'+f.head.userId,expectedVersion:0,reason:'Original independently appointed routine reviewer'},randomUUID());
 const actor=await f.member('original-active-routine-creator',role);actor.iam=await loadIamAuthority(holding,f.company,actor.userId,role);expect(actor.modules).toEqual([]);expect((await activeStaff(holding,f.company,actor.userId)).engagement_id).toBe('eng-'+actor.userId);
 const source=await legacyCompanyModuleAccess(holding,f.company);expect(source.beta).toBe(false);expect(source.witnesses.find(row=>row.assignmentId===actor.assignmentId)).toMatchObject({roleRevision:1,modules:[],scopeKind:'holding',routineAccess:{source:'operations/original-routines/v1',activeStaff:true,retainedTemplates:0}});
 const snapshot=async()=>({assignments:(await holding.prepare('SELECT id,user_id,role,modules_json,scope_kind,scope_node_id,version FROM assignments WHERE company_id=? ORDER BY id').bind(f.company).all()).results,roles:(await holding.prepare('SELECT id,published_revision,status FROM iam_roles WHERE company_id=? ORDER BY id').bind(f.company).all()).results,rollouts:(await holding.prepare('SELECT * FROM company_feature_rollouts WHERE company_id=? ORDER BY module_id').bind(f.company).all()).results}),before=await snapshot(),saveKey=randomUUID(),input=templateSchema.parse({title:'Original standalone routine',assigneeUserId:actor.userId,completionApprovers:[f.head.userId],published:true}),runKey=randomUUID(),dueAt=Date.parse('2026-11-05T10:00:00Z');
 vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','disabled');
 try{
  const saved=await saveTemplate(holding,actor,null,input,saveKey),id=String(saved.id);expect(await saveTemplate(holding,actor,null,input,saveKey)).toEqual(saved);expect(await getTemplate(holding,actor,id)).toMatchObject({id,published:1,project_id:null});expect((await listTemplates(holding,actor)).map(row=>row.id)).toContain(id);const ran=await runTemplate(holding,actor,id,dueAt,runKey);expect(await runTemplate(holding,actor,id,dueAt,runKey)).toEqual(ran);expect(await holding.prepare('SELECT template_id,template_version,assignee_user_id,completion_approvers_json FROM staff_tasks WHERE id=?').bind(String(ran.id)).first()).toMatchObject({template_id:id,template_version:1,assignee_user_id:actor.userId,completion_approvers_json:JSON.stringify([f.head.userId])});
  const preview=await run(p.actor,false,lease=>legacyGrantMigration(holding,'h001',lease,secret).preview(f.company,'Preserve original standalone employee routines','test:actual-module-free-routine-parity'));expect(preview.comparison.roles.find(row=>row.assignmentId===actor.assignmentId)).toMatchObject({modulesBefore:[],modulesAfter:[],missingPurchasedCoverage:[]});await run(p.actor,true,lease=>legacyGrantMigration(holding,'h001',lease,secret).apply(preview.request!,preview.review!.token));expect(await snapshot()).toEqual(before);vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');
  expect(await getTemplate(holding,actor,id)).toMatchObject({id,published:1,project_id:null});expect((await listTemplates(holding,actor)).map(row=>row.id)).toContain(id);expect(await saveTemplate(holding,actor,null,input,saveKey)).toEqual(saved);expect(await runTemplate(holding,actor,id,dueAt,runKey)).toEqual(ran);const next=await runTemplate(holding,actor,id,dueAt+86400000,randomUUID());expect(next.id).not.toBe(ran.id);expect(await snapshot()).toEqual(before);expect(await holding.prepare('SELECT count(*)::int count FROM staff_tasks WHERE template_id=?').bind(id).first<number>('count')).toBe(2);
 }finally{vi.unstubAllEnvs();}
});

it('preserves actual readable private standalone routine history after all original engagements ended without granting an unrelated account or new staff authority',async()=>{
 const {saveTemplate,getTemplate,listTemplates,templateSchema}=await import('@/server/modules/operations/public/routines'),{setDepartmentHead}=await import('@/server/modules/operations/public/staff-settings'),{activeStaff}=await import('@/server/modules/operations/public/staff-policy'),{legacyCompanyModuleAccess}=await import('@/server/modules/identity_access/public/legacy-module-access'),{loadIamAuthority}=await import('@/server/modules/identity_access/public/iam-store'),p=await provider(),f=await fixture(),role='custom_routine_reader_'+randomUUID().replaceAll('-','');
 await holding.batch([holding.prepare("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES(?,?,'Unrelated original account',false,1,'active')").bind(f.company,role),holding.prepare("INSERT INTO iam_role_revisions VALUES(?,?,1,'[\"records.read\"]'::jsonb,'[]'::jsonb,'[\"holding\"]'::jsonb,'fixture',1)").bind(f.company,role)]);const stranger=await f.member('no-routine-or-staff',role,1,f.department,false);for(const actor of[f.claimant,f.owner,stranger])actor.iam=await loadIamAuthority(holding,f.company,actor.userId,actor.role);
 vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','disabled');
 try{
  await setDepartmentHead(holding,f.owner,f.department,{personId:'person-'+f.head.userId,expectedVersion:0,reason:'Original independently appointed retained routine reviewer'},randomUUID());const input=templateSchema.parse({title:'Original private retained routine',assigneeUserId:f.claimant.userId,completionApprovers:[f.head.userId],published:false}),saved=await saveTemplate(holding,f.claimant,null,input,randomUUID()),id=String(saved.id);
  // Populated historical employment, not an invented template or an orphan
  // label. All current account assignments and the immutable source remain.
  await holding.prepare("UPDATE hr_engagements SET end_date='2026-01-01',version=version+1 WHERE company_id=?").bind(f.company).run();await expect(activeStaff(holding,f.company,f.claimant.userId)).rejects.toMatchObject({code:'STAFF_LINK_REQUIRED'});expect((await getTemplate(holding,f.claimant,id)).id).toBe(id);expect((await getTemplate(holding,f.owner,id)).id).toBe(id);await expect(getTemplate(holding,stranger,id)).rejects.toMatchObject({code:'TEMPLATE_NOT_FOUND'});expect((await listTemplates(holding,stranger)).map(row=>row.id)).not.toContain(id);
  const source=await legacyCompanyModuleAccess(holding,f.company);expect(source.witnesses.every(row=>row.routineAccess?.activeStaff===false)).toBe(true);expect(source.witnesses.find(row=>row.assignmentId===f.claimant.assignmentId)?.routineAccess).toMatchObject({activeStaff:false,retainedTemplates:1});expect(source.witnesses.find(row=>row.assignmentId===stranger.assignmentId)?.routineAccess).toMatchObject({activeStaff:false,retainedTemplates:0});
  const before=(await holding.prepare('SELECT id,role,modules_json,version FROM assignments WHERE company_id=? ORDER BY id').bind(f.company).all()).results,preview=await run(p.actor,false,lease=>legacyGrantMigration(holding,'h001',lease,secret).preview(f.company,'Preserve actual original routine history after employment ended','test:retained-standalone-routine'));expect(preview.comparison.requiredPackages).toContain('work-routines');await run(p.actor,true,lease=>legacyGrantMigration(holding,'h001',lease,secret).apply(preview.request!,preview.review!.token));vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');
  expect((await getTemplate(holding,f.claimant,id)).id).toBe(id);expect((await getTemplate(holding,f.owner,id)).id).toBe(id);expect((await listTemplates(holding,f.claimant)).map(row=>row.id)).toContain(id);await expect(getTemplate(holding,stranger,id)).rejects.toMatchObject({code:'TEMPLATE_NOT_FOUND'});expect((await listTemplates(holding,stranger)).map(row=>row.id)).not.toContain(id);await expect(activeStaff(holding,f.company,f.claimant.userId)).rejects.toMatchObject({code:'STAFF_LINK_REQUIRED'});expect((await holding.prepare('SELECT id,role,modules_json,version FROM assignments WHERE company_id=? ORDER BY id').bind(f.company).all()).results).toEqual(before);
  await holding.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE company_id=? AND user_id IN (?,?)').bind(Date.now(),f.company,f.claimant.userId,f.owner.userId).run();const orphanSource=await legacyCompanyModuleAccess(holding,f.company),{legacyPackagePreview}=await import('@/server/modules/commercial/domain/legacy-packages'),now=Date.now();expect(orphanSource.witnesses.length).toBeGreaterThan(0);expect(orphanSource.witnesses.every(row=>!row.routineAccess?.activeStaff&&row.routineAccess?.retainedTemplates===0)).toBe(true);expect(legacyPackagePreview(orphanSource,[],{startsAt:now,now,reason:'Only readable original history qualifies',evidence:'test:unreadable-private-orphan'}).requiredPackages).not.toContain('work-routines');
 }finally{vi.unstubAllEnvs();}
});

it('rolls back normal legacy grants when a deferred original routine employment witness changes after the batch receipt',async()=>{
 const p=await provider(),f=await fixture(),name='legacy_routine_final_'+randomUUID().replaceAll('-',''),preview=await run(p.actor,false,lease=>legacyGrantMigration(holding,'h001',lease,secret).preview(f.company,'Preserve exact current original routine eligibility','test:deferred-routine-source'));
 expect(preview.comparison.requiredPackages).toContain('work-routines');const before=await holding.prepare('SELECT end_date,version FROM hr_engagements WHERE id=?').bind('eng-'+f.claimant.userId).first();
 await holding.exec(`CREATE SEQUENCE ${name};CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.company_id='${f.company}' THEN PERFORM nextval('${name}');UPDATE hr_engagements SET end_date='2026-01-01',version=version+1 WHERE id='eng-${f.claimant.userId}';END IF;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON company_package_change_sets DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION ${name}()`);
 try{await expect(run(p.actor,true,lease=>legacyGrantMigration(holding,'h001',lease,secret).apply(preview.request!,preview.review!.token))).rejects.toMatchObject({code:'STALE_LEGACY_PREVIEW'});expect(await holding.prepare(`SELECT is_called FROM ${name}`).first('is_called')).toBe(true);expect(await holding.prepare('SELECT end_date,version FROM hr_engagements WHERE id=?').bind('eng-'+f.claimant.userId).first()).toEqual(before);for(const table of ['company_package_grants','company_package_grant_events','company_package_change_sets','company_package_revisions','company_package_review_evidence'])expect(await holding.prepare(`SELECT 1 FROM ${table} WHERE company_id=?`).bind(f.company).first()).toBeNull();}
 finally{await holding.exec(`DROP TRIGGER ${name} ON company_package_change_sets;DROP FUNCTION ${name}();DROP SEQUENCE ${name}`);}
 const applied=await run(p.actor,true,lease=>legacyGrantMigration(holding,'h001',lease,secret).apply(preview.request!,preview.review!.token));expect(applied.after.some(grant=>grant.packageId==='work-routines')).toBe(true);expect(await run(p.actor,true,lease=>legacyGrantMigration(holding,'h001',lease,secret).apply(preview.request!,preview.review!.token))).toEqual(applied);
});

it('opens original module-free standalone Routines from actual Apps and Work SSO without a Projects purchase or presenter dependency',async()=>{
 const {loadIamAuthority}=await import('@/server/modules/identity_access/public/iam-store'),{accountWorkModuleAccess}=await import('@/server/modules/identity_access/public/work-module-access'),{createProductLauncher}=await import('@/server/modules/work_projection/application/product-launcher'),{createProductWorkspace}=await import('@/server/modules/identity_access/application/product-workspace'),{productWorkspaceTarget}=await import('@/server/modules/identity_access/domain/product-workspace'),{initializeWorkSelection,selectWorkRole}=await import('@/server/modules/work_projection/public/work-role-store'),{readCompanyPurchaseFacts}=await import('@/server/modules/commercial/public/package-grants'),{assertProductionModuleEnabled}=await import('@/server/modules/pms/public/hospitality-rollout'),{buildWorkspaceHoldingChoice}=await import('@/lib/work_projection/workspace-choice'),{saveTemplate,getTemplate,templateSchema}=await import('@/server/modules/operations/public/routines'),{setDepartmentHead,staffSettings}=await import('@/server/modules/operations/public/staff-settings'),{activeStaff}=await import('@/server/modules/operations/public/staff-policy'),{listProjects}=await import('@/server/modules/projects/public/staff-projects'),{createStaffPageService}=await import('@/server/modules/work_projection/application/staff-page'),{ApiError}=await import('@/server/modules/runtime/public/http');
 type Access=import('@/server/modules/identity_access/public/authorization').AccountAccessContext;const p=await provider(),f=await fixture(),role='custom_routine_product_'+randomUUID().replaceAll('-','');await holding.batch([holding.prepare("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES(?,?,'Original routine self work',false,1,'active')").bind(f.company,role),holding.prepare("INSERT INTO iam_role_revisions VALUES(?,?,1,'[\"records.read\"]'::jsonb,'[]'::jsonb,'[\"holding\"]'::jsonb,'fixture',1)").bind(f.company,role)]);const actor=await f.member('actual-routine-product',role);actor.iam=await loadIamAuthority(holding,f.company,actor.userId,role);expect(actor.modules).toEqual([]);
 vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','disabled');vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_ENVIRONMENT','local');vi.stubEnv('ERPX_PRODUCT_ENVIRONMENT','development');vi.stubEnv('ERPX_PRODUCT_ROUTING','enabled');vi.stubEnv('ERPX_PRODUCT_SSO','enabled');
 try{
  await setDepartmentHead(holding,f.owner,f.department,{personId:'person-'+f.head.userId,expectedVersion:0,reason:'Original independent routine product reviewer'},randomUUID());const saved=await saveTemplate(holding,actor,null,templateSchema.parse({title:'Original routine on its Work host',assigneeUserId:actor.userId,completionApprovers:[f.head.userId],published:true}),randomUUID()),before=(await holding.prepare('SELECT id,role,modules_json,version FROM assignments WHERE company_id=? ORDER BY id').bind(f.company).all()).results,preview=await run(p.actor,false,lease=>legacyGrantMigration(holding,'h001',lease,secret).preview(f.company,'Preserve original standalone routine product entry','test:actual-apps-routine-entry'));expect(preview.comparison.requiredPackages).toContain('work-routines');expect(preview.comparison.requiredPackages).not.toContain('projects');await run(p.actor,true,lease=>legacyGrantMigration(holding,'h001',lease,secret).apply(preview.request!,preview.review!.token));vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');
  const now=Date.now(),parentId=randomUUID(),createdAt=new Date(now-1000),expiresAt=new Date(now+3600000),store=productSsoStore(control,'development',{...origins,apps:'https://apps-dev-erpx.jesadakorn.com',work:'https://work-dev-erpx.jesadakorn.com'});await control.prepare('INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,1,?,?)').bind(actor.userId,actor.name,actor.userId+'@example.test',now,now).run();await control.prepare('INSERT INTO session(id,token,"userId","createdAt","updatedAt","expiresAt") VALUES(?,?,?,?,?,?)').bind(parentId,randomUUID(),actor.userId,createdAt.toISOString(),createdAt.toISOString(),expiresAt.toISOString()).run();await store.registerCentral({id:parentId,createdAt,expiresAt});
  const child=async(audience:'apps'|'work')=>{const browser=await store.start(audience),grant=await store.authorize({transactionId:browser.id,state:browser.state,challenge:browser.challenge,parentSessionId:parentId});return store.consume({audience,code:grant.code,state:browser.state,browserCookie:browser.browserCookie});},apps=await child('apps'),work=await child('work'),access=async(audience:'apps'|'work'):Promise<Access>=>{const session=await store.session(audience==='apps'?apps.token:work.token,audience);if(!session)throw Error('SESSION_EXPIRED');const row=await holding.prepare('SELECT id,role,version,valid_until FROM assignments WHERE id=? AND user_id=? AND company_id=? AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)').bind(actor.assignmentId,actor.userId,f.company).first<{id:string;role:string;version:number;valid_until:number}>();return {sessionId:session.session.id,expiresAt:session.session.expiresAt.getTime(),userId:session.user.id,name:session.user.name,email:session.user.email,scope:actor.scope,assignments:row?[{id:row.id,role:row.role,companyId:f.company,branchId:null,modules:[],scopeKind:'holding',scopeNodeId:null,version:Number(row.version),validUntil:Number(row.valid_until)}]:[],requestHeaders:new Headers()};},purchaseFacts=(a:Access)=>readCompanyPurchaseFacts(holding,a.scope.companyId),ready=async(a:Access,moduleId:string)=>{await assertProductionModuleEnabled(holding,a.scope.companyId,moduleId);return true;};
  const choice=buildWorkspaceHoldingChoice('test','Test',[{companyId:f.company,companyName:'Original Company',scopeKind:'holding',scopeNodeId:null}],[],true)!,cards=await createProductLauncher({choices:async()=>[choice],access:()=>access('apps'),purchaseFacts,moduleAccess:accountWorkModuleAccess,ready,companySlug:async()=>f.company,origin:id=>`https://${id}-dev-erpx.jesadakorn.com`,denied:()=>false,now:Date.now})();expect(cards).toHaveLength(1);expect(cards[0].productId).toBe('work');const workspace=createProductWorkspace({access:()=>access('work'),modules:accountWorkModuleAccess,purchaseFacts,ready,selection:(a,applicable)=>initializeWorkSelection(holding,{sessionId:a.sessionId,userId:a.userId,companyId:a.scope.companyId,expiresAt:a.expiresAt},a.assignments,applicable),select:(a,option,revision)=>selectWorkRole(holding,{sessionId:a.sessionId,userId:a.userId,companyId:a.scope.companyId,expiresAt:a.expiresAt,assignmentId:option.assignmentId,assignmentVersion:option.assignmentVersion,revision}),now:Date.now,error:(_status,code)=>Error(code)}),handoff=new URL(new URL(cards[0].href).searchParams.get('next')!,'https://work-dev-erpx.jesadakorn.com'),landing=await workspace.read('work',productWorkspaceTarget('work',f.company,handoff.searchParams.get('next'))!);expect(landing.target).toBe('/holdings/test/home');actor.authority={sessionId:work.sessionId,revision:landing.selection!.revision,assignmentVersion:landing.selection!.assignment_version!};
  const target=productWorkspaceTarget('work',f.company,'/holdings/test/projects-routines/routines?status=published#retained')!,opened=await workspace.read('work',target);expect(opened.target).toBe(target.target);expect(opened.options.map(row=>row.assignmentId)).toEqual([actor.assignmentId]);for(const section of['','/overview']){const record=productWorkspaceTarget('work',f.company,`/holdings/test/projects-routines/routines/${saved.id}${section}?filter=mine#source`)!;expect((await workspace.read('work',record)).target).toBe(record.target);}expect((await getTemplate(holding,actor,String(saved.id))).id).toBe(saved.id);
  await expect(listProjects(holding,actor)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});const {routineProjectOptions}=await import('@/server/modules/projects/public/routine-project-options'),page=await createStaffPageService({actor:async()=>actor,prepare:async()=>{},settings:a=>staffSettings(holding,a),projects:a=>routineProjectOptions(holding,a),staff:a=>activeStaff(holding,a.companyId,a.userId),expectedAbsence:e=>e instanceof ApiError,base:()=>'/holdings/test'})('test');expect(page.projects).toEqual([]);expect(page.staff?.engagement_id).toBe('eng-'+actor.userId);
  expect((await holding.prepare('SELECT id,role,modules_json,version FROM assignments WHERE company_id=? ORDER BY id').bind(f.company).all()).results).toEqual(before);await store.revokeFamily(parentId);await expect(workspace.read('work',target)).rejects.toThrow('SESSION_EXPIRED');
 }finally{vi.unstubAllEnvs();}
});


it('preserves original Home Social without a collaboration module through normal reviewed legacy migration without changing Company authority', async () => {
  const { loadIamAuthority } = await import('@/server/modules/identity_access/public/iam-store');
  const { legacyCompanyModuleAccess } = await import('@/server/modules/identity_access/public/legacy-module-access');
  const { assertProductionModuleEnabled } = await import('@/server/modules/pms/public/hospitality-rollout');
  const { savePostDraft, publishPost, postView, postAction, postComments } = await import('@/server/modules/collaboration/public/social-home');
  const p = await provider(), f = await fixture(), author = f.claimant, reader = f.head;
  for (const actor of [author, reader]) {
    actor.iam = await loadIamAuthority(holding, f.company, actor.userId, actor.role);
    expect(actor.modules).toEqual([]);
    expect(await holding.prepare('SELECT iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) active FROM assignments WHERE id=? AND company_id=? AND user_id=?')
      .bind(actor.assignmentId, f.company, actor.userId).first('active')).toBe(true);
  }
  const snapshot = async () => ({
    assignments: (await holding.prepare('SELECT id,user_id,role,modules_json,scope_kind,scope_node_id,version FROM assignments WHERE company_id=? ORDER BY id').bind(f.company).all()).results,
    roles: (await holding.prepare('SELECT id,published_revision,status FROM iam_roles WHERE company_id=? ORDER BY id').bind(f.company).all()).results,
    rollouts: (await holding.prepare('SELECT * FROM company_feature_rollouts WHERE company_id=? ORDER BY module_id').bind(f.company).all()).results,
  });
  vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT', 'disabled');
  try {
    await assertProductionModuleEnabled(holding, f.company, 'collaboration');
    const source = await legacyCompanyModuleAccess(holding, f.company);
    expect(source.beta).toBe(false);
    for (const actor of [author, reader]) {
      const witness = source.witnesses.find(row => row.assignmentId === actor.assignmentId);
      if (!witness) throw new Error('Actual original current source witness is required');
      expect(witness).toMatchObject({ roleId: actor.role, scopeKind: 'holding' });
      expect(witness.modules).not.toContain('collaboration');
      actor.modules = witness.modules;
    }
    const before = await snapshot(), input = { body: 'Original current-account Home update before software migration', audience: { kind: 'company' as const }, expectedVersion: 1 };
    const draft = await savePostDraft(holding, author, null, input, randomUUID());
    const publishKey = randomUUID(), published = await publishPost(holding, author, draft.id, draft.version, publishKey);
    expect((await postView(holding, reader, published.id)).body).toBe(input.body);
    await postAction(holding, reader, published.id, { action: 'comment', body: 'Original current-account response' }, randomUUID());
    expect((await postComments(holding, author, published.id)).items.map(row => row.body)).toContain('Original current-account response');
    const preview = await run(p.actor, false, lease => legacyGrantMigration(holding, 'h001', lease, secret)
      .preview(f.company, 'Preserve original current-account Home Social work', 'test:actual-module-free-home-social-parity'));
    for (const actor of [author, reader]) {
      expect(preview.comparison.roles.find(row => row.assignmentId === actor.assignmentId)).toMatchObject({ modulesBefore: actor.modules, modulesAfter: actor.modules, missingPurchasedCoverage: [] });
    }
    await run(p.actor, true, lease => legacyGrantMigration(holding, 'h001', lease, secret).apply(preview.request!, preview.review!.token));
    expect(await snapshot()).toEqual(before);
    vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT', 'enforced');
    await assertProductionModuleEnabled(holding, f.company, 'collaboration');
    const nextDraft = await savePostDraft(holding, author, null, { ...input, body: 'Same original Home workflow after normal software migration' }, randomUUID());
    const nextPublished = await publishPost(holding, author, nextDraft.id, nextDraft.version, randomUUID());
    expect((await postView(holding, reader, nextPublished.id)).body).toBe('Same original Home workflow after normal software migration');
    expect((await postView(holding, reader, published.id)).body).toBe(input.body);
    expect(await publishPost(holding, author, draft.id, draft.version, publishKey)).toEqual(published);
    await postAction(holding, reader, published.id, { action: 'comment', body: 'Same retained conversation after normal migration' }, randomUUID());
    expect((await postComments(holding, author, published.id)).items.map(row => row.body)).toContain('Same retained conversation after normal migration');
    expect(await snapshot()).toEqual(before);
  } finally {
    vi.unstubAllEnvs();
  }
});
