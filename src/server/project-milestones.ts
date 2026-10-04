import "server-only";
import { samePerson } from "@/lib/iam";
import { z } from "zod";
import { ApiError } from "./http";
import { assertion, executeCommand, replayCommand } from "./commands";
import { activeStaff, staffGuard, type StaffActor } from "./staff-policy";
import { getProject, type ProjectRow } from "./staff-tasks";
import type { SqlDatabase } from "./runtime/types";

export const projectMilestoneSchema = z.object({
  title: z.string().trim().min(2).max(200),
  description: z.string().trim().max(10000).default(""),
  dueAt: z.iso.datetime({ offset: true }),
  ownerUserId: z.string().min(1).max(160),
  parentMilestoneId: z.string().min(1).max(200).nullable().optional(),
  ordinalPosition: z.number().int().min(0).max(1000000).optional(),
  dependsOnMilestoneIds: z.array(z.string().min(1).max(200)).max(50).refine((ids) => new Set(ids).size === ids.length, "Prerequisite milestones must be unique.").optional(),
  expectedVersion: z.number().int().positive().default(1),
  reason: z.string().trim().min(3).max(2000).default("Milestone details reviewed by the project owner."),
}).strict();

export const projectMilestoneTransitionSchema = z.object({
  action: z.enum(["start", "complete", "reopen", "cancel"]),
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
}).strict();

export type ProjectMilestoneRow = {
  id: string;
  company_id: string;
  project_id: string;
  parent_milestone_id: string | null;
  ordinal_position: number;
  title: string;
  description: string;
  due_at: number;
  owner_user_id: string;
  owner_name: string | null;
  state: "planned" | "in_progress" | "completed" | "cancelled";
  is_overdue?: boolean;
  version: number;
  created_by: string;
  completed_at: number | null;
  created_at: number;
  updated_at: number;
  depends_on_ids: string[];
  depends_on_titles: string[];
  blocked_by_titles: string[];
  blocked_by_count: number;
};

type MilestoneInput = z.infer<typeof projectMilestoneSchema>;
type MilestoneEventAction = "created" | "revised" | "started" | "completed" | "reopened" | "cancelled";

/** Preserve durable owner constraints as reviewable lifecycle conflicts. */
export function projectConstraint(error:unknown):never{
  const message=error instanceof Error?error.message:'';
  if((error as {code?:string}).code==='23514'&&/^PROJECT_[A-Z_]+$/.test(message)){
    const descriptions:Record<string,string>={PROJECT_STRUCTURE_FROZEN:'Reopen the project before changing its work or evidence.',PROJECT_PACKAGE_CHILDREN_INCOMPLETE:'Complete or cancel the active child packages before closing their parent.',PROJECT_PACKAGE_TASKS_INCOMPLETE:'Complete or cancel the assigned tasks before closing this work package.',PROJECT_PACKAGE_CYCLE_OR_DEPTH:'Choose a parent that keeps the work structure acyclic and within thirty levels.',PROJECT_EXECUTION_GRAPH_CYCLE:'The selected hierarchy and prerequisites would prevent the project from completing. Choose an acyclic work structure.'};
    const conflict=new ApiError(409,message,descriptions[message]??'The project source changed. Reload its current work structure and review again.');conflict.cause=error;throw conflict;
  }
  throw error;
}

function projectOwner(project: ProjectRow, actor: StaffActor) {
  if (!samePerson(actor, project.owner_user_id))
    throw new ApiError(403, "PROJECT_OWNER_REQUIRED", "Only the project owner manages milestones.");
}

async function assertProjectMember(db: SqlDatabase, project: ProjectRow, userId: string) {
  const member = !!(await db.prepare(`SELECT 1 FROM staff_projects p WHERE p.id=? AND p.company_id=?
    AND (iam_same_person(p.owner_user_id,?,p.company_id) OR EXISTS(
      SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?))`)
    .bind(project.id, project.company_id, userId, userId).first());
  if (!member) throw new ApiError(422, "PROJECT_MEMBER_REQUIRED", "Assign the milestone to the project owner or a current project member.");
  await activeStaff(db, project.company_id, userId);
}

async function loadMilestone(db: SqlDatabase, projectId: string, milestoneId: string) {
  const row = await db.prepare("SELECT * FROM staff_project_milestones WHERE id=? AND project_id=?")
    .bind(milestoneId, projectId).first<ProjectMilestoneRow>();
  if (!row) throw new ApiError(404, "PROJECT_MILESTONE_NOT_FOUND", "Project milestone not found.");
  return row;
}

function eventStatement(db: SqlDatabase, id: string, version: number, action: MilestoneEventAction, actor: StaffActor, reason: string, snapshot: Record<string, unknown>, now: number) {
  return db.prepare(`INSERT INTO staff_project_milestone_events
    (id,milestone_id,milestone_version,action,actor_id,reason,snapshot_json,created_at)
    VALUES(?,?,?,?,?,?,?::jsonb,?)`).bind(`${id}:${version}`, id, version, action, actor.userId, reason, JSON.stringify(snapshot), now);
}

export async function listProjectMilestones(db: SqlDatabase, actor: StaffActor, projectId: string) {
  await getProject(db, actor, projectId);
  const rows = (await db.prepare(`SELECT m.*,p.legal_name owner_name,
      dependency.depends_on_ids,dependency.depends_on_titles,dependency.blocked_by_titles,
      (m.state IN ('planned','in_progress') AND m.due_at<floor(extract(epoch FROM clock_timestamp())*1000)::bigint) is_overdue
    FROM staff_project_milestones m
    LEFT JOIN hr_people p ON p.company_id=m.company_id AND p.user_id=m.owner_user_id AND p.merged_into_id IS NULL
    LEFT JOIN LATERAL (
      SELECT COALESCE(jsonb_agg(pr.id ORDER BY pr.due_at,pr.title),'[]'::jsonb)::text depends_on_ids,
        COALESCE(jsonb_agg(pr.title ORDER BY pr.due_at,pr.title),'[]'::jsonb)::text depends_on_titles,
        COALESCE(jsonb_agg(pr.title ORDER BY pr.due_at,pr.title) FILTER (WHERE pr.state<>'completed'),'[]'::jsonb)::text blocked_by_titles
      FROM staff_project_milestone_dependencies d
      JOIN staff_project_milestones pr ON pr.id=d.predecessor_milestone_id AND pr.company_id=d.company_id AND pr.project_id=d.project_id
      WHERE d.company_id=m.company_id AND d.project_id=m.project_id AND d.successor_milestone_id=m.id
    ) dependency ON true
    WHERE m.company_id=? AND m.project_id=? ORDER BY m.due_at,m.id`)
    .bind(actor.companyId, projectId).all<ProjectMilestoneRow & { depends_on_ids: string; depends_on_titles: string; blocked_by_titles: string }>()).results;
  return rows.map((row) => {
    const parse = (value: string) => {
      try { const parsed = JSON.parse(value) as unknown; return Array.isArray(parsed) ? parsed.filter((item): item is string => typeof item === "string") : []; }
      catch { return []; }
    };
    const { depends_on_ids, depends_on_titles, blocked_by_titles, ...milestone } = row;
    const blockedByTitles = parse(blocked_by_titles);
    return { ...milestone, depends_on_ids: parse(depends_on_ids), depends_on_titles: parse(depends_on_titles), blocked_by_titles: blockedByTitles, blocked_by_count: blockedByTitles.length };
  });
}

export async function saveProjectMilestone(db: SqlDatabase, actor: StaffActor, projectId: string, milestoneId: string | null, input: MilestoneInput, key: string) {
  const id = milestoneId ?? `milestone-${key}`;
  const operation = milestoneId ? "project.milestone.revise" : "project.milestone.create";
  const prior = await replayCommand<{ id: string; version: number }>(db, actor, key, operation, id, { projectId, ...input });
  if (prior) return prior;
  const project = await getProject(db, actor, projectId);
  projectOwner(project, actor);
  await assertProjectMember(db, project, input.ownerUserId);
  const current = milestoneId ? await loadMilestone(db, projectId, milestoneId) : null;
  const parentId=input.parentMilestoneId===undefined?current?.parent_milestone_id??null:input.parentMilestoneId,position=input.ordinalPosition??current?.ordinal_position??0;
  const parent=parentId?await db.prepare('SELECT id,version,state FROM staff_project_milestones WHERE id=? AND company_id=? AND project_id=?').bind(parentId,actor.companyId,project.id).first<{id:string;version:number;state:string}>():null;
  if(parentId&&(!parent||parentId===id))throw new ApiError(422,'PROJECT_PACKAGE_PARENT_INVALID','Choose a different parent package in this project.');
  if(parent&&parent.state!=='planned'&&parent.state!=='in_progress')throw new ApiError(409,'PROJECT_PACKAGE_PARENT_TERMINAL','Reopen the parent package before adding or revising its work.');
  if (current && (current.state === "completed" || current.state === "cancelled"))
    throw new ApiError(409, "PROJECT_MILESTONE_TERMINAL", "Reopen the milestone before editing its details.");
  const priorDependencies = current ? (await db.prepare(`SELECT predecessor_milestone_id FROM staff_project_milestone_dependencies
    WHERE company_id=? AND project_id=? AND successor_milestone_id=? ORDER BY predecessor_milestone_id`)
    .bind(actor.companyId, project.id, id).all<{ predecessor_milestone_id: string }>()).results.map((row) => row.predecessor_milestone_id) : [];
  const dependencyIds = input.dependsOnMilestoneIds ?? priorDependencies;
  if (dependencyIds.includes(id)) throw new ApiError(422, "PROJECT_DEPENDENCY_CYCLE", "A milestone cannot depend on itself.");
  const referenced = dependencyIds.length ? (await db.prepare(`SELECT milestone.id,milestone.state FROM staff_project_milestones milestone
    JOIN jsonb_array_elements_text(?::jsonb) chosen(value) ON chosen.value=milestone.id
    WHERE milestone.company_id=? AND milestone.project_id=?`)
    .bind(JSON.stringify(dependencyIds), actor.companyId, project.id).all<{ id: string; state: string }>()).results : [];
  if (referenced.length !== dependencyIds.length)
    throw new ApiError(422, "PROJECT_DEPENDENCY_SCOPE_INVALID", "Prerequisites must be milestones in this project.");
  if (referenced.some((row) => row.state === "cancelled"))
    throw new ApiError(422, "PROJECT_DEPENDENCY_CANCELLED", "A cancelled milestone cannot be a prerequisite.");
  if (dependencyIds.length) {
    const cycle = await db.prepare(`WITH RECURSIVE reachable(milestone_id,path) AS (
        SELECT ?::text, ARRAY[?::text]
        UNION ALL
        SELECT d.successor_milestone_id,reachable.path||d.successor_milestone_id
        FROM reachable JOIN staff_project_milestone_dependencies d
          ON d.predecessor_milestone_id=reachable.milestone_id AND d.company_id=? AND d.project_id=?
        WHERE NOT d.successor_milestone_id=ANY(reachable.path)
      ) SELECT EXISTS(SELECT 1 FROM jsonb_array_elements_text(?::jsonb) candidate
        JOIN reachable ON reachable.milestone_id=candidate.value) cyclic`)
      .bind(id, id, actor.companyId, project.id, JSON.stringify(dependencyIds)).first<{ cyclic: boolean }>();
    if (cycle?.cyclic) throw new ApiError(422, "PROJECT_DEPENDENCY_CYCLE", "This prerequisite would create a project dependency cycle.");
  }
  const now = Date.now(), version = current ? input.expectedVersion + 1 : 1;
  const snapshot = { projectId, title: input.title, description: input.description, dueAt: Date.parse(input.dueAt), ownerUserId: input.ownerUserId, dependsOnMilestoneIds: dependencyIds,parentMilestoneId:parentId,ordinalPosition:position };
  const dependencyJson = JSON.stringify(dependencyIds);
  return executeCommand(db, actor, key, operation, { projectId, ...input }, id, version, { id, version }, [
    db.prepare("SELECT id FROM staff_projects WHERE id=? AND company_id=? FOR UPDATE").bind(project.id, actor.companyId),
    staffGuard(db, key, actor.companyId, actor.userId),
    staffGuard(db, key, actor.companyId, input.ownerUserId, "milestone-owner"),
    assertion(db, key, "project-owner", "EXISTS(SELECT 1 FROM staff_projects WHERE id=? AND company_id=? AND version=? AND iam_same_person(owner_user_id,?,company_id))", [project.id, actor.companyId, project.version, actor.userId]),
    ...(parent?[assertion(db,key,'package-parent',"EXISTS(SELECT 1 FROM staff_project_milestones WHERE id=? AND company_id=? AND project_id=? AND version=? AND state IN ('planned','in_progress'))",[parent.id,actor.companyId,project.id,parent.version])]:[]),
    assertion(db, key, "milestone-prerequisites", `jsonb_array_length(?::jsonb)=(SELECT count(*) FROM jsonb_array_elements_text(?::jsonb) candidate
      JOIN staff_project_milestones dependency ON dependency.id=candidate.value AND dependency.company_id=? AND dependency.project_id=? AND dependency.state<>'cancelled'
      WHERE dependency.id<>?)`, [dependencyJson, dependencyJson, actor.companyId, project.id, id]),
    assertion(db, key, "milestone-prerequisites-acyclic", `NOT EXISTS(
      WITH RECURSIVE reachable(milestone_id,path) AS (
        SELECT ?::text, ARRAY[?::text]
        UNION ALL
        SELECT d.successor_milestone_id,reachable.path||d.successor_milestone_id
        FROM reachable JOIN staff_project_milestone_dependencies d
          ON d.predecessor_milestone_id=reachable.milestone_id AND d.company_id=? AND d.project_id=?
        WHERE NOT d.successor_milestone_id=ANY(reachable.path)
      ) SELECT 1 FROM jsonb_array_elements_text(?::jsonb) candidate JOIN reachable ON reachable.milestone_id=candidate.value
    )`, [id, id, actor.companyId, project.id, dependencyJson]),
    ...(current ? [
      assertion(db, key, "milestone-version", "EXISTS(SELECT 1 FROM staff_project_milestones WHERE id=? AND project_id=? AND company_id=? AND version=? AND state IN ('planned','in_progress'))", [id, project.id, actor.companyId, input.expectedVersion]),
      db.prepare(`UPDATE staff_project_milestones SET title=?,description=?,due_at=?,owner_user_id=?,parent_milestone_id=?,ordinal_position=?,version=version+1,updated_at=? WHERE id=?`)
        .bind(input.title, input.description, Date.parse(input.dueAt), input.ownerUserId,parentId,position,now,id),
    ] : [
      assertion(db, key, "project-member", "EXISTS(SELECT 1 FROM staff_projects p WHERE p.id=? AND p.company_id=? AND (iam_same_person(p.owner_user_id,?,p.company_id) OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?)))", [project.id, actor.companyId, input.ownerUserId, input.ownerUserId]),
      db.prepare(`INSERT INTO staff_project_milestones(id,company_id,project_id,title,description,due_at,owner_user_id,parent_milestone_id,ordinal_position,state,version,created_by,completed_at,created_at,updated_at)
        VALUES(?,?,?,?,?,?,?,?,?,'planned',1,?,NULL,?,?)`)
        .bind(id, actor.companyId, project.id, input.title, input.description, Date.parse(input.dueAt), input.ownerUserId,parentId,position,actor.userId,now,now),
    ]),
    db.prepare("DELETE FROM staff_project_milestone_dependencies WHERE company_id=? AND project_id=? AND successor_milestone_id=?")
      .bind(actor.companyId, project.id, id),
    ...dependencyIds.map((predecessorId) => db.prepare(`INSERT INTO staff_project_milestone_dependencies
      (company_id,project_id,predecessor_milestone_id,successor_milestone_id,created_by,created_at) VALUES(?,?,?,?,?,?)`)
      .bind(actor.companyId, project.id, predecessorId, id, actor.userId, now)),
    db.prepare("UPDATE staff_projects SET version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(now, project.id, actor.companyId),
    eventStatement(db, id, version, current ? "revised" : "created", actor, input.reason, snapshot, now),
  ], { companyId: project.company_id, branchId: project.branch_id, departmentId: project.department_id }).catch(projectConstraint);
}

export async function transitionProjectMilestone(db: SqlDatabase, actor: StaffActor, projectId: string, milestoneId: string, input: z.infer<typeof projectMilestoneTransitionSchema>, key: string) {
  const operation = `project.milestone.${input.action}`;
  const prior = await replayCommand<{ id: string; version: number; state: ProjectMilestoneRow["state"] }>(db, actor, key, operation, milestoneId, { projectId, ...input });
  if (prior) return prior;
  const project = await getProject(db, actor, projectId);
  projectOwner(project, actor);
  const current = await loadMilestone(db, projectId, milestoneId);
  const nextState = input.action === "start" ? "in_progress"
    : input.action === "complete" ? "completed"
      : input.action === "reopen" ? "in_progress" : "cancelled";
  const allowed = input.action === "start" ? current.state === "planned"
    : input.action === "complete" ? current.state === "planned" || current.state === "in_progress"
      : input.action === "reopen" ? current.state === "completed"
        : current.state === "planned" || current.state === "in_progress";
  if (!allowed) throw new ApiError(409, "PROJECT_MILESTONE_TRANSITION_INVALID", "This milestone cannot move to the requested state.");
  const incompletePrerequisites = (await db.prepare(`SELECT p.title FROM staff_project_milestone_dependencies d
    JOIN staff_project_milestones p ON p.id=d.predecessor_milestone_id AND p.company_id=d.company_id AND p.project_id=d.project_id
    WHERE d.company_id=? AND d.project_id=? AND d.successor_milestone_id=? AND p.state<>'completed' ORDER BY p.title`)
    .bind(actor.companyId, project.id, milestoneId).all<{ title: string }>()).results;
  if ((input.action === "start" || input.action === "complete") && incompletePrerequisites.length)
    throw new ApiError(409, "PROJECT_DEPENDENCIES_INCOMPLETE", `Complete these prerequisites first: ${incompletePrerequisites.map((item) => item.title).join(", ")}.`);
  if (input.action === "cancel") {
    const activeSuccessors = await db.prepare(`SELECT 1 FROM staff_project_milestone_dependencies d
      JOIN staff_project_milestones s ON s.id=d.successor_milestone_id AND s.company_id=d.company_id AND s.project_id=d.project_id
      WHERE d.company_id=? AND d.project_id=? AND d.predecessor_milestone_id=? AND s.state IN ('planned','in_progress','completed') LIMIT 1`)
      .bind(actor.companyId, project.id, milestoneId).first();
    if (activeSuccessors) throw new ApiError(409, "PROJECT_MILESTONE_REQUIRED_PREREQUISITE", "Remove or complete dependent milestones before cancelling this prerequisite.");
  }
  if (input.action === "reopen") {
    const completedSuccessor = await db.prepare(`SELECT 1 FROM staff_project_milestone_dependencies d
      JOIN staff_project_milestones s ON s.id=d.successor_milestone_id AND s.company_id=d.company_id AND s.project_id=d.project_id
      WHERE d.company_id=? AND d.project_id=? AND d.predecessor_milestone_id=? AND s.state='completed' LIMIT 1`)
      .bind(actor.companyId, project.id, milestoneId).first();
    if (completedSuccessor) throw new ApiError(409, "PROJECT_MILESTONE_DEPENDENT_COMPLETED", "Reopen completed dependent milestones before reopening this prerequisite.");
  }
  const now = Date.now(), version = input.expectedVersion + 1;
  const snapshot = { projectId, state: nextState, dueAt: current.due_at, ownerUserId: current.owner_user_id };
  const action: MilestoneEventAction = input.action === "start" ? "started"
    : input.action === "complete" ? "completed"
      : input.action === "reopen" ? "reopened" : "cancelled";
  return executeCommand(db, actor, key, operation, { projectId, ...input }, milestoneId, version, { id: milestoneId, version, state: nextState }, [
    db.prepare("SELECT id FROM staff_projects WHERE id=? AND company_id=? FOR UPDATE").bind(project.id, actor.companyId),
    staffGuard(db, key, actor.companyId, actor.userId),
    assertion(db, key, "project-owner", "EXISTS(SELECT 1 FROM staff_projects WHERE id=? AND company_id=? AND version=? AND iam_same_person(owner_user_id,?,company_id))", [project.id, actor.companyId, project.version, actor.userId]),
    assertion(db, key, "milestone-version", "EXISTS(SELECT 1 FROM staff_project_milestones WHERE id=? AND project_id=? AND company_id=? AND version=? AND state=?)", [milestoneId, project.id, actor.companyId, input.expectedVersion, current.state]),
    assertion(db, key, "milestone-prerequisites-complete", `(? NOT IN ('start','complete') OR NOT EXISTS(
      SELECT 1 FROM staff_project_milestone_dependencies d JOIN staff_project_milestones p ON p.id=d.predecessor_milestone_id
      WHERE d.company_id=? AND d.project_id=? AND d.successor_milestone_id=? AND p.state<>'completed'))`,
      [input.action, actor.companyId, project.id, milestoneId]),
    assertion(db, key, "milestone-prerequisite-still-used", `(?<>'cancel' OR NOT EXISTS(
      SELECT 1 FROM staff_project_milestone_dependencies d JOIN staff_project_milestones s ON s.id=d.successor_milestone_id
      WHERE d.company_id=? AND d.project_id=? AND d.predecessor_milestone_id=? AND s.state IN ('planned','in_progress','completed')))` ,
      [input.action, actor.companyId, project.id, milestoneId]),
    assertion(db, key, "milestone-no-completed-successor", `(?<>'reopen' OR NOT EXISTS(
      SELECT 1 FROM staff_project_milestone_dependencies d JOIN staff_project_milestones s ON s.id=d.successor_milestone_id
      WHERE d.company_id=? AND d.project_id=? AND d.predecessor_milestone_id=? AND s.state='completed'))`,
      [input.action, actor.companyId, project.id, milestoneId]),
    db.prepare(`UPDATE staff_project_milestones SET state=?,completed_at=?,version=version+1,updated_at=? WHERE id=?`)
      .bind(nextState, nextState === "completed" ? now : null, now, milestoneId),
    db.prepare("UPDATE staff_projects SET version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(now, project.id, actor.companyId),
    eventStatement(db, milestoneId, version, action, actor, input.reason, snapshot, now),
  ], { companyId: project.company_id, branchId: project.branch_id, departmentId: project.department_id }).catch(projectConstraint);
}
