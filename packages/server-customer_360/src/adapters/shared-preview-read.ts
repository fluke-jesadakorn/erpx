import 'server-only';
import type {CustomerPortalSessionActor} from '@erpx/server-customer_360/domain/portal';
import {isTransactionalSqlDatabase,type SqlDatabase,type StoredFile} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {lockPackageSettlementSources} from '@erpx/server-commercial/public/package-locks';
import {customerPortalSourceReadLocks} from '@erpx/server-customer_authority/public/portal-source-locks';
import {supportOrderAttachmentSourceLocks,supportOrderAttachmentSourceImage} from '@erpx/server-commerce/public/support-attachment-sources';
import {portalPreviewSupportSourceReadLocks,portalPreviewSupportSourceImage} from '@erpx/server-support/public/portal-preview-sources';
import {attachmentPrivateReadSourceLocks,bufferedAttachmentRead} from '@erpx/server-files/public/private-attachment-reads';
import {customerPortalPreviewRow,customerPortalSharedPreviewSource,customerPortalPreviewReadAudits} from '@erpx/server-files/public/customer-portal-files';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';

function changed(){return new ApiError(409,'CUSTOMER_SHARED_FILE_READ_CHANGED','The verified customer access, original document or shared preview changed during its read.');}
/** This is the original explicit customer-shareable Support path, not a staff
 * Files read. No staff assignment or Helpdesk/Finance purchase is fabricated.
 * All lazy bytes and the actual access event stay inside the fresh owner unit. */
export async function withCustomerPortalSharedPreviewRead<Result extends {object:StoredFile}>(owner:SqlDatabase,actor:Pick<CustomerPortalSessionActor,'userId'|'email'>,linkId:string,read:(database:SqlDatabase)=>Promise<Result>,assertRequestAuthority?:()=>Promise<void>):Promise<Result>{
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'CUSTOMER_SHARED_FILE_READ_TRANSACTION_REQUIRED','Shared customer previews require their fresh owned read transaction.');
 const context=JSON.stringify(actor);
 return owner.transaction(async database=>{
  await assertRequestAuthority?.();
  await lockPackageSettlementSources(database);
  await database.batch([supportOrderAttachmentSourceLocks(database),...customerPortalSourceReadLocks(database),portalPreviewSupportSourceReadLocks(database),attachmentPrivateReadSourceLocks(database)]);
  await lockScheduledFeatureSources(database);
  const before=await customerPortalSharedPreviewSource(database,actor,linkId),order=before?await supportOrderAttachmentSourceImage(database,before.companyId,before.orderId):null,ticket=before?await portalPreviewSupportSourceImage(database,before.companyId,before.ticketId):null,
   checkpoint=new Set((await customerPortalPreviewReadAudits(database,actor,linkId)).map(event=>event.id)),result=await read(database),events=(await customerPortalPreviewReadAudits(database,actor,linkId)).filter(event=>!checkpoint.has(event.id));
  if(!before||!order||!ticket||events.length!==1)throw changed();
  const bytes=await result.object.arrayBuffer();
  await flushFinanceDeferredEffects(database);
  const current=await customerPortalPreviewRow(database,actor,linkId,Date.now()),after=await customerPortalSharedPreviewSource(database,actor,linkId),event=(await customerPortalPreviewReadAudits(database,actor,linkId)).find(item=>item.id===events[0].id);
  if(JSON.stringify(actor)!==context||!current||current.company_id!==before.companyId||after?.witness!==before.witness||await supportOrderAttachmentSourceImage(database,before.companyId,before.orderId)!==order||await portalPreviewSupportSourceImage(database,before.companyId,before.ticketId)!==ticket||event?.witness!==events[0].witness)throw changed();
  await assertRequestAuthority?.();
  return bufferedAttachmentRead(result,bytes);
 });
}
