import "server-only";
import type { WorkContext } from "./authorization";
import { ApiError } from "./http";
import { brandIdentityWorkspace } from "./brand-identity";
import { agentWorkspace } from "./agent-governance";
import { productionWorkspace } from "./production";
import { ecommerceWorkspace } from "./ecommerce";
import { learningWorkspace } from "./learning";
import { benefitsWorkspace } from "./benefits";
import { talentWorkspace } from "./talent";
export const domainRecordKinds = ['agent_pool','brand_draft','brand_version','agent_version','agent_assignment','manufacturing_spec','planning_scenario','ecommerce_listing','ecommerce_cart','ecommerce_channel','learning_course','benefit_plan','talent_requisition','talent_candidate'] as const;
export type DomainRecordKind = typeof domainRecordKinds[number];
export function isDomainRecordKind(kind:string):kind is DomainRecordKind{return domainRecordKinds.some(value=>value===kind);}
function found<T>(row:T|undefined):T{if(!row)throw new ApiError(404,'RECORD_NOT_FOUND','Record not found in this work scope.');return row;}
/** Use the owning service's visibility, including personal and department restrictions. */
export async function recordDomainData(actor:WorkContext,kind:DomainRecordKind,id:string){
 if(kind==='brand_draft'||kind==='brand_version'){
  const data=await brandIdentityWorkspace(actor),row=found(kind==='brand_draft'?data.drafts.find(r=>r.id===id):data.versions.find(r=>r.id===id));
  return {domain:'brand' as const,kind,row,data:{...data,drafts:data.drafts.filter(r=>r.id===id),versions:data.versions.filter(r=>r.id===id)}};
 }
 if(kind==='agent_pool'){
  const data=await agentWorkspace(actor),row=found(data.pools.find(pool=>pool.id===id));
  return {domain:'agents' as const,kind,row,data:{...data,agents:[],versions:[],pools:[row],assignments:[],invocations:[],sessions:[],members:data.members.filter(member=>member.pool_id===id),poolHistory:data.poolHistory.filter(revision=>revision.pool_id===id)}};
 }
 if(kind==='agent_version'||kind==='agent_assignment'){
  const data=await agentWorkspace(actor),row=found((kind==='agent_version'?data.versions:data.assignments).find(r=>r.id===id));
  return {domain:'agents' as const,kind,row,data:{...data,agents:data.agents.filter(agent=>agent.id===row.agent_id||data.versions.some(version=>version.id===row.agent_version_id&&version.agent_id===agent.id)),versions:data.versions.filter(r=>r.id===id||r.id===row.agent_version_id),assignments:data.assignments.filter(r=>r.id===id),pools:[],invocations:[],sessions:[]}};
 }
 if(kind==='manufacturing_spec'||kind==='planning_scenario'){
  const productionModule:'planning'|'manufacturing'=kind==='planning_scenario'?'planning':'manufacturing',data=await productionWorkspace(actor,productionModule),row=found((kind==='planning_scenario'?data.plans:data.specifications).find(r=>r.id===id));
  return {domain:'production' as const,kind,module:productionModule,row,data:{...data,specifications:data.specifications.filter(r=>r.id===id||r.id===row.specification_id),plans:data.plans.filter(r=>r.id===id),recommendations:data.recommendations.filter(r=>r.scenario_id===id),orders:[]}};
 }
 if(kind==='ecommerce_listing'||kind==='ecommerce_cart'||kind==='ecommerce_channel'){
  const data=await ecommerceWorkspace(actor),row=found((kind==='ecommerce_listing'?data.listings:kind==='ecommerce_cart'?data.carts:data.channels).find(r=>r.id===id));
  return {domain:'ecommerce' as const,kind,row,data:{...data,listings:data.listings.filter(r=>r.id===id),carts:data.carts.filter(r=>r.id===id),channels:data.channels.filter(r=>r.id===id||r.id===row.channel_id),checkouts:[]}};
 }
 if(kind==='learning_course'){
  const data=await learningWorkspace(actor),row=found(data.courses.find(r=>r.id===id));
  return {domain:'learning' as const,kind,row,data:{...data,courses:[row],assignments:[]}};
 }
 if(kind==='benefit_plan'){
  const data=await benefitsWorkspace(actor),row=found(data.plans.find(r=>r.id===id));
  return {domain:'benefits' as const,kind,row,data:{...data,plans:[row],elections:[]}};
 }
 const data=await talentWorkspace(actor),row=found((kind==='talent_requisition'?data.requisitions:data.candidates).find(r=>r.id===id));
 return {domain:'talent' as const,kind,row,data:{...data,requisitions:data.requisitions.filter(r=>r.id===id),candidates:data.candidates.filter(r=>r.id===id),applications:[]}};
}
