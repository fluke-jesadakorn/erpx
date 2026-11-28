import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import type {PortalCommand} from "@erpx/customer-360-server/public/portal-command-contract";
import { nativeClearCommandAssertions } from '@erpx/collaboration-server/adapters/native-command-assertions';
import { workflowWave3Statement } from './native-workflow-wave3';
export function clearCustomerPortalAssertions(db:SqlDatabase,key:string,companyId:string) { return nativeClearCommandAssertions(db, companyId, key); }
export async function auditCustomerPortalCommand(db:SqlDatabase,command:PortalCommand,eventId:string) {
 const {key,actor,input,binding,now}=command;
 await (await workflowWave3Statement(db,{kind:'company',id:binding.company_id},'wf_audit_direct',[eventId,key,actor.userId,'external-customer',`customer.portal.${input.action}`,'commerce_order',binding.order_id,Number(binding.order_version),'Customer portal action',now,JSON.stringify({kind:'external-customer',bindingId:binding.binding_id,email:actor.email})],`portal-audit:${eventId}`,now)).run();
}
export async function enqueueCustomerPortalCommand(db:SqlDatabase,command:PortalCommand,eventId:string,manifest:unknown[]) {
 const {key,actor,input,binding,now}=command;
 await (await workflowWave3Statement(db,{kind:'company',id:binding.company_id},'wf_outbox_direct',[eventId,key,`customer.portal.${input.action}`,'commerce_order',binding.order_id,Number(binding.order_version),JSON.stringify({action:input.action,bindingId:binding.binding_id,customerId:binding.customer_id,userId:actor.userId,note:input.note,attachmentManifest:manifest}),now],`portal-outbox:${eventId}`,now)).run();
}
