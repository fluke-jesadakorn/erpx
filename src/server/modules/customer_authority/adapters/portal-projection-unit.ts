import 'server-only';
import {createHash} from 'node:crypto';
import {z} from 'zod';
import {isTransactionalSqlDatabase,type SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {lockPackageSettlementSources} from '../../commercial/public/package-locks';
import {commerceCompleteOwnerSourceReadLocks} from '../../commerce/public/owner-source-locks';
import {finalRootOwnerProjectionSources as commerce} from '../../commerce/public/final-root-owner-projection-sources';
import {portalPreviewSupportSourceReadLocks} from '../../support/public/portal-preview-sources';
import {attachmentPrivateReadSourceLocks} from '../../files/public/private-attachment-reads';
import {customerPortalPreviewRow,customerPortalSharedPreviewSource} from '../../files/public/customer-portal-files';
import {lockScheduledFeatureSources} from '../../pms/public/scheduled-feature-source-locks';
import {flushFinanceDeferredEffects} from '../../finance/public/finance-transactions';
import {customerPortalSourceReadLocks} from './portal-source-locks';
import {portalSupportInstalled,portalSupportSourceReadLocks} from './portal-support-sources';
import {customerPortalPurchaseScope} from './portal-read-purchases';

type Actor={userId:string;email:string};
type Projection={items:{binding_id:string;sharedFiles:{linkId:string}[]}[]};
const nativeRow=z.object({id:z.string(),image:z.string()});
const sourceSchema=z.object({core:z.string(),identities:z.array(nativeRow.extend({userId:z.string(),email:z.string()})),reads:z.array(nativeRow.extend({bindingId:z.string(),userId:z.string(),bindingVersion:z.number().int().positive(),projectionVersion:z.string()}))});
type SourceImage=z.infer<typeof sourceSchema>;
const units=new WeakMap<SqlDatabase,{actor:string;supportInstalled:boolean}>();
const digest=(value:unknown)=>createHash('sha256').update(JSON.stringify(value)).digest('hex');
const changed=()=>new ApiError(409,'CUSTOMER_PORTAL_PROJECTION_CHANGED','The verified customer document projection changed before its read committed.');

/** Only this fresh, source-leased constructor mints the same-owner thread read.
 * A supplied active transaction or another actor cannot inherit the phase. */
export function customerPortalProjectionSupportInstalled(database:SqlDatabase,actor:Actor){const unit=units.get(database);if(!unit||unit.actor!==JSON.stringify(actor))throw new ApiError(503,'CUSTOMER_PORTAL_PROJECTION_COMPOSITION_REQUIRED','Use the complete customer document read.');return unit.supportInstalled;}

async function source(database:SqlDatabase,actor:Actor,installed:boolean):Promise<SourceImage>{
 const support=installed?`jsonb_build_array((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM customer_portal_support_responses r JOIN bindings b ON b.id=r.binding_id),(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM customer_portal_support_acknowledgements a JOIN bindings b ON b.id=a.binding_id),(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM customer_portal_support_closures c JOIN bindings b ON b.id=c.binding_id))`:'NULL';
 const row=await database.prepare(`WITH bindings AS(SELECT * FROM customer_access_bindings WHERE email=?)
 SELECT jsonb_build_array(
 (SELECT jsonb_agg(to_jsonb(b) ORDER BY b.id) FROM bindings b),
 (SELECT jsonb_agg(to_jsonb(o) ORDER BY o.id) FROM (${commerce.commerce_orders}) o WHERE EXISTS(SELECT 1 FROM bindings b WHERE b.order_id=o.id AND b.company_id=o.company_id)),
 (SELECT jsonb_agg(to_jsonb(f) ORDER BY f.id) FROM (${commerce.commerce_fulfillments}) f JOIN (${commerce.commerce_orders}) o ON o.id=f.order_id WHERE EXISTS(SELECT 1 FROM bindings b WHERE b.order_id=f.order_id AND b.company_id=o.company_id)),
 (SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM customer_portal_actions a WHERE EXISTS(SELECT 1 FROM bindings b WHERE b.id=a.binding_id)),${support})::text core,
 COALESCE((SELECT jsonb_agg(jsonb_build_object('id',i.binding_id,'image',to_jsonb(i)::text,'userId',i.user_id,'email',i.email) ORDER BY i.binding_id) FROM customer_access_identities i JOIN bindings b ON b.id=i.binding_id),'[]'::jsonb) identities,
 COALESCE((SELECT jsonb_agg(jsonb_build_object('id',r.id,'image',to_jsonb(r)::text,'bindingId',r.binding_id,'userId',r.user_id,'bindingVersion',r.binding_version,'projectionVersion',r.projection_version) ORDER BY r.id) FROM customer_access_reads r JOIN bindings b ON b.id=r.binding_id WHERE r.user_id=?),'[]'::jsonb) reads`).bind(actor.email.toLowerCase(),actor.userId).first<{core:string;identities:string;reads:string}>();
 if(!row)throw changed();return sourceSchema.parse({core:row.core,identities:JSON.parse(row.identities),reads:JSON.parse(row.reads)});
}

/** Original direct/native projections keep their legacy constructor. HTTP and
 * SSR requests use this closed unit so identity/read audits and the complete
 * Support enrichment remain under the same final Control authority check. */
export async function withCustomerPortalRequestProjection<Result extends Projection>(owner:SqlDatabase,actor:Actor,read:(database:SqlDatabase)=>Promise<Result>,assertCurrent:()=>Promise<void>):Promise<Result>{
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'CUSTOMER_PORTAL_PROJECTION_TRANSACTION_REQUIRED','Customer document requests require their fresh read transaction.');
 const installed=await portalSupportInstalled(owner),context=JSON.stringify(actor);
 return owner.transaction(async database=>{
  await lockPackageSettlementSources(database);
  await database.batch([...commerceCompleteOwnerSourceReadLocks(database),...customerPortalSourceReadLocks(database),...(installed?portalSupportSourceReadLocks(database):[]),portalPreviewSupportSourceReadLocks(database),attachmentPrivateReadSourceLocks(database)]);
  await lockScheduledFeatureSources(database);
  units.set(database,{actor:context,supportInstalled:installed});
  try{
   await assertCurrent();const before=await source(database,actor,installed),result=await read(database),after=await source(database,actor,installed),oldReads=new Map(before.reads.map(row=>[row.id,row.image])),oldIdentities=new Map(before.identities.map(row=>[row.id,row.image])),nextReads=new Map(after.reads.map(row=>[row.id,row.image])),nextIdentities=new Map(after.identities.map(row=>[row.id,row.image])),bindings=new Set(result.items.map(item=>item.binding_id)),newReads=after.reads.filter(row=>!oldReads.has(row.id)),newIdentities=after.identities.filter(row=>!oldIdentities.has(row.id));
   if(before.core!==after.core||before.reads.some(row=>nextReads.get(row.id)!==row.image)||before.identities.some(row=>nextIdentities.get(row.id)!==row.image)||newReads.length!==result.items.length||newReads.some(row=>!bindings.has(row.bindingId)||row.userId!==actor.userId||row.projectionVersion!=='customer-order-v1')||newIdentities.some(row=>!bindings.has(row.id)||row.userId!==actor.userId||row.email!==actor.email.toLowerCase()))throw changed();
   const links=[...new Set(result.items.flatMap(item=>item.sharedFiles.map(file=>file.linkId)))],fileImages=new Map(await Promise.all(links.map(async id=>[id,(await customerPortalSharedPreviewSource(database,actor,id))?.witness] as const)));
   await flushFinanceDeferredEffects(database);
   if(JSON.stringify(actor)!==context||digest(await source(database,actor,installed))!==digest(after))throw changed();
   const purchase=await customerPortalPurchaseScope(database,'b');
   if(newReads.length){const current=await database.prepare(`SELECT count(*)::int n FROM customer_access_reads r JOIN customer_access_bindings b ON b.id=r.binding_id AND b.version=r.binding_version JOIN customer_access_identities i ON i.binding_id=b.id AND i.user_id=r.user_id AND i.email=b.email JOIN (${commerce.commerce_orders}) o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id WHERE r.id IN (${newReads.map(()=>'?').join(',')}) AND r.user_id=? AND b.email=? AND b.status='approved' AND b.expires_at>? AND o.kind IN ('quote','sales') AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled') AND ${purchase}`).bind(...newReads.map(row=>row.id),actor.userId,actor.email.toLowerCase(),Date.now()).first<number>('n');if(current!==newReads.length)throw changed();}
   for(const id of links)if(!fileImages.get(id)||!await customerPortalPreviewRow(database,actor,id,Date.now())||(await customerPortalSharedPreviewSource(database,actor,id))?.witness!==fileImages.get(id))throw changed();
   await assertCurrent();return result;
  }finally{units.delete(database);}
 });
}
