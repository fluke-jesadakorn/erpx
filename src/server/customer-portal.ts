import 'server-only';
import {currentSession} from './authorization';
import {holdingBindingBySlug,holdingResources} from './runtime';
import {ApiError} from './http';
import type {Row} from './commerce';
type CustomerPortalItem=Record<string,unknown>&{binding_id:string;binding_version:number;order_id:string;number:string;kind:string;state:string;due_date:string;currency:string;updated_at:number;fulfillments:Record<string,unknown>[]};
export async function customerPortalContext(slug:string){const {session}=await currentSession();if(!session.user.emailVerified)throw new ApiError(403,'CUSTOMER_EMAIL_UNVERIFIED','Verify your account email before opening customer documents.');const binding=await holdingBindingBySlug(slug),resources=await holdingResources(binding.id);return {...resources,actor:{userId:session.user.id,email:session.user.email.toLowerCase()}};}
export async function customerPortalProjection(write:import('./runtime/types').SqlDatabase,actor:{userId:string;email:string}){const now=Date.now(),readId=`customer-read-${crypto.randomUUID()}`;
 const rows=(await write.prepare(`WITH eligible AS (
  SELECT b.id,b.version,b.order_id,b.customer_id,b.email,o.number,o.kind,o.state,o.due_date,o.currency,o.updated_at
  FROM customer_access_bindings b JOIN commerce_orders o ON o.id=b.order_id AND o.company_id=b.company_id
  WHERE b.email=? AND b.status='approved' AND b.expires_at>? AND o.party_id=b.customer_id
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
 ) SELECT a.id binding_id,a.version binding_version,a.order_id,a.number,a.kind,a.state,a.due_date,a.currency,a.updated_at,
 COALESCE((SELECT jsonb_agg(jsonb_build_object('number',f.number,'createdAt',f.created_at,'recorded',true) ORDER BY f.created_at) FROM commerce_fulfillments f WHERE f.order_id=a.order_id),'[]'::jsonb) fulfillments
 FROM accessible a JOIN audit x ON x.binding_id=a.id ORDER BY a.updated_at DESC,a.id`).bind(actor.email,now,actor.userId,actor.userId,actor.email,now,actor.userId,actor.email,readId,actor.userId,now).all<Row>()).results;
 const items:CustomerPortalItem[]=rows.map(r=>({...r,fulfillments:typeof r.fulfillments==='string'?JSON.parse(r.fulfillments):r.fulfillments}) as CustomerPortalItem);
 return {items,projectionVersion:'customer-order-v1'};
}
export async function customerPortalList(slug:string){const {write,actor}=await customerPortalContext(slug);return customerPortalProjection(write,actor);}
