import {beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {PDFDocument} from 'pdf-lib';
import {env,applyMigrations} from './runtime-env';
import {seedExpenseCompany} from '../fixtures/expenses/company';
import {legalCommand,detail} from '../src/server/legal/service';
import {uploadDocument,documentAccess} from '../src/server/legal/documents';
import {retentionCommand,retentionView,drainLegalDestruction} from '../src/server/legal/retention';
import {legalDestructionManifest} from '../src/server/legal/destruction-sources';
import {LocalFileStore} from '../src/server/runtime/filesystem';
import {createAttachmentSet,linkExistingAttachment,withdrawAttachmentLink} from '../src/server/attachments/service';
vi.mock('../src/server/runtime',()=>({runtimeEnv:async()=>env}));
vi.mock('../src/server/modules/files/public/attachments/scanner',()=>({attachmentScannerReadiness:async()=>({enabled:true,ready:true,engine:'retention scanner fixture',definitionUpdatedAt:Date.now(),error:null}),scanAttachmentBytes:async()=>({clean:true,signature:null,engine:'retention scanner fixture',definitionUpdatedAt:Date.now()})}));
vi.mock('../src/server/modules/files/public/attachments/preview',()=>({attachmentPreviewReadiness:async()=>({ready:true,error:null}),detectedAttachmentMediaType:(_bytes:Uint8Array,type:string)=>type,sanitizedAttachmentPreview:async()=>null}));
const db=env.H001_DB,key=()=>crypto.randomUUID();let f:Awaited<ReturnType<typeof seedExpenseCompany>>,files:LocalFileStore;
beforeAll(()=>applyMigrations(db,env.STAFF_TEST_MIGRATIONS));
beforeEach(async()=>{f=await seedExpenseCompany(db,env.H001_FILES);f.manager=await f.member('retention-author','legal_manager');f.tie=await f.member('retention-reviewer','legal_manager');files=new LocalFileStore(env.H001_FILES.root,db);await legalCommand(db,f.manager,{action:'settings',enabled:true},key());vi.stubEnv('ERPX_ATTACHMENTS_V2','true');});
const act=async(id:string,action:string,extra:Record<string,unknown>={},actor=f.manager)=>legalCommand(db,actor,{id,action,expectedVersion:(await detail(db,actor,id)).item.version,...extra},key());
const retain=async(id:string,action:string,extra:Record<string,unknown>={},actor=f.manager)=>retentionCommand(db,actor,{id,action,expectedVersion:(await detail(db,actor,id)).item.version,reason:'Synthetic isolated retention decision',...extra},key());
async function original(){
 const item=await legalCommand(db,f.manager,{action:'create',item:{kind:'request',title:'Disposable original fixture',ownerId:f.manager.userId,data:{requestType:'Synthetic test',question:'Dispose only this independently reviewed fixture'}}},key());await act(item.id,'member',{userId:f.tie.userId,access:'edit'});
 const pdf=await PDFDocument.create();pdf.addPage();await uploadDocument(db,files,f.manager,item.id,(await detail(db,f.manager,item.id)).item.version,'Retained original.pdf','application/pdf',await pdf.save(),key());
 const document=(await detail(db,f.manager,item.id)).documents[0],source=await db.prepare('SELECT object_key,source_state,bytes FROM attachments WHERE id=?').bind(document.attachment_source_id).first<{object_key:string;source_state:string;bytes:number}>();expect(source).toBeTruthy();expect(source!.object_key.startsWith('legal/')).toBe(false);
 return {id:item.id,document,source:source!};
}
async function eligible(id:string){
 for(const state of ['review','approval'])await act(id,'transition',{state,reason:'Synthetic closure review'});await act(id,'transition',{state:'approved',reason:'Independent synthetic closure approval'},f.tie);for(const state of ['active','completed'])await act(id,'transition',{state,reason:'Synthetic completion'});
 // Fixture-only historical close time; no operational retention rule is shortened.
 await db.prepare('UPDATE legal_items SET updated_at=? WHERE id=?').bind(Date.now()-3*86_400_000,id).run();
 const policyId=key();await retentionCommand(db,f.manager,{action:'retention.policy',name:'One day synthetic training data',days:1,reason:'Synthetic Company policy only'},policyId);await retentionCommand(db,f.tie,{action:'retention.approve-policy',policyId,policyVersion:1,reason:'Independent synthetic policy review'},key());await retain(id,'retention.assign',{policyId});
}
it('protects canonical original bytes under native holds and resumes an interrupted approved manifest without duplicate disposal or quota',async()=>{
 const r=await original();await eligible(r.id);await act(r.id,'hold.create',{reason:'Preserve native File Sources original'});
 await expect(files.delete(r.source.object_key)).rejects.toThrow('LEGAL_HOLD_BLOCKS_FILE_MUTATION');await expect(files.put(r.source.object_key,'Changed original')).rejects.toThrow('LEGAL_HOLD_BLOCKS_FILE_MUTATION');await expect(retain(r.id,'retention.request')).rejects.toThrow();
 const hold=(await detail(db,f.manager,r.id)).holds[0];await act(r.id,'hold.request-release',{holdId:hold.id,reason:'Synthetic preservation has ended'});await act(r.id,'hold.release',{holdId:hold.id,reason:'Independent preservation release'},f.tie);await retain(r.id,'retention.request');const request=(await retentionView(db,f.manager,r.id)).requests[0];
 await expect(retain(r.id,'retention.approve',{requestId:request.id,confirmation:request.id})).rejects.toMatchObject({code:'LEGAL_INDEPENDENT_REVIEW'});await retain(r.id,'retention.approve',{requestId:request.id,confirmation:request.id},f.tie);
 const realDestroy=files.destroyPreserved.bind(files);files.destroyPreserved=async(keys,authorization)=>{await realDestroy(keys,authorization);throw new Error('INJECTED_CRASH_AFTER_BYTE_DELETION');};await drainLegalDestruction(db,files);files.destroyPreserved=realDestroy;
 let current=(await retentionView(db,f.manager,r.id)).requests[0];expect(current).toMatchObject({id:request.id,state:'executing',error:'DESTRUCTION_REQUIRES_REVIEW'});expect(current.manifest).toEqual(request.manifest);expect(await files.head(r.source.object_key)).toBeNull();await expect(documentAccess(db,f.manager,r.document.id)).rejects.toMatchObject({code:'LEGAL_RETENTION_FROZEN'});
 await retain(r.id,'retention.retry',{requestId:request.id});await drainLegalDestruction(db,files);current=(await retentionView(db,f.manager,r.id)).requests[0];expect(current).toMatchObject({id:request.id,state:'completed',error:null});expect(current.manifest).toEqual(request.manifest);
 expect(await db.prepare('SELECT source_state FROM attachments WHERE id=?').bind(r.document.attachment_source_id).first('source_state')).toBe('destroyed');expect(await db.prepare("SELECT count(*) n FROM attachment_storage_ledger WHERE attachment_id=? AND state='stored'").bind(r.document.attachment_source_id).first('n')).toBe(0);
 expect(await db.prepare('SELECT checksum FROM legal_disposals WHERE document_id=?').bind(r.document.id).first('checksum')).toBe(r.document.checksum);await expect(documentAccess(db,f.manager,r.document.id)).rejects.toMatchObject({code:'LEGAL_DOCUMENT_DISPOSED'});
 await drainLegalDestruction(db,files);expect(await db.prepare("SELECT count(*) n FROM legal_events WHERE item_id=? AND action='retention.destroyed'").bind(r.id).first('n')).toBe(1);expect(await db.prepare("SELECT count(*) n FROM attachment_events WHERE attachment_id=? AND action='source.bytes_deleted'").bind(r.document.attachment_source_id).first('n')).toBe(1);
});
it('refuses destruction of an original reused by another native owner until the independent manifest can include only unshared sources',async()=>{
 const r=await original(),other=await legalCommand(db,f.manager,{action:'create',item:{kind:'matter',title:'Another live evidence owner',ownerId:f.manager.userId,data:{parties:'Synthetic Company A parties'}}},key());
 const set=await createAttachmentSet(db,f.manager,{targetType:'legal_item',targetId:other.id,componentKind:'record',purposeKey:'legal_document',expectedTargetVersion:1},key());const linked=await linkExistingAttachment(db,f.manager,set.id,{sourceLinkId:r.document.attachment_link_id!},key());
 await eligible(r.id);await expect(retain(r.id,'retention.request')).rejects.toThrow('LEGAL_DESTRUCTION_SOURCE_SHARED');expect(await files.head(r.source.object_key)).toBeTruthy();
 await withdrawAttachmentLink(db,f.manager,linked.attachmentLinkId,'Synthetic evidence reuse withdrawn before retention',key());await retain(r.id,'retention.request');expect((await retentionView(db,f.manager,r.id)).requests).toHaveLength(1);
});
it('retains immutable approved manifests and refuses keys or original checksums outside the selected Legal owner',async()=>{
 const r=await original();await eligible(r.id);await retain(r.id,'retention.request');const request=(await retentionView(db,f.manager,r.id)).requests[0];await expect(db.prepare("UPDATE legal_destructions SET manifest='{}'::jsonb WHERE id=?").bind(request.id).run()).rejects.toThrow('LEGAL_DESTRUCTION_EVIDENCE_IMMUTABLE');
 const manifest=await legalDestructionManifest(db,r.id,f.company);expect(manifest.canonicalSources).toHaveLength(1);expect(manifest.documents[0].checksum).toBe(r.document.checksum);await retain(r.id,'retention.approve',{requestId:request.id,confirmation:request.id},f.tie);
 await expect(files.destroyPreserved(['foreign-owner-key'],{database:db,destructionId:request.id})).rejects.toThrow('LEGAL_DESTRUCTION_NOT_AUTHORIZED');expect(await files.head(r.source.object_key)).toBeTruthy();
});
