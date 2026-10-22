import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {PortalCommand} from "@erpx/server-customer_360/public/portal-command-contract";
export function clearCustomerPortalAssertions(db:SqlDatabase,key:string) { return db.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key); }
export async function auditCustomerPortalCommand(db:SqlDatabase,command:PortalCommand,eventId:string) {
 const {key,actor,input,binding,now}=command;
 await db.prepare(`INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) VALUES(?,?,?,?,?,?,?,?,?,?,?)`).bind(eventId,key,actor.userId,'external-customer',`customer.portal.${input.action}`,'commerce_order',binding.order_id,Number(binding.order_version),'Customer portal action',now,JSON.stringify({kind:'external-customer',bindingId:binding.binding_id,email:actor.email})).run();
}
export async function enqueueCustomerPortalCommand(db:SqlDatabase,command:PortalCommand,eventId:string,manifest:unknown[]) {
 const {key,actor,input,binding,now}=command;
 await db.prepare(`INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES(?,?,?,?,?,?,?,?)`).bind(eventId,key,`customer.portal.${input.action}`,'commerce_order',binding.order_id,Number(binding.order_version),JSON.stringify({action:input.action,bindingId:binding.binding_id,customerId:binding.customer_id,userId:actor.userId,note:input.note,attachmentManifest:manifest}),now).run();
}
