import { projectReadProjection as projectpeople_payrollProjection } from '@erpx/server-people_payroll/public/project-read-projections';
import 'server-only';
import { z } from 'zod';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { ApiError } from '@erpx/server-runtime/public/http';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {replayCommand,executeCommand} from '@erpx/server-workflow/public/commands';
import { requirePermission } from '@erpx/server-identity_access/public/iam-permissions';
import { activeStaff, staffGuard, type StaffActor } from '@erpx/server-operations/public/staff-policy';
import { getProject } from '@erpx/server-projects/public/staff-projects';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { projectMilestoneTransitionSchema, type ProjectMilestoneRow, type MilestoneInput, type MilestoneEventAction } from '@erpx/server-projects/domain/milestones';
import { createProjectMilestoneApplication, type ProjectMilestoneRepository } from '@erpx/server-projects/application/milestones';
import {requireProjectPurchasedRead,withProjectPurchasedCommand} from '@erpx/server-projects/adapters/purchases';
async function inward<T>(action: () => Promise<T>) { try {
    return await action();
}
catch (error) {
    if (error instanceof ApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
async function exposed<T>(action: () => Promise<T>) { try {
    return await action();
}
catch (error) {
    if (error instanceof BusinessRuleError) {
        const out = new ApiError(error.status, error.code, error.message);
        out.cause = error.cause;
        throw out;
    }
    throw error;
} }
function application(db: SqlDatabase) {
    const repository: ProjectMilestoneRepository<SqlStatement> = { async isProjectMember(id: string, company_id: string, userId: string, userId2: string) {
            return db.prepare(`SELECT 1 FROM staff_projects p WHERE p.id=? AND p.company_id=?
    AND (iam_same_person(p.owner_user_id,?,p.company_id) OR EXISTS(
      SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?))`)
                .bind(id, company_id, userId, userId2).first();
        },
        async milestone(milestoneId: string, projectId: string) {
            return db.prepare("SELECT * FROM staff_project_milestones WHERE id=? AND project_id=?")
                .bind(milestoneId, projectId).first<ProjectMilestoneRow>();
        },
        prepareMilestoneEvent(value: string, id: string, version: number, action: MilestoneEventAction, userId: string, reason: string, value2: string, now: number) {
            return db.prepare(`INSERT INTO staff_project_milestone_events
    (id,milestone_id,milestone_version,action,actor_id,reason,snapshot_json,created_at)
    VALUES(?,?,?,?,?,?,?::jsonb,?)`).bind(value, id, version, action, userId, reason, value2, now);
        },
        async milestoneRows(companyId: string, projectId: string) {
            return db.prepare(`SELECT m.*,p.legal_name owner_name,
      dependency.depends_on_ids,dependency.depends_on_titles,dependency.blocked_by_titles,
      (m.state IN ('planned','in_progress') AND m.due_at<floor(extract(epoch FROM clock_timestamp())*1000)::bigint) is_overdue
    FROM staff_project_milestones m
    ${projectpeople_payrollProjection.milestoneOwner}
    LEFT JOIN LATERAL (
      SELECT COALESCE(jsonb_agg(pr.id ORDER BY pr.due_at,pr.title),'[]'::jsonb)::text depends_on_ids,
        COALESCE(jsonb_agg(pr.title ORDER BY pr.due_at,pr.title),'[]'::jsonb)::text depends_on_titles,
        COALESCE(jsonb_agg(pr.title ORDER BY pr.due_at,pr.title) FILTER (WHERE pr.state<>'completed'),'[]'::jsonb)::text blocked_by_titles
      FROM staff_project_milestone_dependencies d
      JOIN staff_project_milestones pr ON pr.id=d.predecessor_milestone_id AND pr.company_id=d.company_id AND pr.project_id=d.project_id
      WHERE d.company_id=m.company_id AND d.project_id=m.project_id AND d.successor_milestone_id=m.id
    ) dependency ON true
    WHERE m.company_id=? AND m.project_id=? ORDER BY m.due_at,m.id`)
                .bind(companyId, projectId).all<Omit<ProjectMilestoneRow, "depends_on_ids" | "depends_on_titles" | "blocked_by_titles"> & {
                depends_on_ids: string;
                depends_on_titles: string;
                blocked_by_titles: string;
            }>();
        },
        async parentMilestone(parentId: string, companyId: string, id: string) { return db.prepare('SELECT id,version,state FROM staff_project_milestones WHERE id=? AND company_id=? AND project_id=?').bind(parentId, companyId, id).first<{
            id: string;
            version: number;
            state: string;
        }>(); },
        async previousDependencies(companyId: string, id: string, id2: string) {
            return db.prepare(`SELECT predecessor_milestone_id FROM staff_project_milestone_dependencies
    WHERE company_id=? AND project_id=? AND successor_milestone_id=? ORDER BY predecessor_milestone_id`)
                .bind(companyId, id, id2).all<{
                predecessor_milestone_id: string;
            }>();
        },
        async referencedDependencies(value: string, companyId: string, id: string) {
            return db.prepare(`SELECT milestone.id,milestone.state FROM staff_project_milestones milestone
    JOIN jsonb_array_elements_text(?::jsonb) chosen(value) ON chosen.value=milestone.id
    WHERE milestone.company_id=? AND milestone.project_id=?`)
                .bind(value, companyId, id).all<{
                id: string;
                state: string;
            }>();
        },
        async dependencyCycle(id: string, id2: string, companyId: string, id3: string, value: string) {
            return db.prepare(`WITH RECURSIVE reachable(milestone_id,path) AS (
        SELECT ?::text, ARRAY[?::text]
        UNION ALL
        SELECT d.successor_milestone_id,reachable.path||d.successor_milestone_id
        FROM reachable JOIN staff_project_milestone_dependencies d
          ON d.predecessor_milestone_id=reachable.milestone_id AND d.company_id=? AND d.project_id=?
        WHERE NOT d.successor_milestone_id=ANY(reachable.path)
      ) SELECT EXISTS(SELECT 1 FROM jsonb_array_elements_text(?::jsonb) candidate
        JOIN reachable ON reachable.milestone_id=candidate.value) cyclic`)
                .bind(id, id2, companyId, id3, value).first<{
                cyclic: boolean;
            }>();
        },
        prepareProjectLock(id: string, companyId: string) { return db.prepare("SELECT id FROM staff_projects WHERE id=? AND company_id=? FOR UPDATE").bind(id, companyId); },
        guardProjectOwner(key: string, value: "project-owner", id: string, companyId: string, version: number, userId: string) { return assertion(db, key, value, "EXISTS(SELECT 1 FROM staff_projects WHERE id=? AND company_id=? AND version=? AND iam_same_person(owner_user_id,?,company_id))", [id, companyId, version, userId]); },
        guardParent(key: string, value: "package-parent", id: string, companyId: string, id2: string, version: number) { return assertion(db, key, value, "EXISTS(SELECT 1 FROM staff_project_milestones WHERE id=? AND company_id=? AND project_id=? AND version=? AND state IN ('planned','in_progress'))", [id, companyId, id2, version]); },
        guardPrerequisites(key: string, value: "milestone-prerequisites", dependencyJson: string, dependencyJson2: string, companyId: string, id: string, id2: string) {
            return assertion(db, key, value, `jsonb_array_length(?::jsonb)=(SELECT count(*) FROM jsonb_array_elements_text(?::jsonb) candidate
      JOIN staff_project_milestones dependency ON dependency.id=candidate.value AND dependency.company_id=? AND dependency.project_id=? AND dependency.state<>'cancelled'
      WHERE dependency.id<>?)`, [dependencyJson, dependencyJson2, companyId, id, id2]);
        },
        guardAcyclic(key: string, value: "milestone-prerequisites-acyclic", id: string, id2: string, companyId: string, id3: string, dependencyJson: string) {
            return assertion(db, key, value, `NOT EXISTS(
      WITH RECURSIVE reachable(milestone_id,path) AS (
        SELECT ?::text, ARRAY[?::text]
        UNION ALL
        SELECT d.successor_milestone_id,reachable.path||d.successor_milestone_id
        FROM reachable JOIN staff_project_milestone_dependencies d
          ON d.predecessor_milestone_id=reachable.milestone_id AND d.company_id=? AND d.project_id=?
        WHERE NOT d.successor_milestone_id=ANY(reachable.path)
      ) SELECT 1 FROM jsonb_array_elements_text(?::jsonb) candidate JOIN reachable ON reachable.milestone_id=candidate.value
    )`, [id, id2, companyId, id3, dependencyJson]);
        },
        guardMilestoneRevision(key: string, value: "milestone-version", id: string, id2: string, companyId: string, expectedVersion: number) { return assertion(db, key, value, "EXISTS(SELECT 1 FROM staff_project_milestones WHERE id=? AND project_id=? AND company_id=? AND version=? AND state IN ('planned','in_progress'))", [id, id2, companyId, expectedVersion]); },
        prepareMilestoneRevision(title: string, description: string, value: number, ownerUserId: string, parentId: string | null, position: number, now: number, id: string) {
            return db.prepare(`UPDATE staff_project_milestones SET title=?,description=?,due_at=?,owner_user_id=?,parent_milestone_id=?,ordinal_position=?,version=version+1,updated_at=? WHERE id=?`)
                .bind(title, description, value, ownerUserId, parentId, position, now, id);
        },
        guardMember(key: string, value: "project-member", id: string, companyId: string, ownerUserId: string, ownerUserId2: string) { return assertion(db, key, value, "EXISTS(SELECT 1 FROM staff_projects p WHERE p.id=? AND p.company_id=? AND (iam_same_person(p.owner_user_id,?,p.company_id) OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?)))", [id, companyId, ownerUserId, ownerUserId2]); },
        prepareMilestone(id: string, companyId: string, id2: string, title: string, description: string, value: number, ownerUserId: string, parentId: string | null, position: number, userId: string, now: number, now2: number) {
            return db.prepare(`INSERT INTO staff_project_milestones(id,company_id,project_id,title,description,due_at,owner_user_id,parent_milestone_id,ordinal_position,state,version,created_by,completed_at,created_at,updated_at)
        VALUES(?,?,?,?,?,?,?,?,?,'planned',1,?,NULL,?,?)`)
                .bind(id, companyId, id2, title, description, value, ownerUserId, parentId, position, userId, now, now2);
        },
        prepareDependenciesReplacement(companyId: string, id: string, id2: string) {
            return db.prepare("DELETE FROM staff_project_milestone_dependencies WHERE company_id=? AND project_id=? AND successor_milestone_id=?")
                .bind(companyId, id, id2);
        },
        prepareDependency(companyId: string, id: string, predecessorId: string, id2: string, userId: string, now: number) {
            return db.prepare(`INSERT INTO staff_project_milestone_dependencies
      (company_id,project_id,predecessor_milestone_id,successor_milestone_id,created_by,created_at) VALUES(?,?,?,?,?,?)`)
                .bind(companyId, id, predecessorId, id2, userId, now);
        },
        prepareProjectVersion(now: number, id: string, companyId: string) { return db.prepare("UPDATE staff_projects SET version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(now, id, companyId); },
        async incompletePrerequisites(companyId: string, id: string, milestoneId: string) {
            return db.prepare(`SELECT p.title FROM staff_project_milestone_dependencies d
    JOIN staff_project_milestones p ON p.id=d.predecessor_milestone_id AND p.company_id=d.company_id AND p.project_id=d.project_id
    WHERE d.company_id=? AND d.project_id=? AND d.successor_milestone_id=? AND p.state<>'completed' ORDER BY p.title`)
                .bind(companyId, id, milestoneId).all<{
                title: string;
            }>();
        },
        async activeSuccessors(companyId: string, id: string, milestoneId: string) {
            return db.prepare(`SELECT 1 FROM staff_project_milestone_dependencies d
      JOIN staff_project_milestones s ON s.id=d.successor_milestone_id AND s.company_id=d.company_id AND s.project_id=d.project_id
      WHERE d.company_id=? AND d.project_id=? AND d.predecessor_milestone_id=? AND s.state IN ('planned','in_progress','completed') LIMIT 1`)
                .bind(companyId, id, milestoneId).first();
        },
        async completedSuccessor(companyId: string, id: string, milestoneId: string) {
            return db.prepare(`SELECT 1 FROM staff_project_milestone_dependencies d
      JOIN staff_project_milestones s ON s.id=d.successor_milestone_id AND s.company_id=d.company_id AND s.project_id=d.project_id
      WHERE d.company_id=? AND d.project_id=? AND d.predecessor_milestone_id=? AND s.state='completed' LIMIT 1`)
                .bind(companyId, id, milestoneId).first();
        },
        prepareTransitionLock(id: string, companyId: string) { return db.prepare("SELECT id FROM staff_projects WHERE id=? AND company_id=? FOR UPDATE").bind(id, companyId); },
        guardTransitionOwner(key: string, value: "project-owner", id: string, companyId: string, version: number, userId: string) { return assertion(db, key, value, "EXISTS(SELECT 1 FROM staff_projects WHERE id=? AND company_id=? AND version=? AND iam_same_person(owner_user_id,?,company_id))", [id, companyId, version, userId]); },
        guardTransitionVersion(key: string, value: "milestone-version", milestoneId: string, id: string, companyId: string, expectedVersion: number, state: "completed" | "cancelled" | "planned" | "in_progress") { return assertion(db, key, value, "EXISTS(SELECT 1 FROM staff_project_milestones WHERE id=? AND project_id=? AND company_id=? AND version=? AND state=?)", [milestoneId, id, companyId, expectedVersion, state]); },
        guardPrerequisitesCompleted(key: string, value: "milestone-prerequisites-complete", action: "start" | "complete" | "reopen" | "cancel", companyId: string, id: string, milestoneId: string) {
            return assertion(db, key, value, `(? NOT IN ('start','complete') OR NOT EXISTS(
      SELECT 1 FROM staff_project_milestone_dependencies d JOIN staff_project_milestones p ON p.id=d.predecessor_milestone_id
      WHERE d.company_id=? AND d.project_id=? AND d.successor_milestone_id=? AND p.state<>'completed'))`, [action, companyId, id, milestoneId]);
        },
        guardStillUsed(key: string, value: "milestone-prerequisite-still-used", action: "start" | "complete" | "reopen" | "cancel", companyId: string, id: string, milestoneId: string) {
            return assertion(db, key, value, `(?<>'cancel' OR NOT EXISTS(
      SELECT 1 FROM staff_project_milestone_dependencies d JOIN staff_project_milestones s ON s.id=d.successor_milestone_id
      WHERE d.company_id=? AND d.project_id=? AND d.predecessor_milestone_id=? AND s.state IN ('planned','in_progress','completed')))`, [action, companyId, id, milestoneId]);
        },
        guardSuccessorReopening(key: string, value: "milestone-no-completed-successor", action: "start" | "complete" | "reopen" | "cancel", companyId: string, id: string, milestoneId: string) {
            return assertion(db, key, value, `(?<>'reopen' OR NOT EXISTS(
      SELECT 1 FROM staff_project_milestone_dependencies d JOIN staff_project_milestones s ON s.id=d.successor_milestone_id
      WHERE d.company_id=? AND d.project_id=? AND d.predecessor_milestone_id=? AND s.state='completed'))`, [action, companyId, id, milestoneId]);
        },
        prepareTransition(nextState: "completed" | "cancelled" | "in_progress", value: number | null, now: number, milestoneId: string) {
            return db.prepare(`UPDATE staff_project_milestones SET state=?,completed_at=?,version=version+1,updated_at=? WHERE id=?`)
                .bind(nextState, value, now, milestoneId);
        },
        prepareTransitionProjectVersion(now: number, id: string, companyId: string) { return db.prepare("UPDATE staff_projects SET version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(now, id, companyId); } };
    return createProjectMilestoneApplication({ repository, clock: { now: () => Date.now() }, commands: { replay: (...args) => inward(() => replayCommand(db, ...args)), commit: (...args) => inward(() => executeCommand(db, ...args)) }, getProject: (...args) => inward(() => getProject(db, ...args)), activeStaff: (...args) => inward(() => activeStaff(db, ...args)), staffGuard: (...args) => staffGuard(db, ...args), requirePermission: (...args) => requirePermission(...args) });
}
export function projectConstraint(error: unknown): never { try {
    return application(null as unknown as SqlDatabase).projectConstraint(error);
}
catch (failure) {
    if (failure instanceof BusinessRuleError) {
        const out = new ApiError(failure.status, failure.code, failure.message);
        out.cause = failure.cause;
        throw out;
    }
    throw failure;
} }
export async function listProjectMilestones(db: SqlDatabase, actor: StaffActor, projectId: string) { await requireProjectPurchasedRead(db,actor,projectId);return exposed(() => application(db).listProjectMilestones(actor, projectId)); }
export async function saveProjectMilestone(db: SqlDatabase, actor: StaffActor, projectId: string, milestoneId: string | null, input: MilestoneInput, key: string) { return withProjectPurchasedCommand(db,actor,{key,operations:[milestoneId?'project.milestone.revise':'project.milestone.create'],projectId,purpose:'new-work',existing:true,target:{kind:'milestone',id:milestoneId??`milestone-${key}`}},tx=>exposed(() => application(tx).saveProjectMilestone(actor, projectId, milestoneId, input, key))); }
export async function transitionProjectMilestone(db: SqlDatabase, actor: StaffActor, projectId: string, milestoneId: string, input: z.infer<typeof projectMilestoneTransitionSchema>, key: string) { return withProjectPurchasedCommand(db,actor,{key,operations:[`project.milestone.${input.action}`],projectId,purpose:input.action==='reopen'?'new-work':'settle-existing',existing:true,target:{kind:'milestone',id:milestoneId}},tx=>exposed(() => application(tx).transitionProjectMilestone(actor, projectId, milestoneId, input, key))); }
