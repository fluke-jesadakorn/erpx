import { requirePermission } from "@/server/iam-permissions";
import { hasPermission } from "@/lib/permissions";
import type { SqlDatabase } from "./runtime/types";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import { runtimeEnv, holdingResources } from "./runtime";
import { ApiError } from "./http";
import { levelChangeStatements } from "./staff-settings";
import { assertion, executeCommand, replayCommand } from "./commands";
import { requireCompanyAuthority, authorityGuard } from "./staff-policy";
import { organizationGuard, validateStaffOrganization } from "./organization";

export const directorySchema = z.object({
  legalName: z.string().trim().min(2).max(200),
  preferredName: z.string().trim().max(160).default(""),
  email: z.email().transform((v) => v.toLowerCase()),
  phone: z.string().trim().max(60).default(""),
  countryCode: z.string().regex(/^[A-Z]{2}$/),
  branchId: z.string().min(1),
  departmentId: z.string().nullable().default(null),
  jobTitle: z.string().trim().min(2).max(160),
  workerType: z.enum(["employee", "contractor"]),
  employmentType: z.enum(["salaried", "hourly", "shift", "contractor"]),
  startDate: z.iso.date(),
  currency: z.enum(["USD", "EUR", "GBP", "THB", "SGD", "CAD"]),
  managerPersonId: z.string().nullable().default(null),
  levelId: z.string().min(1).nullable().default(null),
  positionId: z.string().min(1).nullable().optional(),
  reason: z.string().trim().min(3).max(1000).default("Staff profile updated"),
  linkedMemberUserId: z.string().max(160).optional(),
});
export const directoryUpdateSchema = directorySchema.extend({
  expectedVersion: z.number().int().positive(),
});
export const directoryStatusSchema = z.object({
  status: z.enum(["active", "inactive", "archived"]),
  expectedVersion: z.number().int().positive(),
});
export function canManagePeople(actor: WorkContext) {
  return hasPermission(actor,"people.manage");
}
export function canReadCompensation(actor: WorkContext, userId: unknown) {
  return (
    canManagePeople(actor) ||
    hasPermission(actor,"people.compensation.payroll") ||
    userId === actor.userId
  );
}
function manage(actor: WorkContext) {
  return requirePermission(actor,"people.manage");
}
export async function directoryOptions(actor: WorkContext) {
  const { write } = await holdingResources(actor.holdingId);
  const nodes = await write
    .prepare(
      "SELECT id,kind,display_name,branch_id FROM organization_nodes WHERE company_id=? AND status='active' AND kind IN ('branch','department') AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR kind='branch' OR id=?) ORDER BY display_name",
    )
    .bind(
      actor.companyId,
      actor.branchId,
      actor.branchId,
      actor.departmentId,
      actor.departmentId,
    )
    .all();
  return nodes.results;
}
export async function directoryPerson(actor: WorkContext, personId: string) {
  const { write } = await holdingResources(actor.holdingId);
  const person = await write
    .prepare(
      `SELECT p.*,e.id AS engagement_id,e.version AS engagement_version,e.branch_id,e.department_id,e.job_title,e.worker_type,e.employment_type,e.start_date,e.end_date,e.currency,e.status AS engagement_status,e.manager_person_id,e.pay_group_id,e.position_id,e.level_id,(SELECT name FROM staff_levels WHERE id=e.level_id) level_name,(SELECT level_number FROM staff_levels WHERE id=e.level_id) level_number
    FROM hr_people p LEFT JOIN hr_engagements e ON e.id=(SELECT x.id FROM hr_engagements x WHERE x.person_id=p.id AND x.company_id=p.company_id ORDER BY CASE WHEN x.status IN ('active','onboarding','suspended') THEN 0 ELSE 1 END,x.start_date DESC,x.id LIMIT 1)
    WHERE p.id=? AND p.company_id=? AND p.merged_into_id IS NULL AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)`,
    )
    .bind(
      personId,
      actor.companyId,
      actor.branchId,
      actor.branchId,
      actor.departmentId,
      actor.departmentId,
    )
    .first<Record<string, string | number | null>>();
  if (!person)
    throw new ApiError(
      404,
      "PERSON_NOT_FOUND",
      "Person not found in this workspace.",
    );
  return person;
}
function placement(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
  input: z.infer<typeof directorySchema>,
) {
  if (
    (actor.branchId && input.branchId !== actor.branchId) ||
    (actor.departmentId && input.departmentId !== actor.departmentId)
  )
    throw new ApiError(
      403,
      "SCOPE_FORBIDDEN",
      "Choose a placement in your assigned workspace.",
    );
  if (
    (input.workerType === "contractor") !==
    (input.employmentType === "contractor")
  )
    throw new ApiError(
      422,
      "WORKER_TYPE_INVALID",
      "Contractor worker and employment types must agree.",
    );
  return [
    assertion(
      db,
      id,
      "branch",
      "EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active')",
      [input.branchId, actor.companyId],
    ),
    assertion(
      db,
      id,
      "department",
      "?::text IS NULL OR EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND branch_id=? AND kind='department' AND status='active')",
      [input.departmentId, input.departmentId, actor.companyId, input.branchId],
    ),
    assertion(
      db,
      id,
      "manager",
      "?::text IS NULL OR EXISTS(SELECT 1 FROM hr_people WHERE id=? AND company_id=? AND status='active' AND merged_into_id IS NULL)",
      [input.managerPersonId, input.managerPersonId, actor.companyId],
    ),
  ];
}
export async function saveDirectoryPerson(
  actor: WorkContext,
  personId: string | null,
  input: z.infer<typeof directoryUpdateSchema>,
  commandId: string,
) {
  actor = manage(actor);
  const { write } = await holdingResources(actor.holdingId);
  const current = personId ? await directoryPerson(actor, personId) : null;
  // Stable IDs make a lost-response retry target the same aggregate.
  const id = personId ?? `person-${commandId}`;
  const engagement = String(current?.engagement_id ?? `eng-${commandId}`);
  const version = current ? input.expectedVersion + 1 : 1;
  const now = Date.now();
  const steps = [await organizationGuard(write, actor.companyId, commandId), ...placement(write, actor, commandId, input)];
  if (input.linkedMemberUserId) {
    await requireCompanyAuthority(write, actor);
    const env = await runtimeEnv(),
      account = await env.CONTROL_DB.prepare(
        'SELECT id,email FROM "user" WHERE id=?',
      )
        .bind(input.linkedMemberUserId)
        .first<{ id: string; email: string }>();
    if (
      !account ||
      account.email.toLowerCase() !== input.email.toLowerCase() ||
      (current?.user_id && current.user_id !== account.id)
    )
      throw new ApiError(
        422,
        "MEMBER_LINK_INVALID",
        "Choose the existing company member and their verified account email.",
      );
    steps.push(
      authorityGuard(write, commandId, actor),
      assertion(
        write,
        commandId,
        "member-link",
        "EXISTS(SELECT 1 FROM assignments WHERE user_id=? AND company_id=? AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?) AND NOT EXISTS(SELECT 1 FROM hr_people WHERE user_id=? AND company_id=? AND id!=? AND merged_into_id IS NULL)",
        [
          account.id,
          actor.companyId,
          now,
          now,
          account.id,
          actor.companyId,
          id,
        ],
      ),
    );
  }
  const levelId =
    input.levelId ?? (current?.level_id ? String(current.level_id) : null);
  const positionId = input.positionId === undefined ? (current?.position_id ? String(current.position_id) : null) : input.positionId;
  await validateStaffOrganization(write, actor.companyId, {person_id:id,branch_id:input.branchId,department_id:input.departmentId,position_id:positionId,level_id:levelId,manager_person_id:input.managerPersonId}, current?.engagement_status === "active");
  if (current?.engagement_status === "active" && !levelId)
    throw new ApiError(
      422,
      "LEVEL_REQUIRED",
      "Assign a staff level before updating an active engagement.",
    );
  if (current)
    steps.push(
      assertion(
        write,
        commandId,
        "engagement-version",
        "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=?)",
        [engagement, Number(current.engagement_version)],
      ),
    );
  steps.push(
    assertion(
      write,
      commandId,
      "email",
      "NOT EXISTS(SELECT 1 FROM hr_people WHERE company_id=? AND lower(email)=? AND id!=? AND merged_into_id IS NULL)",
      [actor.companyId, input.email, id],
    ),
  );
  if (input.managerPersonId === id)
    throw new ApiError(
      422,
      "SELF_MANAGER",
      "A person cannot report to themselves.",
    );
  if (current) {
    steps.push(
      assertion(
        write,
        commandId,
        "version",
        "EXISTS(SELECT 1 FROM hr_people WHERE id=? AND version=? AND status!='archived')",
        [id, input.expectedVersion],
      ),
    );
    steps.push(
      write
        .prepare(
          "UPDATE hr_people SET legal_name=?,preferred_name=?,email=?,phone=?,country_code=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(
          input.legalName,
          input.preferredName || null,
          input.email,
          input.phone || null,
          input.countryCode,
          now,
          id,
        ),
    );
    steps.push(
      write
        .prepare(
          "UPDATE hr_engagements SET branch_id=?,department_id=?,job_title=?,worker_type=?,employment_type=?,start_date=?,currency=?,manager_person_id=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(
          input.branchId,
          input.departmentId,
          input.jobTitle,
          input.workerType,
          input.employmentType,
          input.startDate,
          input.currency,
          input.managerPersonId,
          now,
          engagement,
        ),
    );
    steps.push(
      write
        .prepare(
          "UPDATE employees SET display_name=?,branch_id=?,department_id=? WHERE person_id=?",
        )
        .bind(input.legalName, input.branchId, input.departmentId, id),
    );
  } else {
    steps.push(
      write
        .prepare(
          "INSERT INTO hr_people(id,company_id,legal_name,preferred_name,email,phone,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          id,
          actor.companyId,
          input.legalName,
          input.preferredName || null,
          input.email,
          input.phone || null,
          input.countryCode,
          actor.userId,
          now,
          now,
        ),
    );
    steps.push(
      write
        .prepare(
          "INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,manager_person_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,'onboarding',?,?,?,?,?,?,?)",
        )
        .bind(
          engagement,
          id,
          actor.companyId,
          input.branchId,
          input.departmentId,
          input.workerType,
          input.employmentType,
          input.startDate,
          input.jobTitle,
          input.currency,
          input.managerPersonId,
          actor.userId,
          now,
          now,
        ),
    );
    steps.push(
      write
        .prepare(
          "INSERT INTO employees(id,person_id,company_id,branch_id,department_id,display_name,status,created_at) VALUES(?,?,?,?,?,?,'active',?)",
        )
        .bind(
          `employee-${commandId}`,
          id,
          actor.companyId,
          input.branchId,
          input.departmentId,
          input.legalName,
          now,
        ),
    );
  }
  if (levelId) {
    steps.push(
      ...levelChangeStatements(
        write,
        commandId,
        actor,
        engagement,
        levelId,
        current?.level_id ? String(current.level_id) : null,
        "Engagement details updated",
      ),
      write
        .prepare("UPDATE hr_engagements SET level_id=? WHERE id=?")
        .bind(levelId, engagement),
    );
  }
  if (input.linkedMemberUserId)
    steps.push(
      write
        .prepare("UPDATE hr_people SET user_id=? WHERE id=?")
        .bind(input.linkedMemberUserId, id),
      write
        .prepare("UPDATE employees SET user_id=? WHERE person_id=?")
        .bind(input.linkedMemberUserId, id),
    );
  steps.push(
    write.prepare("UPDATE hr_engagements SET position_id=? WHERE id=?").bind(positionId,engagement),
    write.prepare("INSERT INTO staff_organization_history(id,company_id,engagement_id,before_json,after_json,actor_id,reason,command_id,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(`org-history-${commandId}`,actor.companyId,engagement,JSON.stringify({positionId:current?.position_id??null,levelId:current?.level_id??null,managerPersonId:current?.manager_person_id??null,branchId:current?.branch_id??null,departmentId:current?.department_id??null}),JSON.stringify({positionId,levelId,managerPersonId:input.managerPersonId,branchId:input.branchId,departmentId:input.departmentId}),actor.userId,input.reason,commandId,now),
  );
  return executeCommand(
    write,
    actor,
    commandId,
    current ? "people.directory.update" : "people.directory.create",
    input,
    id,
    version,
    { id, version },
    steps,
  );
}
export async function setDirectoryStatus(
  actor: WorkContext,
  personId: string,
  input: z.infer<typeof directoryStatusSchema>,
  commandId: string,
) {
  actor = manage(actor);
  const { write } = await holdingResources(actor.holdingId);
  const replay = await replayCommand(write, actor, commandId, "people.directory.status", personId, input);
  if (replay) return replay;
  const person = await directoryPerson(actor, personId);
  const graphGuard = await organizationGuard(write, actor.companyId, commandId);
  if (!actor.beta) {
    if (input.status === "inactive")
      throw new ApiError(409, "LIFECYCLE_REQUIRED", "Use Suspend access or End employment to review all linked accounts and the effective time.");
    if (input.status === "archived" && person.engagement_status !== "ended")
      throw new ApiError(409, "ENGAGEMENT_NOT_ENDED", "End employment before archiving its history.");
    if (input.status === "active" && person.status !== "archived" && await write.prepare("SELECT 1 FROM people_access_cutoffs WHERE company_id=? AND person_id=? AND cancelled_at IS NULL AND restored_at IS NULL").bind(actor.companyId,personId).first())
      throw new ApiError(409, "LIFECYCLE_REQUIRED", "Request new access through the lifecycle workflow before returning to work.");
  }
  if(input.status === "active" && person.engagement_status !== "ended") await validateStaffOrganization(write,actor.companyId,{person_id:personId,branch_id:person.branch_id as string|null,department_id:person.department_id as string|null,position_id:person.position_id as string|null,level_id:person.level_id as string|null,manager_person_id:person.manager_person_id as string|null},true);
  if (person.user_id === actor.userId && input.status !== "active")
    throw new ApiError(
      409,
      "SELF_DEACTIVATION",
      "Another HR administrator must deactivate your employee record.",
    );
  return executeCommand(
    write,
    actor,
    commandId,
    "people.directory.status",
    input,
    personId,
    input.expectedVersion + 1,
    { id: personId, status: input.status, version: input.expectedVersion + 1 },
    [
      graphGuard,
      ...(!actor.beta && input.status === "archived" ? [assertion(write,commandId,"ended-engagements","NOT EXISTS(SELECT 1 FROM hr_engagements WHERE company_id=? AND person_id=? AND status!='ended')",[actor.companyId,personId])] : []),
      assertion(
        write,
        commandId,
        "version",
        "EXISTS(SELECT 1 FROM hr_people WHERE id=? AND version=?)",
        [personId, input.expectedVersion],
      ),
      ...(input.status === "active"
        ? [
            assertion(
              write,
              commandId,
              "active-level",
              "NOT EXISTS(SELECT 1 FROM hr_engagements e LEFT JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id WHERE e.person_id=? AND e.status!='ended' AND (e.level_id IS NULL OR l.active IS DISTINCT FROM 1))",
              [personId],
            ),
          ]
        : []),
      write
        .prepare(
          "UPDATE hr_people SET status=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(input.status, Date.now(), personId),
      write
        .prepare(
          "UPDATE hr_engagements SET status=?,version=version+1,updated_at=? WHERE person_id=? AND status!='ended'",
        )
        .bind(
          input.status === "active" ? "active" : "suspended",
          Date.now(),
          personId,
        ),
      write
        .prepare("UPDATE employees SET status=? WHERE person_id=?")
        .bind(input.status === "active" && person.engagement_status !== "ended" ? "active" : "inactive", personId),
      // Revoke this company's access; restoration always requires an explicit new assignment.
      ...(input.status === "active" || !person.user_id
        ? []
        : [
            write
              .prepare(
                "UPDATE assignments SET revoked_at=?,version=version+1 WHERE user_id=? AND company_id=? AND revoked_at IS NULL",
              )
              .bind(Date.now(), person.user_id, actor.companyId),
          ]),
    ],
  );
}
export async function personAccess(actor: WorkContext, personId: string) {
  const person = await directoryPerson(actor, personId);
  if (!canManagePeople(actor) && person.user_id !== actor.userId) return null;
  if (!person.user_id) return { account: null, assignments: [] };
  const { write } = await holdingResources(actor.holdingId);
  const env = await runtimeEnv();
  const account = await env.CONTROL_DB.prepare(
    'SELECT id,name,email FROM "user" WHERE id=?',
  )
    .bind(person.user_id)
    .first();
  const assignments = await write
    .prepare(
      "SELECT id,role,scope_kind,scope_node_id,company_id,valid_until,revoked_at FROM assignments WHERE user_id=? AND company_id=? ORDER BY role",
    )
    .bind(person.user_id, actor.companyId)
    .all<Record<string, string | number | null>>();
  return {
    account,
    assignments: assignments.results.map(
      (a): Record<string, string | number | null> => ({
        ...a,
        access_status: a.revoked_at
          ? "revoked"
          : Number(a.valid_until) <= Date.now()
            ? "expired"
            : "active",
      }),
    ),
  };
}
