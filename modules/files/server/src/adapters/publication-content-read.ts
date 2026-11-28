import 'server-only';
import {sha256} from '@erpx/platform-runtime-server/public/crypto';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {isTransactionalSqlDatabase,type SqlDatabase,type StoredFile} from '@erpx/platform-runtime-server/public/runtime/types';
import {flushFinanceDeferredEffects} from '@erpx/finance-server/public/finance-transactions';
import {bufferedAttachmentRead} from '@erpx/files-server/adapters/private-attachment-reads';
import {imageSnapshotSql} from './read-tail-r26-source-sql';

/** Publication approval is the original public authority. Private source policy
 * is not substituted for it. Keep that exact published revision stable until
 * the sanitized object's lazy bytes have been consumed and verified. */
export async function withPublicationContentRead<Result extends {object:StoredFile;row:{derivative_checksum_sha256:string}}>(owner:SqlDatabase,id:string,read:(database:SqlDatabase)=>Promise<Result>):Promise<Result>{
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'PUBLICATION_READ_TRANSACTION_REQUIRED','Published media requires its fresh read transaction.');
 return owner.transaction(async database=>{
  // Query-free and compatible with the original publication writer. No Company
  // purchase, membership, consent or publication state is created by this read.
  await database.prepare('LOCK TABLE attachment_publications IN SHARE ROW EXCLUSIVE MODE').run();
  const image=()=>database.prepare(imageSnapshotSql('publication','attachment_publications')).bind(id).first<string>('image');
  const before=await image(),result=await read(database),bytes=await result.object.arrayBuffer();
  if(await sha256(bytes)!==result.row.derivative_checksum_sha256)throw new ApiError(503,'FILE_UNAVAILABLE','Published media is temporarily unavailable.');
  await flushFinanceDeferredEffects(database);
  if(await image()!==before)throw new ApiError(409,'PUBLICATION_READ_CHANGED','The published media changed while it was being read.');
  return bufferedAttachmentRead(result,bytes);
 });
}
