import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {nativeSupportReadAll,nativeSupportReadFragment,SUPPORT_RD_TICKET_PORTAL_INSTALLED,SUPPORT_RD_TICKET_PORTAL_TICKET_IMAGE,SUPPORT_RD_TICKET_PORTAL_VALID_BINDINGS,SUPPORT_RD_TICKET_PORTAL_IMAGE_AGGREGATE_FULL,SUPPORT_RD_TICKET_PORTAL_IMAGE_AGGREGATE_CORE} from '@erpx/server-support/adapters/native-support-read';
import {portalImageSelectedCte} from './ticket-portal-image-sql';
import { readTailR43ColumnsFromSql } from './read-tail-r43-sql';
export async function supportTicketPortalInstalled(database:SqlDatabase){return Boolean((await nativeSupportReadAll<{installed:boolean}>(SUPPORT_RD_TICKET_PORTAL_INSTALLED, [], database)).results[0]?.installed);}
export function supportTicketPortalSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE support_tickets,support_ticket_events,support_ticket_portal_bindings,support_ticket_portal_events,support_ticket_portal_identities,support_ticket_portal_reads,support_ticket_portal_responses,support_ticket_portal_acknowledgements IN SHARE ROW EXCLUSIVE MODE')];}
/** Plan the native valid-binding fragment and append one pinned outer tail.
 * The server owns the subquery and the admitted tail shapes (staff 1, customer
 * 4, workspace 9); the caller owns only its closed tail and binds. */
export async function portalValidBindingRead(columns:string,tailBinds:1|4|9){
  const head=await nativeSupportReadFragment(SUPPORT_RD_TICKET_PORTAL_VALID_BINDINGS,tailBinds);
  return readTailR43ColumnsFromSql(columns,`(${head}) binding`);
}
export async function supportTicketPortalTicketImage(database:SqlDatabase,ticketId:string){return nativeSupportReadAll<{image:string}>(SUPPORT_RD_TICKET_PORTAL_TICKET_IMAGE,[ticketId],database).then(result => result.results[0]?.image ?? null);}
export async function supportTicketPortalImage(database:SqlDatabase,bindingIds:string[]){if(!bindingIds.length)return '[]';const aggregate=await nativeSupportReadFragment(SUPPORT_RD_TICKET_PORTAL_IMAGE_AGGREGATE_FULL,0);return database.prepare(`${portalImageSelectedCte(bindingIds)} ${aggregate}`).bind(...bindingIds).first<string>('image');}
/** Public reads may only add their exact identity/read event; acknowledgements
 * may add their exact immutable acknowledgement. Neither changes this core. */
export async function supportTicketPortalCoreImage(database:SqlDatabase,bindingIds:string[]){if(!bindingIds.length)return '[]';const aggregate=await nativeSupportReadFragment(SUPPORT_RD_TICKET_PORTAL_IMAGE_AGGREGATE_CORE,0);return database.prepare(`${portalImageSelectedCte(bindingIds)} ${aggregate}`).bind(...bindingIds).first<string>('image');}
