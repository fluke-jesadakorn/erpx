import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { assertAcmeBrowserSeedTarget } from "../../fixtures/shared/builders/acme-browser-seed-target";
import { localSeedLevels } from "../../fixtures/organization/builders/local-seed-levels";
import { join } from "node:path";
import { hashPassword } from "better-auth/crypto";
import { getLocalProxy } from "../local/fixtures";
import { businessRoles } from "../../src/lib/identity_access/business-roles";
import { reportingIssues } from "../../src/lib/organization/organization";
import { organizationRoster } from "../../src/server/modules/organization/public/organization";
import { expenseRoute, spendingUsers } from "../../src/server/modules/people_payroll/public/expense-routing";
import { ensureOrganizationChannels, organizationMemberSql } from "../../src/server/modules/organization/public/organization-channels";
import type { SqlDatabase, SqlStatement } from "../../src/server/modules/runtime/adapters/runtime/types";
import { demoBranches, demoCompany, demoDepartments, demoExpiry, demoInitialPassword, demoPrefix, demoStaffPlan, type DemoStaff } from "../../fixtures/organization/builders/acme-organization";

// The development CLI and the owned browser fixture invoke the same explicit workflow.
async function seedAcmeOrganizationWorkflow(platform: Awaited<ReturnType<typeof getLocalProxy>>, apply: boolean, verifyOnly: boolean, evidenceDirectory = join(".scratch", "acme-organization")) {
const db = platform.env.H001_DB, control = platform.env.CONTROL_DB;
const now = Date.now();
const reason = "User-authorized fictional Acme staffing expansion for workflow testing; existing profiles, roles and documents preserved.";

function audit(statements: SqlStatement[], id: string, action: string, type: string) {
  statements.push(db.prepare("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) VALUES(?,?,?, ?,?,?,?,1,?,?)")
    .bind(`${demoPrefix}-audit-${id}`, demoPrefix, "local-demo-seed", "local-demo-seed", action, type, id, reason, now));
}
async function snapshot(database: SqlDatabase, company: string) {
  const result: Record<string, unknown> = {};
  for (const table of ["organization_nodes", "staff_levels", "organization_positions", "hr_people", "hr_engagements", "employees", "assignments", "department_heads", "spending_approval_templates"])
    result[table] = (await database.prepare(`SELECT * FROM ${table} WHERE company_id=? ORDER BY 1`).bind(company).all()).results;
  return result;
}
async function verify(plan: DemoStaff[], ceoUser: string) {
  const roster = await organizationRoster(db, demoCompany);
  for (const person of roster.people.filter(person=>plan.some(fixture=>fixture.personId===person.person_id))) {
    const issues = reportingIssues(person, roster.people, roster.positions);
    if (issues.length) throw new Error(`${person.legal_name}: ${issues.join(" ")}`);
  }
  const failures: string[] = [];
  for (const person of plan) {
    const actual = roster.people.find(p => p.person_id === person.personId);
    if (!actual || actual.user_id !== person.userId || actual.status !== "active") throw new Error(`Missing active fixture: ${person.key}`);
    if (!await control.prepare("SELECT u.id FROM \"user\" u JOIN account a ON a.userId=u.id AND a.providerId='credential' JOIN principals p ON p.user_id=u.id AND p.status='active' WHERE u.id=? AND a.password IS NOT NULL").bind(person.userId).first()) throw new Error(`Missing login: ${person.key}`);
    const memberships = (await db.prepare(`SELECT n.id FROM organization_nodes n WHERE n.company_id=? AND n.status='active' AND ${organizationMemberSql("n", "?")}`).bind(demoCompany, person.userId).all<{ id: string }>()).results;
    const expected = [demoCompany, actual.branch_id, actual.department_id].sort();
    if (JSON.stringify(memberships.map(m => m.id).sort()) !== JSON.stringify(expected)) throw new Error(`Unexpected channel eligibility: ${person.key}`);
  }
  const results: Record<string, unknown>[] = [];
  // Evaluate real domain readiness, including requests made by Accounting, Finance and the CEO.
  const requesters = [...plan.map(p => ({ userId: p.userId, name: p.name })), { userId: ceoUser, name: "Existing Acme CEO" }];
  for (const [index, person] of requesters.entries()) {
    for (const kind of ["expense", "purchase", "payment"] as const) {
      const route = await expenseRoute(db, demoCompany, person.userId, [], false, kind);
      if (route.blockedReason) { failures.push(`${person.name} / ${kind}: ${route.blockedReason}`); continue; }
      const supervisor = route.steps.find(s => s.stage === "department_approval")?.userId;
      const preparers = route.steps.find(s => s.stage === "accounts_review")!.candidateUserIds!;
      const reviewers = route.steps.find(s => s.stage === "accounts_approval")!.candidateUserIds!;
      const payers = (await spendingUsers(db, route.scope, "treasury")).map(p => p.user_id!);
      const assignment = preparers.flatMap(preparer => reviewers.flatMap(reviewer => reviewers.flatMap(poster => payers.map(payer => [person.userId, supervisor, preparer, reviewer, poster, payer].filter(Boolean) as string[]))))
        .find(ids => new Set(ids).size === ids.length);
      if (!assignment) failures.push(`${person.name} / ${kind}: no independent preparer, reviewer, GL poster and payer combination`);
      results.push({ userId: person.userId, kind, supervisor: supervisor ?? "CEO exemption", independentParticipants: assignment });
    }
    if ((index + 1) % 20 === 0) console.log(`Readiness checked: ${index + 1}/${requesters.length} requesters`);
  }
  const matrix = (await db.prepare("SELECT e.branch_id,e.department_id,l.level_number,count(*)::int staff FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id JOIN staff_levels l ON l.id=e.level_id WHERE e.company_id=? AND p.status='active' AND e.status='active' GROUP BY e.branch_id,e.department_id,l.level_number ORDER BY e.branch_id,e.department_id,l.level_number").bind(demoCompany).all()).results;
  const report = { company: demoCompany, profiles: roster.people.length, addedFixtureProfiles: plan.length, readinessChecks: results.length, failures, matrix, routes: results };
  const path = join(evidenceDirectory, "verification.json");
  mkdirSync(evidenceDirectory, { recursive: true, mode: 0o700 });
  writeFileSync(path, JSON.stringify(report, null, 2), { mode: 0o600 });
  if (failures.length) throw new Error(failures.join("\n"));
  console.log(JSON.stringify({ company: demoCompany, profiles: roster.people.length, fixtureLogins: plan.length, readinessChecks: results.length, reportingIssues: 0, channelEligibilityChecks: plan.length, report: path }));
}

try {
  // Fail closed on every target, even when a production cluster has demo-looking company IDs.
  const roster = await organizationRoster(db, demoCompany);
  const ceos = roster.people.filter(p => roster.positions.some(position => position.id === p.position_id && position.is_ceo === 1) && p.status === "active");
  if (ceos.length !== 1 || !ceos[0].user_id || !ceos[0].position_id) throw new Error("Complete exactly one existing Acme CEO profile and login first.");
  const ceo = ceos[0];
  const plan = demoStaffPlan({ personId: ceo.person_id, positionId: ceo.position_id! });
  const existingIds = new Set(roster.people.map(p => p.person_id));
  const missing = plan.filter(person => !existingIds.has(person.personId));
  // Archived or edited fixtures are not silently recreated or reactivated.
  for (const person of missing) if (await db.prepare("SELECT id FROM hr_people WHERE id=?").bind(person.personId).first()) throw new Error(`Preserved inactive fixture ${person.key}; review it in People.`);
  for (const branch of demoBranches) if (!await db.prepare("SELECT id FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active'").bind(branch.id, demoCompany).first()) throw new Error(`Missing branch: ${branch.id}`);
  const levels = new Map((await db.prepare("SELECT id,level_number FROM staff_levels WHERE company_id=? AND active=1 AND level_number BETWEEN 1 AND 6").bind(demoCompany).all<{id:string;level_number:number}>()).results.map(level=>[level.level_number,level.id]));
  for (const level of [1, 2, 3, 4, 5, 6]) if (!levels.has(level)) throw new Error(`Missing numbered level: ${level}`);
  console.log(JSON.stringify({ mode: apply ? "apply" : verifyOnly ? "verify" : "preview", company: demoCompany, existingProfiles: roster.people.length, newProfiles: missing.length, totalAfter: roster.people.length + missing.length, branches: demoBranches.length, departmentTypes: demoDepartments.length }));
  if (!apply && !verifyOnly) { console.log("Preview only. Run bun run seed:organization:acme to apply, or bun run seed:organization:verify to verify."); }
  else {
    if (apply && missing.length) {
      const backupDir = join(evidenceDirectory, `before-${now}`);
      mkdirSync(backupDir, { recursive: true, mode: 0o700 });
      const before = await snapshot(db, demoCompany);
      const untouched = { beacon: await snapshot(db, "beacon-co"), northstar: await snapshot(platform.env.H002_DB, "northstar-co") };
      writeFileSync(join(backupDir, "acme.json"), JSON.stringify(before, null, 2), { mode: 0o600 });
      writeFileSync(join(backupDir, "plan.json"), JSON.stringify(missing, null, 2), { mode: 0o600 });
      console.log(`Saved pre-change snapshot: ${backupDir}`);

      // Control and holding databases are separate transactions. Stable IDs permit recovery
      // after an interrupted run; existing credentials and access are never overwritten.
      for (const person of missing) {
        const existingUser = await control.prepare("SELECT id,email FROM \"user\" WHERE id=? OR email=?").bind(person.userId, person.email).first<{ id: string; email: string }>();
        if (existingUser && (existingUser.id !== person.userId || existingUser.email !== person.email)) throw new Error(`Login identity collision: ${person.key}`);
        const credential = await control.prepare("SELECT id FROM account WHERE userId=? AND providerId='credential'").bind(person.userId).first();
        const accountStatements = [control.prepare("INSERT INTO \"user\"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?) ON CONFLICT(id) DO NOTHING").bind(person.userId, person.name, person.email, now, now),
          control.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?) ON CONFLICT(user_id) DO NOTHING").bind(`${person.key}-principal`, person.userId, person.name, now)];
        if (!credential) accountStatements.push(control.prepare("INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,'credential','local:credential',?,?,?,?)").bind(`${person.key}-credential`, person.userId, person.userId, await hashPassword(demoInitialPassword(person)), now, now));
        await control.batch(accountStatements);
      }
      const statements: SqlStatement[] = [];
      for (const branch of demoBranches) for (const department of demoDepartments) {
        const id = `dept:${branch.id}:${department.id}`;
        const existing = await db.prepare("SELECT company_id,branch_id,kind,status FROM organization_nodes WHERE id=?").bind(id).first<{ company_id: string; branch_id: string; kind: string; status: string }>();
        if (existing && (existing.company_id !== demoCompany || existing.branch_id !== branch.id || existing.kind !== "department" || existing.status !== "active")) throw new Error(`Department collision: ${id}`);
        if (!existing) {
          statements.push(db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES(?,'department',?,?,?, ?,?,'active',1)").bind(id, department.id, `${branch.name} ${department.name}`, branch.id, demoCompany, branch.id));
          audit(statements, id, "organization.department.seed", "organization_node");
        }
      }
      for (const person of missing) {
        const levelId = levels.get(person.level)!;
        statements.push(db.prepare("INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,reports_to_position_id,roster_required,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?, 'local-demo-seed',?,?)").bind(person.positionId, demoCompany, person.branchId, person.departmentId, person.title, levelId, person.reportsToPositionId, person.branchId==='bangkok'&&['operations','warehouse'].some(dept=>person.departmentId.endsWith(`:${dept}`))&&['staff','senior','associate','supervisor'].some(seat=>person.key.endsWith(`-${seat}`))?1:0, now, now));
        statements.push(db.prepare("INSERT INTO hr_people(id,user_id,company_id,legal_name,email,country_code,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,'active','local-demo-seed',?,?)").bind(person.personId, person.userId, demoCompany, person.name, person.email, person.country, now, now));
        statements.push(db.prepare("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,manager_person_id,job_title,currency,created_by,created_at,updated_at,pay_group_id,level_id,position_id) VALUES(?,?,?,?,?,'employee','salaried','active','2026-01-05',?,?,'USD','local-demo-seed',?,?,'paygroup-acme-co',?,?)").bind(person.engagementId, person.personId, demoCompany, person.branchId, person.departmentId, person.managerPersonId, person.title, now, now, levelId, person.positionId));
        statements.push(db.prepare("INSERT INTO employees(id,user_id,company_id,branch_id,department_id,person_id,display_name,status,created_at) VALUES(?,?,?,?,?,?,?,'active',?)").bind(`${person.key}-employee`, person.userId, demoCompany, person.branchId, person.departmentId, person.personId, person.name, now));
        statements.push(db.prepare("INSERT INTO staff_organization_history(id,company_id,engagement_id,before_json,after_json,actor_id,reason,command_id,created_at) VALUES(?,?,?,'{}',?,'local-demo-seed',?,?,?)").bind(`${person.key}-history`, demoCompany, person.engagementId, JSON.stringify({ position_id: person.positionId, level_id: levelId, manager_person_id: person.managerPersonId, branch_id: person.branchId, department_id: person.departmentId }), reason, demoPrefix, now));
        audit(statements, person.personId, "people.staff.seed", "hr_person");
        for (const grant of person.grants) {
          const id = `${person.key}-${grant.role}`;
          statements.push(db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,scope_kind,scope_node_id) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(id, person.userId, grant.role, JSON.stringify(businessRoles[grant.role]), demoCompany, grant.scope === "holding" ? null : person.branchId, now - 1000, demoExpiry, grant.scope, grant.scope === "holding" ? null : grant.scope === "branch" ? person.branchId : person.departmentId));
          audit(statements, id, "access.role.seed", "assignment");
        }
        if (person.level === 3) statements.push(db.prepare("INSERT INTO department_heads(department_id,company_id,person_id,updated_by,updated_at) VALUES(?,?,?,'local-demo-seed',?) ON CONFLICT DO NOTHING").bind(person.departmentId, demoCompany, person.personId, now));
      }
      await db.batch(statements);
      await ensureOrganizationChannels(db, demoCompany);
      // Compare old rows by primary key, allowing only newly inserted records.
      const after = await snapshot(db, demoCompany);
      for (const [table, rows] of Object.entries(before)) for (const row of rows as Record<string, unknown>[]) {
        const identity = row.id ?? row.department_id ?? row.company_id;
        const current = (after[table] as Record<string, unknown>[]).find(r => (r.id ?? r.department_id ?? r.company_id) === identity);
        if (JSON.stringify(row) !== JSON.stringify(current)) throw new Error(`Existing data changed unexpectedly: ${table}/${identity}`);
      }
      if (JSON.stringify(untouched) !== JSON.stringify({ beacon: await snapshot(db, "beacon-co"), northstar: await snapshot(platform.env.H002_DB, "northstar-co") })) throw new Error("Another company's staff data changed during verification.");
      console.log(`Created ${missing.length} staff. Existing Acme rows and both other companies verified unchanged.`);
    }
    if(apply) for(const person of plan.filter(person=>person.branchId==='bangkok'&&['operations','warehouse'].some(dept=>person.departmentId.endsWith(`:${dept}`))&&['staff','senior','associate','supervisor'].some(seat=>person.key.endsWith(`-${seat}`)))) {
      await db.prepare("UPDATE organization_positions SET roster_required=1 WHERE id=? AND company_id=? AND roster_required=0 AND version=1 AND created_by='local-demo-seed' AND updated_at=created_at AND title=?")
        .bind(person.positionId,demoCompany,person.title).run();
    }
    await verify(plan, ceo.user_id!);
    const hr = plan.find(person => person.key === `${demoPrefix}-bangkok-people-manager`)!;
    const reader = plan.find(person => person.key === `${demoPrefix}-bangkok-operations-staff`)!;
    return { hr: { userId: hr.userId, email: hr.email, password: demoInitialPassword(hr) }, reader: { userId: reader.userId, email: reader.email, password: demoInitialPassword(reader) } };
  }
} finally { await platform.dispose(); }

}

export async function seedAcmeOrganizationForBrowser(directory: string) {
  const platform = await getLocalProxy();
  try {
    const manifest = JSON.parse(readFileSync(process.env.ERPX_DATABASE_MANIFEST!, "utf8"));
    const state = JSON.parse(readFileSync(join(directory, "state.json"), "utf8"));
    assertAcmeBrowserSeedTarget(manifest, state, process.env.ERPX_BROWSER_SIMULATION_RUN_ID);
    if (platform.env.ERPX_ENVIRONMENT !== "local") throw new Error("LOCAL_DEVELOPMENT_ONLY");
    const db = platform.env.H001_DB;
    const existing = await db.prepare("SELECT id FROM organization_positions WHERE company_id=? AND active=1 AND is_ceo=1").bind(demoCompany).first();
    if (!existing) {
      const levels = await localSeedLevels(db, demoCompany, Date.now()), now = Date.now();
      // A dedicated fixture profile holds the existing normal owner login;
      // Mali and every original prepared worker retain their roles and rows.
      const person = "browser-acme-ceo-person", engagement = "browser-acme-ceo-engagement", position = "browser-acme-ceo-position";
      const owner = await platform.env.CONTROL_DB.prepare('SELECT id,name,email FROM "user" WHERE id=?').bind("usr_jesadakorn").first<{id:string;name:string;email:string}>();
      if (!owner) throw new Error("LOCAL_BROWSER_OWNER_ACCOUNT_REQUIRED");
      await db.batch([
        db.prepare("INSERT INTO organization_positions(id,company_id,title,level_id,is_ceo,roster_required,created_by,created_at,updated_at) VALUES(?,?,'Fictional Company CEO',?,1,0,'local-demo-seed',?,?)").bind(position,demoCompany,levels.get(1)!,now,now),
        db.prepare("INSERT INTO hr_people(id,user_id,company_id,legal_name,email,country_code,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'US','active','local-demo-seed',?,?)").bind(person,owner.id,demoCompany,owner.name,owner.email,now,now),
        db.prepare("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at,pay_group_id,level_id,position_id) VALUES(?,?,?,'bangkok','dept:bangkok:operations','employee','salaried','active','2026-01-05','Fictional Company CEO','USD','local-demo-seed',?,?,'paygroup-acme-co',?,?)").bind(engagement,person,demoCompany,now,now,levels.get(1)!,position),
        db.prepare("INSERT INTO employees(id,user_id,company_id,branch_id,department_id,person_id,display_name,status,created_at) VALUES(?,?,?,'bangkok','dept:bangkok:operations', ?,?,'active',?)").bind("browser-acme-ceo-employee",owner.id,demoCompany,person,owner.name,now),
      ]);
    }
  } catch (error) { await platform.dispose(); throw error; }
  const actors = await seedAcmeOrganizationWorkflow(platform, true, false, join(directory, "acme-organization"));
  if (!actors) throw new Error("LOCAL_BROWSER_ACME_ACTORS_REQUIRED");
  return actors;
}

// Deliberately separate from seed:local: expanding a development team is explicit.
if (import.meta.main) {
  const apply = process.argv.includes("--apply"), verifyOnly = process.argv.includes("--verify");
  if (process.argv.slice(2).some(arg => !["--apply", "--verify"].includes(arg)) || apply && verifyOnly) throw new Error("Use --apply, --verify, or no flags for a read-only preview.");
  const platform = await getLocalProxy();
  try {
  const manifest = JSON.parse(readFileSync(process.env.ERPX_DATABASE_MANIFEST!, "utf8")) as { databases: Record<string, string> };
  for (const binding of ["CONTROL_DB", "H001_DB", "H002_DB"]) {
    const url = new URL(manifest.databases[binding]);
    if (!["localhost", "127.0.0.1"].includes(url.hostname) || url.port !== "5432" || !url.pathname.startsWith("/erpx_dev_")) throw new Error("ACME_FIXTURES_REQUIRE_LOCAL_DEVELOPMENT_DATABASES_ON_5432");
  }
  if (platform.env.ERPX_ENVIRONMENT !== "local") throw new Error("LOCAL_DEVELOPMENT_ONLY");
  } catch (error) { await platform.dispose(); throw error; }
  await seedAcmeOrganizationWorkflow(platform, apply, verifyOnly);
}
