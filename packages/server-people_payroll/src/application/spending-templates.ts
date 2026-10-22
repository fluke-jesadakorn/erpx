import {z} from 'zod';import type {WorkContext} from '@erpx/server-identity_access/public/authorization';import type {SpendingTemplate} from "@erpx/feature-shared/lib/spending";import {BusinessRuleError,type Clock} from '@erpx/server-shared/index';import {spendingTemplateSchema,type SpendingTemplateInput} from '@erpx/server-people_payroll/domain/spending-templates';export type SpendingTemplateMutation={actor:WorkContext;input:SpendingTemplateInput;key:string;version:number;now:number};export interface SpendingTemplatePorts{clock:Clock;authorize(actor:WorkContext,permission:string):WorkContext;read(companyId:string):Promise<{version:number;exceptions_json:string;reason:string}|null>;activePosition(companyId:string,id:string):Promise<boolean>;commit<Result>(mutation:SpendingTemplateMutation,result:Result):Promise<Result>;}export function createSpendingTemplateApplication(ports:SpendingTemplatePorts){async function saveSpendingTemplate(actor: WorkContext, input: z.infer<typeof spendingTemplateSchema>, key:string) {
  actor=ports.authorize(actor,"organization.manage");
  if(actor.assignmentScope!=="holding") throw new BusinessRuleError(403,"COMPANY_AUTHORITY_REQUIRED","Company-authorized HR or an owner must manage spending templates.");
  const seen=new Set<string>();
  for(const exception of input.exceptions){
    const selector=JSON.stringify([exception.positionId,exception.requestKind]);
    if(seen.has(selector)||(!exception.positionId&&!exception.requestKind))throw new BusinessRuleError(422,"EXCEPTION_SELECTOR_REQUIRED","Use a unique position or request type for each exception.");
    seen.add(selector);
    if(exception.positionId&&!await ports.activePosition(actor.companyId,exception.positionId))throw new BusinessRuleError(422,"POSITION_INVALID","Choose an active company position.");
    if(exception.action==="named_supervisor"&&!exception.supervisorUserId)throw new BusinessRuleError(422,"SUPERVISOR_REQUIRED","Choose an independent supervisor for this exception.");
  }
  const version=input.expectedVersion+1;
return ports.commit({actor,input,key,version,now:ports.clock.now()},{id:actor.companyId,version});
}return {saveSpendingTemplate,async spendingTemplate(companyId:string):Promise<SpendingTemplate>{const row=await ports.read(companyId);return row?{version:row.version,exceptions:JSON.parse(row.exceptions_json),reason:row.reason}:{version:1,exceptions:[],reason:'Company default: reporting supervisor, Accounting, posting and Finance payment.'};}};}
