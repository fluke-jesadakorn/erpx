import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {commerceCompleteOwnerSourceReadLocks} from '@erpx/server-commerce/public/owner-source-locks';
import {finalRootOwnerProjectionSources as commerce} from '@erpx/server-commerce/public/final-root-owner-projection-sources';
import {commercePackageEnforcementMode,checkPurchasedCapability,readPurchasedAccesses,purchaseCommandFailure} from '@erpx/server-commercial/public/purchase-command-boundary';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {financialConstraintError} from '@erpx/server-workflow/public/command-guards';
import {customerPortalSourceReadLocks} from '@erpx/server-customer_authority/adapters/portal-source-locks';
type BindingSource={id:string;company_id:string;branch_id:string;department_id:string|null;created_by:string;version:number;source_state:string;identity:string;image:string};
async function source(database:SqlDatabase,actor:WorkContext,id:string){
 const row=await database.prepare(`SELECT b.id,b.company_id,o.branch_id,o.department_id,b.created_by,b.version,o.state source_state,
 jsonb_build_array(b.id,b.company_id,b.order_id,b.customer_id,b.email,o.id,o.company_id,o.branch_id,o.department_id,o.kind,o.party_id,o.state)::text identity,
 jsonb_build_array(to_jsonb(b),o.id,o.company_id,o.branch_id,o.department_id,o.kind,o.party_id,o.state)::text image
 FROM customer_access_bindings b JOIN (${commerce.commerce_orders}) o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id AND o.kind IN ('quote','sales') WHERE b.id=? AND b.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)`).bind(id,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).first<BindingSource>();
 if(!row)throw new ApiError(404,'CUSTOMER_ACCESS_NOT_FOUND','Customer access request not found in this scope.');return row;
}
async function current(database:SqlDatabase,actor:WorkContext,review=false){
 const bound=await requireQueuedCommandAuthority(database,actor,'commerce.read');await requireQueuedCommandAuthority(database,bound,'commerce.sales');if(review)await requireQueuedCommandAuthority(database,bound,'commerce.approve');
 if(!await database.prepare(`SELECT ${scheduledFeaturePredicate('portal_release.company_id','customer_authority')} allowed FROM (SELECT ?::text company_id) portal_release`).bind(actor.companyId).first<boolean>('allowed'))throw new ApiError(403,'FEATURE_COMING_SOON','Customer access is not released for this Company.');return bound;
}
export async function customerAccessPurchaseControls(database:SqlDatabase,actor:WorkContext){
 const mode=await commercePackageEnforcementMode();if(mode==='disabled')return undefined;await current(database,actor);const access=(await readPurchasedAccesses(database,mode,actor.companyId,['sales.portal']))['sales.portal'];return {newWork:access.newWork,settleExisting:access.settleExisting,history:access.history,revision:access.revision};
}
export async function requireCustomerAccessPurchasedRead(database:SqlDatabase,actor:WorkContext,id?:string){
 const mode=await commercePackageEnforcementMode();if(mode==='disabled')return;await current(database,actor);if(id)await source(database,actor,id);await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'sales.portal',purpose:'history'});
}
/** Public accounts have no staff Company assignment. This short source unit
 * fences grant writes before its first query, then locks the actual native
 * binding/order/action chain. It creates no staff authority or entitlement. */
export async function customerPortalOwnedUnit<Result>(owner:SqlDatabase,run:(database:SqlDatabase)=>Promise<Result>){
 const mode=await commercePackageEnforcementMode();if(mode==='disabled')return run(owner);if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'CUSTOMER_PORTAL_TRANSACTION_REQUIRED','Customer portal access requires its owned Holding transaction.');
 try{return await owner.transaction(async database=>{await database.prepare('LOCK TABLE company_package_grants IN SHARE ROW EXCLUSIVE MODE').run();await database.batch([...commerceCompleteOwnerSourceReadLocks(database),...customerPortalSourceReadLocks(database)]);return run(database);});}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
