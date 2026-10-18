import 'server-only';
import {randomUUID} from 'node:crypto';
import {hasModuleAccess} from '@/data/module-access';
import type {EcommercePurchaseControls} from '@/lib/ecommerce/purchase-controls';
import type {WorkContext} from '../../identity_access/public/authorization';
import {requireQueuedCommandAuthority} from '../../identity_access/public/queued-command-authority';
import {commerceOwnerTransaction,deferCommerceOwnerCheck} from '../../commerce/public/purchase-boundary';
import {commercialOwnerProjectionSources as commerce} from '../../commerce/public/commercial-owner-projection-sources';
import {commercialOwnerProjectionSources as finance} from '../../finance/public/commercial-owner-projection-sources';
import {commercePackageEnforcementMode,checkPurchasedCapability,readPurchasedAccesses,purchaseCommandBoundary} from '../../commercial/public/purchase-command-boundary';
import {scheduledFeaturePredicate} from '../../pms/public/hospitality-rollout';
import {holdingResources} from '../../runtime/public/runtime';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
type Target={kind:'channel'|'listing'|'cart'|'checkout';id:string};
type Source={company_id:string;branch_id:string|null;department_id:string|null;version:number;identity:string;image:string};
export type EcommercePurchaseCommand={operation:string;key:string;id:string;purpose:'new-work'|'settle-existing';review?:boolean;before?:Target;after:Target};
/** Closed owner joins use actual stored identities, including the exact native
 * Sales order. A channel label or supplied product name grants no authority. */
async function source(database:SqlDatabase,actor:WorkContext,target:Target){
 let sql:string;
 if(target.kind==='channel')sql="SELECT x.company_id,NULL::text branch_id,NULL::text department_id,x.version,jsonb_build_array(x.id,x.company_id)::text identity,to_jsonb(x)::text image FROM ecommerce_channels x WHERE x.id=?";
 else if(target.kind==='listing')sql=`SELECT c.company_id,NULL::text branch_id,NULL::text department_id,x.version,jsonb_build_array(x.id,x.channel_id,x.product_id,c.company_id)::text identity,jsonb_build_array(to_jsonb(x),to_jsonb(c),p.id,p.company_id)::text image FROM ecommerce_listings x JOIN ecommerce_channels c ON c.id=x.channel_id JOIN (${commerce.commerce_products}) p ON p.id=x.product_id AND p.company_id=c.company_id WHERE x.id=?`;
 else{
  const checkout=target.kind==='checkout';
  sql=`SELECT c.company_id,c.branch_id,c.department_id,${checkout?'x':'c'}.version,jsonb_build_array(${checkout?'x.id,x.cart_id,x.sales_order_id,':''}c.id,c.company_id,c.branch_id,c.department_id,c.channel_id,c.party_id,c.warehouse_id,c.currency)::text identity,
   jsonb_build_array(to_jsonb(c),to_jsonb(channel),party.id,party.company_id,warehouse.id,warehouse.company_id,warehouse.branch_id,
   COALESCE((SELECT jsonb_agg(to_jsonb(line) ORDER BY line.ordinal) FROM ecommerce_cart_lines line WHERE line.cart_id=c.id),'[]'::jsonb)
   ${checkout?`,to_jsonb(x),o.id,o.company_id,o.branch_id,o.department_id,o.kind,o.party_id,o.warehouse_id,o.currency,
   COALESCE((SELECT jsonb_agg(to_jsonb(link) ORDER BY link.id) FROM ecommerce_fulfillment_links link WHERE link.checkout_id=x.id),'[]'::jsonb),
   COALESCE((SELECT jsonb_agg(to_jsonb(link) ORDER BY link.id) FROM ecommerce_return_links link WHERE link.checkout_id=x.id),'[]'::jsonb)`:''})::text image
   FROM ${checkout?'ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id':'ecommerce_carts c'}
   JOIN ecommerce_channels channel ON channel.id=c.channel_id AND channel.company_id=c.company_id
   JOIN (${finance.finance_parties}) party ON party.id=c.party_id AND party.company_id=c.company_id AND party.kind='customer'
   JOIN (${commerce.commerce_warehouses}) warehouse ON warehouse.id=c.warehouse_id AND warehouse.company_id=c.company_id AND warehouse.branch_id=c.branch_id
   ${checkout?`JOIN (${commerce.commerce_orders}) o ON o.id=x.sales_order_id AND o.id='order-ecommerce-order-'||c.id AND o.company_id=c.company_id AND o.branch_id=c.branch_id AND o.department_id IS NOT DISTINCT FROM c.department_id AND o.kind='sales' AND o.party_id=c.party_id AND o.warehouse_id=c.warehouse_id AND o.currency=c.currency`:''}
   WHERE ${checkout?'x':'c'}.id=?`;
 }
 const row=await database.prepare(sql).bind(target.id).first<Source>();
 if(!row||row.company_id!==actor.companyId||target.kind!=='channel'&&target.kind!=='listing'&&(actor.branchId&&row.branch_id!==actor.branchId||actor.departmentId&&row.department_id!==actor.departmentId))throw new ApiError(404,'ECOMMERCE_SOURCE_NOT_FOUND','The actual Ecommerce source is unavailable in this workspace.');
 return row;
}
async function current(database:SqlDatabase,actor:WorkContext,review=false,read=false){
 if(!hasModuleAccess(actor.modules,'ecommerce'))throw new ApiError(403,'MODULE_FORBIDDEN','Your current role cannot access E-commerce.');
 const bound=await requireQueuedCommandAuthority(database,actor,read?'commerce.read':'commerce.sales');
 if(review)await requireQueuedCommandAuthority(database,bound,'commerce.approve');
 const selected=await database.prepare("SELECT 1 FROM assignments WHERE id=? AND company_id=? AND (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> '[\"ecommerce\"]'::jsonb)").bind(actor.assignmentId,actor.companyId).first();
 if(!selected)throw new ApiError(403,'ECOMMERCE_ACCESS_CHANGED','The selected assignment no longer permits Ecommerce.');
 if(!await database.prepare(`SELECT ${scheduledFeaturePredicate('ecommerce_release.company_id','ecommerce')} allowed FROM (SELECT ?::text company_id) ecommerce_release`).bind(actor.companyId).first<boolean>('allowed'))throw new ApiError(403,'FEATURE_COMING_SOON','Ecommerce is not released for this Company.');
 return bound;
}
export async function requireEcommercePurchasedRead(database:SqlDatabase,actor:WorkContext,target?:Target){
 const mode=await commercePackageEnforcementMode();if(mode==='disabled')return;
 await current(database,actor,false,true);if(target)await source(database,actor,target);
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'sales.ecommerce',purpose:'history'});
}
export async function ecommercePurchaseControls(database:SqlDatabase,actor:WorkContext):Promise<EcommercePurchaseControls|undefined>{
 const mode=await commercePackageEnforcementMode();if(mode==='disabled')return undefined;await current(database,actor,false,true);const access=(await readPurchasedAccesses(database,mode,actor.companyId,['sales.ecommerce']))['sales.ecommerce'];
 const permitted=async(permission:string)=>requireQueuedCommandAuthority(database,actor,permission).then(()=>true).catch(error=>{if(error instanceof ApiError&&error.status===403)return false;throw error;}),write=await permitted('commerce.sales'),review=write&&await permitted('commerce.approve');
 return {newWork:access.newWork&&write,settleExisting:access.settleExisting&&write,history:access.history,revision:access.revision,canReview:review};
}
/** Only the original complete owner unit may encompass child Sales/Stock/
 * Finance commands. Supplied arbitrary transactions cannot defer our checks. */
export async function withEcommercePurchasedCommand<Result extends Record<string,unknown>>(actor:WorkContext,command:EcommercePurchaseCommand,run:(database:SqlDatabase)=>Promise<Result>,provided?:SqlDatabase){
 const mode=await commercePackageEnforcementMode(),owner=(await holdingResources(actor.holdingId)).write;
 if(mode==='disabled')return run(provided??owner);
 if(provided)throw new ApiError(503,'ECOMMERCE_COMPOSITION_REQUIRED','Ecommerce requires its complete owned Holding handoff.');
 return commerceOwnerTransaction(owner,actor,async database=>{
  const bound=await current(database,actor,command.review),prior=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(command.key,command.operation,command.id,actor.userId,actor.companyId).first(),before=command.before?await source(database,actor,command.before):null;
  if(prior&&!command.before)await source(database,actor,command.after);
  const scope={companyId:actor.companyId,capability:'sales.ecommerce',purpose:prior?'history' as const:command.purpose},key=`ecommerce-purchase:${randomUUID()}`,boundary=purchaseCommandBoundary(database,mode,key,scope)!;
  await checkPurchasedCapability(database,mode,scope);await database.batch(boundary.before);
  const result=await run(database),after=await source(database,actor,command.after);
  if(before&&command.before?.kind===command.after.kind&&before.identity!==after.identity||!prior&&(typeof result.version!=='number'||Number(after.version)!==result.version))throw new ApiError(409,'ECOMMERCE_SOURCE_CHANGED','The original Ecommerce handoff changed before commit.');
  deferCommerceOwnerCheck(database,actor,async()=>{
   await current(database,bound,command.review);
   const final=await source(database,actor,command.after);
   if(final.image!==after.image)throw new ApiError(409,'ECOMMERCE_SOURCE_CHANGED','The original Ecommerce source changed after its owner effects.');
   const observed=await database.batch([...boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)]);boundary.observe?.(observed);
  });
  return result;
 });
}
