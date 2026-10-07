import { lastAuditOwnerProjectionSources as customer_authorityLastAuditProjectionSources } from "../../customer_authority/public/last-audit-owner-projection-sources";
import { hospitalityOwnerProjectionSources as commerceProjectionSources } from "../../commerce/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as supportProjectionSources } from "../../support/public/hospitality-owner-projection-sources";
import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import type {CustomerPortalSessionActor} from "../../customer_360/public/portal-contract";
export async function customerPortalFileTarget(db:SqlDatabase,kind:"set"|"link"|"upload",id:string) {
  const predicate=kind==='set'?'s.id=?':kind==='link'?'l.id=?':'u.id=?';
  const joins=kind==='set'?'':kind==='link'?'JOIN attachment_links l ON l.set_id=s.id':'JOIN upload_intents u ON u.attachment_set_id=s.id';
  const row=await db.prepare(`SELECT s.target_id,s.target_type,s.purpose_key FROM attachment_sets s ${joins} WHERE ${predicate} AND s.target_type='customer_access_binding' AND s.purpose_key='customer_support_evidence'`).bind(id).first<{target_id:string;target_type:string;purpose_key:string}>();
  return row?.target_id??null;
}
export async function customerPortalSharedFileRows(db:SqlDatabase,actor:Pick<CustomerPortalSessionActor,"userId"|"email">,bindingIds:string[],now:number) {
  return (await db.prepare(`SELECT b.id AS binding_id,l.id AS link_id,
      COALESCE(NULLIF(l.caption,''),'Shared support document') AS label
    FROM (${customer_authorityLastAuditProjectionSources.customer_access_bindings}) b
    JOIN (${customer_authorityLastAuditProjectionSources.customer_access_identities}) i ON i.binding_id=b.id AND i.user_id=? AND i.email=?
    JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id
      AND o.kind IN ('sales','quote') AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled')
    JOIN (${supportProjectionSources.support_tickets}) t ON t.order_id=b.order_id AND t.company_id=b.company_id AND t.source_kind='sales_order'
    JOIN attachment_sets s ON s.company_id=t.company_id AND s.target_type='support_ticket'
      AND s.target_id=t.id AND s.purpose_key='customer_shareable' AND s.state='bound'
    JOIN attachment_links l ON l.set_id=s.id AND l.state='bound'
    JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
      AND a.source_state='available' AND a.preview_object_key IS NOT NULL
    WHERE b.id IN (${bindingIds.map(() => "?").join(",")}) AND b.email=?
      AND b.status='approved' AND b.expires_at>?
    ORDER BY b.id,l.bound_at,l.id`).bind(actor.userId,actor.email.toLowerCase(),...bindingIds,actor.email.toLowerCase(),now).all<{binding_id:string;link_id:string;label:string}>()).results;
}
export async function customerPortalPreviewRow(db:SqlDatabase,actor:Pick<CustomerPortalSessionActor,"userId"|"email">,linkId:string,now:number) {
  return db.prepare(`SELECT s.company_id,l.id AS link_id,a.id AS attachment_id,
      a.preview_object_key,a.preview_media_type
    FROM attachment_links l
    JOIN attachment_sets s ON s.id=l.set_id AND s.target_type='support_ticket'
      AND s.purpose_key='customer_shareable' AND s.state='bound'
    JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
      AND a.source_state='available' AND a.preview_object_key IS NOT NULL
    JOIN (${supportProjectionSources.support_tickets}) t ON t.id=s.target_id AND t.company_id=s.company_id AND t.source_kind='sales_order'
    JOIN (${customer_authorityLastAuditProjectionSources.customer_access_bindings}) b ON b.order_id=t.order_id AND b.company_id=t.company_id
      AND b.email=? AND b.status='approved' AND b.expires_at>?
    JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id
      AND o.kind IN ('sales','quote') AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled')
    JOIN (${customer_authorityLastAuditProjectionSources.customer_access_identities}) i ON i.binding_id=b.id AND i.user_id=? AND i.email=?
    WHERE l.id=? AND l.state='bound'`).bind(actor.email.toLowerCase(),now,actor.userId,actor.email.toLowerCase(),linkId).first<{company_id:string;link_id:string;attachment_id:string;preview_object_key:string;preview_media_type:string}>();
}
export async function recordCustomerPortalFileAccess(db:SqlDatabase,event:{id:string;companyId:string;attachmentId:string;linkId:string;userId:string;now:number}) {
  await db.prepare(`INSERT INTO attachment_access_events(id,company_id,attachment_id,link_id,actor_id,variant,outcome,created_at) VALUES(?,?,?,?,?,'preview','allowed',?)`).bind(event.id,event.companyId,event.attachmentId,event.linkId,event.userId,event.now).run();
}
