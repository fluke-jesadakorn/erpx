import type {Clock} from "@erpx/platform-shared-server/index";
import {BusinessRuleError} from "@erpx/platform-shared-server/index";
import type {Row} from '@erpx/commerce-server/public/commerce';
import {crmActivityWorkspaceView,crmActivityWorkItems,type CRMActivity} from "@erpx/crm-server/domain/activities";
export interface CRMActivityPorts{
 actorId:string;hasRead:boolean;sales:boolean;manager:boolean;clock:Clock;isSamePerson(id:string):boolean;
 authorize(permission:"commerce.read"|"commerce.sales"):void;
 lead(id:string):Promise<Row|null>;activities(leadId:string):Promise<CRMActivity[]>;history(leadId:string):Promise<Row[]>;recipients(leadId:string):Promise<{id:string;name:string}[]>;
 workRows():Promise<CRMActivity[]>;
}
export function createCRMActivityApplication(ports:CRMActivityPorts){
 async function lead(id:string){const row=await ports.lead(id);if(!row)throw new BusinessRuleError(404,"CRM_LEAD_NOT_FOUND","Lead not found in this scope.");return row;}
 return {
  async workspace(leadId:string){ports.authorize("commerce.read");const current=await lead(leadId),items=await ports.activities(leadId),history=await ports.history(leadId);return {...crmActivityWorkspaceView(current,items,ports.actorId,ports.sales,ports.sales&&ports.manager,ports.isSamePerson),history,recipients:await ports.recipients(leadId)};},
  async work(){if(!ports.hasRead)return [];return crmActivityWorkItems(await ports.workRows(),ports.actorId,ports.sales);},
 };
}
