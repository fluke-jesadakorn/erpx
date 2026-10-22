import 'server-only';
import {randomUUID} from 'node:crypto';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {companyPurchaseReadLocks,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from '@erpx/server-commercial/public/purchase-command-boundary';
import {lockPackageSettlementSources} from '@erpx/server-commercial/public/package-locks';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {assertProductionModuleEnabled} from '@erpx/server-pms/public/hospitality-rollout';
import {assetsFinancialPhysicalSourceLocks,assetsFinancialPhysicalSource} from '@erpx/server-assets/public/financial-workflow-sources';
import {financialConstraintError} from '@erpx/server-workflow/public/commands';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/adapters/finance-transactions';
import {assetsFinancialFinanceSourceLocks,assetsFinancialReferences,assetsFinancialOriginalSource,assetsFinancialOwnerImage,assetsFinancialRetainedJournalImage} from '@erpx/server-finance/adapters/assets-financial-sources';
import type {AssetsFinancialResult} from '@erpx/server-finance/domain/assets-financial-workflow';
export type AssetsFinancialOperation='finance.asset.create'|'finance.asset.edit'|'finance.asset.cancel'|'finance.asset.activate'|'finance.asset.disposal.prepare'|'finance.asset.disposal.post'|'finance.asset.disposal.cancel';
const authorityImage=(actor:WorkContext)=>JSON.stringify([actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam]);
export async function requireAssetsFinancialAuthority(database:SqlDatabase,actor:WorkContext,permission:'finance.read'|'finance.write'|'finance.approve'='finance.read'){
 requirePermission(actor,'finance.read');requirePermission(actor,permission);
 if(actor.beta||actor.scope.kind!=='holding'||actor.branchId||actor.departmentId)throw new ApiError(403,'COMPANY_FINANCE_REQUIRED','Use a current Company Finance assignment for Asset recognition and disposal.');
 const current=await requireQueuedCommandAuthority(database,actor,permission);requirePermission(current,'finance.read');
 if(current.beta||current.scope.kind!=='holding'||current.branchId||current.departmentId||current.companyId!==actor.companyId)throw new ApiError(403,'COMPANY_FINANCE_REQUIRED','Use the current Company Finance assignment.');
 for(const feature of ['assets','finance'] as const)await assertProductionModuleEnabled(database,actor.companyId,feature);
 return current;
}
export async function assetsFinancialOwner(actor:WorkContext){const owner=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'TRANSACTION_REQUIRED','Asset financial commands require their original owned Holding transaction.');return owner;}
export async function assetsFinancialLeases(database:SqlDatabase,companyId:string,write=false){
 const purchase=companyPurchaseReadLocks(database,companyId);
 if(write){await lockPackageSettlementSources(database);await database.prepare('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE').run();}else await database.batch([purchase[0]]);
 await database.batch([...assetsFinancialPhysicalSourceLocks(database),...assetsFinancialFinanceSourceLocks(database,write)]);
 await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);if(write)await database.batch(purchase.slice(1));
}
const changed=()=>new ApiError(409,'ASSETS_FINANCIAL_SOURCE_CHANGED','The original Asset, physical retirement, posting references or financial history changed before completion.');
const receiptImage=(database:SqlDatabase,actor:WorkContext,key:string,operation:AssetsFinancialOperation)=>database.prepare("SELECT to_jsonb(r)::text image FROM command_receipts r WHERE r.command_id=? AND r.operation=? AND r.actor_id=? AND r.authority_json::jsonb->>'companyId'=?").bind(key,operation,actor.userId,actor.companyId).first<string>('image');
/** Private composition used only by named Finance asset application calls.
 * Every original identity, version, same-person decision and receipt stays in
 * the original application; no database or operation selector is public. */
export async function withAssetsFinancialCommand(actor:WorkContext,command:{operation:AssetsFinancialOperation;key:string;assetId?:string;disposalId?:string},run:(database:SqlDatabase)=>Promise<AssetsFinancialResult>):Promise<AssetsFinancialResult>{
 const permission=command.operation==='finance.asset.activate'||command.operation==='finance.asset.disposal.post'?'finance.approve':'finance.write';
 const capability=command.operation.startsWith('finance.asset.disposal.')?'finance.asset-disposal':'finance.asset-recognition';
 try{return await (await assetsFinancialOwner(actor)).transaction(async database=>{
  await assetsFinancialLeases(database,actor.companyId,true);const current=await requireAssetsFinancialAuthority(database,actor,permission);
  let assetId=command.assetId;
  if(command.disposalId){const source=await database.prepare('SELECT asset_id FROM finance_asset_disposals WHERE id=? AND company_id=?').bind(command.disposalId,actor.companyId).first<{asset_id:string}>();if(!source)throw new ApiError(404,'ASSET_DISPOSAL_NOT_FOUND','Disposal not found.');assetId=source.asset_id;}
  if(!assetId)throw new ApiError(503,'ASSETS_FINANCIAL_COMPOSITION_REQUIRED','Use the original Asset financial command.');
  const prior=await receiptImage(database,actor,command.key,command.operation),purpose=prior?'history' as const:command.operation==='finance.asset.create'||command.operation==='finance.asset.edit'?'new-work' as const:'settle-existing' as const,capabilities=['assets.register',capability],guard=`assets-financial:${randomUUID()}`;
  for(const selected of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability:selected,purpose});
  const boundaries=capabilities.map((selected,index)=>({key:`${guard}:${index}`,boundary:purchaseCommandBoundary(database,'enforced',`${guard}:${index}`,{companyId:actor.companyId,capability:selected,purpose})!}));await database.batch(boundaries.flatMap(item=>item.boundary.before));
  const before=await assetsFinancialOriginalSource(database,actor,assetId),references=await assetsFinancialReferences(database,actor),result=await run(database),expectedOwner=await assetsFinancialOwnerImage(database,actor,assetId),expectedReceipt=await receiptImage(database,actor,command.key,command.operation);
  if(!expectedOwner||!expectedReceipt)throw changed();
  // All original deferred book projections and owner effects finish before
  // final source/current-authority/purchase reads. No subsequent writes.
  await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(item.key)]));await flushFinanceDeferredEffects(database);
  if(expectedOwner!==await assetsFinancialOwnerImage(database,actor,assetId)||expectedReceipt!==await receiptImage(database,actor,command.key,command.operation)||references!==await assetsFinancialReferences(database,actor)||(await assetsFinancialPhysicalSource(database,actor.companyId,assetId)).image!==before.physical.image)throw changed();
  const oldHistory=await assetsFinancialRetainedJournalImage(database,actor,before.ids);if(oldHistory.invalid||oldHistory.image!==before.journals.image)throw changed();
  await assetsFinancialOriginalSource(database,actor,assetId);
  const final=await requireAssetsFinancialAuthority(database,actor,permission);if(authorityImage(current)!==authorityImage(final))throw new ApiError(403,'QUEUED_AUTHORITY_CHANGED','The original Company Finance authority changed.');
  for(const selected of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability:selected,purpose});return result;
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
