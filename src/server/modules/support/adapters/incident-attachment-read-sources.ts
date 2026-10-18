import 'server-only';
import type {WorkContext} from '../../identity_access/public/authorization';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {finalRootOwnerProjectionSources as people} from '../../people_payroll/public/final-root-owner-projection-sources';
import {supportOrderAttachmentSourceLocks,supportOrderAttachmentSourceImage} from '../../commerce/public/support-attachment-sources';

export function supportIncidentAttachmentReadSourceLocks(database:SqlDatabase){return [
 database.prepare('LOCK TABLE support_incidents,support_incident_tickets,support_incident_events,support_tickets,support_ticket_events IN SHARE ROW EXCLUSIVE MODE'),
 supportOrderAttachmentSourceLocks(database),
];}
/** Existing incident source links select actual same-Company tickets/orders.
 * HR facts are the original recipient foundation, without a People purchase. */
export async function supportIncidentAttachmentReadSourceImage(database:SqlDatabase,actor:WorkContext,id:string){
 const incident=await database.prepare(`SELECT to_jsonb(t)::text image,
 (SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM support_incident_events event WHERE event.incident_id=t.id)::text events,
 (SELECT jsonb_agg(jsonb_build_object('person',to_jsonb(person),'engagements',(SELECT jsonb_agg(to_jsonb(engagement) ORDER BY engagement.id) FROM (${people.hr_engagements}) engagement WHERE engagement.person_id=person.id AND engagement.company_id=t.company_id)) ORDER BY person.id) FROM (${people.hr_people}) person WHERE person.company_id=t.company_id AND person.user_id IN (?,t.owner_id,t.mitigated_by))::text recipients
 FROM support_incidents t WHERE t.id=? AND t.company_id=?`).bind(actor.userId,id,actor.companyId).first<{image:string;events:string|null;recipients:string|null}>();
 if(!incident)return null;
 const links=(await database.prepare(`SELECT to_jsonb(link)::text image,to_jsonb(ticket)::text ticket,ticket.order_id
 FROM support_incident_tickets link JOIN support_incidents incident ON incident.id=link.incident_id
 LEFT JOIN support_tickets ticket ON ticket.id=link.ticket_id AND ticket.company_id=incident.company_id
 WHERE incident.id=? AND incident.company_id=? ORDER BY link.ticket_id`).bind(id,actor.companyId).all<{image:string;ticket:string|null;order_id:string|null}>()).results;
 const sources=[];for(const link of links)sources.push({...link,order:link.order_id?await supportOrderAttachmentSourceImage(database,actor.companyId,link.order_id):null});
 return JSON.stringify([incident,sources]);
}
