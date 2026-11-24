import {staffProjectByRefHead} from './read-tail-r34';
/** Native joined facts preserve the caller's transaction and original SQL snapshot. */
const allocationSumSql = "COALESCE((SELECT SUM(a.amount_minor) FROM staff_project_cost_allocations a WHERE a.journal_line_id=l.id AND a.state IN ('draft','approved')";
export const projectReadProjection = {
    "taskBinding": "LEFT JOIN staff_project_task_packages b ON b.task_id=t.id",
    "expenseProject": "JOIN staff_projects p ON p.id=? AND p.company_id=?",
    "allocatedExpense": `${allocationSumSql}),0)`,
    "otherAllocation": `${allocationSumSql} AND a.id!=?),0)`,
    "taskProject": "LEFT JOIN staff_projects p ON p.id=t.project_id AND p.company_id=t.company_id",
    "taskMember": "EXISTS(SELECT 1 FROM staff_project_members pm WHERE pm.project_id=t.project_id AND pm.user_id=?)",
    "hasTaskBinding": "NOT EXISTS(SELECT 1 FROM staff_project_task_packages b WHERE b.task_id=t.id)"
} as const;
/** Owner facts used inside existing joined task and handover snapshots. */
export function projectTaskOwnerSql(task:string,user:string){return `EXISTS(${staffProjectByRefHead('1','hp',`${task}.project_id`,`${task}.company_id`,' ','\n      ')} AND iam_same_person(hp.owner_user_id,${user},${task}.company_id))`;}
export function projectTaskMemberSql(task:string,user:string){return `EXISTS(SELECT 1 FROM staff_projects hpm JOIN staff_project_members hm ON hm.project_id=hpm.id
      WHERE hpm.id=${task}.project_id AND hpm.company_id=${task}.company_id AND hm.user_id=${user})`;}
/** Owner-or-member scope shared by structure handoff reads and executive milestone cohorts. */
export function projectOwnerOrMemberSql(alias:string){return `(iam_same_person(${alias}.owner_user_id,?,${alias}.company_id) OR EXISTS(SELECT 1 FROM staff_project_members member WHERE member.project_id=${alias}.id AND iam_same_person(member.user_id,?,${alias}.company_id)))`;}
/** Byte-identical owner-or-member scope shared by the Projects collaboration-home,
 * staff and executive metric reads; one definition keeps every source relation and
 * bind order identical. */
export const projectMemberScopeSql = "(p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?))";
/** Byte-identical member-scope assertion guard shared by the Projects Social
 * audience guard and the Operations staff-task guard. The scope fragment is the
 * only variable; the source relation and bind order stay exactly as in both
 * former inline copies. */
export const projectMemberScopeGuard = (scope: string) => `EXISTS(SELECT 1 FROM staff_projects p WHERE p.id=? AND p.company_id=? AND ${scope})`;
/** Byte-identical owner-or-member scope with column-ref viewer aliases shared by
 * the Projects social audience, social purchase, evaluation source and audience
 * statement reads. Only the aliases, id column and EXISTS gap vary; the source
 * relation and bind order stay exactly as in the former inline copies. */
export const projectOwnerOrMemberColumnSql = (project: string, user: string, member: string, id = 'id', gap = '') =>
    `(${project}.owner_user_id=${user} OR EXISTS(${gap}SELECT 1 FROM staff_project_members ${member} WHERE ${member}.project_id=${project}.${id} AND ${member}.user_id=${user}))`;
export function projectHandoverSourceSql(post:string,user:string){return `${staffProjectByRefHead('1','hp','hs.source_id',`${post}.company_id`)}
          AND (iam_same_person(hp.owner_user_id,${user},hp.company_id)
            OR EXISTS(SELECT 1 FROM staff_project_members hm WHERE hm.project_id=hp.id AND hm.user_id=${user}))`;}

export function socialProjectAudienceSql(alias:string){return `${staffProjectByRefHead('1','pr',`${alias}.audience_id`,`${alias}.company_id`)}
      AND ${projectOwnerOrMemberColumnSql('pr','a.user_id','pm','id','\n        ')}`;}
