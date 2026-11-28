import "server-only";
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import type {Row} from '@erpx/people-payroll-server/public/people-evaluation-contract';
import contract from './evaluation-read-sources.json' with {type:'json'};
/** Native-owned fixed templates. The only substitution is the owner-built
 * visibility scope; an absent token fails closed instead of emitting SQL. */
const fill=(template:string,values:Record<string,string>)=>{
 let rendered=template;
 for(const [name,value] of Object.entries(values)){
  const token=`{${name}}`;
  if(!rendered.includes(token))throw new Error('EVALUATION_READ_CONTRACT_INVALID');
  rendered=rendered.split(token).join(value);
 }
 return rendered;
};
const evaluationProjectReadScope=async(database:SqlDatabase,actor:WorkContext,kind:'project'|'task')=>(await import('@erpx/projects-server/public/evaluation-source-policy')).evaluationProjectReadScope(database,actor,kind);
export const peopleEvaluationProjectTaskReadScope=(database:SqlDatabase,actor:WorkContext)=>evaluationProjectReadScope(database,actor,'task');
export function peopleEvaluationProjectSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE')];}
export async function peopleEvaluationProjectSourceImage(database:SqlDatabase,actor:WorkContext){
 const scope=await evaluationProjectReadScope(database,actor,'project');return database.prepare(fill(contract.peopleSourceImage,{scope:scope.where})).bind(...scope.args,actor.companyId,actor.companyId).first<string>('image');
}
export function peopleEvaluationProjectsFacts(database:SqlDatabase){return {
 async readEvaluationEvidenceSourcesTasksprojectslearning2(companyId:string,userId:string|null,sourceId:string|null,sourceId2:string|null,branchId:string|null,branchId2:string|null,departmentId:string|null,departmentId2:string|null,actor:WorkContext){const scope=await evaluationProjectReadScope(database,actor,'project');return database.prepare(fill(contract.peopleSourceRows,{scope:scope.where})).bind(companyId,userId,sourceId,sourceId2,branchId,branchId2,departmentId,departmentId2,...scope.args).all<Row>();}
};}
