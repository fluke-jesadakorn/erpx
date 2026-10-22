import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {filtersSchema,type Filters} from "@erpx/server-runtime/public/rules/canvas-domain";
import {BusinessRuleError,type Clock} from "@erpx/server-shared/index";
import {benefitEvidenceInput,benefitEvidenceMetrics,benefitEvidenceProvider,benefitEvidenceFields,benefitEvidenceCursor,benefitEvidenceVersion,type BenefitEvidenceInput,type BenefitEvidenceRow,type BenefitEvidenceMetric} from "@erpx/server-benefits/domain/executive-evidence";
import {z} from "zod";
export interface BenefitEvidencePorts{
 clock:Clock;digest(value:string):Promise<string>;
 authorize(actor:WorkContext,metric:BenefitEvidenceMetric):Promise<void>;
 rows(actor:WorkContext,input:BenefitEvidenceInput,cursor:ReturnType<typeof benefitEvidenceCursor>,id?:string):Promise<BenefitEvidenceRow[]>;
 audit(actor:WorkContext,metric:BenefitEvidenceMetric,action:"search"|"open",rows:{id:string;version:number}[],queryHash:string):Promise<void>;
}
export function createBenefitEvidenceApplication(ports:BenefitEvidencePorts){
 const href=(metric:BenefitEvidenceMetric,id:string)=>metric==="active-benefit-elections"?`/benefits/${encodeURIComponent(id)}`:`/benefits/records/benefit_plan/${encodeURIComponent(id)}`;
 const hash=(actor:WorkContext,metric:BenefitEvidenceMetric,filters:Filters,value:unknown)=>ports.digest(JSON.stringify({provider:benefitEvidenceProvider,metric,filters,scope:[actor.companyId,actor.assignmentId,actor.branchId,actor.departmentId],value}));
 return {
  async search(actor:WorkContext,raw:unknown){const input=benefitEvidenceInput.parse(raw);await ports.authorize(actor,input.metric);const cursor=benefitEvidenceCursor(input.cursor),rows=await ports.rows(actor,input,cursor),page=rows.slice(0,input.limit),queryHash=await hash(actor,input.metric,input.filters,{query:input.query,cursor:input.cursor??null});await ports.authorize(actor,input.metric);await ports.audit(actor,input.metric,"search",page.map(row=>({id:row.id,version:benefitEvidenceVersion(row.version)})),queryHash);await ports.authorize(actor,input.metric);return {provider:benefitEvidenceProvider,metric:input.metric,fields:benefitEvidenceFields,items:page.map(row=>({id:row.id,label:row.label,summary:row.status,status:row.status,version:benefitEvidenceVersion(row.version),href:href(input.metric,row.id)})),hasMore:rows.length>input.limit,nextCursor:rows.length>input.limit&&page.length?Buffer.from(JSON.stringify({id:page.at(-1)!.id})).toString("base64url"):null,queryHash};},
  async read(actor:WorkContext,metric:string,id:string,rawFilters:unknown){const parsed=z.enum(benefitEvidenceMetrics).parse(metric),filters=filtersSchema.parse(rawFilters);await ports.authorize(actor,parsed);const row=(await ports.rows(actor,{metric:parsed,filters,query:"",limit:1},null,id))[0];if(!row)throw new BusinessRuleError(404,"EVIDENCE_NOT_FOUND","The original Benefits evidence is unavailable in this role and source.");const version=benefitEvidenceVersion(row.version),queryHash=await hash(actor,parsed,filters,{id});await ports.authorize(actor,parsed);await ports.audit(actor,parsed,"open",[{id:row.id,version}],queryHash);await ports.authorize(actor,parsed);return {provider:benefitEvidenceProvider,metric:parsed,fields:benefitEvidenceFields,record:{id:row.id,status:row.status,version,updated_at:Number(row.updated_at),effective_from:row.effective_from,effective_to:row.effective_to,href:href(parsed,row.id)},queryHash};},
 };
}
