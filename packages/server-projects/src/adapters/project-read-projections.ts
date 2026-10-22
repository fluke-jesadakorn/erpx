/** Native joined facts preserve the caller's transaction and original SQL snapshot. */ export const projectReadProjection = {
    "taskBinding": "LEFT JOIN staff_project_task_packages b ON b.task_id=t.id",
    "expenseProject": "JOIN staff_projects p ON p.id=? AND p.company_id=?",
    "allocatedExpense": "COALESCE((SELECT SUM(a.amount_minor) FROM staff_project_cost_allocations a\n        WHERE a.journal_line_id=l.id AND a.state IN ('draft','approved')),0)",
    "otherAllocation": "COALESCE((SELECT SUM(a.amount_minor)\n        FROM staff_project_cost_allocations a WHERE a.journal_line_id=l.id AND a.state IN ('draft','approved') AND a.id!=?),0)",
    "taskProject": "LEFT JOIN staff_projects p ON p.id=t.project_id AND p.company_id=t.company_id",
    "taskMember": "EXISTS(SELECT 1 FROM staff_project_members pm WHERE pm.project_id=t.project_id AND pm.user_id=?)",
    "hasTaskBinding": "NOT EXISTS(SELECT 1 FROM staff_project_task_packages b WHERE b.task_id=t.id)"
} as const;
/** Owner facts used inside existing joined task and handover snapshots. */
export function projectTaskOwnerSql(task:string,user:string){return `EXISTS(SELECT 1 FROM staff_projects hp WHERE hp.id=${task}.project_id
      AND hp.company_id=${task}.company_id AND iam_same_person(hp.owner_user_id,${user},${task}.company_id))`;}
export function projectTaskMemberSql(task:string,user:string){return `EXISTS(SELECT 1 FROM staff_projects hpm JOIN staff_project_members hm ON hm.project_id=hpm.id
      WHERE hpm.id=${task}.project_id AND hpm.company_id=${task}.company_id AND hm.user_id=${user})`;}
export function projectHandoverSourceSql(post:string,user:string){return `SELECT 1 FROM staff_projects hp WHERE hp.id=hs.source_id AND hp.company_id=${post}.company_id
          AND (iam_same_person(hp.owner_user_id,${user},hp.company_id)
            OR EXISTS(SELECT 1 FROM staff_project_members hm WHERE hm.project_id=hp.id AND hm.user_id=${user}))`;}

export function socialProjectAudienceSql(alias:string){return `SELECT 1 FROM staff_projects pr WHERE pr.id=${alias}.audience_id AND pr.company_id=${alias}.company_id
      AND (pr.owner_user_id=a.user_id OR EXISTS(
        SELECT 1 FROM staff_project_members pm WHERE pm.project_id=pr.id AND pm.user_id=a.user_id))`;}
