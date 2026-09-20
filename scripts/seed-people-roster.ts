/** Additive fictional roster examples. Run only against the Acme development database. */
import { readFileSync } from "node:fs";
import { getLocalProxy } from "./local/fixtures";
import { rosterCommand, rosterData } from "../src/server/people-roster";
import { addDays, rosterMonday } from "../src/lib/people-roster";
import type { WorkContext } from "../src/server/authorization";
import type { PostgresDatabase } from "../src/server/runtime/postgres";

const platform = await getLocalProxy();
try {
  const manifest = JSON.parse(readFileSync(process.env.ERPX_DATABASE_MANIFEST!, "utf8"));
  const target = new URL(manifest.databases.H001_DB);
  if (platform.env.ERPX_ENVIRONMENT !== "local" || !["127.0.0.1", "localhost"].includes(target.hostname) || !target.pathname.startsWith("/erpx_dev_") || process.env.NODE_ENV === "production") throw new Error("ROSTER_SEED_REQUIRES_LOCAL_DEVELOPMENT_DATABASE");
  const db = platform.env.H001_DB as PostgresDatabase;
  const today = new Intl.DateTimeFormat("en-CA", { timeZone: "Asia/Bangkok", year: "numeric", month: "2-digit", day: "2-digit" }).format(new Date());
  const thisWeek = rosterMonday(today), start = addDays(today, 3) <= addDays(thisWeek, 6) ? thisWeek : addDays(thisWeek, 7);
  const firstDay = start === thisWeek ? addDays(today, 1) : start;
  const summary: unknown[] = [];
  async function actor(dept: string, seat: string, manager = false): Promise<WorkContext> {
    const userId = `demo-org-v1-bangkok-${dept}-${seat}-user`;
    const a = await db.prepare(`SELECT a.id,a.role,p.legal_name FROM assignments a JOIN hr_people p ON p.user_id=a.user_id AND p.company_id=a.company_id
      WHERE a.user_id=? AND a.company_id='acme-co' AND a.role=? AND iam_assignment_active(a.id,?) ORDER BY a.id LIMIT 1`).bind(userId, manager ? "people_manager" : "employee", Date.now()).first<{ id: string; role: string; legal_name: string }>();
    if (!a) throw new Error(`Missing active Acme demo staff: ${dept}/${seat}. Run the existing organization seed first.`);
    return { userId, assignmentId: a.id, role: a.role, companyId: "acme-co", branchId: "bangkok", departmentId: `dept:bangkok:${dept}`, holdingId: "h001", holdingSlug: "acme", holdingName: "Acme Group", name: a.legal_name, email: "", assignmentScope: "department", inherited: false, modules: [], assignments: [], workspaces: [], scope: { kind: "department", holdingId: "h001", holdingSlug: "acme", holdingName: "Acme Group", companyId: "acme-co", branchId: "bangkok", branchSlug: "bangkok", departmentId: `dept:bangkok:${dept}`, departmentSlug: dept, departmentName: dept } };
  }
  for (const dept of ["operations", "warehouse"]) {
    const prefix = `demo-roster-v1-${dept}-${start}`;
    if (await db.prepare("SELECT 1 FROM people_roster_history WHERE id=?").bind(`${prefix}-complete`).first()) { summary.push({ department: dept, state: "preserved existing examples" }); continue; }
    const manager = await actor(dept, "manager", true), people = await Promise.all(["staff", "senior", "associate", "supervisor"].map(seat => actor(dept, seat)));
    const person = (a: WorkContext) => a.userId.replace(/-user$/, "-person");
    async function once(tag: string, input: unknown, who = manager) {
      const key = `${prefix}-${tag}`;
      const receipt = await db.prepare("SELECT result_json FROM command_receipts WHERE command_id=?").bind(key).first<{ result_json: string }>();
      if (receipt) return JSON.parse(receipt.result_json) as { id: string; version: number; state?: string };
      return rosterCommand(db, who, input, key);
    }
    const templates = [];
    for (const [name, startTime, endTime] of [["Morning", "06:00", "14:00"], ["Afternoon", "14:00", "22:00"], ["Overnight", "22:00", "06:00"]]) templates.push(await once(`template-${name}`, { action: "template.save", departmentId: manager.departmentId, name, startTime, endTime, capacity: 2 }));
    // Never overwrite another manager's week, including weeks created before seeding.
    const existing = await db.prepare("SELECT id FROM people_roster_weeks WHERE company_id='acme-co' AND department_id=? AND week_start=?::date").bind(manager.departmentId, start).first<{ id: string }>();
    if (existing && existing.id !== `roster-${prefix}-week`) { summary.push({ department: dept, state: "preserved pre-existing week" }); continue; }
    const w = await once("week", { action: "week.create", departmentId: manager.departmentId, weekStart: start, timezone: "Asia/Bangkok" });
    const shifts: string[][] = [];
    for (let day = 0; day < 3; day++) {
      shifts[day] = [];
      for (const [index, name, startTime, endTime, staff] of [[0, "Morning", "06:00", "14:00", [people[0], people[1]]], [1, "Afternoon", "14:00", "22:00", [people[2]]], [2, "Overnight", "22:00", "06:00", [people[3]]]] as const) {
        const current = await db.prepare("SELECT version FROM people_roster_weeks WHERE id=?").bind(w.id).first<{ version: number }>();
        const s = await once(`shift-${day}-${index}`, { action: "shift.save", weekId: w.id, expectedVersion: current!.version, templateId: templates[index].id, date: addDays(firstDay, day), name, startTime, endTime, capacity: 2, personIds: staff.map(person), reason: "Fictional development roster" });
        shifts[day].push(s.id);
      }
    }
    const current = await db.prepare("SELECT version FROM people_roster_weeks WHERE id=?").bind(w.id).first<{ version: number }>();
    await once("publish", { action: "week.publish", id: w.id, expectedVersion: current!.version });
    // Copy before sample swaps so the draft demonstrates the original planning pattern.
    const next = addDays(start, 7);
    const nextExisting = await db.prepare("SELECT id FROM people_roster_weeks WHERE company_id='acme-co' AND department_id=? AND week_start=?::date").bind(manager.departmentId, next).first();
    if (!nextExisting) await once("next-week", { action: "week.create", departmentId: manager.departmentId, weekStart: next, timezone: "Asia/Bangkok", copyWeekId: w.id });
    for (let day = 0; day < 3; day++) {
      const source = await db.prepare("SELECT id FROM people_roster_assignments WHERE shift_id=? AND person_id=? ORDER BY state ASC LIMIT 1").bind(shifts[day][0], person(people[0])).first<{ id: string }>();
      const dest = await db.prepare("SELECT id FROM people_roster_assignments WHERE shift_id=? AND person_id=? ORDER BY state ASC LIMIT 1").bind(shifts[day][1], person(people[2])).first<{ id: string }>();
      const request = await once(`swap-${day}`, { action: "request.create", kind: "swap", sourceAssignmentId: source!.id, targetShiftId: shifts[day][1], targetAssignmentId: dest!.id, reason: ["Morning appointment; can we exchange shifts?", "Family commitment; afternoon would help.", "Agreed to cover each other's shifts."][day] }, people[0]);
      if (day >= 1) await once(`accept-${day}`, { action: "request.decide", id: request.id, expectedVersion: 1, decision: "accept", reason: "I can cover the morning." }, people[2]);
      if (day === 2) await once(`approve-${day}`, { action: "request.decide", id: request.id, expectedVersion: 2, decision: "approve", reason: "Coverage checked; swap approved." });
    }
    const extra = await once("extra", { action: "request.create", kind: "extra", targetShiftId: shifts[0][2], reason: "Available for an extra overnight shift." }, people[1]);
    await once("reject", { action: "request.decide", id: extra.id, expectedVersion: 1, decision: "reject", reason: "Keep the overnight slot open for relief coverage." });
    const data = await rosterData(db, manager, { department: manager.departmentId!, week: start });
    await db.prepare("INSERT INTO people_roster_history(id,company_id,department_id,week_id,record_id,action,actor_id,reason,evidence_json,created_at) VALUES(?,'acme-co',?,?,?,'demo.seeded','local-demo-seed','Fictional roster examples','{}',?) ON CONFLICT DO NOTHING").bind(`${prefix}-complete`, manager.departmentId, w.id, w.id, Date.now()).run();
    summary.push({ department: dept, week: start, shifts: data.shifts.filter(s => s.week_id === w.id).length, requests: data.requests.map(r => r.state), url: `/holdings/acme/branches/bangkok/departments/${dept}/people/roster?week=${start}` });
  }
  console.log(JSON.stringify(summary, null, 2));
} finally { await platform.dispose(); }
