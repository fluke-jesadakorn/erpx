import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {lockQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {privateFileDenialAuthoritySourceLocks} from '@erpx/server-identity_access/public/private-file-denial-authority';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {lockPackageSettlementSources} from '@erpx/server-commercial/public/package-locks';
import {companyPurchaseReadLocks} from '@erpx/server-commercial/public/purchase-command-boundary';
import {posImageProductSourceLocks,posImageProductSourceImage} from '@erpx/server-commerce/public/pos-images-facts';
import {attachmentPrivateReadSourceLocks,attachmentReadSource,requirePrivateAttachmentReaderCurrent} from '@erpx/server-files/public/private-attachment-reads';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {requirePosPurchased} from '@erpx/server-pos/adapters/purchase-boundary';
import {requirePosImageSource} from '@erpx/server-pos/adapters/pos-image-source';

const identity=(actor:WorkContext)=>JSON.stringify(actor);
function changed(){return new ApiError(409,'POS_IMAGE_READ_CHANGED','The product image, original POS scope or current account changed while its private bytes were read.');}
async function image(database:SqlDatabase,companyId:string,imageId:string){
 return database.prepare(`SELECT jsonb_build_object('image',to_jsonb(image),'catalog',to_jsonb(catalog),
 'registers',(SELECT jsonb_agg(jsonb_build_object('selection',to_jsonb(selection),'register',to_jsonb(register_row)) ORDER BY selection.register_id)
 FROM pos_register_catalog selection JOIN pos_registers register_row ON register_row.id=selection.register_id
 WHERE selection.product_id=image.product_id AND register_row.company_id=image.company_id))::text image
 FROM pos_product_images image LEFT JOIN pos_catalog catalog ON catalog.product_id=image.product_id AND catalog.company_id=image.company_id
 WHERE image.id=? AND image.company_id=?`).bind(imageId,companyId).first<string>('image');
}
/** The specialized POS endpoint consumes and checksums its complete bytes in
 * the original application. Keep its source and successful audit inside one
 * fresh owned unit, including when Commercial enforcement is disabled. */
export async function withPosProductImageRead<Result>(owner:SqlDatabase,actor:WorkContext,imageId:string,read:(database:SqlDatabase)=>Promise<Result>):Promise<Result>{
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'POS_IMAGE_READ_TRANSACTION_REQUIRED','Private product images require their fresh complete source-leased read transaction.');
 const context=identity(actor);
 return owner.transaction(async database=>{
  const purchase=companyPurchaseReadLocks(database,actor.companyId);
  await lockPackageSettlementSources(database);
  await database.batch([database.prepare('LOCK TABLE pos_product_images,pos_catalog,pos_register_catalog,pos_registers IN SHARE ROW EXCLUSIVE MODE'),posImageProductSourceLocks(database),attachmentPrivateReadSourceLocks(database),...privateFileDenialAuthoritySourceLocks(database)]);
  await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(purchase.slice(1));
  await requirePosPurchased(database,actor);
  const source=await requirePosImageSource(database,actor,imageId),before=await image(database,actor.companyId,imageId),product=await posImageProductSourceImage(database,actor.companyId,source.product_id),file=await attachmentReadSource(database,actor.companyId,source.attachment_link_id);
  if(!before||!product||!file)throw changed();
  const result=await read(database);
  await flushFinanceDeferredEffects(database);
  if(identity(actor)!==context)throw changed();
  try{await requirePrivateAttachmentReaderCurrent(database,actor);}catch(error){if(error instanceof ApiError&&error.code==='ATTACHMENT_FILE_READ_CHANGED')throw changed();throw error;}
  await requirePosPurchased(database,actor);await requirePosImageSource(database,actor,imageId);
  if(await image(database,actor.companyId,imageId)!==before||await posImageProductSourceImage(database,actor.companyId,source.product_id)!==product||(await attachmentReadSource(database,actor.companyId,source.attachment_link_id))?.witness!==file.witness)throw changed();
  return result;
 });
}
