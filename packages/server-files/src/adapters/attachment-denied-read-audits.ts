import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
/** Observe only denial events actually written by the original Files reader.
 * An owner may commit that denial without releasing bytes after repeating its
 * current authority/source guards. No caller can supply an audit event. */
export async function attachmentDeniedReadAudits(database:SqlDatabase,actor:WorkContext,linkId:string){return (await database.prepare(`SELECT event.id,event.reason,to_jsonb(event)::text witness FROM attachment_access_events event JOIN attachment_links link ON link.id=event.link_id AND link.attachment_id=event.attachment_id JOIN attachment_sets native_set ON native_set.id=link.set_id AND native_set.company_id=event.company_id WHERE event.company_id=? AND event.actor_id=? AND event.link_id=? AND event.outcome='denied' AND event.variant IN ('original','preview') ORDER BY event.id`).bind(actor.companyId,actor.userId,linkId).all<{id:string;reason:string|null;witness:string}>()).results;}
