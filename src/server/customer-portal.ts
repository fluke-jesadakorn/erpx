import 'server-only';
import {z} from 'zod';
import {currentSession} from './authorization';
import {holdingBindingBySlug,holdingResources} from './runtime';
import type {SqlDatabase} from './runtime/types';
import type {PostgresDatabase} from './runtime/postgres';
import {ApiError} from './http';
import {hash} from './legal/service';
import type {Row} from './commerce';
import {customerPortalSharedFiles,customerPortalWorkContext,type CustomerPortalSessionActor} from './customer-portal-attachments';
import {prepareAttachmentBinding} from './attachments/service';
type CustomerPortalItem=Record<string,unknown>&{binding_id:string;binding_version:number;order_id:string;number:string;kind:string;state:string;due_date:string;currency:string;updated_at:number;fulfillments:Record<string,unknown>[];actions:Record<string,unknown>[];sharedFiles:Array<{linkId:string;label:string;previewAvailable:true}>};
export async function customerPortalContext(slug:string){const {session}=await currentSession();if(!session.user.emailVerified)throw new ApiError(403,'CUSTOMER_EMAIL_UNVERIFIED','Verify your account email before opening customer documents.');const binding=await holdingBindingBySlug(slug),resources=await holdingResources(binding.id);return {...resources,actor:{userId:session.user.id,email:session.user.email.toLowerCase(),holdingId:binding.id,holdingSlug:binding.slug,holdingName:binding.displayName}};}
export async function customerPortalProjection(write:SqlDatabase,actor:{userId:string;email:string}){const now=Date.now(),readId=`customer-read-${crypto.randomUUID()}`;
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
 COALESCE((SELECT jsonb_agg(jsonb_build_object('number',f.number,'createdAt',f.created_at,'recorded',true) ORDER BY f.created_at) FROM commerce_fulfillments f WHERE f.order_id=a.order_id),'[]'::jsonb) fulfillments,
 COALESCE((SELECT jsonb_agg(jsonb_build_object('action',pa.action,'createdAt',pa.created_at) ORDER BY pa.created_at) FROM customer_portal_actions pa WHERE pa.binding_id=a.id AND pa.user_id=?),'[]'::jsonb) actions
 FROM accessible a JOIN audit x ON x.binding_id=a.id ORDER BY a.updated_at DESC,a.id`).bind(actor.email,now,actor.userId,actor.userId,actor.email,now,actor.userId,actor.email,readId,actor.userId,now,actor.userId).all<Row>()).results;
 const shared=await customerPortalSharedFiles(write,actor,rows.map(row=>String(row.binding_id)));
 const items:CustomerPortalItem[]=rows.map(r=>({...r,fulfillments:typeof r.fulfillments==='string'?JSON.parse(r.fulfillments):r.fulfillments,actions:typeof r.actions==='string'?JSON.parse(r.actions):r.actions,sharedFiles:shared.get(String(r.binding_id))??[]}) as CustomerPortalItem);
 return {items,projectionVersion:'customer-order-v1'};
}
export async function customerPortalList(slug:string){const {write,actor}=await customerPortalContext(slug);return customerPortalProjection(write,actor);}

const portalCommandSchema=z.object({
 bindingId:z.string().trim().min(1).max(200),
 expectedBindingVersion:z.coerce.number().int().positive(),
 action:z.enum(['acknowledge_delivery','request_support']),
 note:z.string().trim().max(500).default(''),
 attachmentSetIds:z.array(z.string().min(1).max(180)).max(20).default([]),
});
type CustomerPortalActor=CustomerPortalSessionActor;
type PortalActionRow={id:string;binding_id:string;order_id:string;action:string;status:string;created_at:number;request_hash:string;user_id:string};
const portalActionResult=(r:PortalActionRow)=>({id:r.id,bindingId:r.binding_id,orderId:r.order_id,action:r.action,status:r.status,createdAt:Number(r.created_at)});

/**
 * Record a customer-facing acknowledgement or support request against the
 * explicitly approved source order. The binding and verified identity are
 * rechecked inside the serializable transaction so an expired/revoked grant
 * cannot be used after the page was rendered.
 */
export async function customerPortalCommand(db:PostgresDatabase,actor:CustomerPortalActor,raw:unknown,key:string){
 if(!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))throw new ApiError(422,'COMMAND_KEY_REQUIRED','Use a valid Idempotency-Key for this action.');
 const input=portalCommandSchema.parse(raw),requestHash=hash(JSON.stringify({actor:actor.userId,email:actor.email,input}));
 const id=`customer-portal-action-${crypto.randomUUID()}`,now=Date.now();
 try{
  return await db.transaction(async tx=>{
   const prior=await tx.prepare('SELECT id,binding_id,order_id,action,status,created_at,request_hash,user_id FROM customer_portal_actions WHERE request_key=?').bind(key).first<PortalActionRow>();
   if(prior){if(prior.user_id!==actor.userId||prior.request_hash!==requestHash)throw new ApiError(409,'IDEMPOTENCY_CONFLICT','This command key belongs to a different request.');return portalActionResult(prior);}
   const binding=await tx.prepare(`SELECT b.id binding_id,b.company_id,b.order_id,b.customer_id,b.email,b.status,b.expires_at,b.version binding_version,
       o.version order_version,o.number,o.state
      FROM customer_access_bindings b JOIN commerce_orders o ON o.id=b.order_id AND o.company_id=b.company_id
      WHERE b.id=? AND lower(b.email)=? AND o.party_id=b.customer_id FOR UPDATE`).bind(input.bindingId,actor.email.toLowerCase()).first<{binding_id:string;company_id:string;order_id:string;customer_id:string;email:string;status:string;expires_at:number;binding_version:number;order_version:number;number:string;state:string}>();
   if(!binding)throw new ApiError(403,'CUSTOMER_PORTAL_BINDING_UNAVAILABLE','This customer document is not available to your account.');
   if(binding.status!=='approved'||Number(binding.expires_at)<=now)throw new ApiError(403,'CUSTOMER_PORTAL_BINDING_EXPIRED','This customer access has expired or was revoked.');
   if(Number(binding.binding_version)!==input.expectedBindingVersion)throw new ApiError(409,'CUSTOMER_PORTAL_BINDING_CHANGED','The customer document access changed. Refresh before submitting.');
   if(!['confirmed','converted','partially_fulfilled','fulfilled'].includes(binding.state))throw new ApiError(409,'CUSTOMER_PORTAL_SOURCE_UNAVAILABLE','This customer document is no longer available for portal actions.');
   const identity=await tx.prepare('SELECT 1 FROM customer_access_identities WHERE binding_id=? AND user_id=? AND email=?').bind(binding.binding_id,actor.userId,actor.email.toLowerCase()).first();
   if(!identity)throw new ApiError(403,'CUSTOMER_PORTAL_IDENTITY_REQUIRED','Open the approved document once before submitting a customer action.');
   const existing=await tx.prepare('SELECT id,binding_id,order_id,action,status,created_at,request_hash,user_id FROM customer_portal_actions WHERE binding_id=? AND action=?').bind(binding.binding_id,input.action).first<PortalActionRow>();
   if(existing)throw new ApiError(409,'CUSTOMER_PORTAL_ACTION_ALREADY_RECORDED','This customer action has already been recorded.');
   if(input.action!=='request_support'&&input.attachmentSetIds.length)
    throw new ApiError(422,'CUSTOMER_PORTAL_ATTACHMENT_ACTION','Files may be attached only to a support request.');
   if(input.attachmentSetIds.length&&(!actor.holdingId||!actor.holdingSlug||!actor.holdingName))
    throw new ApiError(503,'CUSTOMER_PORTAL_ATTACHMENT_CONTEXT','Reload the customer portal before attaching files.');
   const attachmentActor=input.attachmentSetIds.length?customerPortalWorkContext(actor as Required<CustomerPortalSessionActor>,{
    id:binding.binding_id,company_id:binding.company_id,branch_id:null,department_id:null,version:Number(binding.binding_version),
   }):null;
   const prepared=attachmentActor?await prepareAttachmentBinding(tx,attachmentActor,{attachmentSetIds:input.attachmentSetIds,bindingContract:{targetType:'customer_access_binding',bindingType:'customer_portal_support_request',contexts:[{purposeKey:'customer_support_evidence',componentKind:'action',componentRef:'request_support'}]},targetId:binding.binding_id,expectedTargetVersion:Number(binding.binding_version),commandId:key,bindingId:id}):null;
   const result={id,bindingId:binding.binding_id,orderId:binding.order_id,action:input.action,status:'recorded',createdAt:now};
   await tx.prepare(`INSERT INTO customer_portal_actions(id,company_id,binding_id,order_id,customer_id,user_id,action,note,source_version,request_key,request_hash,status,created_at)
     VALUES(?,?,?,?,?,?,?,?,?,?,?,'recorded',?)`).bind(id,binding.company_id,binding.binding_id,binding.order_id,binding.customer_id,actor.userId,input.action,input.note,Number(binding.order_version),key,requestHash,now).run();
   if(prepared){
    await tx.batch([
     ...prepared.guards,
     tx.prepare('UPDATE customer_access_bindings SET version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=?').bind(now,binding.binding_id,binding.company_id,Number(binding.binding_version)),
     tx.prepare("INSERT INTO customer_access_events(id,binding_id,version,action,actor_id,reason,created_at) VALUES(?,?,?,'portal-evidence-bind',?,?,?)")
      .bind(`customer-access-event-${key}`,binding.binding_id,Number(binding.binding_version)+1,actor.userId,'Customer attached clean evidence to the support request',now),
     ...prepared.steps,
     tx.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key),
    ]);
   }
   const message=input.action==='request_support'?'Customer requested support on an order; review and contact the customer.':'Customer acknowledged delivery; review the order and fulfillment record.';
   await tx.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
     SELECT DISTINCT 'customer-portal-'||?||'-'||a.user_id,a.company_id,a.user_id,'commerce_order',o.id,o.version,?,?::bigint FROM assignments a JOIN commerce_orders o ON o.id=? AND o.company_id=a.company_id
     WHERE a.company_id=? AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
       AND iam_role_allows(a.role,a.company_id,'commerce.read')
       AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=o.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=o.department_id)) ON CONFLICT DO NOTHING`).bind(id,message,now,binding.order_id,binding.company_id).run();
   await tx.prepare(`INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json)
     VALUES(?,?,?,?,?,?,?,?,?,?,?)`).bind(crypto.randomUUID(),key,actor.userId,'external-customer',`customer.portal.${input.action}`,'commerce_order',binding.order_id,Number(binding.order_version),'Customer portal action',now,JSON.stringify({kind:'external-customer',bindingId:binding.binding_id,email:actor.email})).run();
   await tx.prepare(`INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at)
     VALUES(?,?,?,?,?,?,?,?)`).bind(crypto.randomUUID(),key,`customer.portal.${input.action}`,'commerce_order',binding.order_id,Number(binding.order_version),JSON.stringify({action:input.action,bindingId:binding.binding_id,customerId:binding.customer_id,userId:actor.userId,note:input.note,attachmentManifest:prepared?.manifest??[]}),now).run();
   return result;
  });
 }catch(error){
  if((error as {code?:string}).code==='23505'){
   const prior=await db.prepare('SELECT id,binding_id,order_id,action,status,created_at,request_hash,user_id FROM customer_portal_actions WHERE request_key=?').bind(key).first<PortalActionRow>();
   if(prior){if(prior.user_id!==actor.userId||prior.request_hash!==requestHash)throw new ApiError(409,'IDEMPOTENCY_CONFLICT','This command key belongs to a different request.');return portalActionResult(prior);}
   throw new ApiError(409,'CUSTOMER_PORTAL_ACTION_ALREADY_RECORDED','This customer action has already been recorded.');
  }
  throw error;
 }
}
