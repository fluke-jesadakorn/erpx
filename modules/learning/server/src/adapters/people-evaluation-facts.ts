import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import type {Row} from '@erpx/people-payroll-server/public/people-evaluation-contract';
import {assignmentCourseJoinHead,assignmentsImage,coursesImage,workspaceImageFrame} from './learning-read-sql';
const evaluationLearningReadScope=async(database:SqlDatabase,actor:WorkContext)=>(await import('@erpx/learning-server/public/evaluation-source-policy')).evaluationLearningReadScope(database,actor);
export function peopleEvaluationLearningSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE learning_assignments,learning_courses IN SHARE ROW EXCLUSIVE MODE')];}
export async function peopleEvaluationLearningSourceImage(database:SqlDatabase,actor:WorkContext){const scope=await evaluationLearningReadScope(database,actor);return database.prepare(workspaceImageFrame(`'assignments',${assignmentsImage(`to_jsonb(a)||jsonb_build_object('visible',${scope.where})`)},'courses',${coursesImage}`)).bind(...scope.args,actor.companyId,actor.companyId).first<string>('image');}
export function peopleEvaluationLearningFacts(database:SqlDatabase){return {
 async readEvaluationEvidenceSourcesTasksprojectslearning3(companyId:string,userId:string|null,sourceId:string|null,sourceId2:string|null,branchId:string|null,branchId2:string|null,departmentId:string|null,departmentId2:string|null,actor:WorkContext){const scope=await evaluationLearningReadScope(database,actor);return database.prepare(`${assignmentCourseJoinHead('a.id,c.title,a.status,a.version,a.updated_at')} WHERE a.company_id=? AND a.learner_id=? AND a.status='completed' AND (?::text IS NULL OR a.id=?) AND (?::text IS NULL OR a.branch_id=?) AND (?::text IS NULL OR a.department_id=?) AND ${scope.where} ORDER BY a.updated_at DESC LIMIT 100`).bind(companyId,userId,sourceId,sourceId2,branchId,branchId2,departmentId,departmentId2,...scope.args).all<Row>();}
};}
