import 'server-only';
import { nativeClearCommandAssertions } from '@erpx/collaboration-server/adapters/native-command-assertions';
import {randomUUID} from 'node:crypto';
import {hasModuleAccess} from '@erpx/platform-catalogue/module-access';
import type {EcommercePurchaseControls} from '@erpx/ecommerce-ui/lib/purchase-controls';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import {requireQueuedCommandAuthority} from '@erpx/identity-access-server/public/queued-command-authority';
import {commerceOwnerTransaction,deferCommerceOwnerCheck} from '@erpx/commerce-server/public/purchase-boundary';
import {commercialOwnerProjectionSources as commerce} from '@erpx/commerce-server/public/commercial-owner-projection-sources';
import {commercialOwnerProjectionSources as finance} from '@erpx/finance-server/public/commercial-owner-projection-sources';
import {commercePackageEnforcementMode,checkPurchasedCapability,readPurchasedAccesses,purchaseCommandBoundary} from '@erpx/commercial-server/public/purchase-command-boundary';
import { scheduledFeaturePredicate } from "@erpx/pms-server/public/feature-availability";
import {holdingResources} from '../../../../../platform/runtime/server/src/public/runtime';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {ecommerceModuleAssignment,ecommerceRowProbe,ecommerceScopeRow,ecommerceSourceIdentity,ecommerceReleaseGate,ecommerceScalar} from './read-tail-sql';
type Target={kind:'channel'|'listing'|'cart'|'checkout';id:string};
type Source={company_id:string;branch_id:string|null;department_id:string|null;version:number;identity:string;image:string};
export type EcommercePurchaseCommand={operation:string;key:string;id:string;purpose:'new-work'|'settle-existing';review?:boolean;before?:Target;after:Target};
const childAgg=(alias:string,table:string,orderBy:string,fk:string,parent:string)=>`COALESCE((${ecommerceScalar(`jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.${orderBy}) FROM ${table} ${alias} WHERE ${alias}.${fk}=${parent}.id`)}),'[]'::jsonb)`;
/** Closed owner joins use actual stored identities, including the exact native
 * Sales order. A channel label or supplied product name grants no authority. */
async function source(database:SqlDatabase,actor:WorkContext,target:Target){
 let sql:string;
 if(target.kind==='channel')sql=ecommerceSourceIdentity("x.company_id,NULL::text branch_id,NULL::text department_id,x.version,jsonb_build_array(x.id,x.company_id)::text identity,to_jsonb(x)::text image","ecommerce_channels x");
 else if(target.kind==='listing')sql=ecommerceSourceIdentity(`c.company_id,NULL::text branch_id,NULL::text department_id,x.version,jsonb_build_array(x.id,x.channel_id,x.product_id,c.company_id)::text identity,jsonb_build_array(to_jsonb(x),to_jsonb(c),p.id,p.company_id)::text image`,`ecommerce_listings x JOIN ecommerce_channels c ON c.id=x.channel_id JOIN (${commerce.commerce_products}) p ON p.id=x.product_id AND p.company_id=c.company_id`);
 else{
  const checkout=target.kind==='checkout';
  const columns=`c.company_id,c.branch_id,c.department_id,${checkout?'x':'c'}.version,jsonb_build_array(${checkout?'x.id,x.cart_id,x.sales_order_id,':''}c.id,c.company_id,c.branch_id,c.department_id,c.channel_id,c.party_id,c.warehouse_id,c.currency)::text identity,
   jsonb_build_array(to_jsonb(c),to_jsonb(channel),party.id,party.company_id,warehouse.id,warehouse.company_id,warehouse.branch_id,
   ${childAgg('line','ecommerce_cart_lines','ordinal','cart_id','c')}
   ${checkout?`,to_jsonb(x),o.id,o.company_id,o.branch_id,o.department_id,o.kind,o.party_id,o.warehouse_id,o.currency,
   ${childAgg('link','ecommerce_fulfillment_links','id','checkout_id','x')},
   ${childAgg('link','ecommerce_return_links','id','checkout_id','x')}`:''})::text image
  `;
  const from=`${checkout?'ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id':'ecommerce_carts c'}
   JOIN ecommerce_channels channel ON channel.id=c.channel_id AND channel.company_id=c.company_id
   JOIN (${finance.finance_parties}) party ON party.id=c.party_id AND party.company_id=c.company_id AND party.kind='customer'
   JOIN (${commerce.commerce_warehouses}) warehouse ON warehouse.id=c.warehouse_id AND warehouse.company_id=c.company_id AND warehouse.branch_id=c.branch_id
   ${checkout?`JOIN (${commerce.commerce_orders}) o ON o.id=x.sales_order_id AND o.id='order-ecommerce-order-'||c.id AND o.company_id=c.company_id AND o.branch_id=c.branch_id AND o.department_id IS NOT DISTINCT FROM c.department_id AND o.kind='sales' AND o.party_id=c.party_id AND o.warehouse_id=c.warehouse_id AND o.currency=c.currency`:''}
  `;
  sql=ecommerceSourceIdentity(columns,from,checkout?'x':'c');
 }
 const row=await database.prepare(sql).bind(target.id).first<Source>();
 if(!row||row.company_id!==actor.companyId||target.kind!=='channel'&&target.kind!=='listing'&&(actor.branchId&&row.branch_id!==actor.branchId||actor.departmentId&&row.department_id!==actor.departmentId))throw new ApiError(404,'ECOMMERCE_SOURCE_NOT_FOUND','The actual Ecommerce source is unavailable in this workspace.');
 return row;
}
async function current(database:SqlDatabase,actor:WorkContext,review=false,read=false){
 if(!hasModuleAccess(actor.modules,'ecommerce'))throw new ApiError(403,'MODULE_FORBIDDEN','Your current role cannot access E-commerce.');
 const bound=await requireQueuedCommandAuthority(database,actor,read?'commerce.read':'commerce.sales');
 if(review)await requireQueuedCommandAuthority(database,bound,'commerce.approve');
 const selected=await database.prepare(ecommerceModuleAssignment()).bind(actor.assignmentId,actor.companyId).first();
 if(!selected)throw new ApiError(403,'ECOMMERCE_ACCESS_CHANGED','The selected assignment no longer permits Ecommerce.');
 if(!await database.prepare(ecommerceReleaseGate(ecommerceScopeRow('ecommerce_release'),`${scheduledFeaturePredicate('ecommerce_release.company_id','ecommerce')}`)).bind(actor.companyId).first<boolean>('allowed'))throw new ApiError(403,'FEATURE_COMING_SOON','Ecommerce is not released for this Company.');
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
  const bound=await current(database,actor,command.review),prior=await database.prepare(ecommerceRowProbe('command_receipts',"command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?")).bind(command.key,command.operation,command.id,actor.userId,actor.companyId).first(),before=command.before?await source(database,actor,command.before):null;
  if(prior&&!command.before)await source(database,actor,command.after);
  const scope={companyId:actor.companyId,capability:'sales.ecommerce',purpose:prior?'history' as const:command.purpose},key=`ecommerce-purchase:${randomUUID()}`,boundary=purchaseCommandBoundary(database,mode,key,scope)!;
  await checkPurchasedCapability(database,mode,scope);await database.batch(boundary.before);
  const result=await run(database),after=await source(database,actor,command.after);
  if(before&&command.before?.kind===command.after.kind&&before.identity!==after.identity||!prior&&(typeof result.version!=='number'||Number(after.version)!==result.version))throw new ApiError(409,'ECOMMERCE_SOURCE_CHANGED','The original Ecommerce handoff changed before commit.');
  deferCommerceOwnerCheck(database,actor,async()=>{
   await current(database,bound,command.review);
   const final=await source(database,actor,command.after);
   if(final.image!==after.image)throw new ApiError(409,'ECOMMERCE_SOURCE_CHANGED','The original Ecommerce source changed after its owner effects.');
   const observed=await database.batch([...boundary.after,await nativeClearCommandAssertions(database, actor.companyId, key)]);boundary.observe?.(observed);
  });
  return result;
 });
}
