import "server-only";
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {WorkContext} from '../../identity_access/public/authorization';
import type {Row} from '../../people_payroll/public/people-evaluation-contract';
const evaluationProjectReadScope=async(database:SqlDatabase,actor:WorkContext,kind:'project'|'task')=>(await import('./record-purchases')).evaluationProjectReadScope(database,actor,kind);
export const peopleEvaluationProjectTaskReadScope=(database:SqlDatabase,actor:WorkContext)=>evaluationProjectReadScope(database,actor,'task');
export function peopleEvaluationProjectSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE')];}
export async function peopleEvaluationProjectSourceImage(database:SqlDatabase,actor:WorkContext){
 const scope=await evaluationProjectReadScope(database,actor,'project');return database.prepare(`SELECT jsonb_build_object('projects',(SELECT jsonb_agg(to_jsonb(evaluation_project)||jsonb_build_object('visible',${scope.where}) ORDER BY evaluation_project.id) FROM staff_projects evaluation_project WHERE evaluation_project.company_id=?),'members',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.project_id,m.user_id) FROM staff_project_members m JOIN staff_projects p ON p.id=m.project_id WHERE p.company_id=?))::text image`).bind(...scope.args,actor.companyId,actor.companyId).first<string>('image');
}
export function peopleEvaluationProjectsFacts(database:SqlDatabase){return {
 async readEvaluationEvidenceSourcesTasksprojectslearning2(companyId:string,userId:string|null,sourceId:string|null,sourceId2:string|null,branchId:string|null,branchId2:string|null,departmentId:string|null,departmentId2:string|null,actor:WorkContext){const scope=await evaluationProjectReadScope(database,actor,'project');return database.prepare(`SELECT evaluation_project.id,evaluation_project.title,evaluation_project.version,evaluation_project.updated_at FROM staff_projects evaluation_project JOIN staff_project_members m ON m.project_id=evaluation_project.id WHERE evaluation_project.company_id=? AND m.user_id=? AND (?::text IS NULL OR evaluation_project.id=?) AND (?::text IS NULL OR evaluation_project.branch_id=?) AND (?::text IS NULL OR evaluation_project.department_id=?) AND ${scope.where} ORDER BY evaluation_project.updated_at DESC LIMIT 100`).bind(companyId,userId,sourceId,sourceId2,branchId,branchId2,departmentId,departmentId2,...scope.args).all<Row>();}
};}
