import type { seedExpenseCompany } from "../../expenses/company";
import { directoryPerson, directoryUpdateSchema, saveDirectoryPerson } from "@erpx/server-people_payroll/public/people-directory";
import { positionSchema, savePosition } from "@erpx/server-organization/public/organization";

type Fixture = Awaited<ReturnType<typeof seedExpenseCompany>>;
type Placement = { userId: string; assignmentId: string; scope: "branch" | "department"; nodeId: string; parentNodeId?: string };

/** Place explicitly declared fictional staff through the HR owner after their scoped account is created. */
export async function placeScopedBrowserEmployee(f: Fixture, input: Placement) {
  const assignment = await f.db.prepare(`SELECT a.scope_kind,a.scope_node_id,n.branch_id
    FROM assignments a JOIN organization_nodes n ON n.id=a.scope_node_id AND n.company_id=a.company_id
    WHERE a.id=? AND a.user_id=? AND a.company_id=? AND a.role='employee'
      AND a.scope_kind=? AND a.scope_node_id=? AND n.kind=? AND n.status='active'
      AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)`)
    .bind(input.assignmentId, input.userId, f.company, input.scope, input.nodeId, input.scope)
    .first<{ scope_kind: string; scope_node_id: string; branch_id: string }>();
  if (!assignment || f.owner.companyId !== f.company || f.owner.branchId || f.owner.departmentId)
    throw new Error("EXACT_SCOPED_BROWSER_EMPLOYEE_REQUIRED");
  const branchId = input.scope === "branch" ? input.nodeId : assignment.branch_id;
  if (branchId !== f.branch || input.scope === "department" && (input.nodeId !== f.department || input.parentNodeId !== branchId))
    throw new Error("EXACT_SCOPED_BROWSER_PLACEMENT_REQUIRED");
  const departmentId = input.scope === "department" ? input.nodeId : null;
  const ceo = await f.db.prepare("SELECT id FROM organization_positions WHERE company_id=? AND active=1 AND is_ceo=1")
    .bind(f.company).first<{ id: string }>();
  const level = await f.db.prepare("SELECT id FROM staff_levels WHERE company_id=? AND active=1 AND level_number=6")
    .bind(f.company).first<{ id: string }>();
  if (!ceo || !level) throw new Error("BROWSER_ORGANIZATION_SETUP_REQUIRED");
  const person = await directoryPerson(f.owner, `person-${input.userId}`, `eng-${input.userId}`);
  const position = await savePosition(f.db, f.owner, null, positionSchema.parse({
    title: `Fictional ${input.scope} employee`, branchId, departmentId, levelId: level.id,
    reportsToPositionId: ceo.id, expectedVersion: 0, reason: "Declare the scoped fictional employee's actual organization placement.",
  }), crypto.randomUUID());
  const result = await saveDirectoryPerson(f.owner, String(person.id), directoryUpdateSchema.parse({
    legalName: person.legal_name, preferredName: person.preferred_name ?? "", email: person.email,
    phone: person.phone ?? "", workEmail: person.work_email ?? "", workPhone: person.work_phone ?? "", countryCode: person.country_code,
    branchId, departmentId, jobTitle: person.job_title, workerType: person.worker_type, employmentType: person.employment_type,
    startDate: person.start_date, currency: person.currency, managerPersonId: `person-${f.owner.userId}`,
    levelId: level.id, positionId: position.id, expectedVersion: Number(person.version),
    engagementId: person.engagement_id, expectedEngagementVersion: Number(person.engagement_version),
    reason: "Complete the scoped fictional staff placement through the normal HR owner workflow.",
  }), crypto.randomUUID());
  return { personId: result.id, personVersion: result.version, engagementId: String(person.engagement_id), positionId: String(position.id), levelId: level.id, branchId, departmentId };
}
