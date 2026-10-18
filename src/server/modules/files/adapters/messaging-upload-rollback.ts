import 'server-only';
import type {FileMetadata,FileStore,SqlDatabase} from '../../runtime/public/runtime/types';
import {isTransactionalSqlDatabase} from '../../runtime/public/runtime/types';
import {runtimePreservationAuthority} from '../../legal/public/runtime-preservation-authority';

/** The caller supplies only normal reserve results. Keys eligible for rollback
 * are observed successful create-only puts, never an HTTP supplied object key.
 * Ambiguous writes and surviving references remain for normal preservation. */
export async function withMessagingUploadRollback<Result>(owner:SqlDatabase,store:FileStore,scope:{companyId:string;holdingId:string},run:(ports:{files:FileStore;rememberIntent:(id:string)=>void})=>Promise<Result>):Promise<Result>{
 const intents=new Set<string>(),created=new Map<string,{intent:string;metadata:FileMetadata}>();
 const files:FileStore={supportsTransactionDeletion:store.supportsTransactionDeletion,
  put:async(key,value,options)=>{const result=await store.put(key,value,options);if(result&&options?.onlyIf?.etagDoesNotMatch==='*'&&result.key===key){for(const intent of intents){const prefix=`${scope.holdingId}/quarantine/${intent}/`,suffix=key.startsWith(prefix)?key.slice(prefix.length):'';if(suffix==='original'||/^preview-[a-f0-9]{64}\.(jpg|webp)$/.test(suffix))created.set(key,{intent,metadata:result});}}return result;},
  get:(...args)=>store.get(...args),head:key=>store.head(key),delete:(...args)=>store.delete(...args),list:options=>store.list(options),...(store.destroyPreserved?{destroyPreserved:(...args:Parameters<NonNullable<FileStore['destroyPreserved']>>)=>store.destroyPreserved!(...args)}:{})};
 try{return await run({files,rememberIntent:id=>intents.add(id)});}catch(error){
  if(isTransactionalSqlDatabase(owner)&&created.size)await owner.transaction(async database=>{
   // All native metadata leases precede the first advisory/data SELECT. A
   // competing normal reservation/source must finish before absence is read.
   await database.prepare('LOCK TABLE attachments,upload_intents,attachment_links,attachment_sets,attachment_scan_attempts,attachment_storage_ledger,attachment_publications,attachment_orphan_preservations IN SHARE ROW EXCLUSIVE MODE').run();
   for(const intent of [...new Set([...created.values()].map(item=>item.intent))].sort())await database.prepare('SELECT pg_advisory_xact_lock(hashtextextended(?,0))').bind(`attachment-upload:${scope.companyId}:${intent}`).run();
   const preservation=runtimePreservationAuthority(database);
   for(const [key,item] of created){
    const referenced=await database.prepare(`SELECT 1 WHERE EXISTS(SELECT 1 FROM attachments WHERE object_key=? OR preview_object_key=?) OR EXISTS(SELECT 1 FROM upload_intents WHERE quarantine_key=? OR staging_key=?) OR EXISTS(SELECT 1 FROM attachment_storage_ledger WHERE object_key=?) OR EXISTS(SELECT 1 FROM attachment_publications WHERE derivative_object_key=?) OR EXISTS(SELECT 1 FROM attachment_orphan_preservations WHERE object_key=?)`).bind(key,key,key,key,key,key,key).first();
    if(referenced)continue;
    const current=await store.head(key);if(!current||current.key!==key||current.etag!==item.metadata.etag||current.size!==item.metadata.size)continue;
    await preservation.assertMutation([key]);
    await store.delete(key,store.supportsTransactionDeletion?{database}:undefined);
   }
  }).catch(()=>undefined);
  throw error;
 }
}
