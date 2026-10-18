import { finalRootOwnerProjectionSources as commerceProjectionSources } from "../../commerce/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as customer360ProjectionSources } from "../../customer_360/public/final-root-owner-projection-sources";
import "server-only";
import type { SqlDatabase, SqlStatement } from "../../runtime/public/runtime/types";
import type { CustomerPortalSessionActor, CustomerPortalBindingScope, PortalBinding } from "../../customer_360/public/portal-contract";
import {customerPortalPurchaseScope} from './portal-read-purchases';
export async function projectCustomerPortalAccess(db: SqlDatabase, actor: {userId:string;email:string}, now:number, readId:string) {
  const purchase=await customerPortalPurchaseScope(db,'b');
  return (await db.prepare(`WITH eligible AS (
  SELECT b.id,b.company_id,b.version,b.order_id,b.customer_id,b.email,o.number,o.kind,o.state,o.due_date,o.currency,o.updated_at
  FROM customer_access_bindings b JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=b.order_id AND o.company_id=b.company_id
  WHERE b.email=? AND b.status='approved' AND b.expires_at>? AND o.party_id=b.customer_id AND ${purchase}
    AND o.kind IN ('sales','quote') AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled')
    AND NOT EXISTS(SELECT 1 FROM customer_access_identities i WHERE i.binding_id=b.id AND i.user_id<>?)
  FOR SHARE OF b,o
 ), identity AS (
  INSERT INTO customer_access_identities(binding_id,user_id,email,bound_at)
  SELECT id,?,?,? FROM eligible ON CONFLICT(binding_id) DO NOTHING RETURNING binding_id
 ), accessible AS (
  SELECT e.* FROM eligible e WHERE EXISTS(SELECT 1 FROM customer_access_identities i WHERE i.binding_id=e.id AND i.user_id=? AND i.email=?) OR EXISTS(SELECT 1 FROM identity i WHERE i.binding_id=e.id)
 ), audit AS (
  INSERT INTO customer_access_reads(id,binding_id,binding_version,user_id,read_at,projection_version)
  SELECT ?||':'||id,id,version,?,?,'customer-order-v1' FROM accessible RETURNING binding_id
 ) SELECT a.company_id portal_purchase_company_id,a.id binding_id,a.version binding_version,a.order_id,a.number,a.kind,a.state,a.due_date,a.currency,a.updated_at,
 COALESCE((SELECT jsonb_agg(jsonb_build_object('number',f.number,'createdAt',f.created_at,'recorded',true) ORDER BY f.created_at) FROM (${commerceProjectionSources.commerce_fulfillments}) f WHERE f.order_id=a.order_id),'[]'::jsonb) fulfillments,
 COALESCE((SELECT jsonb_agg(jsonb_build_object('action',pa.action,'createdAt',pa.created_at) ORDER BY pa.created_at) FROM (${customer360ProjectionSources.customer_portal_actions}) pa WHERE pa.binding_id=a.id AND pa.user_id=?),'[]'::jsonb) actions
 FROM accessible a JOIN audit x ON x.binding_id=a.id ORDER BY a.updated_at DESC,a.id`).bind(actor.email,now,actor.userId,actor.userId,actor.email,now,actor.userId,actor.email,readId,actor.userId,now,actor.userId).all<Record<string,unknown>>()).results;
}
export async function lockCustomerPortalBinding(db: SqlDatabase, id:string, email:string) {
  return db.prepare(`SELECT b.id binding_id,b.company_id,b.order_id,b.customer_id,b.email,b.status,b.expires_at,b.version binding_version,
       o.version order_version,o.number,o.state
      FROM customer_access_bindings b JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=b.order_id AND o.company_id=b.company_id
      WHERE b.id=? AND lower(b.email)=? AND o.party_id=b.customer_id AND o.kind IN ('sales','quote') FOR UPDATE`).bind(id,email).first<PortalBinding>();
}
export async function customerPortalIdentityExists(db:SqlDatabase, bindingId:string,actor:CustomerPortalSessionActor) {
  return !!await db.prepare('SELECT 1 FROM customer_access_identities WHERE binding_id=? AND user_id=? AND email=?').bind(bindingId,actor.userId,actor.email.toLowerCase()).first();
}
export async function readCustomerPortalBindingScope(db:SqlDatabase,actor:Pick<CustomerPortalSessionActor,"userId"|"email">,bindingId:string,now:number) {
  const purchase=await customerPortalPurchaseScope(db,'b');
  return db.prepare(`SELECT b.id,b.company_id,o.branch_id,o.department_id,b.version
    FROM customer_access_bindings b
    JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id
    JOIN customer_access_identities i ON i.binding_id=b.id AND i.user_id=? AND i.email=?
    WHERE b.id=? AND b.email=? AND b.status='approved' AND b.expires_at>? AND ${purchase}
      AND o.kind IN ('sales','quote') AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled')`).bind(actor.userId,actor.email.toLowerCase(),bindingId,actor.email.toLowerCase(),now).first<CustomerPortalBindingScope>();
}
export function prepareCustomerPortalEvidenceChange(db:SqlDatabase,input:{now:number;bindingId:string;companyId:string;version:number;actorId:string;key:string}):SqlStatement[] {
  return [db.prepare('UPDATE customer_access_bindings SET version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=?').bind(input.now,input.bindingId,input.companyId,input.version),
    db.prepare("INSERT INTO customer_access_events(id,binding_id,version,action,actor_id,reason,created_at) VALUES(?,?,?,'portal-evidence-bind',?,?,?)").bind(`customer-access-event-${input.key}`,input.bindingId,input.version+1,input.actorId,'Customer attached clean evidence to the support request',input.now)];
}
