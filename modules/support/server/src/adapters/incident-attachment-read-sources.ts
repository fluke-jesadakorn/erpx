import 'server-only';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {supportOrderAttachmentSourceLocks,supportOrderAttachmentSourceImage} from '@erpx/commerce-server/public/support-attachment-sources';
import {nativeSupportReadAll,SUPPORT_RD_INCIDENT_ATTACHMENT_SOURCE_IMAGE,SUPPORT_RD_INCIDENT_ATTACHMENT_SOURCE_LINKS} from './native-support-read';

export function supportIncidentAttachmentReadSourceLocks(database:SqlDatabase){return [
  database.prepare('LOCK TABLE support_incidents,support_incident_tickets,support_incident_events,support_tickets,support_ticket_events IN SHARE ROW EXCLUSIVE MODE'),
  supportOrderAttachmentSourceLocks(database),
];}
/** Existing incident source links select actual same-Company tickets/orders.
 * HR facts are the original recipient foundation, without a People purchase.
 * The image/link projections are owner-planned in Rust; the Commerce source
 * image and the lock fence stay with their owners on the caller's open unit. */
export async function supportIncidentAttachmentReadSourceImage(database:SqlDatabase,actor:WorkContext,id:string){
  const incident=(await nativeSupportReadAll<{image:string;events:string|null;recipients:string|null}>(
    SUPPORT_RD_INCIDENT_ATTACHMENT_SOURCE_IMAGE,[actor.userId,id,actor.companyId],database)).results[0]??null;
  if(!incident)return null;
  const links=(await nativeSupportReadAll<{image:string;ticket:string|null;order_id:string|null}>(
    SUPPORT_RD_INCIDENT_ATTACHMENT_SOURCE_LINKS,[id,actor.companyId],database)).results;
  const sources=[];for(const link of links)sources.push({...link,order:link.order_id?await supportOrderAttachmentSourceImage(database,actor.companyId,link.order_id):null});
  return JSON.stringify([incident,sources]);
}
