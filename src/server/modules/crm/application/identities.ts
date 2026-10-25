import type {Clock} from "../../shared";
import {BusinessRuleError} from "../../shared";
import type {Row} from "../../commerce/public/commerce";
import {crmIdentityCommand,type CRMIdentityInput} from "../domain/identities";
export type CRMIdentityMutation={input:CRMIdentityInput;key:string;recordId:string;operation:string;version:number;now:number};
export interface CRMIdentityPorts{
 canReview:boolean;canCreate:boolean;canGuestReview?:boolean;clock:Clock;authorize(permission:"commerce.read"|"commerce.sales"|"commerce.approve"|"pms.read"):void;
 replay(key:string,operation:string,id:string,input:unknown):Promise<Record<string,unknown>|null>;
 identities():Promise<{accounts:Row[];contacts:Row[];customers:Row[];guestLinks?:Row[]}>;
 contact(id:string):Promise<Row|null>;stays(id:string):Promise<Row[]>;
 commit<Result>(mutation:CRMIdentityMutation,result:Result):Promise<Result>;
}
export function createCRMIdentityApplication(ports:CRMIdentityPorts){return {
 async workspace(){ports.authorize("commerce.read");return {...await ports.identities(),canReview:ports.canReview,canCreate:ports.canCreate,canGuestReview:ports.canGuestReview??false};},
 async change(raw:unknown,key:string){
  const input=crmIdentityCommand.parse(raw);ports.authorize("commerce.sales");
  const recordId=input.action==="create-account"?`account-${key}`:input.action==="create-contact"?`contact-${key}`:input.action==="link-finance"?input.accountId:input.action==="guest-sharing"?input.linkId:input.contactId,operation=`crm.identity.${input.action}`,prior=await ports.replay(key,operation,recordId,input);if(prior)return prior;
  const now=ports.clock.now();if(input.action==="link-finance")ports.authorize("commerce.approve");if(input.action==="link-guest"||input.action==="guest-sharing"){ports.authorize("commerce.approve");ports.authorize("pms.read");}
  const version=input.action==="link-finance"||input.action==="guest-sharing"?input.expectedVersion+1:1;
  return ports.commit({input,key,recordId,operation,version,now},{id:recordId,version});
 },
 async history(contactId:string){
  ports.authorize("commerce.read");ports.authorize("pms.read");const contact=await ports.contact(contactId);
  if(!contact)throw new BusinessRuleError(404,"CRM_CONTACT_NOT_FOUND","Contact not found.");const rows=await ports.stays(contactId);return {contact,stays:rows.slice(0,200),hasMore:rows.length>200};
 },
};}

