import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import type { RuntimeEnv } from "../../src/server/runtime/env";
import { PostgresDatabase } from "../../src/server/runtime/postgres";
import type { SqlDatabase } from "../../src/server/runtime/types";
import type { WorkContext } from "../../src/server/modules/identity_access/public/authorization";
import { loadIamAuthority } from "../../src/server/modules/identity_access/public/iam-store";
import { rosterCommand, rosterData } from "../../src/server/modules/people_payroll/public/people-roster";
import { addDays, rosterMonday } from "../../src/server/modules/people_payroll/public/rules/people-roster-domain";
import type { DirectoryInventory } from "../../src/lib/directory-operations";
import { assertAcmeBrowserSeedTarget } from "./acme-browser-seed-target";
import { demoCompany, demoPrefix } from "./acme-organization";

/** The examples use a future shift in the actual current week, including Sundays. */
function currentWeekShift(now: number) {
  const parts = new Intl.DateTimeFormat("en-CA", { timeZone: "Asia/Bangkok", year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hourCycle: "h23" }).formatToParts(new Date(now));
  const part = (name: Intl.DateTimeFormatPartTypes) => parts.find(value => value.type === name)!.value;
  const today = `${part("year")}-${part("month")}-${part("day")}`, weekStart = rosterMonday(today);
  const tomorrow = addDays(today, 1), endOfWeek = addDays(weekStart, 6);
  if (tomorrow <= endOfWeek) return { weekStart, date: tomorrow, startTime: "06:00", endTime: "14:00" };
  // A Sunday preparation must still show the current week. The display name
  // remains the declared example; its actual time obeys the owner's future fence.
  const minute = Number(part("hour")) * 60 + Number(part("minute"));
  const starts = Math.min(minute + 10, 1439);
  if (starts <= minute) throw new Error("CURRENT_WEEK_BROWSER_ROSTER_HAS_NO_FUTURE_MINUTE");
  const time = (value: number) => `${String(Math.floor(value / 60) % 24).padStart(2, "0")}:${String(value % 60).padStart(2, "0")}`;
  return { weekStart, date: today, startTime: time(starts), endTime: time(starts + 60) };
}

/** Provided-db fixture preparation, reused by the native roster regression. */
export async function prepareLocalBrowserRosterExample(db: PostgresDatabase, manager: WorkContext, worker: WorkContext, now = Date.now()) {
  if (manager.companyId !== worker.companyId || manager.userId === worker.userId) throw new Error("INDEPENDENT_BROWSER_ROSTER_ACTORS_REQUIRED");
  const window = currentWeekShift(now), self = await rosterData(db, worker, { week: window.weekStart }), department = self.departmentId;
  if (!department || manager.departmentId && manager.departmentId !== department || worker.departmentId && worker.departmentId !== department) throw new Error("INDEPENDENT_BROWSER_ROSTER_ACTORS_REQUIRED");
  const initial = await rosterData(db, manager, { department, week: window.weekStart });
  if (!initial.canManage || !self.selfRosterRequired || !self.selfPersonId || !initial.people.some(person => person.person_id === self.selfPersonId)) throw new Error("NORMAL_BROWSER_ROSTER_AUTHORITY_REQUIRED");
  const prefix = `browser-roster-${manager.companyId}-${department}-${window.weekStart}`;
  // Stable receipts make preparation restartable without overwriting another
  // person's week. Only this exact fixture's published week can be reused.
  const receipt = await db.prepare("SELECT result_json FROM command_receipts WHERE command_id=?").bind(`${prefix}-week`).first<{ result_json: string }>();
  if (initial.week && (!receipt || JSON.parse(receipt.result_json).id !== initial.week.id)) throw new Error("BROWSER_ROSTER_WEEK_ALREADY_OWNED");
  const templateReceipt = await db.prepare("SELECT result_json FROM command_receipts WHERE command_id=?").bind(`${prefix}-template`).first<{ result_json: string }>();
  const template: { id: string } = templateReceipt ? JSON.parse(templateReceipt.result_json) : await rosterCommand(db, manager, { action: "template.save", departmentId: department, name: "Morning", startTime: window.startTime, endTime: window.endTime, capacity: initial.people.length }, `${prefix}-template`);
  const week = await rosterCommand(db, manager, { action: "week.create", departmentId: department, weekStart: window.weekStart, timezone: "Asia/Bangkok" }, `${prefix}-week`);
  const shiftKey = `${prefix}-shift`, shiftReceipt = await db.prepare("SELECT result_json FROM command_receipts WHERE command_id=?").bind(shiftKey).first<{ result_json: string }>();
  const current = await db.prepare("SELECT version FROM people_roster_weeks WHERE id=? AND company_id=?").bind(week.id, manager.companyId).first<{ version: number }>();
  if (!current) throw new Error("BROWSER_ROSTER_WEEK_MISSING");
  let shiftId: string;
  if (shiftReceipt) shiftId = String(JSON.parse(shiftReceipt.result_json).id);
  else {
    const shift = await rosterCommand(db, manager, { action: "shift.save", weekId: week.id, expectedVersion: current.version, templateId: template.id, date: window.date, name: "Morning", startTime: window.startTime, endTime: window.endTime, capacity: initial.people.length, personIds: initial.people.map(person => person.person_id), reason: "Declared current-week browser fixture covering every actual roster Position holder." }, shiftKey);
    shiftId = shift.id;
  }
  const state = await rosterData(db, manager, { department, week: window.weekStart });
  if (!state.week) throw new Error("BROWSER_ROSTER_WEEK_MISSING");
  if (state.week.state !== "published") await rosterCommand(db, manager, { action: "week.publish", id: week.id, expectedVersion: state.week.version }, `${prefix}-publish`);
  const verified = await rosterData(db, worker, { department, week: window.weekStart });
  if (verified.week?.id !== week.id || verified.week.state !== "published" || !verified.shifts.some(shift => shift.id === shiftId && shift.name === "Morning") || !verified.assignments.some(assignment => assignment.shift_id === shiftId && assignment.person_id === verified.selfPersonId && assignment.state === "assigned")) throw new Error("BROWSER_ROSTER_EXAMPLE_NOT_VISIBLE");
  return { weekId: week.id, shiftId, weekStart: window.weekStart, workerPersonId: self.selfPersonId, departmentId: department };
}

/** A saved, explicitly unavailable connection supplies inventory only; delivery stays disabled. */
export async function prepareLocalBrowserDirectoryInventory(db: SqlDatabase, companyId: string, creatorUserId: string, now = Date.now()) {
  const id = `browser-directory-${companyId}`, inventory: DirectoryInventory = {
    providerVersion: "declared-browser-inventory-v1", observedAt: now, complete: true, fields: [], objects: [
      { id: `${id}-gpo`, dn: "cn=Browser example policy,dc=fixture,dc=invalid", kind: "gpo", name: "Browser example policy", parentId: null, version: "1", module: "policies/gpo", properties: { securityFilterGroupIds: [] }, memberIds: [] },
      { id: `${id}-group`, dn: "cn=Browser example group,dc=fixture,dc=invalid", kind: "group", name: "Browser example group", parentId: null, version: "1", module: "groups/group", properties: {}, memberIds: [] },
      { id: `${id}-ou`, dn: "ou=Browser example computers,dc=fixture,dc=invalid", kind: "ou", name: "Browser example computers", parentId: null, version: "1", module: "container/ou", properties: { gpoLinks: [] }, memberIds: [] },
    ],
  };
  const existing = await db.prepare("SELECT id FROM iam_connections WHERE id=?").bind(id).first();
  if (!existing) await db.batch([
    db.prepare("INSERT INTO iam_connections(id,company_id,name,provider,config,status,version,created_by,created_at) VALUES(?,?,?,'active_directory',?::jsonb,'draft',1,?,?)").bind(id, companyId, "Declared read-only browser inventory", JSON.stringify({ mode: "ldaps", authority: "browser-fixture.invalid", ldapUrl: "ldaps://browser-fixture.invalid", baseDn: "dc=fixture,dc=invalid", secretProfile: "unavailable-browser-fixture" }), creatorUserId, now),
    db.prepare("INSERT INTO people_directory_state(connection_id,company_id,tested_version,write_enabled,inventory,observed_at) VALUES(?,?,1,false,?::jsonb,?)").bind(id, companyId, JSON.stringify(inventory), now),
  ]);
  const actual = await db.prepare("SELECT c.company_id,c.status,s.write_enabled,s.inventory FROM iam_connections c JOIN people_directory_state s ON s.connection_id=c.id WHERE c.id=?").bind(id).first<{ company_id: string; status: string; write_enabled: boolean; inventory: string | DirectoryInventory }>();
  const stored: DirectoryInventory | null = actual ? typeof actual.inventory === "string" ? JSON.parse(actual.inventory) : actual.inventory : null;
  if (!actual || actual.company_id !== companyId || actual.status !== "draft" || actual.write_enabled !== false || stored?.providerVersion !== inventory.providerVersion || !["gpo", "group", "ou"].every(kind => stored.objects.some(object => object.kind === kind))) throw new Error("EXACT_READ_ONLY_BROWSER_DIRECTORY_INVENTORY_REQUIRED");
  return { connectionId: id, gpoId: `${id}-gpo`, groupId: `${id}-group`, ouId: `${id}-ou` };
}

/** Main entry: only the exact unstarted, disposable cohort may receive these prerequisites. */
export async function seedLocalBrowserRosterDirectory(env: RuntimeEnv, directory: string) {
  const run = process.env.ERPX_BROWSER_SIMULATION_RUN_ID, manifestPath = process.env.ERPX_DATABASE_MANIFEST;
  if (!run || !manifestPath || resolve(directory) !== resolve(".runtime/browser", run) || env.ERPX_ENVIRONMENT !== "local") throw new Error("EXACT_ACME_BROWSER_SEED_COHORT_REQUIRED");
  const manifest = JSON.parse(await readFile(manifestPath, "utf8")), state = JSON.parse(await readFile(join(directory, "state.json"), "utf8"));
  assertAcmeBrowserSeedTarget(manifest, state, run);
  for (const binding of Object.keys(manifest.databases)) {
    const database = env[binding];
    if (!(database instanceof PostgresDatabase)) throw new Error("EXACT_ACME_BROWSER_NATIVE_DATABASE_REQUIRED");
    const target = await database.prepare("SELECT current_database() name,current_user owner").first<{ name: string; owner: string }>();
    if (target?.name !== `erpx_browser_${run}_${binding.toLowerCase()}` || target.owner !== "erpx_test_admin") throw new Error("EXACT_ACME_BROWSER_PROVIDED_DATABASE_REQUIRED");
  }
  const db = env.H001_DB as PostgresDatabase;
  async function actor(seat: "manager" | "staff", role: "people_manager" | "employee"): Promise<WorkContext> {
    const userId = `${demoPrefix}-bangkok-operations-${seat}-user`;
    const assignment = await db.prepare("SELECT a.id,p.legal_name,p.email,a.modules_json FROM assignments a JOIN hr_people p ON p.user_id=a.user_id AND p.company_id=a.company_id WHERE a.user_id=? AND a.company_id=? AND a.role=? AND a.scope_kind='department' AND a.scope_node_id='dept:bangkok:operations' AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?)").bind(userId, demoCompany, role, Date.now(), Date.now()).first<{ id: string; legal_name: string; email: string; modules_json: string | string[] }>();
    if (!assignment) throw new Error("CANONICAL_BROWSER_ROSTER_ACTOR_REQUIRED");
    return { userId, assignmentId: assignment.id, role, name: assignment.legal_name, email: assignment.email, companyId: demoCompany, branchId: "bangkok", departmentId: "dept:bangkok:operations", holdingId: "h001", holdingSlug: "acme", holdingName: "Acme Group", assignmentScope: "department", inherited: false, modules: typeof assignment.modules_json === "string" ? JSON.parse(assignment.modules_json) : assignment.modules_json, assignments: [], workspaces: [], iam: await loadIamAuthority(db, demoCompany, userId, role), scope: { kind: "department", holdingId: "h001", holdingSlug: "acme", holdingName: "Acme Group", companyId: demoCompany, branchId: "bangkok", branchSlug: "bangkok", departmentId: "dept:bangkok:operations", departmentSlug: "operations", departmentName: "Operations" } };
  }
  const manager = await actor("manager", "people_manager"), worker = await actor("staff", "employee");
  const roster = await prepareLocalBrowserRosterExample(db, manager, worker), directoryInventory = await prepareLocalBrowserDirectoryInventory(db, demoCompany, manager.userId);
  return { roster, directory: directoryInventory };
}
