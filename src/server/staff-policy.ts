import { policyRead, policyNow } from "./iam-read-snapshot";
import { permissionForRoles } from "@/lib/permissions";
import type { SqlDatabase } from "./runtime/types";
import type { WorkContext } from "./authorization";
import { assertion } from "./commands";
import { ApiError } from "./http";

export type StaffActor = Pick<
  WorkContext,
  "userId" | "assignmentId" | "role" | "companyId" | "branchId" | "departmentId" | "authority" | "iam"
>;
export type RecordScope = {
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
};
export type StaffPerson = RecordScope & {
  person_id: string;
  user_id: string;
  legal_name: string;
  engagement_id: string;
  engagement_version: number;
  worker_type: "employee" | "contractor";
  start_date: string;
  end_date: string | null;
  manager_person_id: string | null;
  level_id: string | null;
  level_name: string | null;
  level_rank: number | null;
  level_number?: number | null;
  position_id?: string | null;
  is_ceo?: number | null;
  self_service_top_level?: boolean;
};
// Account access and employment eligibility are separate from the currently selected business role.
export const activeStaffSelect = `SELECT p.id person_id,p.user_id,p.legal_name,e.id engagement_id,e.version engagement_version,
  e.company_id,e.branch_id,e.department_id,e.worker_type,e.start_date,e.end_date,e.manager_person_id,e.level_id,l.name level_name,l.rank level_rank,l.level_number,e.position_id,pos.is_ceo,e.self_service_top_level
  FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
  LEFT JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id
  LEFT JOIN organization_positions pos ON pos.id=e.position_id AND pos.company_id=e.company_id AND pos.active=1
  WHERE p.company_id=? AND p.status='active' AND p.merged_into_id IS NULL AND p.user_id IS NOT NULL
  AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))
  AND EXISTS(SELECT 1 FROM assignments a WHERE a.user_id=p.user_id AND a.company_id=p.company_id
    AND a.revoked_at IS NULL AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND a.valid_from<=? AND a.valid_until>?)`;

export async function staffPeople(
  db: SqlDatabase,
  companyId: string,
  filters: { departmentId?: string; levelId?: string } = {},
) {
  const now = Date.now();
  const { results } = await db
    .prepare(
      `${activeStaffSelect} AND (?::text IS NULL OR e.department_id=?) AND (?::text IS NULL OR (?='unset' AND e.level_id IS NULL) OR e.level_id=?) ORDER BY p.legal_name,e.start_date DESC,e.id`,
    )
    .bind(
      companyId,
      now,
      now,
      filters.departmentId ?? null,
      filters.departmentId ?? null,
      filters.levelId ?? null,
      filters.levelId ?? null,
      filters.levelId ?? null,
    )
    .all<StaffPerson>();
  return results;
}
export async function activeStaff(
  db: SqlDatabase,
  companyId: string,
  userId: string,
): Promise<StaffPerson> {
  return policyRead(db, JSON.stringify(["active-staff", companyId, userId]), () => readActiveStaff(db, companyId, userId));
}
async function readActiveStaff(db: SqlDatabase, companyId: string, userId: string): Promise<StaffPerson> {
  const now = Date.now();
  const { results } = await db
    .prepare(`${activeStaffSelect} AND iam_same_person(p.user_id,?,p.company_id)`)
    .bind(companyId, now, now, userId)
    .all<StaffPerson>();
  if (results.length !== 1)
    throw new ApiError(
      409,
      "STAFF_LINK_REQUIRED",
      results.length > 1
        ? "Multiple active engagements need correction before this action."
        : "An active employee or contractor engagement and account are required in this company.",
    );
  return results[0];
}
export function staffGuard(
  db: SqlDatabase,
  key: string,
  companyId: string,
  userId: string,
  step = `staff-${userId}`,
) {
  const now = Date.now();
  return assertion(
    db,
    key,
    step,
    `(SELECT COUNT(*) FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
    WHERE iam_same_person(p.user_id,?,p.company_id) AND p.company_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active'
    AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))=1
    AND EXISTS(SELECT 1 FROM assignments WHERE iam_same_person(user_id,?,company_id) AND company_id=? AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?)`,
    [userId, companyId, userId, companyId, now, now],
  );
}
export async function companyAuthority(
  db: SqlDatabase,
  actor: Pick<StaffActor, "userId" | "companyId" | "assignmentId" | "role">,
  roles:string[]|string = "organization.manage",
) {
  return policyRead(db, JSON.stringify(["company-authority", actor, roles]), async () => {
  const now = policyNow();
  return !!(await db
    .prepare(
      `SELECT 1 FROM assignments WHERE id=? AND company_id=? AND iam_role_allows(role,company_id,?) AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND scope_kind='holding' AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>? LIMIT 1`,
    )
    .bind(actor.assignmentId, actor.companyId, (typeof roles==="string"?roles:roles.includes("it_admin")?"ai.manage":roles.length===2&&roles.includes("hr_admin")?"organization.manage":permissionForRoles(roles)), now, now)
    .first());
  });
}
export function authorityGuard(
  db: SqlDatabase,
  key: string,
  actor: Pick<StaffActor, "userId" | "companyId" | "assignmentId" | "role">,
  roles:string[]|string = "organization.manage",
) {
  const now = Date.now();
  return assertion(
    db,
    key,
    "company-authority",
    `EXISTS(SELECT 1 FROM assignments WHERE id=? AND company_id=? AND iam_role_allows(role,company_id,?) AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND scope_kind='holding' AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?)`,
    [actor.assignmentId, actor.companyId, (typeof roles==="string"?roles:roles.includes("it_admin")?"ai.manage":roles.length===2&&roles.includes("hr_admin")?"organization.manage":permissionForRoles(roles)), now, now],
  );
}
export async function requireCompanyAuthority(
  db: SqlDatabase,
  actor: StaffActor,
  roles?: string[]|string,
) {
  if (!(await companyAuthority(db, actor, roles)))
    throw new ApiError(
      403,
      "COMPANY_AUTHORITY_REQUIRED",
      "A company-authorized HR administrator or owner is required.",
    );
}
export function scopeActor<T extends StaffActor>(
  actor: T,
  record: RecordScope,
): T {
  return {
    ...actor,
    companyId: record.company_id,
    branchId: record.branch_id,
    departmentId: record.department_id,
  };
}
export type RouteStep = {
  userId: string;
  authority: "named" | "department" | "hr_admin" | "holding_owner";
  candidateUserIds?: string[];
  requiredRole?: string;
  stage?: string;
  snapshot?: Record<string, unknown>;
};
export type ApprovalRoute = {
  steps: RouteStep[];
  blockedReason: string | null;
  scope: RecordScope;
  checks?: { query: string; args: (string | number | null)[] }[];
};

/** Traverse the full chain before selecting a reviewer so a cycle cannot silently confer authority. */
export function reportingChain(
  people: StaffPerson[],
  personId: string,
): StaffPerson[] {
  const chain: StaffPerson[] = [];
  const seen = new Set([personId]);
  let next = people.find((p) => p.person_id === personId)?.manager_person_id;
  while (next) {
    if (seen.has(next))
      throw new ApiError(
        409,
        "REPORTING_CYCLE",
        "A reporting cycle must be corrected before submission.",
      );
    seen.add(next);
    const matches = people.filter((p) => p.person_id === next);
    if (matches.length !== 1)
      throw new ApiError(
        409,
        "MANAGER_UNAVAILABLE",
        "The reporting manager needs one active engagement and account.",
      );
    chain.push(matches[0]);
    next = matches[0].manager_person_id;
    if (chain.length > 100)
      throw new ApiError(
        409,
        "REPORTING_CYCLE",
        "The reporting chain is too deep to resolve safely.",
      );
  }
  return chain;
}
export async function eligibleRoleUsers(
  db: SqlDatabase,
  scope: RecordScope,
  role: "hr_admin" | "holding_owner",
) {
  const now = Date.now();
  const { results } = await db
    .prepare(
      `SELECT DISTINCT user_id FROM assignments WHERE company_id=? AND iam_role_allows(role,company_id,'workflow.'||?) AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?
    AND (scope_kind='holding' OR (?='hr_admin' AND ((scope_kind='branch' AND scope_node_id=?) OR (scope_kind='department' AND scope_node_id=?)))) ORDER BY user_id`,
    )
    .bind(
      scope.company_id,
      role,
      now,
      now,
      role,
      scope.branch_id,
      scope.department_id,
    )
    .all<{ user_id: string }>();
  return results.map((x) => x.user_id);
}
export async function requestRoute(
  db: SqlDatabase,
  companyId: string,
  userId: string,
  kind: "expense" | "leave",
  excluded: string[] = [],
): Promise<ApprovalRoute> {
  let scope: RecordScope = {
    company_id: companyId,
    branch_id: null,
    department_id: null,
  };
  try {
    const staff = await activeStaff(db, companyId, userId);
    scope = staff;
    if (kind === "leave") {
      const people = await staffPeople(db, companyId);
      const chain = reportingChain(people, staff.person_id);
      const forbidden = new Set([userId, staff.user_id, ...excluded]);
      let manager = chain[0];
      let authority: RouteStep["authority"] = "named";
      if (!manager) {
        if(!staff.self_service_top_level)throw new ApiError(409,"MANAGER_REQUIRED","Ask HR to set your reporting manager, or explicitly confirm a top-level engagement in People settings.");
        const owners = await eligibleRoleUsers(db, scope, "holding_owner");
        const owner = owners.find(
          (id) => !forbidden.has(id) && people.filter((p) => p.user_id === id).length === 1,
        );
        if (!owner)
          throw new ApiError(
            409,
            "OWNER_REQUIRED",
            "Configure an independent company owner for this top-level staff member.",
          );
        manager = people.find((p) => p.user_id === owner)!;
        authority = "holding_owner";
      }
      if (forbidden.has(manager.user_id))
        throw new ApiError(409, "SELF_APPROVAL", "An independent reporting manager is required.");
      if(authority==='named'&&!await db.prepare(`SELECT 1 FROM assignments WHERE user_id=? AND company_id=? AND iam_role_allows(role,company_id,'workflow.people_manager') AND revoked_at IS NULL AND valid_from<=? AND valid_until>? AND iam_assignment_active(id,?) AND (scope_kind='holding' OR scope_kind='branch' AND scope_node_id=? OR scope_kind='department' AND scope_node_id=?)`).bind(manager.user_id,companyId,Date.now(),Date.now(),Date.now(),scope.branch_id,scope.department_id).first())throw new ApiError(409,'MANAGER_AUTHORITY_REQUIRED','Ask HR to give the reporting manager approval authority for your Company scope.');
      forbidden.add(manager.user_id);
      const hrUsers = await eligibleRoleUsers(db, scope, "hr_admin");
      const hrUser = hrUsers.find(
        (id) => !forbidden.has(id) && people.filter((p) => p.user_id === id).length === 1,
      );
      if (!hrUser)
        throw new ApiError(
          409,
          "HR_APPROVER_REQUIRED",
          "Configure an eligible HR reviewer independent of the requester and reporting manager.",
        );
      return {
        steps: [
          { userId: manager.user_id, authority },
          { userId: hrUser, authority: "hr_admin" },
        ],
        blockedReason: null,
        scope,
        checks: [staff, manager]
          .map((person) => ({
            query: "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=? AND status='active')",
            args: [person.engagement_id, companyId, person.engagement_version],
          })),
      };
    }
    if (!staff.department_id)
      throw new ApiError(
        409,
        "DEPARTMENT_REQUIRED",
        "Set the requester's department before submission.",
      );
    const head = await db
      .prepare(
        "SELECT h.person_id,h.version FROM department_heads h JOIN organization_nodes n ON n.id=h.department_id WHERE h.company_id=? AND h.department_id=? AND n.company_id=h.company_id AND n.status='active' AND n.kind='department'",
      )
      .bind(companyId, staff.department_id)
      .first<{ person_id: string; version: number }>();
    if (!head)
      throw new ApiError(
        409,
        "DEPARTMENT_HEAD_REQUIRED",
        "Configure a named head for the requester's department.",
      );
    const people = await staffPeople(db, companyId);
    const heads = people.filter((p) => p.person_id === head.person_id);
    if (heads.length !== 1)
      throw new ApiError(
        409,
        "HEAD_UNAVAILABLE",
        "The department head needs one active engagement and account.",
      );
    const chain = reportingChain(people, head.person_id);
    const forbidden = new Set([userId, ...excluded]);
    let reviewer = heads[0],
      authority: RouteStep["authority"] = "department";
    if (head.person_id === staff.person_id) {
      if (chain.length) reviewer = chain[0];
      else {
        const owners = await eligibleRoleUsers(db, scope, "holding_owner");
        const owner = owners.find(
          (id) =>
            !forbidden.has(id) &&
            people.filter((p) => p.user_id === id).length === 1,
        );
        if (!owner)
          throw new ApiError(
            409,
            "OWNER_REQUIRED",
            "Configure an independent company owner at the top of the reporting chain.",
          );
        reviewer = people.find((p) => p.user_id === owner)!;
        authority = "holding_owner";
      }
    }
    if (forbidden.has(reviewer.user_id))
      throw new ApiError(
        409,
        "SELF_APPROVAL",
        "An independent department approver is required.",
      );
    const steps: RouteStep[] = [{ userId: reviewer.user_id, authority }];
    return {
      steps,
      blockedReason: null,
      scope,
      checks: [
        {
          query:
            "EXISTS(SELECT 1 FROM department_heads h JOIN organization_nodes n ON n.id=h.department_id WHERE h.department_id=? AND h.company_id=? AND h.version=? AND n.status='active')",
          args: [staff.department_id, companyId, head.version],
        },
        ...[
          ...new Map(
            [staff, heads[0], ...chain].map((p) => [p.engagement_id, p]),
          ).values(),
        ].map((p) => ({
          query:
            "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=? AND status='active')",
          args: [p.engagement_id, companyId, p.engagement_version],
        })),
      ],
    };
  } catch (error) {
    if (!(error instanceof ApiError)) throw error;
    return { steps: [], blockedReason: error.message, scope };
  }
}
export function notification(
  db: SqlDatabase,
  scope: RecordScope,
  userId: string,
  type: string,
  id: string,
  version: number,
  message: string,
) {
  return db
    .prepare(
      "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING",
    )
    .bind(
      crypto.randomUUID(),
      scope.company_id,
      userId,
      type,
      id,
      version,
      message,
      Date.now(),
    );
}
