import type {Clock} from "@erpx/server-shared/index";
import {BusinessRuleError} from "@erpx/server-shared/index";
import type {Row} from "@erpx/server-commerce/public/commerce";
import {crmActivityCreateSchema,crmActivityChangeSchema,crmActivityWorkspaceView,crmActivityWorkItems,type CRMActivity,type CRMActivityCreate,type CRMActivityChange} from "@erpx/server-crm/domain/activities";
type Common={id:string;key:string;operation:string;now:number};
export type CRMActivityMutation=Common&({action:"create";input:CRMActivityCreate;lead:Row}|{action:"change";input:CRMActivityChange;record:CRMActivity;owner:string;due:string;status:string;outcome:string|null});
export interface CRMActivityPorts{
 actorId:string;hasRead:boolean;sales:boolean;manager:boolean;clock:Clock;isSamePerson(id:string):boolean;
 authorize(permission:"commerce.read"|"commerce.sales"):void;
 replay(key:string,operation:string,id:string,input:unknown):Promise<Record<string,unknown>|null>;
 lead(id:string):Promise<Row|null>;activities(leadId:string):Promise<CRMActivity[]>;history(leadId:string):Promise<Row[]>;recipients(leadId:string):Promise<{id:string;name:string}[]>;
 activity(id:string):Promise<CRMActivity|null>;workRows():Promise<CRMActivity[]>;commit<Result>(mutation:CRMActivityMutation,result:Result):Promise<Result>;
}
export function createCRMActivityApplication(ports:CRMActivityPorts){
 async function lead(id:string){const row=await ports.lead(id);if(!row)throw new BusinessRuleError(404,"CRM_LEAD_NOT_FOUND","Lead not found in this scope.");return row;}
 return {
  async workspace(leadId:string){ports.authorize("commerce.read");const current=await lead(leadId),items=await ports.activities(leadId),history=await ports.history(leadId);return {...crmActivityWorkspaceView(current,items,ports.actorId,ports.sales,ports.sales&&ports.manager,ports.isSamePerson),history,recipients:await ports.recipients(leadId)};},
  async create(raw:unknown,key:string){
   const input=crmActivityCreateSchema.parse(raw);ports.authorize("commerce.sales");ports.authorize("commerce.read");
   const id=`crm-activity-${key}`,operation="crm.activity.create",prior=await ports.replay(key,operation,id,input);if(prior)return prior;
   const current=await lead(input.leadId),now=ports.clock.now();
   if(!["open","qualified"].includes(String(current.status))||["won","lost"].includes(String(current.stage??"")))throw new BusinessRuleError(409,"CRM_ACTIVITY_SOURCE_CLOSED","Only an open or active opportunity can receive a new activity.");
   return ports.commit({action:"create",id,key,operation,input,lead:current,now},{id,version:1});
  },
  async change(id:string,raw:unknown,key:string){
   const input=crmActivityChangeSchema.parse(raw);ports.authorize("commerce.sales");ports.authorize("commerce.read");
   const operation=`crm.activity.${input.action}`,prior=await ports.replay(key,operation,id,input);if(prior)return prior;
   const record=await ports.activity(id);if(!record)throw new BusinessRuleError(404,"CRM_ACTIVITY_NOT_FOUND","CRM activity not found in this scope.");
   if(input.action==="reassign"&&!ports.manager||input.action==="cancel"&&!ports.manager&&record.owner_id!==ports.actorId||input.action==="complete"&&record.owner_id!==ports.actorId)throw new BusinessRuleError(403,"CRM_ACTIVITY_OWNER_REQUIRED","The assigned salesperson completes an activity; a sales manager manages responsibility.");
   if(record.status!=="open")throw new BusinessRuleError(409,"CRM_ACTIVITY_STATE_CHANGED","This activity is already closed.");
   const owner=input.action==="reassign"?input.ownerId:String(record.owner_id),due=input.action==="reassign"?input.dueDate:String(record.due_date),status=input.action==="complete"?"completed":input.action==="cancel"?"cancelled":"open",now=ports.clock.now(),outcome=input.action==="reassign"?null:input.reason;
   return ports.commit({action:"change",id,key,operation,input,record,owner,due,status,outcome,now},{id,version:input.expectedVersion+1,status});
  },
  async work(){if(!ports.hasRead)return [];return crmActivityWorkItems(await ports.workRows(),ports.actorId,ports.sales);},
 };
}
