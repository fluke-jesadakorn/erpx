import 'server-only';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {holdingPurchaseReadLocks} from '@erpx/server-commercial/public/purchase-command-boundary';
import {attachmentPrivateReadSourceLocks} from '@erpx/server-files/public/private-attachment-reads';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import type {PortalActor} from '@erpx/server-legal/domain/portal';
import {legalPortalSnapshotSourceImage} from '@erpx/server-legal/adapters/portal-purchases';


/** The complete list and optional selected detail share one owned source
 * snapshot and audit commit. No private source image leaves this continuation. */
export async function withLegalPortalSnapshotRead<Result>(owner:SqlDatabase,actor:PortalActor,read:(database:SqlDatabase)=>Promise<Result>,assertCurrent?:()=>Promise<void>){
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'LEGAL_PORTAL_READ_TRANSACTION_REQUIRED','Shared Legal collections require their fresh owned read.');
 const context=JSON.stringify(actor);
 return owner.transaction(async database=>{
  const purchases=holdingPurchaseReadLocks(database);
  await database.batch([purchases[0],database.prepare('LOCK TABLE legal_items,legal_members,legal_documents,legal_shares,legal_settings,legal_destructions,legal_comments,legal_envelopes,legal_signers,legal_counsel_entries IN SHARE ROW EXCLUSIVE MODE'),attachmentPrivateReadSourceLocks(database)]);
  await lockScheduledFeatureSources(database);await database.batch(purchases.slice(1));
  const before=await legalPortalSnapshotSourceImage(database,actor),result=await read(database);
  await flushFinanceDeferredEffects(database);
  if(JSON.stringify(actor)!==context||await legalPortalSnapshotSourceImage(database,actor)!==before)throw new ApiError(409,'LEGAL_SHARE_CHANGED','The shared Legal collection changed.');
  if(assertCurrent)await assertCurrent();
  return result;
 });
}
