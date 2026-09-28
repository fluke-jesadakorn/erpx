import type { SqlDatabase, SqlStatement } from "../src/server/runtime/types";
import { businessRoles,type BusinessRole } from "../src/lib/business-roles";
import { seedWorker } from "./seed-roster";
import { getLocalProxy } from "./local/fixtures";
import type { RuntimeEnv } from "../src/server/runtime/env";
import { hashPassword } from "better-auth/crypto";
import { moduleRegistry } from "../src/data/module-registry";
import { project, type EventMessage, type JobEnv } from "../src/workers/jobs";
import { calculatePayrollAmounts } from "../src/lib/payroll-math";
import { readFile } from "node:fs/promises";

/**
 * Reconcile a useful, deterministic local dataset without deleting developer data.
 * The command talks to the configured development PostgreSQL and private files.
 */
type SeedEnv = Pick<RuntimeEnv, "ERPX_ENVIRONMENT" | "CONTROL_DB" | "H001_DB" | "H001_READ_DB" | "H001_FILES" | "H002_DB" | "H002_READ_DB" | "H002_FILES">;
type Db = SqlDatabase;

const now = Date.UTC(2026, 8, 6, 9, 0, 0);
const validUntil = Date.UTC(2027, 8, 6, 9, 0, 0);
const allModules = JSON.stringify(moduleRegistry.map((item) => item.id));
const encoder = new TextEncoder();
const devSuperuser = {
  id: "usr_jesadakorn",
  name: "Jesadakorn Kirtnu",
  email: "jesadakorn.kirtnu@gmail.com",
  password: "12345678",
} as const;
const devRoles: Array<BusinessRole | "holding_owner"> = [
  "holding_owner",
  ...(Object.keys(businessRoles) as BusinessRole[]),
];

const people = [
  ["usr_mali", "Mali Srisai", "mali@erpx.dev", "Mali-ERPX-2026!", "h001", "acme-co", "bangkok", "people_manager"],
  ["usr_nisa", "Nisa Chen", "nisa@erpx.dev", "Nisa-ERPX-2026!", "h001", "acme-co", "new-york", "finance_approver"],
  ["usr_ava", "Ava Williams", "ava@erpx.dev", "Ava-ERPX-2026!", "h001", "acme-co", "bangkok", "hr_admin"],
  ["usr_eli", "Eli Morgan", "eli@erpx.dev", "Eli-ERPX-2026!", "h001", "beacon-co", "london", "payroll_approver"],
  ["usr_zoe", "Zoe Laurent", "zoe@erpx.dev", "Zoe-ERPX-2026!", "h001", "beacon-co", "singapore", "treasury"],
  ["usr_noah", "Noah Patel", "noah@erpx.dev", "Noah-ERPX-2026!", "h001", "acme-co", "new-york", "employee"],
  ["usr_sam", "Sam Rivera", "sam@erpx.dev", "Sam-ERPX-2026!", "h002", "northstar-co", "rotterdam", "holding_owner"],
  ["usr_iris", "Iris Johnson", "iris@erpx.dev", "Iris-ERPX-2026!", "h002", "northstar-co", "toronto", "finance_preparer"],
  ["usr_luca", "Luca Rossi", "luca@erpx.dev", "Luca-ERPX-2026!", "h002", "northstar-co", "rotterdam", "payroll_preparer"],
  ["usr_mia", "Mia Okafor", "mia@erpx.dev", "Mia-ERPX-2026!", "h002", "northstar-co", "toronto", "people_manager"],
  ["usr_jun", "Jun Park", "jun@erpx.dev", "Jun-ERPX-2026!", "h002", "northstar-co", "rotterdam", "auditor"],
  ["usr_olivia", "Olivia Grant", "olivia@erpx.dev", "Olivia-ERPX-2026!", "h002", "northstar-co", "toronto", "employee"],
] as const;

const companies = [
  { holding: "h001", id: "acme-co", slug: "acme-co", name: "Acme Corporation", country: "US", branches: [["bangkok", "Bangkok Operations"], ["new-york", "New York Commercial"]] },
  { holding: "h001", id: "beacon-co", slug: "beacon-co", name: "Beacon Labs", country: "GB", branches: [["london", "London Studio"], ["singapore", "Singapore Delivery"]] },
  { holding: "h002", id: "northstar-co", slug: "northstar-co", name: "Northstar Supply", country: "NL", branches: [["rotterdam", "Rotterdam Logistics"], ["toronto", "Toronto Services"]] },
] as const;

const chunks = <T>(values: T[], size: number) => { const result: T[][] = []; for (let i = 0; i < values.length; i += size) result.push(values.slice(i, i + size)); return result; };
const text = (value: string) => encoder.encode(value);
const holdingActor = (holding: string) =>
  people.find((person) => person[4] === holding)?.[0] ?? "usr_mali";
const holdingReviewer = (holding: string) =>
  people.find(
    (person) =>
      person[4] === holding &&
      person[0] !== holdingActor(holding) &&
      ["people_manager", "payroll_approver", "finance_approver", "treasury"].includes(person[7]),
  )?.[0] ?? holdingActor(holding);

async function digest(value: Uint8Array | string) {
  const bytes = typeof value === "string" ? text(value) : Uint8Array.from(value);
  const result = await crypto.subtle.digest("SHA-256", bytes.buffer as ArrayBuffer);
  return Array.from(new Uint8Array(result), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function batch(db: Db, statements: SqlStatement[]) {
  for (const part of chunks(statements, 40)) {
    await db.batch(part);
  }
}

function orgStatements(db: Db, holding: string) {
  const statements: SqlStatement[] = [];
  const heldCompanies = companies.filter((company) => company.holding === holding);
  for (const company of heldCompanies) {
    statements.push(db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,status,hierarchy_revision) VALUES(?,?,?,?,?,?,?,1) ON CONFLICT DO NOTHING")
      .bind(company.id, "company", company.slug, company.name, null, company.id, "active"));
    statements.push(db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,status,hierarchy_revision) VALUES(?,?,?,?,?,?,?,1) ON CONFLICT DO NOTHING")
      .bind(`${company.id}-hq`, "headquarters", "headquarters", `${company.name} Headquarters`, company.id, company.id, "active"));
    for (const [branchId, branchName] of company.branches) {
      statements.push(db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES(?,?,?,?,?,?,?,?,1) ON CONFLICT DO NOTHING")
        .bind(branchId, "branch", branchId, branchName, company.id, company.id, branchId, "active"));
      for (const [slug, label] of [["operations", "Operations"], ["finance", "Finance"], ["people", "People"]] as const) {
        statements.push(db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES(?,?,?,?,?,?,?,?,1) ON CONFLICT DO NOTHING")
          .bind(`dept:${branchId}:${slug}`, "department", slug, `${branchName} ${label}`, branchId, company.id, branchId, "active"));
      }
    }
  }
  return statements;
}

async function seedControl(db: Db) {
  const statements: SqlStatement[] = [];
  for (const [id, name, email, password] of people) {
    const hash = await hashPassword(password);
    statements.push(db.prepare("INSERT INTO \"user\"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,email=excluded.email,emailVerified=excluded.emailVerified,updatedAt=excluded.updatedAt").bind(id, name, email, 1, now, now));
    // Demo credentials are deterministic so a reset can always restore a usable
    // account. Only the fixture's stable acc_<usr_*> rows are updated; real
    // application accounts are never selected by this local seed.
    statements.push(db.prepare("INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET password=excluded.password,updatedAt=excluded.updatedAt").bind(`acc_${id}`, id, "credential", "local:credential", id, hash, now, now));
    statements.push(db.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,?,?) ON CONFLICT DO NOTHING").bind(`pri_${id}`, id, name, "active", now));
    if (id === "usr_mali") statements.push(db.prepare("INSERT INTO platform_assignments(id,user_id,role,modules_json,valid_from,valid_until,version) VALUES(?,?,?,?,?,?,1) ON CONFLICT DO NOTHING").bind(`platform_${id}`, id, "platform_admin", allModules, now - 1_000, validUntil));
  }
  const devHash = await hashPassword(devSuperuser.password);
  statements.push(db.prepare("INSERT INTO \"user\"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,email=excluded.email,emailVerified=excluded.emailVerified,updatedAt=excluded.updatedAt").bind(devSuperuser.id, devSuperuser.name, devSuperuser.email, 1, now, now));
  statements.push(db.prepare("INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET password=excluded.password,updatedAt=excluded.updatedAt").bind(`acc_${devSuperuser.id}`, devSuperuser.id, "credential", "local:credential", devSuperuser.id, devHash, now, now));
  statements.push(db.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,?,?) ON CONFLICT DO NOTHING").bind(`pri_${devSuperuser.id}`, devSuperuser.id, devSuperuser.name, "active", now));
  statements.push(db.prepare("INSERT INTO platform_assignments(id,user_id,role,modules_json,valid_from,valid_until,version) VALUES(?,?,?,?,?,?,1) ON CONFLICT DO NOTHING").bind(`platform_${devSuperuser.id}`, devSuperuser.id, "platform_admin", allModules, now - 1_000, validUntil));
  statements.push(db.prepare("UPDATE platform_assignments SET role=?,modules_json=?,valid_from=?,valid_until=?,revoked_at=NULL WHERE id=? AND user_id=?").bind("platform_admin", allModules, now - 1_000, validUntil, `platform_${devSuperuser.id}`, devSuperuser.id));
  await batch(db, statements);
}

function assignmentStatements(db: Db, holding: string) {
  const statements: SqlStatement[] = [];
  const heldPeople = people.filter((person) => person[4] === holding);
  for (const person of heldPeople) {
    const userId = person[0];
    const company = person[5];
    const branch = person[6];
    const role = person[7];
    const scope = role === "holding_owner" || role === "people_manager" ? "holding" : "branch";
    const scopeNode = scope === "branch" ? branch : null;
    statements.push(db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,version,scope_kind,scope_node_id) VALUES(?,?,?,?,?,?,?,?,1,?,?) ON CONFLICT DO NOTHING")
      .bind(`seed_${userId}`, userId, role, role === "holding_owner" ? allModules : JSON.stringify(businessRoles[role as BusinessRole]), company, scope === "branch" ? branch : null, now - 1_000, validUntil, scope, scopeNode));
  }
  // Northstar's seeded payroll run is approved by the holding owner. Give a
  // separate finance approver an independent holding-scoped assignment so the
  // demo can complete the payroll → GL handoff without relying on production
  // data or a self-approval shortcut.
  if (holding === "h002") {
    const role: BusinessRole = "finance_approver";
    const userId = "usr_iris";
    const companyId = "northstar-co";
    const assignmentId = "seed_usr_iris_finance_approver";
    const modules = JSON.stringify(businessRoles[role]);
    statements.push(db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,version,scope_kind,scope_node_id) VALUES(?,?,?,?,?,?,?,?,1,'holding',NULL) ON CONFLICT DO NOTHING")
      .bind(assignmentId, userId, role, modules, companyId, null, now - 1_000, validUntil));
    statements.push(db.prepare("UPDATE assignments SET role=?,modules_json=?,company_id=?,branch_id=NULL,valid_from=?,valid_until=?,revoked_at=NULL,scope_kind='holding',scope_node_id=NULL WHERE id=? AND user_id=?")
      .bind(role, modules, companyId, now - 1_000, validUntil, assignmentId, userId));
    const treasuryRole: BusinessRole = "treasury";
    const treasuryUserId = "usr_jun";
    const treasuryAssignmentId = "seed_usr_jun_treasury";
    const treasuryModules = JSON.stringify(businessRoles[treasuryRole]);
    statements.push(db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,version,scope_kind,scope_node_id) VALUES(?,?,?,?,?,?,?,?,1,'holding',NULL) ON CONFLICT DO NOTHING")
      .bind(treasuryAssignmentId, treasuryUserId, treasuryRole, treasuryModules, companyId, null, now - 1_000, validUntil));
    statements.push(db.prepare("UPDATE assignments SET role=?,modules_json=?,company_id=?,branch_id=NULL,valid_from=?,valid_until=?,revoked_at=NULL,scope_kind='holding',scope_node_id=NULL WHERE id=? AND user_id=?")
      .bind(treasuryRole, treasuryModules, companyId, now - 1_000, validUntil, treasuryAssignmentId, treasuryUserId));
  }
  for (const company of companies.filter((item) => item.holding === holding)) {
    for (const role of devRoles) {
      const assignmentId = `dev_${devSuperuser.id}_${company.id}_${role}`;
      const modules = role === "holding_owner" ? allModules : JSON.stringify(businessRoles[role]);
      statements.push(db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,version,scope_kind,scope_node_id) VALUES(?,?,?,?,?,?,?,?,1,?,?) ON CONFLICT DO NOTHING")
        .bind(assignmentId, devSuperuser.id, role, modules, company.id, null, now - 1_000, validUntil, "holding", null));
      statements.push(db.prepare("UPDATE assignments SET role=?,modules_json=?,company_id=?,branch_id=NULL,valid_from=?,valid_until=?,revoked_at=NULL,scope_kind='holding',scope_node_id=NULL WHERE id=? AND user_id=?").bind(role, modules, company.id, now - 1_000, validUntil, assignmentId, devSuperuser.id));
    }
  }
  return statements;
}

function employeeStatements(db: Db, holding: string) {
  const statements: SqlStatement[] = [];
  let ordinal=0;
  for(const company of companies.filter(c=>c.holding===holding)) for(let index=0;index<20;index++) {
    ordinal++;
    const worker=seedWorker(company.id,index);
    const id=company.id==="acme-co"&&index===0?"emp_mali":`seed-emp-${String(ordinal).padStart(3,"0")}`;
    statements.push(db.prepare("INSERT INTO employees(id,user_id,company_id,branch_id,display_name,status,created_at) VALUES(?,?,?,?,?,'active',?) ON CONFLICT DO NOTHING").bind(id,worker.userId,company.id,company.branches[index%2][0],worker.name,now));
  }
  return statements;
}

function peoplePayrollStatements(db: Db, holding: string) {
  const statements: SqlStatement[] = [];
  const heldCompanies = companies.filter((company) => company.holding === holding);
  const actor = holdingActor(holding);
  const reviewer = holdingReviewer(holding);
  for (const company of heldCompanies) {
    const currency = company.id === "beacon-co" ? "GBP" : company.id === "northstar-co" ? "EUR" : "USD";
    const pack = `pack-${company.id}`;
    const group = `paygroup-${company.id}`;
    statements.push(db.prepare("INSERT INTO hr_country_packs(id,country_code,version,status,effective_from,rules_json,export_formats_json,approval_evidence,created_by,validated_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
      .bind(pack, company.country, "2026.1", "testing", "2026-01-01", JSON.stringify({ currency, taxYear: 2026, withholding: "synthetic demo rules" }), JSON.stringify(["generic-csv", "iso20022"]), "Development examples; statutory validation pending", actor, null, now, now));
    statements.push(db.prepare("INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,calendar_json,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
      .bind(group, company.id, `${company.name} Monthly Payroll`, company.country, currency, "monthly", company.country === "US" ? "America/New_York" : "Europe/London", JSON.stringify({ payDay: 25 }), "active", actor, now, now));
    statements.push(db.prepare("INSERT INTO finance_tax_codes(id,company_id,code,name,rate_micros,recoverable_micros,effective_from,status) VALUES(?,?,?,?,?,?,?,'active') ON CONFLICT DO NOTHING")
      .bind(`tax-${company.id}`, company.id, "VAT20", "Standard sales tax", 200000, 1000000, "2026-01-01"));
    for (let index = 0; index < 20; index += 1) {
      const person = `hr-${company.id}-${String(index + 1).padStart(3, "0")}`;
      const engagement = `eng-${company.id}-${String(index + 1).padStart(3, "0")}`;
      const branch = company.branches[index % company.branches.length][0];
      const country = company.country;
      const worker=seedWorker(company.id,index);
      const name=worker.name;
      const position=`seed-position-${company.id}-${String(index+1).padStart(3,"0")}`;
      statements.push(db.prepare("INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,roster_required,created_by,created_at,updated_at) VALUES(?,?,?,?,?, ?,0,'local-demo-seed',?,?) ON CONFLICT DO NOTHING")
        .bind(position,company.id,branch,`dept:${branch}:operations`,worker.jobTitle,`org-level-${company.id}-6`,now,now));
      statements.push(db.prepare("INSERT INTO hr_people(id,user_id,company_id,legal_name,preferred_name,email,phone,country_code,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?, 'active',?,?,?) ON CONFLICT DO NOTHING")
        .bind(person, null, company.id, name, null, worker.email, null, country, actor, now - index * 86_400_000, now));
      statements.push(db.prepare("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,manager_person_id,job_title,currency,created_by,created_at,updated_at,pay_group_id) VALUES(?,?,?,?,?,?,?,'active',?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
        .bind(engagement, person, company.id, branch, `dept:${branch}:operations`, index % 5 === 0 ? "contractor" : "employee", index % 5 === 0 ? "contractor" : "salaried", "2025-01-06", null, worker.jobTitle, currency, actor, now - index * 86_400_000, now, group));
      statements.push(db.prepare("UPDATE hr_engagements SET position_id=? WHERE id=? AND company_id=? AND position_id IS NULL")
        .bind(position,engagement,company.id));
      statements.push(db.prepare("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,hourly_rate_minor,expected_hours,currency,components_json,status,approved_by,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,'approved',?,?,?,?) ON CONFLICT DO NOTHING")
        .bind(`comp-${company.id}-${String(index + 1).padStart(3, "0")}`, engagement, "2026-01-01", "monthly", (index % 5 === 0 ? 3_200 : 5_800 + index * 175) * 100, 0, 160, currency, JSON.stringify([{ code: "health", label: "Health contribution", kind: "employer", amount: "180.00" }, { code: "tax", label: "Withholding", kind: "deduction", amount: index % 3 === 0 ? "600.00" : "820.00" }]), actor, actor, now, now));
      statements.push(db.prepare("INSERT INTO hr_leave_balances(id,engagement_id,leave_code,year,opening_days,accrued_days,used_days,carried_days) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
        .bind(`leavebal-${company.id}-${String(index + 1).padStart(3, "0")}`, engagement, "annual", 2026, 5, 15, index % 4, 2));
      for (let day = 0; day < 2; day += 1) statements.push(db.prepare("INSERT INTO hr_time_entries(id,engagement_id,company_id,work_date,regular_hours,overtime_hours,status,note,created_at,updated_at) VALUES(?,?,?,?,?,?, 'approved',?,?,?) ON CONFLICT DO NOTHING")
        .bind(`time-${company.id}-${index}-${day}`, engagement, company.id, `2026-09-${String(2 + day).padStart(2, "0")}`, 8, index % 7 === 0 ? 2 : 0, "Month-end close support", now, now));
    }
    for (let runIndex = 0; runIndex < 2; runIndex += 1) {
      const runId = `payrun-${company.id}-2026-${runIndex === 0 ? "07" : "08"}`;
      const start = runIndex === 0 ? "2026-07-01" : "2026-08-01";
      const end = runIndex === 0 ? "2026-07-31" : "2026-08-31";
      const status = runIndex === 0 ? "approved" : company.id === "beacon-co" ? "draft" : "preview";
      const payrollItems = Array.from({ length: 20 }, (_, index) => {
        const employmentType = index % 5 === 0 ? "contractor" : "salaried";
        const baseMinor = (index % 5 === 0 ? 3_200 : 5_800 + index * 175) * 100;
        return calculatePayrollAmounts({
          employmentType,
          baseMinor,
          hourlyMinor: 0,
          regularHours: 160,
          overtimeHours: 0,
          components: [
            { code: "health", label: "Health contribution", kind: "employer", amount: "180.00" },
            { code: "tax", label: "Withholding", kind: "deduction", amount: index % 3 === 0 ? "600.00" : "820.00" },
          ],
        });
      });
      const total = payrollItems.reduce((sum, item) => sum + item.gross, 0);
      const deductionsTotal = payrollItems.reduce((sum, item) => sum + item.deductions, 0);
      const employerTotal = payrollItems.reduce((sum, item) => sum + item.employer, 0);
      statements.push(db.prepare("INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,country_pack_id,period_start,period_end,pay_date,currency,mode,status,revision,totals_json,input_digest,version,created_by,approved_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,1,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
        .bind(runId, company.id, group, pack, start, end, runIndex === 0 ? "2026-07-25" : "2026-08-25", currency, "preview", status, JSON.stringify({ gross: total, deductions: deductionsTotal, employer: employerTotal, net: total - deductionsTotal, headcount: payrollItems.length }), `seed-digest-${runId}`, status === "approved" ? 2 : 1, actor, status === "approved" ? reviewer : null, now, now));
      statements.push(db.prepare("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
        .bind(`audit-${runId}-create`, `seed-command-${runId}`, actor, `seed_${actor}`, "people.payroll.create", "hr_payroll_run", runId, 1, "seed payroll run", now));
      if (status === "approved") statements.push(db.prepare("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
        .bind(`audit-${runId}-approve`, `seed-command-${runId}-approve`, reviewer, `seed_${reviewer}`, "people.payroll.approve", "hr_payroll_run", runId, 2, "independent seed payroll approval", now));
      for (let index = 0; index < 20; index += 1) {
        const engagement = `eng-${company.id}-${String(index + 1).padStart(3, "0")}`;
        const calculated = payrollItems[index];
        statements.push(db.prepare("INSERT INTO hr_payroll_items(id,run_id,engagement_id,worker_type,input_snapshot_json,result_json,gross_minor,deductions_minor,employer_cost_minor,net_minor,currency,status,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,'calculated',?) ON CONFLICT DO NOTHING")
          .bind(`payitem-${company.id}-${runIndex}-${index}`, runId, engagement, index % 5 === 0 ? "contractor" : "employee", JSON.stringify({ period: [start, end], engagementId: engagement }), JSON.stringify({ grossMinor: calculated.gross, deductionsMinor: calculated.deductions, employerCostMinor: calculated.employer, netMinor: calculated.net, applied: calculated.applied }), calculated.gross, calculated.deductions, calculated.employer, calculated.net, currency, now));
      }
    }
  }
  return statements;
}

function leaveStatements(db: Db, holding: string) {
  const statements: SqlStatement[] = [];
  const held = companies.filter((company) => company.holding === holding);
  const actor = holdingActor(holding);
  const reviewer = holdingReviewer(holding);
  let employeeNumber = 0;
  let sequence = 0;
  for (const company of held) for (let index = 0; index < 20; index += 1) {
    sequence += 1;
    employeeNumber += 1;
    const branch = company.branches[index % company.branches.length][0];
    const employee = company.id === "acme-co" && index === 0 ? "emp_mali" : `seed-emp-${String(employeeNumber).padStart(3, "0")}`;
    const id = `seed-${holding}-leave-${String(sequence).padStart(3, "0")}`;
    const state = ["draft", "pending_approval", "approved", "rejected"][sequence % 4];
    const creator = actor;
    statements.push(db.prepare("INSERT INTO leave_requests(id,number,employee_id,company_id,branch_id,start_date,days,handover_note,state,version,created_by,prepared_by,decided_by,decision_note,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
      .bind(id, `LV-${holding.toUpperCase()}-2026-${String(sequence).padStart(3, "0")}`, employee, company.id, branch, `2026-09-${String(7 + (sequence % 20)).padStart(2, "0")}`, 1 + (sequence % 4), `Handover for ${company.name} ${branch} is assigned to the operations desk.`, state, state === "approved" || state === "rejected" ? 2 : 1, creator, state === "pending_approval" || state === "approved" ? creator : null, state === "approved" || state === "rejected" ? reviewer : null, state === "rejected" ? "Please choose a handover owner before resubmitting." : null, now - sequence * 3_600_000, now - sequence * 1_800_000));
    statements.push(db.prepare("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
      .bind(`audit-${id}-create`, `seed-command-${id}`, creator, `seed_${creator}`, "leave.create", "leave_request", id, 1, "seed leave request", now - sequence * 3_600_000));
    if (state === "approved" || state === "rejected") statements.push(db.prepare("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
      .bind(`audit-${id}-decision`, `seed-command-${id}-decision`, reviewer, `seed_${reviewer}`, `leave.${state}`, "leave_request", id, 2, "independent seed leave decision", now - sequence * 1_800_000));
  }
  return statements;
}

function financeStatements(db: Db, holding: string) {
  const statements: SqlStatement[] = [];
  const held = companies.filter((company) => company.holding === holding);
  const actor = holdingActor(holding);
  const reviewer = holdingReviewer(holding);
  for (const company of held) {
    const currency = company.id === "beacon-co" ? "GBP" : company.id === "northstar-co" ? "EUR" : "USD";
    const chart = `chart-${company.id}`;
    // The original local seed already owns Acme's September period. Reuse that
    // deterministic key so this richer reconciliation can reference it safely.
    const period = company.id === "acme-co" ? "period_2026_09" : `period-${company.id}-2026-09`;
    const accounts = [
      ["1100", "Operating cash", "asset", 1, "bank"], ["1200", "Trade receivables", "asset", 1, "receivable"], ["1300", "Inventory", "asset", 1, "none"], ["1500", "Equipment", "asset", 0, "none"],
      ["2100", "Trade payables", "liability", 1, "payable"], ["2200", "Tax payable", "liability", 1, "none"], ["3000", "Retained earnings", "equity", 0, "none"], ["4000", "Product revenue", "revenue", 0, "none"], ["5000", "Cost of sales", "expense", 0, "none"], ["5100", "Operating expenses", "expense", 0, "none"],
    ] as const;
    statements.push(db.prepare("INSERT INTO finance_charts(id,company_id,name,base_currency,accounting_basis,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING").bind(chart, company.id, `${company.name} IFRS`, "USD", "IFRS", actor, now, now));
    statements.push(db.prepare("INSERT INTO finance_periods(id,company_id,name,start_date,end_date,state,created_at) VALUES(?,?,?,?,?,'open',?) ON CONFLICT DO NOTHING").bind(period, company.id, "September 2026", "2026-09-01", "2026-09-30", now));
    for (const [code, name, category, control, controlKind] of accounts) statements.push(db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,is_control,control_kind,currency,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET is_control=excluded.is_control,control_kind=excluded.control_kind,updated_at=excluded.updated_at").bind(`acct-${company.id}-${code}`, chart, company.id, code, name, category, control, controlKind, currency, now, now));
    const cash = `acct-${company.id}-1100`, revenue = `acct-${company.id}-4000`, expense = `acct-${company.id}-5100`, ar = `acct-${company.id}-1200`, ap = `acct-${company.id}-2100`;
    const partyCustomer = company.id === "acme-co" ? "party_customer_demo" : `party-${company.id}-customer`;
    const partySupplier = company.id === "acme-co" ? "party_supplier_demo" : `party-${company.id}-supplier`;
    statements.push(db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING").bind(partyCustomer, company.id, "customer", company.id === "acme-co" ? "CUS-001" : `CUS-${company.id.toUpperCase()}`, `${company.name} Retail Customer`, currency, 30, 25000000, now, now));
    statements.push(db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING").bind(partySupplier, company.id, "supplier", company.id === "acme-co" ? "SUP-001" : `SUP-${company.id.toUpperCase()}`, `${company.name} Strategic Supplier`, currency, 45, 0, now, now));
    for (let index = 0; index < 50; index += 1) {
      const journal = `journal-${company.id}-${String(index + 1).padStart(3, "0")}`;
      const amount = (index + 1) * 12500;
      const debit = index % 2 === 0 ? expense : ar;
      const credit = index % 2 === 0 ? cash : revenue;
      const status = index % 17 === 0 ? "reversed" : index % 11 === 0 ? "in_review" : "posted";
      statements.push(db.prepare("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,is_opening,version,created_by,posted_by,posted_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,0,1,?,?,?,?,?) ON CONFLICT DO NOTHING")
        .bind(journal, company.id, chart, period, `JE-${company.id.toUpperCase()}-${String(index + 1).padStart(4, "0")}`, `2026-${String(7 + (index % 3)).padStart(2, "0")}-${String(1 + (index % 27)).padStart(2, "0")}`, index % 10 === 0 ? "EUR" : currency, index % 10 === 0 ? 1_080_000 : 1_000_000, "seed", journal, `Seed operating entry ${index + 1}`, status, actor, status === "posted" || status === "reversed" ? reviewer : null, status === "posted" || status === "reversed" ? now : null, now, now));
      statements.push(db.prepare("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
        .bind(`audit-${journal}-create`, `seed-command-${journal}`, actor, `seed_${actor}`, "finance.journal.create", "finance_journal", journal, 1, "seed balanced journal", now));
      if (status === "posted" || status === "reversed") statements.push(db.prepare("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
        .bind(`audit-${journal}-post`, `seed-command-${journal}-post`, reviewer, `seed_${reviewer}`, `finance.journal.${status}`, "finance_journal", journal, 1, "independent seed journal review", now));
      statements.push(db.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING").bind(`${journal}-dr`, journal, debit, "Operating transaction", amount, 0, amount, 0, JSON.stringify({ branch: company.branches[index % company.branches.length][0] }), 0));
      statements.push(db.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING").bind(`${journal}-cr`, journal, credit, "Operating transaction", 0, amount, 0, amount, JSON.stringify({ branch: company.branches[index % company.branches.length][0] }), 1));
    }
    for (let index = 0; index < 30; index += 1) {
      const kind = index % 2 === 0 ? "receivable" : "payable";
      const party = kind === "receivable" ? partyCustomer : partySupplier;
      const subtotal = (index + 4) * 10_000;
      const tax = Math.round(subtotal * 0.2);
      const status = index % 6 === 0 ? "paid" : index % 4 === 0 ? "partially_paid" : index % 3 === 0 ? "in_review" : "posted";
      const invoice = `invoice-${company.id}-${String(index + 1).padStart(3, "0")}`;
      statements.push(db.prepare("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,tax_code,debit_account_id,credit_account_id,source_type,source_id,status,created_by,approved_by,posted_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
        .bind(invoice, company.id, party, kind, `${kind === "receivable" ? "INV" : "BILL"}-${company.id.toUpperCase()}-${String(index + 1).padStart(4, "0")}`, `2026-${String(7 + index % 3).padStart(2, "0")}-15`, `2026-${String(8 + index % 2).padStart(2, "0")}-15`, currency, subtotal, tax, subtotal + tax, status === "paid" ? 0 : status === "partially_paid" ? Math.round((subtotal + tax) * 0.4) : subtotal + tax, "VAT20", kind === "receivable" ? ar : expense, kind === "receivable" ? revenue : ap, "seed", invoice, status, actor, status === "posted" || status === "paid" || status === "partially_paid" ? reviewer : null, status === "posted" || status === "paid" || status === "partially_paid" ? now : null, now, now));
    }
    for (let index = 0; index < 15; index += 1) {
      const kind = index % 2 === 0 ? "receipt" : "payment";
      const party = kind === "receipt" ? partyCustomer : partySupplier;
      const payment = `payment-${company.id}-${String(index + 1).padStart(3, "0")}`;
      const amount = (index + 3) * 12_000;
      const status = index % 5 === 0 ? "draft" : index % 4 === 0 ? "in_review" : index % 3 === 0 ? "released" : "settled";
      const allocationAmount = status === "settled" ? amount : status === "released" ? Math.round(amount * 0.5) : 0;
      statements.push(db.prepare("INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,status,version,created_by,approved_by,released_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,1,?,?,?,?,?) ON CONFLICT DO NOTHING")
        .bind(payment, company.id, party, kind, `PAY-${company.id.toUpperCase()}-${String(index + 1).padStart(4, "0")}`, `2026-09-${String(2 + index).padStart(2, "0")}`, currency, amount, amount - allocationAmount, `${company.id}-operating`, status, actor, status === "settled" || status === "released" ? reviewer : null, status === "settled" || status === "released" ? reviewer : null, now, now));
      const invoiceIndex = (index * 2) % 30;
      if (allocationAmount > 0) statements.push(db.prepare("INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,created_by,created_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT DO NOTHING").bind(`allocation-${company.id}-${String(index + 1).padStart(3, "0")}`, company.id, payment, `invoice-${company.id}-${String(invoiceIndex + 1).padStart(3, "0")}`, allocationAmount, actor, now));
    }
    for (let index = 0; index < 3; index += 1) {
      const statement = `statement-${company.id}-2026-${index + 7}`;
      const opening = 5000000 + index * 100000;
      const net = Array.from({ length: 20 }, (_, row) => row % 2 ? -(row + 1) * 10000 : (row + 1) * 15000).reduce((sum, amount) => sum + amount, 0);
      statements.push(db.prepare("INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,status,source_hash,version,imported_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,1,?,?) ON CONFLICT DO NOTHING").bind(statement, company.id, `${company.id}-operating`, `STMT-${company.id}-${index + 1}`, `2026-${String(7 + index).padStart(2, "0")}-28`, opening, opening + net, "matching", `seed-bank-${company.id}-${index}`, actor, now));
      for (let row = 0; row < 20; row += 1) statements.push(db.prepare("INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,description,amount_minor,currency,match_status,matched_type,matched_id) VALUES(?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING").bind(`banktx-${company.id}-${index}-${row}`, statement, company.id, `2026-${String(7 + index).padStart(2, "0")}-${String(1 + row).padStart(2, "0")}`, `EXT-${company.id}-${index}-${row}`, row % 2 ? "Supplier settlement" : "Customer receipt", row % 2 ? -(row + 1) * 10000 : (row + 1) * 15000, currency, "unmatched", null, null));
    }
    statements.push(db.prepare("INSERT INTO finance_assets(id,company_id,asset_number,name,account_id,depreciation_account_id,currency,cost_minor,residual_minor,useful_life_months,in_service_date,status) VALUES(?,?,?,?,?,?,?,?,?,?,?,'in_service') ON CONFLICT DO NOTHING").bind(`asset-${company.id}-001`, company.id, `FA-${company.id.toUpperCase()}-001`, `${company.name} delivery equipment`, `acct-${company.id}-1500`, `acct-${company.id}-5100`, currency, 4500000, 250000, 60, "2025-02-01"));
    statements.push(db.prepare("INSERT INTO finance_budgets(id,company_id,name,fiscal_year,status,currency,created_by,created_at,updated_at) VALUES(?,?,?,?, 'approved',?,?,?,?) ON CONFLICT DO NOTHING").bind(`budget-${company.id}-2026`, company.id, "FY2026 operating plan", 2026, currency, actor, now, now));
    for (const code of ["5000", "5100", "4000"]) statements.push(db.prepare("INSERT INTO finance_budget_lines(id,budget_id,account_id,period,amount_minor,dimension_json) VALUES(?,?,?,?,?,?) ON CONFLICT DO NOTHING").bind(`budgetline-${company.id}-${code}`, `budget-${company.id}-2026`, `acct-${company.id}-${code}`, "2026-09", 2500000, JSON.stringify({ branch: company.branches[0][0] })));
    statements.push(db.prepare("INSERT INTO finance_expenses(id,company_id,employee_id,expense_date,merchant,currency,amount_minor,tax_minor,category_account_id,status,evidence_json,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,'approved',?,?,?,?) ON CONFLICT DO NOTHING").bind(`expense-${company.id}-001`, company.id, `hr-${company.id}-001`, "2026-09-03", "Atlas Travel", currency, 185000, 37000, `acct-${company.id}-5100`, JSON.stringify([{ receipt: "r2://expense-receipt-001.pdf" }]), actor, now, now));
    statements.push(db.prepare("INSERT INTO finance_accrual_schedules(id,company_id,description,debit_account_id,credit_account_id,currency,total_minor,start_date,end_date,next_run_date,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?, 'active',?,?,?) ON CONFLICT DO NOTHING").bind(`accrual-${company.id}-001`, company.id, "Annual software subscription", expense, ap, currency, 1200000, "2026-07-01", "2026-12-31", "2026-10-01", actor, now, now));
    const contract = `contract-${company.id}-001`;
    statements.push(db.prepare("INSERT INTO finance_revenue_contracts(id,company_id,customer_id,contract_number,currency,transaction_price_minor,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,'active',?,?,?) ON CONFLICT DO NOTHING").bind(contract, company.id, partyCustomer, `CON-${company.id.toUpperCase()}-001`, currency, 3600000, actor, now, now));
    statements.push(db.prepare("INSERT INTO finance_revenue_obligations(id,contract_id,description,allocated_minor,recognition_method,start_date,end_date,revenue_account_id,deferred_account_id,status) VALUES(?,?,?,?,?,?,?,?,?,'active') ON CONFLICT DO NOTHING").bind(`obligation-${company.id}-001`, contract, "Managed service delivery", 3600000, "over_time", "2026-07-01", "2026-12-31", revenue, ap));
    statements.push(db.prepare("INSERT INTO finance_leases(id,company_id,lease_number,counterparty,currency,commencement_date,end_date,payment_frequency,payment_minor,discount_rate_micros,right_of_use_minor,liability_minor,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,'active',?,?,?) ON CONFLICT DO NOTHING").bind(`lease-${company.id}-001`, company.id, `LEASE-${company.id.toUpperCase()}-001`, "Global Workspace Partners", currency, "2025-01-01", "2027-12-31", "monthly", 900000, 45000, 28000000, 25000000, actor, now, now));
    statements.push(db.prepare("INSERT INTO finance_loans(id,company_id,loan_number,lender,currency,principal_minor,annual_rate_micros,commencement_date,maturity_date,status,outstanding_minor,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,'active',?,?,?,?) ON CONFLICT DO NOTHING").bind(`loan-${company.id}-001`, company.id, `LOAN-${company.id.toUpperCase()}-001`, "Union Commercial Bank", currency, 15000000, 65000, "2026-01-15", "2029-01-15", 12000000, actor, now, now));
  }
  return statements;
}

function socialStatements(db: Db, holding: string) {
  const statements: SqlStatement[] = [];
  const held = companies.filter((company) => company.holding === holding);
  const heldPeople = people.filter((person) => person[4] === holding);
  const actor = holdingActor(holding);
  for (let index = 0; index < 25; index += 1) {
    const company = held[index % held.length];
    const branch = company.branches[index % company.branches.length][0];
    const author = actor;
    const post = `seed-${holding}-post-${String(index + 1).padStart(3, "0")}`;
    const tag = ["delivery", "customer-success", "month-end", "safety", "teamwork"][index % 5];
    statements.push(db.prepare("INSERT INTO social_posts(id,kind,author_id,author_name,company_id,branch_id,body,state,version,created_at,updated_at) VALUES(?,?,?,?,?,?,?,'published',1,?,?) ON CONFLICT DO NOTHING")
      .bind(post, index % 4 === 0 ? "record" : "team", author, people.find((person) => person[0] === author)?.[1] ?? "Demo User", company.id, branch, `${company.name} ${branch} completed milestone ${index + 1}. The team captured the handover and next action for a clean customer experience. #${tag}`, now - index * 2_700_000, now - index * 2_700_000));
    statements.push(db.prepare("INSERT INTO social_topics(tag,created_by,created_at) VALUES(?,?,?) ON CONFLICT DO NOTHING").bind(tag, author, now));
    statements.push(db.prepare("INSERT INTO social_post_topics(post_id,tag) VALUES(?,?) ON CONFLICT DO NOTHING").bind(post, tag));
    const commenter = heldPeople[(index + 1) % heldPeople.length];
    const reactor = heldPeople[(index + 2) % heldPeople.length];
    const follower = heldPeople[(index + 3) % heldPeople.length];
    const mention = heldPeople[(index + 4) % heldPeople.length];
    statements.push(db.prepare("INSERT INTO social_comments(id,post_id,author_id,author_name,body,state,created_at,updated_at) VALUES(?,?,?,?,?,'published',?,?) ON CONFLICT DO NOTHING").bind(`${post}-comment`, post, commenter[0], commenter[1], "Thanks for documenting the handoff. I have picked up the next action.", now - index * 2_700_000 + 60_000, now - index * 2_700_000 + 60_000));
    statements.push(db.prepare("INSERT INTO social_reactions(post_id,principal_id,kind,created_at) VALUES(?,?, 'appreciate',?) ON CONFLICT DO NOTHING").bind(post, reactor[0], now));
    statements.push(db.prepare("INSERT INTO social_follows(principal_id,target_type,target_id,created_at) VALUES(?,?,?,?) ON CONFLICT DO NOTHING").bind(follower[0], "post", post, now));
    if (index < 8) statements.push(db.prepare("INSERT INTO social_mentions(owner_type,owner_id,target_kind,target_id,target_label,created_by,created_at) VALUES('post',?,'person',?,?,?,?) ON CONFLICT DO NOTHING").bind(post, mention[0], mention[1], author, now));
  }
  for (let index = 0; index < 10; index += 1) {
    const company = held[index % held.length]; const branch = company.branches[index % company.branches.length][0]; const author = heldPeople[(index + 2) % heldPeople.length];
    statements.push(db.prepare("INSERT INTO social_stories(id,author_id,author_name,company_id,branch_id,title,body,theme,state,version,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,'published',1,?,?) ON CONFLICT DO NOTHING").bind(`seed-${holding}-story-${String(index + 1).padStart(3, "0")}`, author[0], author[1], company.id, branch, `${company.name} in motion`, `A field note from ${branch}: the team closed the loop on customer, finance, and delivery work.`, ["sage", "sunset", "sky", "plum"][index % 4], now - index * 3_600_000, now - index * 3_600_000));
  }
  return statements;
}

async function seedFiles(env: SeedEnv, holding: string, db: Db) {
  const files = holding === "h001" ? env.H001_FILES : env.H002_FILES;
  const held = companies.filter((company) => company.holding === holding);
  const actor = holdingActor(holding);
  const reviewer = holdingReviewer(holding);
  const statements: SqlStatement[] = [];
  // Do not synthesize legacy Leave uploads here. The old seed used an ambiguous
  // free-form purpose and alternated PDF/CSV handover summaries. Those
  // bytes cannot truthfully be classified as either a medical certificate or a
  // supporting photo, and CSV is not a Leave format. Canonical demo evidence is
  // created through the real attachment lifecycle so quarantine and scanning
  // are exercised instead of declaring generated fixture bytes clean.
  for (const company of held) for (let index = 0; index < 2; index += 1) {
    const runId = `payrun-${company.id}-2026-${index === 0 ? "07" : "08"}`;
    const batchId = `payment-batch-${company.id}-${index + 1}`;
    const amounts = Array.from({ length: 20 }, (_, row) => (3000 + row * 175) * 100);
    const currency = company.id === "beacon-co" ? "GBP" : company.id === "northstar-co" ? "EUR" : "USD";
    const rows = ["reference,payee,amount,currency,status", ...amounts.map((amount, row) => `PAY-${runId}-${row + 1},Worker ${row + 1},${(amount / 100).toFixed(2)},${currency},outstanding`)].join("\n");
    const body = text(rows); const checksum = await digest(body); const key = `${holding}/people/payments/${batchId}.csv`;
    if (!(await files.head(key))) {
      await files.put(key, body, { httpMetadata: { contentType: "text/csv" }, customMetadata: { sha256: checksum, format: "generic-csv", seed: "erpx-rich-local-2026" } });
    }
    statements.push(db.prepare("INSERT INTO hr_payment_batches(id,company_id,source_type,source_id,currency,status,format_code,file_key,file_checksum,total_minor,version,created_by,approved_by,exported_at,created_at,updated_at) VALUES(?,?,?,?,?,'exported','generic-csv',?,?,?,1,?,?,?, ?,?) ON CONFLICT DO NOTHING")
      .bind(batchId, company.id, "payroll", runId, currency, key, checksum, amounts.reduce((sum, amount) => sum + amount, 0), actor, reviewer, now, now, now));
    for (let row = 0; row < 20; row += 1) statements.push(db.prepare("INSERT INTO hr_payment_batch_items(id,batch_id,engagement_id,person_id,reference,payee_name,amount_minor,currency) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
      .bind(`payment-item-${company.id}-${index}-${row}`, batchId, `eng-${company.id}-${String(row + 1).padStart(3, "0")}`, `hr-${company.id}-${String(row + 1).padStart(3, "0")}`, `PAY-${runId}-${row + 1}`, `Worker ${row + 1}`, amounts[row], currency));
  }
  await batch(db, statements);
}

/** One employee-authored image post for the local Home feed demo. */
async function seedHomeImagePost(env: SeedEnv, holding: "h001" | "h002", db: Db) {
  if (holding !== "h001") return;
  if (env.ERPX_ENVIRONMENT !== "local") throw new Error("DEMO_IMAGE_SEED_LOCAL_ONLY");

  const bytes = await readFile(new URL("./fixtures/assets/team-planning-demo.webp", import.meta.url));
  const checksum = await digest(bytes);
  const draftId = "seed-h001-home-image-draft";
  const postId = "seed-h001-home-image-post";
  const uploadId = "seed-h001-home-image-upload";
  const attachmentId = `attachment-${uploadId}`;
  const name = "team-planning-demo.webp";
  const objectKey = `${holding}/objects/${uploadId}/r1`;
  const first = "Our Bangkok team mapped the next handover together today.";
  const second = "Thanks for bringing your ideas. #teamwork";
  const body = `${first}\n${second}`;
  const content = JSON.stringify([
    { type: "text", text: first },
    { type: "image", slotId: "seed-home-image", attachmentId, name, alt: "Four coworkers planning together around a table in a Bangkok office" },
    { type: "text", text: second },
  ]);
  const createdAt = now + 60_000;
  const files = env.H001_FILES;
  const existing = await files.head(objectKey);
  if (existing && existing.customMetadata?.sha256 !== checksum)
    throw new Error("DEMO_IMAGE_FILE_CONFLICT");
  if (!existing) {
    await files.put(objectKey, bytes, {
      onlyIf: { etagDoesNotMatch: "*" },
      httpMetadata: { contentType: "image/webp" },
      customMetadata: { sha256: checksum, seed: "erpx-home-image-demo-2026" },
    });
  }
  await db.batch([
    db.prepare("INSERT INTO social_post_drafts(id,author_id,company_id,branch_id,body,created_at,updated_at,content_json) VALUES(?,?,?,NULL,?,?,?,?) ON CONFLICT DO NOTHING")
      .bind(draftId, "usr_mali", "acme-co", body, createdAt, createdAt, content),
    db.prepare("INSERT INTO social_posts(id,kind,author_id,author_name,company_id,branch_id,body,state,version,created_at,updated_at,content_json) VALUES(?,'team',?,?,?,NULL,?,'published',1,?,?,?) ON CONFLICT DO NOTHING")
      .bind(postId, "usr_mali", "Mali Srisai", "acme-co", body, createdAt, createdAt, content),
    db.prepare("UPDATE social_post_drafts SET published_post_id=? WHERE id=? AND published_post_id IS NULL")
      .bind(postId, draftId),
    db.prepare("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,revision,status,created_by,expires_at,created_at,updated_at) VALUES(?,'social_draft',?,'post-image',?,'image/webp',?,?,?,?,?,1,'ready',?,?,?,?) ON CONFLICT DO NOTHING")
      .bind(uploadId, draftId, name, bytes.byteLength, bytes.byteLength, checksum, `${holding}/staging/${uploadId}`, objectKey, "usr_mali", validUntil, createdAt, createdAt),
    db.prepare("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at) VALUES(?,?,'social_draft',?,'post-image',?,1,?,'image/webp',?,?,?,?) ON CONFLICT DO NOTHING")
      .bind(attachmentId, uploadId, draftId, objectKey, name, bytes.byteLength, checksum, "usr_mali", createdAt),
    db.prepare("INSERT INTO social_post_topics(post_id,tag) VALUES(?,'teamwork') ON CONFLICT DO NOTHING")
      .bind(postId),
  ]);
  console.log(`[seed:${holding}] Home image post and private attachment ready`);
}

async function seedEvents(env: SeedEnv, holding: "h001" | "h002") {
  const db = holding === "h001" ? env.H001_DB : env.H002_DB;
  const read = holding === "h001" ? env.H001_READ_DB : env.H002_READ_DB;
  const eventStatements: SqlStatement[] = [];
  const events: EventMessage[] = [];
  const add = (eventType: string, aggregateType: string, aggregateId: string, aggregateVersion: number, payload: EventMessage["payload"]) => {
    const id = `seed-event-${holding}-${aggregateType}-${aggregateId}`.replaceAll(/[^a-zA-Z0-9_-]/g, "-").slice(0, 180);
    events.push({ holdingId: holding, id, eventType, aggregateId, aggregateVersion, payload });
    eventStatements.push(db.prepare("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING").bind(id, `seed-command-${id}`, eventType, aggregateType, aggregateId, aggregateVersion, JSON.stringify(payload), now));
  };
  const leaves = await db.prepare("SELECT id,state,company_id,branch_id,version FROM leave_requests WHERE id LIKE ?").bind(`seed-${holding}-leave-%`).all<{ id: string; state: string; company_id: string; branch_id: string; version: number }>();
  for (const row of leaves.results) add(`leave.${row.state}`, "leave_request", row.id, row.version, { id: row.id, state: row.state, version: row.version, companyId: row.company_id, branchId: row.branch_id, departmentId: null });
  const payroll = await db.prepare("SELECT id,status,company_id,version FROM hr_payroll_runs WHERE id LIKE 'payrun-%'").all<{ id: string; status: string; company_id: string; version: number }>();
  for (const row of payroll.results) add(`people.payroll.${row.status}`, "hr_payroll_run", row.id, row.version, { id: row.id, state: row.status, version: row.version, companyId: row.company_id, branchId: null, departmentId: null });
  const records = await db.prepare("SELECT module_id,id,status,company_id,branch_id,department_id,version FROM workspace_records WHERE id LIKE ?").bind(`seed-${holding}-%`).all<{ module_id: string; id: string; status: string; company_id: string; branch_id: string | null; department_id: string | null; version: number }>();
  for (const row of records.results) add("record.seeded", row.module_id, row.id, row.version, { id: row.id, state: row.status, version: row.version, companyId: row.company_id, branchId: row.branch_id, departmentId: row.department_id });
  const journals = await db.prepare("SELECT id,status,company_id,version FROM finance_journals WHERE id LIKE 'journal-%'").all<{ id: string; status: string; company_id: string; version: number }>();
  for (const row of journals.results) add(`finance.journal.${row.status}`, "finance_journal", row.id, row.version, { id: row.id, state: row.status, version: row.version, companyId: row.company_id, branchId: null, departmentId: null });
  await batch(db, eventStatements);
  const jobEnv: JobEnv = { H001_DB: env.H001_DB, H001_READ_DB: env.H001_READ_DB, H002_DB: env.H002_DB, H002_READ_DB: env.H002_READ_DB };
  for (const event of events) await project(jobEnv, event);
  // Keep the projection target explicit even when there are no source events.
  await read.prepare("SELECT 1").first();
  console.log(`[seed:${holding}] ${events.length} outbox events projected`);
}

async function seedHolding(env: SeedEnv, holding: "h001" | "h002") {
  const db = holding === "h001" ? env.H001_DB : env.H002_DB;
  console.log(`[seed:${holding}] organization and access`);
  await batch(db, orgStatements(db, holding));
  await batch(db, assignmentStatements(db, holding));
  console.log(`[seed:${holding}] directory and payroll`);
  await batch(db, employeeStatements(db, holding));
  console.log(`[seed:${holding}] payroll rows prepared`);
  await batch(db, peoplePayrollStatements(db, holding));
  console.log(`[seed:${holding}] leave and finance`);
  await batch(db, leaveStatements(db, holding));
  await batch(db, financeStatements(db, holding));
  console.log(`[seed:${holding}] social and workflows`);
  await batch(db, socialStatements(db, holding));
  console.log(`[seed:${holding}] private files`);
  await seedFiles(env, holding, db);
  await seedHomeImagePost(env, holding, db);
  await seedEvents(env, holding);
}

async function verify(env: SeedEnv) {
  const output: Record<string, Record<string, number>> = {};
  for (const [id, db] of [["h001", env.H001_DB], ["h002", env.H002_DB]] as const) {
    const tables = ["organization_nodes", "employees", "leave_requests", "workspace_records", "hr_people", "hr_engagements", "hr_payroll_runs", "hr_payroll_items", "finance_journals", "finance_invoices", "finance_payments", "finance_bank_transactions", "social_posts", "social_stories", "attachments", "hr_payment_batches"];
    output[id] = {};
    for (const table of tables) output[id][table] = Number((await db.prepare(`SELECT COUNT(*) AS count FROM ${table} WHERE ${table === "workspace_records" ? "id LIKE ?" : table === "organization_nodes" ? "id LIKE ? OR id IN ('acme-co','beacon-co','northstar-co')" : "1=1"}`).bind(...(table === "workspace_records" ? [`seed-${id}-%`] : table === "organization_nodes" ? [`%`] : [])).first<{ count: number }>())?.count ?? 0);
    const bad = await db.prepare("SELECT COUNT(*) AS count FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id WHERE j.status IN ('posted','reversed') GROUP BY j.id HAVING SUM(l.base_debit_minor) != SUM(l.base_credit_minor)").all();
    if (bad.results.length) throw new Error(`${id}: unbalanced posted journals`);
  }
  console.log(JSON.stringify(output, null, 2));
}

const platform = await getLocalProxy({ configPath: "wrangler.jsonc", persist: true, remoteBindings: false });
try {
  await seedControl(platform.env.CONTROL_DB);
  await seedHolding(platform.env, "h001");
  await seedHolding(platform.env, "h002");
  await verify(platform.env);
  console.log("Local ERPX rich seed reconciled. Existing rows and objects were preserved.");
} finally {
  await platform.dispose();
}
