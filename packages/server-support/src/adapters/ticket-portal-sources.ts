import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {nativeSupportReadAll,SUPPORT_RD_TICKET_PORTAL_TICKET_IMAGE} from './native-support-read';
import {portalImageSelectedCte,portalBindingImageSql,portalBoundImageSql,portalImageAggregateSql} from './ticket-portal-image-sql';
export async function supportTicketPortalInstalled(database:SqlDatabase){return Boolean(await database.prepare("SELECT to_regclass('support_ticket_portal_bindings') IS NOT NULL AND to_regclass('support_ticket_portal_acknowledgements') IS NOT NULL installed").first<boolean>('installed'));}
export function supportTicketPortalSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE support_tickets,support_ticket_events,support_ticket_portal_bindings,support_ticket_portal_events,support_ticket_portal_identities,support_ticket_portal_reads,support_ticket_portal_responses,support_ticket_portal_acknowledgements IN SHARE ROW EXCLUSIVE MODE')];}
export const supportTicketPortalIdentitySql=(alias:string)=>`jsonb_build_object('companyId',${alias}.company_id,'branchId',${alias}.branch_id,'departmentId',${alias}.department_id,'sourceKind',${alias}.source_kind,'orderId',${alias}.order_id,'customerId',${alias}.customer_id,'sourceRecord',${alias}.source_record_json)`;
/** One parameterized replay-proof family for the Support ticket portal. The
 * decision-event and response-receipt proofs share one statement shape; only the
 * source table/alias, receipt operation, optional action bind and expected result
 * object vary. Each caller keeps its own binds and its own proof. */
export const portalReplayProofSql=(table:string,alias:string,operation:string,action:string,expected:string)=>`SELECT 1 FROM ${table} ${alias} JOIN command_receipts receipt ON receipt.command_id=${alias}.command_id AND receipt.record_id=${alias}.binding_id AND receipt.actor_id=${alias}.actor_id AND receipt.operation=${operation} AND receipt.authority_json::jsonb->>'companyId'=${alias}.company_id WHERE ${alias}.command_id=? AND ${alias}.binding_id=? AND ${alias}.company_id=? AND ${alias}.actor_id=?${action} AND receipt.result_json::jsonb=?::jsonb AND receipt.result_json::jsonb=${expected}`;
/** Source identity is retained independently of mutable ticket versions/status.
 * Only the explicitly reviewed summary is public. No original issue or foreign
 * record content is selected into the customer projection. */
export const supportTicketPortalValidBindings=`SELECT binding.* FROM support_ticket_portal_bindings binding JOIN support_tickets ticket ON ticket.id=binding.ticket_id AND ticket.company_id=binding.company_id AND ticket.source_kind<>'field_service' AND binding.source_identity=${supportTicketPortalIdentitySql('ticket')} JOIN command_receipts birth ON birth.command_id=binding.prepare_command_id AND birth.operation='support.ticket-portal.prepare' AND birth.record_id=binding.id AND birth.actor_id=binding.prepared_by AND birth.authority_json::jsonb->>'companyId'=binding.company_id AND birth.result_json::jsonb->>'id'=binding.id AND birth.result_json::jsonb->>'status'='draft' AND birth.result_json::jsonb->>'version'='1'`;
/** One parameterized outer read family over the reviewed valid-binding source.
 * The staff single-binding read and the shared customer/work-list prefix render
 * byte-identically; only the projected columns and their own trailing clauses
 * vary. Locks, purchase gates and the earlier r19 prefix constant stay intact;
 * the native ticket-portal reader remains the authority. */
export const portalValidBindingRead=(columns:string)=>`SELECT ${columns} FROM (${supportTicketPortalValidBindings}) binding`;
/** One byte-identical outer read prefix over the reviewed valid-binding source. The
 * customer identity join and the staff work-list join share this exact text;
 * only their own JOIN/WHERE/ORDER clauses vary. Each caller keeps its own binds
 * and its own independent statement; the native ticket-portal reader remains
 * the authority. */
export const portalValidBindingSelect=portalValidBindingRead('binding.*');
export async function supportTicketPortalTicketImage(database:SqlDatabase,ticketId:string){return nativeSupportReadAll<{image:string}>(SUPPORT_RD_TICKET_PORTAL_TICKET_IMAGE,[ticketId],database).then(result => result.results[0]?.image ?? null);}
export async function supportTicketPortalImage(database:SqlDatabase,bindingIds:string[]){if(!bindingIds.length)return '[]';return database.prepare(`${portalImageSelectedCte(bindingIds)} ${portalImageAggregateSql([portalBindingImageSql(),portalBoundImageSql('ticket','support_tickets','id','binding.ticket_id=ticket.id'),portalBoundImageSql('event','support_ticket_portal_events','id','binding.id=event.binding_id'),portalBoundImageSql('identity','support_ticket_portal_identities','binding_id','binding.id=identity.binding_id'),portalBoundImageSql('read','support_ticket_portal_reads','id','binding.id=read.binding_id'),portalBoundImageSql('response','support_ticket_portal_responses','id','binding.id=response.binding_id'),portalBoundImageSql('ack','support_ticket_portal_acknowledgements','id','binding.id=ack.binding_id')])}`).bind(...bindingIds).first<string>('image');}
/** Public reads may only add their exact identity/read event; acknowledgements
 * may add their exact immutable acknowledgement. Neither changes this core. */
export async function supportTicketPortalCoreImage(database:SqlDatabase,bindingIds:string[]){if(!bindingIds.length)return '[]';return database.prepare(`${portalImageSelectedCte(bindingIds)} ${portalImageAggregateSql([portalBindingImageSql(),portalBoundImageSql('ticket','support_tickets','id','binding.ticket_id=ticket.id'),portalBoundImageSql('event','support_ticket_portal_events','id','binding.id=event.binding_id'),portalBoundImageSql('response','support_ticket_portal_responses','id','binding.id=response.binding_id')])}`).bind(...bindingIds).first<string>('image');}
