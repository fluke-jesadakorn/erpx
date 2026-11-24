import {legalPackageEnforcementMode} from '@erpx/server-commercial/public/purchase-command-boundary';
import {withLegalPurchasedCommand,requireLegalItemPurchasedRead,inLegalCanonicalDocumentReadUnit} from '@erpx/server-legal/adapters/purchases';
import "server-only";
import {deferLegalCanonicalDocumentSourceRead} from "@erpx/server-legal/adapters/canonical-attachment-read";
import {createHash} from "node:crypto";
import type {SqlDatabase,SqlStatement,FileStore} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {ApiError} from "@erpx/server-runtime/public/http";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {legalLaterApprovalGuardSql} from "../legal-read-tail-r15-sources";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {attachmentsV2ReadOnly} from "@erpx/server-files/public/attachments/feature";
import {attachmentContent,completeAttachmentUpload,createAttachmentSet,prepareAttachmentBinding,reserveAttachmentFile,scanAttachmentNow} from "@erpx/server-files/public/attachments/service";
import {legalDocumentSourceFacts} from "@erpx/server-files/public/legal-document-source-facts";
import {access,guards} from "@erpx/server-legal/adapters/legal/access";
import type {ExecutedCopyInput,DocumentVersion} from "@erpx/server-legal/domain/model";
import {createLegalDocumentApplication,readLegalDocumentBytes,type DocumentStorage} from "@erpx/server-legal/application/documents";
import type {LegalWrite} from "@erpx/server-legal/application/service";
import {legalDocumentProvider} from "@erpx/server-legal/adapters/legal/document-provider";
import { legalDocuments } from '@erpx/server-collaboration/adapters/native-legal-documents';
import {legalDocumentByIdSql,legalFilesAdvisoryLockSql} from '../legal-read-tail-r9-sources';
import {legalDisposedDocumentPrefix} from '../legal-read-tail-r20-sources';
type InspectionFacts={inspectionId:string;id:string;kind:"ocr"|"verify";resultJson:string;actorId:string;now:number;item_id:string};
/** Shared by canonical/stored document version reads and the operations read. */
export const nextDocumentVersionSql = "SELECT coalesce(max(version),0)+1 n FROM legal_documents WHERE item_id=?";
function application(db:SqlDatabase,a:WorkContext,storage?:FileStore):ReturnType<typeof createLegalDocumentApplication>{const nativeDocs=legalDocuments(db,a.companyId,a.userId);const inspections=new WeakMap<LegalWrite,InspectionFacts>();const fileFacts=legalDocumentSourceFacts(db),nativeWrites=new WeakMap<LegalWrite,SqlStatement>();const stage=(statement:SqlStatement):LegalWrite=>{const write:LegalWrite={owner:"legal"};nativeWrites.set(write,statement);return write;};const resolve=(writes:LegalWrite[])=>writes.map(write=>{const statement=nativeWrites.get(write);if(!statement)throw new Error("LEGAL_DOCUMENT_REPOSITORY_MISMATCH");return statement;});const files=()=>{if(!storage)throw new Error("LEGAL_DOCUMENT_FILES_REQUIRED");return storage;};
return createLegalDocumentApplication({actor:a,access:(id,p)=>access(db,a,id,p),storage:{get:key=>files().get(key),head:key=>files().head(key),put:(key,bytes,options)=>files().put(key,bytes,options)} satisfies DocumentStorage,clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},hash:bytes=>createHash("sha256").update(bytes).digest("hex"),utf8:value=>Buffer.from(value),ascii:bytes=>Buffer.from(bytes).toString(),hex:bytes=>Buffer.from(bytes).toString("hex"),error:(status,code,message)=>new ApiError(status,code,message),provider:legalDocumentProvider(db,a.companyId,(status,code,message)=>new ApiError(status,code,message)),fileIntake:{readOnly:()=>attachmentsV2ReadOnly(),createSet:(input,key,options)=>createAttachmentSet(db,a,input,key,options),reserve:(set,input,key,options)=>reserveAttachmentFile(db,a,set,input,key,options),complete:(intent,bytes,options)=>completeAttachmentUpload(db,files(),a,intent,bytes,options),scan:id=>scanAttachmentNow(db,files(),id),binding:async(input,options)=>{const result=await prepareAttachmentBinding(db,a,input,options);return {...result,guards:result.guards.map(stage),steps:result.steps.map(stage)};},content:(link,variant,options)=>attachmentContent(db,files(),a,link,variant,options)},repository:{
approvedVersion:(facts:{id:string;})=>db.prepare("SELECT cycle,document_id,document_hash FROM legal_approvals WHERE item_id=? AND decision='approved' ORDER BY cycle DESC LIMIT 1").bind(facts.id).first<{cycle:number;document_id:string;document_hash:string}>(),
async retainExecutedCopy(facts){return stage(await nativeDocs.retainExecutedCopy(facts));},
async lockExecutedDocument(facts){return stage(await nativeDocs.lockExecutedDocument(facts));},
cleanSource:facts=>fileFacts.cleanSource(facts),
nextVersion:(facts:{id:string;})=>db.prepare(nextDocumentVersionSql).bind(facts.id).first<{ n: number }>(),
async insertCanonicalDocument(facts){return stage(await nativeDocs.insertCanonicalDocument(facts));},
async advanceCanonicalItem(facts){return stage(await nativeDocs.advanceCanonicalItem(facts));},
nextStoredVersion:(facts:{id:string;})=>db.prepare(nextDocumentVersionSql).bind(facts.id).first<{ n: number }>(),
async insertStoredDocument(facts){return stage(await nativeDocs.insertStoredDocument(facts));},
async advanceStoredItem(facts){return stage(await nativeDocs.advanceStoredItem(facts));},
document:(facts:{id:string;})=>db.prepare(legalDocumentByIdSql).bind(facts.id).first<DocumentVersion>(),
disposed:(facts:{id:string;})=>db.prepare(`${legalDisposedDocumentPrefix}=?`).bind(facts.id).first(),
destroying:(facts:{item_id:string;})=>db.prepare("SELECT 1 FROM legal_destructions WHERE item_id=? AND state='executing'").bind(facts.item_id).first(),
async recordExport(facts){await (await nativeDocs.accessEvent({identifier:facts.identifier,itemId:facts.item_id,id:facts.id,actorId:facts.actorId,action:facts.fact4,now:facts.now})).run();},
preservationLock:()=>stage(db.prepare(legalFilesAdvisoryLockSql)),
async insertInspection(facts){const write=stage(await nativeDocs.inspection({inspectionId:facts.inspectionId,id:facts.id,kind:facts.kind,resultJson:facts.resultJson,actorId:facts.actorId,now:facts.now,itemId:facts.item_id}));inspections.set(write,facts);return write;},
async recordInspectionRead(facts){await (await nativeDocs.accessEvent({identifier:facts.identifier,itemId:facts.item_id,id:facts.id,actorId:facts.actorId,action:facts.fact4,now:facts.now})).run();},
submission:(facts:{id:string;})=>db.prepare("SELECT u.*,s.item_id FROM legal_external_uploads u JOIN legal_shares s ON s.id=u.share_id WHERE u.id=?").bind(facts.id).first<{
      id: string;
      item_id: string;
      object_key: string;
      checksum: string;
      name: string;
      media_type: string;
      bytes: number;
      attachment_link_id: string | null;
      attachment_source_id: string | null;
    }>(),
async recordSubmissionRead(facts){await (await nativeDocs.accessEvent({identifier:facts.identifier,itemId:facts.item_id,id:facts.null,actorId:facts.actorId,action:facts.fact4,now:facts.now})).run();},
requireExecutedApproval:(key:string,values:(string | number)[])=>stage(assertion(db,key,"legal-executed-approval","EXISTS(SELECT 1 FROM legal_approvals WHERE item_id=? AND cycle=? AND document_id=? AND document_hash=? AND decision='approved') AND "+legalLaterApprovalGuardSql,values)),
requireExecutedAuthority:(key:string,documentId:string,values:string[])=>stage(assertion(db,key,`legal-executed-authority-${documentId}`,`EXISTS(SELECT 1 FROM legal_documents WHERE id=? AND item_id=?) AND NOT EXISTS(${legalDisposedDocumentPrefix}=?)`,values)),
persistInspection:async (writes):Promise<number>=>{if(await legalPackageEnforcementMode()==="disabled")return(await db.batch(resolve(writes)))[1].meta.changes;const facts=writes.map(write=>inspections.get(write)).find((value):value is InspectionFacts=>!!value);if(!facts||writes.length!==2)throw new ApiError(503,"LEGAL_INSPECTION_SOURCE_REQUIRED","Inspection requires its original owner source.");return withLegalPurchasedCommand(db,a,{key:`legal-inspection:${facts.inspectionId}`,id:facts.item_id,operation:"legal.document.inspect",purpose:"settle-existing",permission:"legal.read",additional:["legal.documents"]},async owner=>{const source=await documentAccess(owner,a,facts.id),result=JSON.parse(facts.resultJson) as {checksum?:string};if(source.item_id!==facts.item_id||result.checksum!==source.checksum)throw new ApiError(409,"LEGAL_SOURCE_CHANGED","The inspected Legal original changed.");const outcome=await owner.batch([owner.prepare(legalFilesAdvisoryLockSql),await legalDocuments(owner,a.companyId,a.userId).inspection({inspectionId:facts.inspectionId,id:facts.id,kind:facts.kind,resultJson:facts.resultJson,actorId:facts.actorId,now:facts.now,itemId:facts.item_id})]);return outcome[1].meta.changes;});},
guardChanges:(id,version,key,edit,allowFrozen,preparation)=>guards(db,a,id,version,key,edit,allowFrozen,preparation).map(stage),
event:async(id,action,note,version,identifier,now)=>stage(await nativeDocs.event({id,action,note,version,identifier,now})),replay:(key,operation,id,input)=>replayCommand(db,a,key,operation,id,input),commit:async(key,operation,input,id,version,result,writes)=>executeCommand(db,a,key,operation,input,id,version,result,writes.map(write=>{const statement=nativeWrites.get(write);if(!statement)throw new Error("LEGAL_DOC_REPOSITORY_MISMATCH");return statement;})),
}});}
export function uploadDocument(db:SqlDatabase,files:FileStore,a:WorkContext,id:string,version:number,name:string,type:string,bytes:Uint8Array,key:string,executed?:ExecutedCopyInput){return withLegalPurchasedCommand(db,a,{key,operation:executed?'legal.executed.import':'legal.document.upload',id,purpose:executed?'settle-existing':'new-work',permission:'legal.edit',additional:['legal.documents']},tx=>application(tx,a,files).uploadDocument(id,version,name,type,bytes,key,executed));}
export function importExecutedCopy(db:SqlDatabase,files:FileStore,a:WorkContext,id:string,version:number,metadata:unknown,name:string,type:string,bytes:Uint8Array,key:string){return withLegalPurchasedCommand(db,a,{key,operation:'legal.executed.import',id,purpose:'settle-existing',permission:'legal.edit',additional:['legal.documents']},tx=>application(tx,a,files).importExecutedCopy(id,version,metadata,name,type,bytes,key));}
export async function documentAccess(db:SqlDatabase,a:WorkContext,id:string,permission="legal.read"){const document=await application(db,a).documentAccess(id,permission);const item=await access(db,a,document.item_id,permission);await requireLegalItemPurchasedRead(db,a,item,['legal.documents']);return document;}
export function documentBytes(files:FileStore,d:DocumentVersion){return readLegalDocumentBytes({storage:files,utf8:value=>Buffer.from(value),hash:bytes=>createHash("sha256").update(bytes).digest("hex"),error:(status,code,message)=>new ApiError(status,code,message)},d);}
export async function documentExport(db:SqlDatabase,files:FileStore,a:WorkContext,id:string,format:string|null){return inLegalCanonicalDocumentReadUnit(db,a,'legal.export',async owner=>{const source=await documentAccess(owner,a,id);await deferLegalCanonicalDocumentSourceRead(owner,a,source.item_id);const result=await application(owner,a,files).documentExport(id,format);await documentAccess(owner,a,id,'legal.export');return new Response(new Uint8Array(result.bytes),{headers:{"Content-Type":result.type,"Content-Disposition":`attachment; filename="document"; filename*=UTF-8''${encodeURIComponent(result.name)}`,"Cache-Control":"private, no-store","X-Content-Type-Options":"nosniff"}});});}
export function inspectDocument(db:SqlDatabase,files:FileStore,a:WorkContext,id:string,kind:"ocr"|"verify"){return inLegalCanonicalDocumentReadUnit(db,a,'legal.read',async owner=>{const source=await documentAccess(owner,a,id);await deferLegalCanonicalDocumentSourceRead(owner,a,source.item_id);return application(owner,a,files).inspectDocument(id,kind);});}
export async function externalSubmission(db:SqlDatabase,files:FileStore,a:WorkContext,id:string){return inLegalCanonicalDocumentReadUnit(db,a,'legal.export',async owner=>{const content=await application(owner,a,files).externalSubmission(id);await deferLegalCanonicalDocumentSourceRead(owner,a,content.row.item_id);await requireLegalItemPurchasedRead(owner,a,await access(owner,a,content.row.item_id,"legal.export"),["legal.documents","legal.access"]);return content;});}
