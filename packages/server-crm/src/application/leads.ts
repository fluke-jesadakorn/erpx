import type {Clock} from "@erpx/server-shared/index";
import {BusinessRuleError} from "@erpx/server-shared/index";
import type {Row} from '@erpx/server-commerce/public/commerce';
import {crmForecast} from "@erpx/server-crm/domain/crm-forecast";
import {crmLeadSchema,crmLeadChangeSchema,crmOpportunitySchema,crmLeadWorkItems,type CRMLeadInput,type CRMLeadChange,type CRMOpportunityChange} from "@erpx/server-crm/domain/leads";
type Common={id:string;key:string;operation:string;now:number};
export type CRMMutation=Common&({action:"create";input:CRMLeadInput;accountId:string;contactId:string}|{action:"lead";input:CRMLeadChange;record:Row;opportunityId:string|null;currency:unknown}|{action:"opportunity";input:CRMOpportunityChange;record:Row;opportunityId:string;stage:string});
export interface CRMRepository{
 records(id?:string):Promise<Row[]>;branches():Promise<Row[]>;campaigns():Promise<Row[]>;evidence(id:string):Promise<Row|null>;
 recipients(id:string):Promise<{id:string;name:string}[]>;customers():Promise<Row[]>;quotes(row:Row):Promise<Row[]>;sales(row:Row):Promise<Row[]>;
 currency():Promise<Row|null>;guestCandidates():Promise<Row[]>;reservations(row:Row):Promise<Row[]>;sourceOrders(row:Row):Promise<Row[]>;sourceSupport(row:Row):Promise<Row[]>;sourceFollowups(row:Row):Promise<Row[]>;history(id:string):Promise<Row[]>;
 replay<Result=Record<string,unknown>>(key:string,operation:string,id:string,input:unknown):Promise<Result|null>;
 commit<Result>(mutation:CRMMutation,result:Result):Promise<Result>;
}
export function createCRMApplication(ports:{actorId:string;canRead:boolean;canSales:boolean;canApprove:boolean;canPms:boolean;canSupport:boolean;clock:Clock;authorize(permission:"commerce.read"|"commerce.sales"|"commerce.approve"|"pms.read"):void;repository():Promise<CRMRepository>;activityWork():Promise<import("@erpx/feature-work_projection/lib/work-item").WorkItem[]>}){
 async function getLead(id:string){const repository=await ports.repository();ports.authorize("commerce.read");const row=(await repository.records(id))[0];if(!row)throw new BusinessRuleError(404,"CRM_LEAD_NOT_FOUND","Lead not found in this scope.");return row;}
 return {
  getLead,
  async evidence(id:string){ports.authorize("commerce.read");return (await ports.repository()).evidence(id);},
  async workspace(){ports.authorize("commerce.read");const repository=await ports.repository();ports.authorize("commerce.read");const items=await repository.records(),branches=await repository.branches(),campaigns=await repository.campaigns();return {items,branches,campaigns,forecast:crmForecast(items),canCreate:ports.canSales};},
  async detail(id:string){
   const row=await getLead(id),repository=await ports.repository(),recipients=await repository.recipients(id),customers=await repository.customers(),quotes=row.customer_id?await repository.quotes(row):[],sales=row.quote_id?await repository.sales(row):[],companyCurrency=String((await repository.currency())?.currency??"Company currency not configured");
   const guestCandidates=ports.canPms?await repository.guestCandidates():[],reservations=ports.canPms&&row.contact_id?await repository.reservations(row):[],sourceOrders=row.account_customer_id?await repository.sourceOrders(row):[],sourceSupport=row.account_customer_id&&ports.canSupport?await repository.sourceSupport(row):[],sourceFollowups=row.account_customer_id?await repository.sourceFollowups(row):[];
   return {row,companyCurrency,recipients,customers,quotes,sales,reservations,guestCandidates,sourceOrders,sourceSupport,sourceFollowups,history:await repository.history(id),canAct:ports.canSales&&row.owner_id===ports.actorId&&Number(row.owner_eligible)>0,canManage:ports.canSales&&ports.canApprove};
  },
  async create(raw:unknown,key:string){
   const input=crmLeadSchema.parse(raw);ports.authorize("commerce.sales");ports.authorize("commerce.read");const repository=await ports.repository(),id=`lead-${key}`,operation="crm.lead.create",prior=await repository.replay<{id:string;version:number}>(key,operation,id,input);if(prior)return prior;
   if(input.contactId&&!input.accountId)throw new BusinessRuleError(422,"CRM_ACCOUNT_REQUIRED","Select the account for this contact.");
   return repository.commit({action:"create",input,id,key,operation,now:ports.clock.now(),accountId:input.accountId??`account-${key}`,contactId:input.contactId??`contact-${key}`},{id,version:1});
  },
  async changeLead(id:string,raw:unknown,key:string){
   const input=crmLeadChangeSchema.parse(raw);ports.authorize("commerce.sales");ports.authorize("commerce.read");const repository=await ports.repository(),operation=`crm.lead.${input.action}`,prior=await repository.replay(key,operation,id,input);if(prior)return prior;
   const record=await getLead(id),now=ports.clock.now();
   if(["reassign","reopen"].includes(input.action)&&!ports.canApprove)throw new BusinessRuleError(403,"CRM_MANAGER_REQUIRED","A sales manager must change ownership or reopen a closed lead.");
   if(input.action!=="reassign"&&input.action!=="reopen"&&record.owner_id!==ports.actorId)throw new BusinessRuleError(403,"CRM_OWNER_REQUIRED","The assigned salesperson must qualify this lead.");
   if(input.action==="reopen"?record.status!=="disqualified":input.action==="reassign"?record.stage==="won"||record.status==="disqualified":record.status!=="open")throw new BusinessRuleError(409,"CRM_STATE_CHANGED","Review the current lead or opportunity state.");
   const config=input.action==="qualify"?await repository.currency():null;
   if(input.action==="qualify"&&!config)throw new BusinessRuleError(409,"CRM_CURRENCY_REQUIRED","Configure Company currency before forecasting opportunities.");
   const opportunityId=input.action==="qualify"?`opportunity-${key}`:null;
   return repository.commit({action:"lead",input,id,key,operation,now,record,opportunityId,currency:config?.currency},{id,version:input.expectedVersion+1,opportunityId});
  },
  async changeOpportunity(id:string,raw:unknown,key:string){
   const input=crmOpportunitySchema.parse(raw);ports.authorize("commerce.sales");ports.authorize("commerce.read");const repository=await ports.repository(),operation=`crm.opportunity.${input.action}`,prior=await repository.replay(key,operation,id,input);if(prior)return prior;
   const record=await getLead(id);if(!record.opportunity_id)throw new BusinessRuleError(409,"CRM_OPPORTUNITY_REQUIRED","Qualify the lead before managing a deal.");
   if(input.action==="reopen"){ports.authorize("commerce.approve");if(record.stage!=="lost")throw new BusinessRuleError(409,"CRM_STATE_CHANGED","Only a lost opportunity can be reopened.");}else if(record.owner_id!==ports.actorId)throw new BusinessRuleError(403,"CRM_OWNER_REQUIRED","The assigned salesperson must act on the opportunity.");
   if(input.action!=="reopen"&&!["discovery","proposal","negotiation"].includes(String(record.stage)))throw new BusinessRuleError(409,"CRM_STATE_CHANGED","This opportunity is closed.");
   const now=ports.clock.now(),opportunityId=String(record.opportunity_id),stage=input.action==="lose"?"lost":input.action==="reopen"?"discovery":["win","win-pms"].includes(input.action)?"won":input.action==="link-quote"?"proposal":"negotiation";
   if(input.action==="win"&&!record.quote_id)throw new BusinessRuleError(409,"CRM_QUOTATION_REQUIRED","Link an approved quotation first.");
   if(input.action==="win-pms")ports.authorize("pms.read");
   if(input.action==="negotiate"&&!record.quote_id)throw new BusinessRuleError(409,"CRM_QUOTATION_REQUIRED","Link an approved quotation before negotiation.");
   return repository.commit({action:"opportunity",input,id,key,operation,now,record,opportunityId,stage},{id,version:input.expectedVersion+1,leadVersion:input.leadVersion+1,stage});
  },
  async work(){if(!ports.canRead)return [];const repository=await ports.repository();ports.authorize("commerce.read");return [...crmLeadWorkItems(await repository.records(),ports.actorId,ports.canSales),...await ports.activityWork()];},
 };
}
