import { chmod, writeFile } from "node:fs/promises";
import { basename, join } from "node:path";
import { createHash, randomBytes, randomUUID } from "node:crypto";
import { hashPassword } from "better-auth/crypto";
import type { RuntimeEnv } from "@erpx/server-runtime/adapters/runtime/env";
import { isTransactionalSqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { seedExpenseCompany } from "../../expenses/company";
import { mapSpendingOrganization } from "../../../tests/support/helpers/spending-organization";
import { businessRoles } from "@erpx/feature-identity_access/lib/business-roles";
import registry from "@erpx/catalogue/module-registry.json";
import { moduleBrowserHoldingDisplayName } from "../../../scripts/testing/browser-fixture-metadata";

/** Called only after browser-simulation validated its exact eight owned DBs. */
export async function prepareModuleBrowser(profile: string, env: RuntimeEnv, directory: string) {
  if (!["local-seed", "staff-work", "expense", "work-cards", "messaging", "services", "executive-work", "booking"].includes(profile)) throw new Error(`UNSUPPORTED_MODULE_BROWSER_PROFILE:${profile}`);
  await env.CONTROL_DB.batch([
    env.CONTROL_DB.prepare("INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES('h001',?,?,'H001_DB','H001_READ_DB','H001_FILES','active')").bind(profile === "booking" ? "booking-test" : "acme", moduleBrowserHoldingDisplayName(profile)),
    env.CONTROL_DB.prepare("INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES('h002','northstar','Isolated Northstar fixture','H002_DB','H002_READ_DB','H002_FILES','active')"),
  ]);
  if (profile === "local-seed" || profile === "staff-work") {
    // These are the normal fictional personas, created in the run's disposable
    // DBs. getLocalProxy uses the cohort's equally guarded jobs manifest.
    await import("../../../scripts/seed/seed-local");
    await import("../../../scripts/seed/repair-seed-identity");
    await import("../../../scripts/seed/seed-commerce");
    await import("../../../scripts/seed/seed-staff-work");
    // Tests use the normal explicitly declared Acme staffing personas; the
    // workflow's separate development CLI retains its existing dev-only guard.
    const { companyId, actors } = await (await import("../../../scripts/seed/seed-acme-organization")).seedAcmeOrganizationForBrowser(directory);
    const current = (await import("@erpx/server-runtime/adapters/runtime/env")).getRuntime();
    if (profile === "local-seed") await (await import("./local-browser-erpx-realm")).seedLocalBrowserRealm(current, directory);
    await (await import("../../people_payroll/builders/local-browser-roster-directory")).seedLocalBrowserRosterDirectory(current, directory);
    await (await import("../../people_payroll/builders/local-browser-leave")).seedLocalBrowserLeave(current, directory);
    const password = `NoRole-${randomBytes(20).toString("hex")}`, hash = await hashPassword(password), now = Date.now();
    const noRole = { email: "no-role@simulation.test", password }, id = "browser-no-role";
    await current.CONTROL_DB.batch([
      current.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)').bind(id, "Fictional account without assignments", noRole.email, now, now),
      current.CONTROL_DB.prepare("INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,'credential','local:credential',?,?,?,?)").bind(`account-${id}`, id, id, hash, now, now),
      current.CONTROL_DB.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?)").bind(`principal-${id}`, id, "Fictional account without assignments", now),
    ]);
    return { profile, holdingSlug: "acme", nativeSeed: true, noRole, actors, releaseVerificationCompanyIds: [companyId] };
  }
  if (profile === "booking") return process.env.ERPX_BROWSER_SIMULATION_PMS_SCALE === "1"
    ? (await import("../../pms/builders/pms-scale-browser")).seedPmsScaleBrowser(env,directory)
    : (await import("../../pms/builders/booking-browser")).seedBookingBrowser(env,directory);
  const password = `Module-${randomBytes(20).toString("hex")}`, hash = await hashPassword(password), now = Date.now();
  async function company() {
    const f = await seedExpenseCompany(env.H001_DB, env.H001_FILES);
    await mapSpendingOrganization(f);
    const missing = await f.member("missing", "employee", 1, f.department, false);
    return { f, actors: { claimant: f.claimant, head: f.head, reviewer: f.reviewer, manager: f.manager, payer: f.payer, owner: f.owner, it: f.it, missing } };
  }
  async function account(actor: Awaited<ReturnType<typeof company>>["actors"]["owner"], name: string) {
    actor.email = `${actor.userId}@example.test`;
    await env.CONTROL_DB.batch([
      env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)').bind(actor.userId, name, actor.email, now, now),
      env.CONTROL_DB.prepare("INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,'credential','local:credential',?,?,?,?)").bind(`account-${actor.userId}`, actor.userId, actor.userId, hash, now, now),
      env.CONTROL_DB.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?)").bind(`principal-${actor.userId}`, actor.userId, name, now),
    ]);
    const assignments = await env.H001_DB.prepare("SELECT id,role FROM assignments WHERE user_id=? AND company_id=?").bind(actor.userId, actor.companyId).all<{ id: string; role: string }>();
    for (const assignment of assignments.results) {
      const modules = assignment.role === "holding_owner" ? registry.map(module => module.id) : businessRoles[assignment.role as keyof typeof businessRoles] ?? [];
      await env.H001_DB.prepare("UPDATE assignments SET modules_json=? WHERE id=?").bind(JSON.stringify(modules), assignment.id).run();
    }
    await env.H001_DB.prepare("UPDATE hr_people SET email=? WHERE user_id=? AND company_id=?").bind(actor.email, actor.userId, actor.companyId).run();
  }
  if (profile === "executive-work") {
    const { f, actors } = await company();
    await account(actors.owner, "Executive Company owner");
    actors.owner.holdingSlug = "acme";
    actors.owner.holdingName = "Acme Group";
    actors.owner.scope = { ...actors.owner.scope, holdingSlug: "acme", holdingName: "Acme Group" };
    const now = Date.now(), packageImage = {
      companyId: f.company, packageId: "work", packageVersion: 1, state: "active", startsAt: now - 1_000,
      endsAt: null, version: 1, reason: "Isolated Executive browser fixture package visibility.",
      evidence: "Disposable browser cohort fixture; not a Commercial purchase or installed grant.",
    };
    const run = basename(directory), eventId = `fixture:executive-work:${run}`;
    if (!isTransactionalSqlDatabase(env.H001_DB)) throw new Error("EXECUTIVE_WORK_PACKAGE_FIXTURE_TRANSACTION_REQUIRED");
    await env.H001_DB.transaction(async tx => {
      await tx.prepare("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
        .bind(packageImage.companyId, packageImage.packageId, packageImage.packageVersion, packageImage.state, packageImage.startsAt, packageImage.endsAt, packageImage.version, packageImage.reason, packageImage.evidence, now).run();
      await tx.prepare("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) VALUES(?,?,?,?,?,NULL,?::jsonb,?)")
        .bind(eventId, packageImage.companyId, packageImage.packageId, actors.owner.userId,
          createHash("sha256").update(JSON.stringify([eventId, packageImage])).digest("hex"), JSON.stringify(packageImage), now).run();
    });
    const { createExternalConnection } = await import("@erpx/server-runtime/public/executive/connections");
    const issued = await createExternalConnection(env.CONTROL_DB, env.H001_DB, actors.owner,
      { name: "Executive read-only browser proof", scopeId: f.company, expiresDays: 7 }, randomUUID());
    const assignment = await env.H001_DB.prepare("SELECT version FROM assignments WHERE id=? AND company_id=? AND user_id=?")
      .bind(actors.owner.assignmentId, f.company, actors.owner.userId).first<{ version: number }>();
    if (!assignment) throw new Error("EXECUTIVE_WORK_OWNER_ASSIGNMENT_REQUIRED");
    const credentials = {
      profile: "executive-work", run, companyId: f.company,
      assignmentId: actors.owner.assignmentId, assignmentVersion: Number(assignment.version), email: actors.owner.email,
      password, token: issued.secret,
    };
    const credentialPath = join(directory, "executive-work-credentials.json");
    await writeFile(credentialPath, JSON.stringify(credentials, null, 2) + "\n", { mode: 0o600 }); await chmod(credentialPath, 0o600);
    const fixture = { profile, company: f.company, branch: f.branch, department: f.department,
      owner: { email: actors.owner.email, userId: actors.owner.userId }, releaseVerificationCompanyIds: [f.company] };
    const path = join(directory, "fixture.json");
    await writeFile(path, JSON.stringify(fixture, null, 2) + "\n", { mode: 0o600 }); await chmod(path, 0o600);
    return { profile, holdingSlug: "acme", fixturePath: path, releaseVerificationCompanyIds: [f.company] };
  }
  let fixture: unknown;
  if (profile === "work-cards") {
    const accounts: Record<string, object> = {};
    for (const beta of [false, true]) {
      const { f, actors } = await company();
      await f.grant(f.it, "warehouse_manager");
      for (const [name, actor] of Object.entries({ owner: actors.owner, member: actors.claimant, missing: actors.missing, handler: actors.it })) {
        await account(actor, name);
        accounts[`${beta ? "beta-" : ""}${name}`] = { email: actor.email, userId: actor.userId, assignmentId: actor.assignmentId, company: f.company, branch: f.branch, department: f.department };
        if (beta) await env.H001_DB.prepare("INSERT INTO beta_members(user_id,joined_at) VALUES(?,?)").bind(actor.userId, now).run();
      }
      if (beta) await env.H001_DB.prepare("INSERT INTO beta_policy(company_id,member_limit,branch_limit,ai_monthly_limit,storage_bytes,created_at) VALUES(?,100,10,100,1073741824,?)").bind(f.company, now).run();
    }
    fixture = { password, accounts };
  } else {
    const { f, actors } = await company();
    if (profile === "messaging") {
      const chatActors = { alice: actors.claimant, bob: actors.head, carol: actors.reviewer };
      for (const [name, actor] of Object.entries(chatActors)) await account(actor, `Messaging ${name}`);
      await env.H001_DB.prepare("INSERT INTO beta_policy(company_id,member_limit,branch_limit,ai_monthly_limit,storage_bytes,created_at) VALUES(?,100,10,100,1073741824,?)").bind(f.company, now).run();
      fixture = { company: f.company, password, actors: chatActors };
    } else {
      let supplies = {};
      if (profile === "services") {
        await f.grant(f.it, "purchasing_operator"); await f.grant(f.it, "warehouse_manager");
        const product = `${f.company}-laptop`, warehouse = `${f.company}-warehouse`, supplier = `${f.company}-supplier`;
        await env.H001_DB.batch([
          env.H001_DB.prepare("INSERT INTO commerce_products(id,company_id,sku,name,price_minor) VALUES(?,?,'LAPTOP','Office laptop',10000)").bind(product, f.company),
          env.H001_DB.prepare("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES(?,?,?,'OFFICE','Office stores')").bind(warehouse, f.company, f.branch),
          env.H001_DB.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'supplier','SUPPLIER','Office supplier','THB',?,?)").bind(supplier, f.company, now, now),
        ]);
        supplies = { product, warehouse, supplier, beta: false };
      }
      for (const [name, actor] of Object.entries(actors)) await account(actor, name);
      fixture = { company: f.company, branch: f.branch, department: f.department, accounts: f.accounts, period: f.period, password, actors, ...supplies };
    }
  }
  const releaseVerificationCompanyIds = profile === "work-cards"
    ? [...new Set(Object.values((fixture as { accounts: Record<string, { company: string }> }).accounts).map(account => account.company))]
    : [(fixture as { company: string }).company];
  const path = join(directory, "fixture.json");
  await writeFile(path, JSON.stringify(fixture, null, 2) + "\n", { mode: 0o600 }); await chmod(path, 0o600);
  return { profile, holdingSlug: "acme", fixturePath: path, releaseVerificationCompanyIds };
}
