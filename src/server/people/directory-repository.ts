import "server-only";
import type { WorkContext } from "../authorization";
import type { SqlDatabase } from "../runtime/types";
import type { HoldingDirectoryPerson, HoldingPeopleDirectory, HoldingDirectoryNode } from "../holding-people-directory";

type CompanyRow = { id: string; slug: string; display_name: string };
type NodeRow = CompanyRow & {
  company_id: string;
  kind: HoldingDirectoryNode["kind"];
  parent_id: string | null;
  branch_id: string | null;
};
type LevelRow = {
  id: string;
  company_id: string;
  code: string;
  name: string;
  level_number: number | null;
};
type PositionRow = {
  id: string;
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  title: string;
  level_id: string;
  reports_to_position_id: string | null;
  is_ceo: number;
};
type PersonRow = {
  person_id: string;
  user_id: string | null;
  engagement_id: string;
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  position_id: string | null;
  legal_name: string;
  job_title: string;
  work_email: string | null;
  work_phone: string | null;
};

export type DirectorySelection = {
  kind: "company" | "branch" | "department" | "team" | "position" | "person" | "level" | "unassigned";
  id: string;
};
export type DirectorySelectionRecord = {
  companyId: string;
  person?: { id: string; personId: string; engagementId: string; companyId: string };
};

const currentPeoplePredicate = `p.status='active' AND p.merged_into_id IS NULL
  AND e.status IN ('active','suspended')
  AND e.start_date<=CURRENT_DATE::text
  AND (e.end_date IS NULL OR e.end_date>=CURRENT_DATE::text)`;
const archivedPeopleFrom = `FROM hr_people p
  LEFT JOIN LATERAL (
    SELECT x.id,x.company_id,x.branch_id,x.department_id,x.position_id,x.job_title,x.status
    FROM hr_engagements x WHERE x.person_id=p.id AND x.company_id=p.company_id
    ORDER BY CASE WHEN x.status IN ('active','onboarding','suspended') THEN 0 ELSE 1 END,
      x.start_date DESC,x.id LIMIT 1
  ) e ON true
  LEFT JOIN organization_positions op
    ON op.id=e.position_id AND op.company_id=p.company_id AND op.active=1
  WHERE p.company_id=? AND p.merged_into_id IS NULL
    AND (p.status='archived' OR e.status='ended')
    AND NOT EXISTS (SELECT 1 FROM hr_engagements current
      WHERE current.person_id=p.id AND current.company_id=p.company_id
        AND current.status IN ('active','onboarding','suspended'))
    AND (?::text IS NULL OR e.branch_id=?)
    AND (?::text IS NULL OR e.department_id=?)`;
const archivedScope = (actor: WorkContext) => [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId];

export async function directoryAssignmentActive(write: SqlDatabase, actor: WorkContext) {
  const now = Date.now();
  return !!await write.prepare(`
    SELECT 1 FROM assignments a
    JOIN organization_nodes c ON c.id=a.company_id AND c.kind='company' AND c.status='active'
    WHERE a.id=? AND a.user_id=? AND a.role=? AND a.company_id=?
      AND a.scope_kind=? AND a.revoked_at IS NULL
      AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND a.valid_from<=? AND a.valid_until>?
  `).bind(actor.assignmentId, actor.userId, actor.role, actor.companyId, actor.assignmentScope, now, now).first();
}

/** Public read model. Never pass profile, account, or compensation rows to a client. */
export async function readHoldingPeopleDirectory(write: SqlDatabase): Promise<HoldingPeopleDirectory> {
  // Every query runs against the resource bound to actor.holdingId. JOINs to
  // active Company roots prevent malformed cross-company references leaking in.
  const [companiesResult, nodesResult, levelsResult, positionsResult, peopleResult] = await Promise.all([
    write.prepare(`SELECT id,slug,display_name FROM organization_nodes
      WHERE kind='company' AND status='active' ORDER BY display_name,id`).all<CompanyRow>(),
    write.prepare(`SELECT n.id,n.company_id,n.kind,n.slug,n.display_name,n.parent_id,n.branch_id
      FROM organization_nodes n JOIN organization_nodes c
        ON c.id=n.company_id AND c.kind='company' AND c.status='active'
      WHERE n.status='active' ORDER BY n.company_id,n.kind,n.display_name,n.id`).all<NodeRow>(),
    write.prepare(`SELECT l.id,l.company_id,l.code,l.name,l.level_number
      FROM staff_levels l JOIN organization_nodes c
        ON c.id=l.company_id AND c.kind='company' AND c.status='active'
      WHERE l.active=1 ORDER BY l.company_id,l.level_number NULLS LAST,l.name,l.id`).all<LevelRow>(),
    write.prepare(`SELECT p.id,p.company_id,p.branch_id,p.department_id,p.title,p.level_id,
        p.reports_to_position_id,p.is_ceo
      FROM organization_positions p JOIN organization_nodes c
        ON c.id=p.company_id AND c.kind='company' AND c.status='active'
      WHERE p.active=1 ORDER BY p.company_id,p.title,p.id`).all<PositionRow>(),
    write.prepare(`SELECT p.id AS person_id,COALESCE(p.user_id,(SELECT m.user_id FROM iam_members m WHERE m.person_id=p.id AND m.company_id=p.company_id AND m.status='active' ORDER BY m.user_id LIMIT 1)) AS user_id,e.id AS engagement_id,e.company_id,e.branch_id,
        e.department_id,e.position_id,p.legal_name,COALESCE(op.title,e.job_title) AS job_title,
        NULLIF(btrim(p.work_email),'') AS work_email,
        NULLIF(btrim(p.work_phone),'') AS work_phone
      FROM hr_engagements e JOIN hr_people p
        ON p.id=e.person_id AND p.company_id=e.company_id
      LEFT JOIN organization_positions op
        ON op.id=e.position_id AND op.company_id=e.company_id AND op.active=1
      JOIN organization_nodes c
        ON c.id=e.company_id AND c.kind='company' AND c.status='active'
      WHERE ${currentPeoplePredicate}
      ORDER BY p.legal_name,p.id,e.id`).all<PersonRow>(),
  ]);

  return {
    companies: companiesResult.results.map(row => ({ id: row.id, slug: row.slug, name: row.display_name })),
    nodes: nodesResult.results.map(row => ({
      id: row.id, companyId: row.company_id, kind: row.kind, slug: row.slug,
      name: row.display_name, parentId: row.parent_id, branchId: row.branch_id,
    })),
    levels: levelsResult.results.map(row => ({
      id: row.id, companyId: row.company_id, code: row.code, name: row.name, number: row.level_number,
    })),
    positions: positionsResult.results.map(row => ({
      id: row.id, companyId: row.company_id, branchId: row.branch_id,
      departmentId: row.department_id, title: row.title, levelId: row.level_id,
      reportsToPositionId: row.reports_to_position_id, isCeo: row.is_ceo === 1,
    })),
    people: peopleResult.results.map(row => ({
      id: row.engagement_id, personId: row.person_id, userId: row.user_id, engagementId: row.engagement_id,
      companyId: row.company_id, branchId: row.branch_id, departmentId: row.department_id,
      positionId: row.position_id, legalName: row.legal_name, jobTitle: row.job_title,
      workEmail: row.work_email, workPhone: row.work_phone,
    })),
  };
}

/** Archived profiles remain an HR view within the actor's selected Company scope. */
export async function readArchivedPeopleDirectory(write: SqlDatabase, actor: WorkContext): Promise<HoldingDirectoryPerson[]> {
  const result = await write.prepare(`SELECT p.id AS person_id,p.user_id,COALESCE(e.id,'') AS engagement_id,
      p.company_id,e.branch_id,e.department_id,e.position_id,p.legal_name,
      COALESCE(op.title,e.job_title,'') AS job_title,
      NULLIF(btrim(p.work_email),'') AS work_email,
      NULLIF(btrim(p.work_phone),'') AS work_phone
    ${archivedPeopleFrom}
    ORDER BY p.legal_name,p.id`).bind(...archivedScope(actor)).all<PersonRow>();
  return result.results.map(row => ({
    id: row.engagement_id || `archived:${row.person_id}`, userId: row.user_id,
    personId: row.person_id, engagementId: row.engagement_id,
    companyId: row.company_id, branchId: row.branch_id, departmentId: row.department_id,
    positionId: row.position_id, legalName: row.legal_name, jobTitle: row.job_title,
    workEmail: row.work_email, workPhone: row.work_phone,
  }));
}

/** Resolve a detail target with the same visibility rules as the public tree. */
export async function readDirectorySelection(
  write: SqlDatabase, actor: WorkContext, selection: DirectorySelection, includeArchived: boolean,
): Promise<DirectorySelectionRecord | null> {
  if (selection.kind === "person") {
    type SelectedPersonRow = { person_id: string; engagement_id: string; company_id: string };
    let person = await write.prepare(`SELECT p.id AS person_id,e.id AS engagement_id,e.company_id
      FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id
      JOIN organization_nodes c ON c.id=e.company_id AND c.kind='company' AND c.status='active'
      WHERE ${currentPeoplePredicate} AND e.id=?`).bind(selection.id).first<SelectedPersonRow>();
    if (!person && includeArchived) {
      person = await write.prepare(`SELECT p.id AS person_id,e.id AS engagement_id,p.company_id
        ${archivedPeopleFrom} AND e.id=?`).bind(...archivedScope(actor), selection.id).first<SelectedPersonRow>();
    }
    return person ? { companyId: person.company_id, person: {
      id: person.engagement_id, personId: person.person_id,
      engagementId: person.engagement_id, companyId: person.company_id,
    } } : null;
  }
  if (selection.kind === "company" || selection.kind === "unassigned") {
    return write.prepare(`SELECT id AS "companyId" FROM organization_nodes
      WHERE id=? AND kind='company' AND status='active'`).bind(selection.id).first<DirectorySelectionRecord>();
  }
  if (selection.kind === "position") {
    return write.prepare(`SELECT p.company_id AS "companyId" FROM organization_positions p
      JOIN organization_nodes c ON c.id=p.company_id AND c.kind='company' AND c.status='active'
      WHERE p.id=? AND p.active=1`).bind(selection.id).first<DirectorySelectionRecord>();
  }
  if (selection.kind === "level") {
    return write.prepare(`SELECT l.company_id AS "companyId" FROM staff_levels l
      JOIN organization_nodes c ON c.id=l.company_id AND c.kind='company' AND c.status='active'
      WHERE l.id=? AND l.active=1`).bind(selection.id).first<DirectorySelectionRecord>();
  }
  return write.prepare(`SELECT n.company_id AS "companyId" FROM organization_nodes n
    JOIN organization_nodes c ON c.id=n.company_id AND c.kind='company' AND c.status='active'
    WHERE n.id=? AND n.status='active' AND (n.kind=? OR (?='branch' AND n.kind='headquarters'))`)
    .bind(selection.id, selection.kind, selection.kind).first<DirectorySelectionRecord>();
}
