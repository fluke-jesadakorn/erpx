import { readFile } from "node:fs/promises";
import { join } from "node:path";
import type { RuntimeEnv } from "../../../src/server/modules/runtime/adapters/runtime/env";
import type { SqlDatabase } from "../../../src/server/modules/runtime/adapters/runtime/types";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import { loadIamAuthority } from "../../../src/server/modules/identity_access/public/iam-store";
import { runLeaveAccruals } from "../../../src/server/modules/people_payroll/public/leave-self-service";
import { changeAppointment, peopleRequestRoute } from "../../../src/server/modules/organization/public/department-authority";
import { activeStaff } from "../../../src/server/modules/operations/public/staff-policy";
import { assertAcmeBrowserSeedTarget } from "../../shared/builders/acme-browser-seed-target";
import { demoPrefix } from "../../organization/builders/acme-organization";

/** Declare the fixture's existing HR reviewer using the normal owner's command. */
export async function prepareLocalBrowserHrReviewer(db: SqlDatabase, owner: WorkContext, reviewerUserId: string) {
  const person = await activeStaff(db, owner.companyId, reviewerUserId);
  const current = await db.prepare("SELECT person_id,position_id,version,effective_at FROM authority_seats WHERE company_id=? AND kind='hr' AND scope_id=?")
    .bind(owner.companyId, owner.companyId).first<{ person_id: string | null; position_id: string | null; version: number; effective_at: number | null }>();
  if (current?.person_id === person.person_id && current.effective_at !== null && current.effective_at <= Date.now())
    return { personId: person.person_id, version: current.version };
  if (current) {
    // Company initialization creates a vacant version-zero seat. An explicit
    // appointment or later vacancy still belongs to its existing preparation.
    if (current.person_id !== null || current.position_id !== null || current.version !== 0 || current.effective_at !== null)
      throw new Error("BROWSER_HR_APPOINTMENT_ALREADY_OWNED");
  }
  const result = await changeAppointment(db, owner, {
    kind: "hr", departmentId: null, positionId: null, personId: person.person_id, expectedVersion: 0,
    reason: "Declare the existing independent HR reviewer for this isolated browser fixture.",
  }, `browser-hr-${owner.companyId}-${person.person_id}`);
  return { personId: person.person_id, version: result.version };
}

/** Add only the prerequisites for normal leave creation and independent UI approval. */
export async function prepareLocalBrowserLeaveRecords(db: SqlDatabase, input: {
  companyId: string; userId: string; requestId: string; requestNumber: string; now: number;
}) {
  const staff = await db.prepare(`SELECT p.id person_id,e.id engagement_id,e.branch_id,e.department_id,p.legal_name
    FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
    WHERE p.company_id=? AND p.user_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active'
    ORDER BY e.start_date DESC,e.created_at DESC,e.id LIMIT 1`).bind(input.companyId, input.userId)
    .first<{ person_id: string; engagement_id: string; branch_id: string | null; department_id: string | null; legal_name: string }>();
  if (!staff) throw new Error("LOCAL_BROWSER_LEAVE_REQUESTER_REQUIRED");
  const existingAssignment = await db.prepare("SELECT id FROM leave_policy_assignments WHERE company_id=? AND engagement_id=? AND effective_to IS NULL")
    .bind(input.companyId, staff.engagement_id).first();
  const prefix = `browser-leave-${input.companyId}`, typeId = `${prefix}-annual`, policyId = `${prefix}-policy`;
  if (!existingAssignment) {
    await db.batch([
      db.prepare("INSERT INTO leave_types(id,company_id,code,name,description,tracked,effective_from,created_by,created_at,updated_at) VALUES(?,?,'browser-annual','Annual leave','Fictional isolated browser allowance.',true,'2026-01-01','browser-fixture',?,?) ON CONFLICT DO NOTHING")
        .bind(typeId, input.companyId, input.now, input.now),
      db.prepare("INSERT INTO leave_policy_versions(id,company_id,policy_key,version,name,status,effective_from,timezone,workweek_json,created_by,created_at) VALUES(?,?,'browser-annual',1,'Fictional browser leave','active','2026-01-01','Asia/Bangkok','[1,2,3,4,5]'::jsonb,'browser-fixture',?) ON CONFLICT DO NOTHING")
        .bind(policyId, input.companyId, input.now),
      db.prepare("INSERT INTO leave_policy_rules(policy_version_id,leave_type_id,grant_kind,grant_units,employee_eligible,contractor_eligible) VALUES(?,?,'annual',30,true,false) ON CONFLICT DO NOTHING")
        .bind(policyId, typeId),
      db.prepare("INSERT INTO leave_policy_assignments(id,company_id,engagement_id,policy_version_id,effective_from,created_by,created_at) VALUES(?,?,?,?,'2026-01-01','browser-fixture',?) ON CONFLICT DO NOTHING")
        .bind(`${prefix}-${staff.engagement_id}`, input.companyId, staff.engagement_id, policyId, input.now),
    ]);
  }
  await runLeaveAccruals(db, "2026-09-01", input.companyId);
  const employee = await db.prepare("SELECT id FROM employees WHERE company_id=? AND person_id=? AND status='active' ORDER BY id LIMIT 1")
    .bind(input.companyId, staff.person_id).first<{ id: string }>();
  const employeeId = employee?.id ?? `${prefix}-${staff.person_id}-employee`;
  await db.batch([
    ...(!employee ? [db.prepare("INSERT INTO employees(id,user_id,company_id,branch_id,department_id,person_id,display_name,status,created_at) VALUES(?,?,?,?,?,?,?,'active',?)")
      .bind(employeeId, input.userId, input.companyId, staff.branch_id ?? "", staff.department_id, staff.person_id, staff.legal_name, input.now)] : []),
    db.prepare(`INSERT INTO leave_requests(id,number,employee_id,person_id,engagement_id,company_id,branch_id,department_id,start_date,days,handover_note,state,version,created_by,created_at,updated_at)
      VALUES(?,?,?,?,?,?,?,?,'2026-11-10',1,'Fictional handover for independent browser approval.','draft',1,?,?,?) ON CONFLICT DO NOTHING`)
      .bind(input.requestId, input.requestNumber, employeeId, staff.person_id, staff.engagement_id, input.companyId, staff.branch_id ?? "", staff.department_id, input.userId, input.now, input.now),
  ]);
  return { personId: staff.person_id, engagementId: staff.engagement_id, requestId: input.requestId };
}

export async function seedLocalBrowserLeave(env: RuntimeEnv, directory: string) {
  const manifest = JSON.parse(await readFile(process.env.ERPX_DATABASE_MANIFEST!, "utf8"));
  const state = JSON.parse(await readFile(join(directory, "state.json"), "utf8"));
  assertAcmeBrowserSeedTarget(manifest, state, process.env.ERPX_BROWSER_SIMULATION_RUN_ID ?? process.env.ERPX_BROWSER_SIMULATION_RUN);
  if (env.ERPX_ENVIRONMENT !== "local") throw new Error("LOCAL_DEVELOPMENT_ONLY");
  const db = env.H001_DB, now = Date.now();
  const facts = await db.prepare(`SELECT a.id,p.legal_name,p.email,a.modules_json FROM assignments a
    JOIN hr_people p ON p.user_id=a.user_id AND p.company_id=a.company_id
    WHERE a.id='asg_mali_owner' AND a.user_id='usr_mali' AND a.company_id='acme-co' AND a.role='holding_owner'
    AND a.scope_kind='holding' AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?)`)
    .bind(now, now).first<{ id: string; legal_name: string; email: string; modules_json: string | string[] }>();
  if (!facts) throw new Error("NORMAL_BROWSER_OWNER_AUTHORITY_REQUIRED");
  const owner: WorkContext = {
    userId: "usr_mali", assignmentId: facts.id, role: "holding_owner", name: facts.legal_name, email: facts.email,
    companyId: "acme-co", branchId: null, departmentId: null, holdingId: "h001", holdingSlug: "acme", holdingName: "Acme Group",
    assignmentScope: "holding", inherited: true, modules: typeof facts.modules_json === "string" ? JSON.parse(facts.modules_json) : facts.modules_json,
    assignments: [], workspaces: [], iam: await loadIamAuthority(db, "acme-co", "usr_mali", "holding_owner"),
    scope: { kind: "holding", holdingId: "h001", holdingSlug: "acme", holdingName: "Acme Group", companyId: "acme-co" },
  };
  const reviewerUserId = `${demoPrefix}-bangkok-people-manager-user`;
  await prepareLocalBrowserHrReviewer(db, owner, reviewerUserId);
  await prepareLocalBrowserLeaveRecords(env.H001_DB, { companyId: "acme-co", userId: "usr_mali", requestId: "leave_001", requestNumber: "LV-2026-001", now: Date.now() });
  const route = await peopleRequestRoute(env.H001_DB, "acme-co", "usr_mali");
  if (route.blockedReason || JSON.stringify(route.steps.map(step => step.userId)) !== JSON.stringify(["usr_nisa", reviewerUserId]))
    throw new Error("LOCAL_BROWSER_INDEPENDENT_LEAVE_ROUTE_REQUIRED");
}
