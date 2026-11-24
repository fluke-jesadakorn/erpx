import {withLegalPortalOriginalRead} from '@erpx/server-legal/adapters/portal-original-read';
import {withLegalPortalSnapshotRead} from '@erpx/server-legal/adapters/portal-snapshot-read';
import {legalPortalPurchasePredicate,withLegalPortalPurchasedCommand,legalPortalPurchaseControls} from '@erpx/server-legal/adapters/portal-purchases';
import {legalPackageEnforcementMode} from '@erpx/server-commercial/public/purchase-command-boundary';
import "server-only";
import type {SqlDatabase,SqlStatement,FileStore} from "@erpx/server-runtime/public/runtime/types";
import {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import {holdingBindingBySlug,holdingResources} from "../../../../../packages/server-runtime/src/public/runtime";
import {currentSession} from "@erpx/server-identity_access/public/authorization";
import {ApiError} from "@erpx/server-runtime/public/http";
import {legalPortalBindingStatements} from "@erpx/server-collaboration/adapters/native-legal-portal-binding";
import {legalPortalRepository} from '@erpx/server-files/public/legal-portal-repository';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import {legalPortalJournal} from "@erpx/server-workflow/public/legal-portal-journal";
import {lock} from "@erpx/server-legal/adapters/legal/access";
import {hash} from "@erpx/server-legal/adapters/legal/service";
import {portalAttachmentBytes,stagePortalAttachment} from "@erpx/server-legal/adapters/legal/portal-attachments";
import type {DocumentVersion} from "@erpx/server-legal/domain/model";
import {legalPortalActor,type PortalActor} from "@erpx/server-legal/domain/portal";
import {createLegalPortalApplication,type LegalPortalWrite,type LegalPortalUnit,type LegalPortalRepository,type LegalPortalShare,type LegalPortalShareFacts} from "@erpx/server-legal/application/portal";
import { legalPortalWrites } from '@erpx/server-collaboration/adapters/native-legal-portal-writes';
import {legalSettingsEnabledSql,legalDocumentByIdSql} from '../legal-read-tail-r9-sources';
export type {PortalActor} from "@erpx/server-legal/domain/portal";
/** Shared read of a portal idempotency receipt by key and actor; both upload
 * repository methods assert the same authority-bound row. */
const portalReceiptReadSql="SELECT request_hash,result_json FROM legal_portal_receipts WHERE id=? AND user_id=?";
export async function portalContext(slug:string){const {session}=await currentSession();const actor=legalPortalActor(session.user,session.session.id,(status,code,message)=>new ApiError(status,code,message)),binding=await holdingBindingBySlug(slug),resources=await holdingResources(binding.id);return {...resources,actor};}
async function shareRecord(database:SqlDatabase,facts:LegalPortalShareFacts,strictHistory=false){return database.prepare(`SELECT s.*,i.company_id,i.branch_id,i.department_id,i.version AS item_version,i.kind FROM legal_shares s JOIN legal_items i ON i.id=s.item_id JOIN legal_documents d ON d.id=s.document_id AND d.item_id=i.id WHERE ${await legalPortalPurchasePredicate('i',strictHistory)} AND s.id=? AND lower(s.email)=? AND (s.user_id IS NULL OR s.user_id=?) AND s.revoked_at IS NULL AND s.expires_at>?`).bind(facts.id,facts.email,facts.actorId,facts.now).first<LegalPortalShare>();}
function unit(db:SqlDatabase):LegalPortalUnit{const nw=legalPortalWrites(db);const legalNative=legalPortalBindingStatements(db),journal=legalPortalJournal(db),writes=new WeakMap<LegalPortalWrite,SqlStatement>();function stage(statement:SqlStatement):LegalPortalWrite{const write:LegalPortalWrite={owner:"legal"};writes.set(write,statement);return write;}
return {
shares:async(facts:{email:string;actorId:string;now:number;})=>db.prepare(`SELECT s.id,s.document_id,s.expires_at,s.user_id,d.name,d.version,d.checksum FROM legal_shares s JOIN legal_documents d ON d.id=s.document_id AND d.item_id=s.item_id JOIN legal_items i ON i.id=s.item_id WHERE ${await legalPortalPurchasePredicate()} AND lower(s.email)=? AND (s.user_id IS NULL OR s.user_id=?) AND s.revoked_at IS NULL AND s.expires_at>? ORDER BY s.created_at DESC LIMIT 100`).bind(facts.email,facts.actorId,facts.now).all(),
share:facts=>shareRecord(db,facts),
uploadRecoveryShare:facts=>shareRecord(db,facts,true),
uploadRecoverySource:facts=>legalPortalRepository(db).recoverySource(facts),
retentionFrozen:(facts:{item_id:string;})=>db.prepare("SELECT 1 FROM legal_destructions WHERE item_id=? AND state IN ('executing','completed')").bind(facts.item_id).first(),
documentSummary:(facts:{document_id:string;})=>db.prepare("SELECT id,name,body,version,checksum,media_type FROM legal_documents WHERE id=?").bind(facts.document_id).first(),
comments:(facts:{item_id:string;document_id:string;})=>db.prepare("SELECT body,created_at,created_by FROM legal_comments WHERE item_id=? AND document_id=? AND visibility='external' ORDER BY created_at LIMIT 100").bind(facts.item_id,facts.document_id).all(),
envelopes:(facts:{item_id:string;document_id:string;actorId:string;})=>db.prepare("SELECT e.id,e.state,e.consent,e.expires_at,s.state signer_state,s.viewed_at FROM legal_envelopes e JOIN legal_signers s ON s.envelope_id=e.id WHERE e.item_id=? AND e.document_id=? AND s.user_id=? ORDER BY e.created_at DESC LIMIT 10").bind(facts.item_id,facts.document_id,facts.actorId).all(),
viewRead:facts=>(async()=>{await (await nw.accessEvent({...facts,action:facts.fact4})).run();return {results:[]};})(),
billing:(facts:{item_id:string;actorId:string;})=>db.prepare("SELECT id,title,state,data,version FROM legal_counsel_entries WHERE item_id=? AND created_by=? ORDER BY updated_at DESC LIMIT 100").bind(facts.item_id,facts.actorId).all<{id:string;title:string;state:string;data:unknown;version:number}>(),
document:(facts:{document_id:string;})=>db.prepare(legalDocumentByIdSql).bind(facts.document_id).first<DocumentVersion>(),
downloadRead:facts=>(async()=>{await (await nw.accessEvent({companyId:facts.companyId,identifier:facts.identifier,item_id:facts.item_id,document_id:facts.id,actorId:facts.actorId,action:facts.fact4,now:facts.now})).run();return {results:[]};})(),
receipt:(facts:{key:string;})=>db.prepare("SELECT * FROM legal_portal_receipts WHERE id=?").bind(facts.key).first<{ user_id: string; request_hash: string; result_json: string }>(),
enabled:(facts:{company_id:string;})=>db.prepare(legalSettingsEnabledSql).bind(facts.company_id).first(),
acceptShare:facts=>(async()=>{await (await nw.acceptShare(facts)).run();return {results:[]};})(),
externalComment:facts=>(async()=>{await (await nw.externalComment(facts)).run();return {results:[]};})(),
counselEntry:facts=>(async()=>{await (await nw.counselEntry(facts)).run();return {results:[]};})(),
counselHistory:facts=>(async()=>{await (await nw.counselHistory(facts)).run();return {results:[]};})(),
advanceItem:facts=>(async()=>{await (await nw.advanceItem(facts)).run();return {results:[]};})(),
actionableEnvelope:(facts:{envelopeId:string | undefined;item_id:string;document_id:string;actorId:string;now:number;})=>db.prepare("SELECT e.*,s.ordinal,s.viewed_at FROM legal_envelopes e JOIN legal_signers s ON s.envelope_id=e.id WHERE e.id=? AND e.item_id=? AND e.document_id=? AND s.user_id=? AND e.state='sent' AND s.state='pending' AND e.expires_at>?").bind(facts.envelopeId,facts.item_id,facts.document_id,facts.actorId,facts.now).first<{
          id: string;
          ordinal: number;
          viewed_at: number | null;
          consent: string;
          document_hash: string;
        }>(),
viewEnvelope:facts=>(async()=>{await (await nw.viewEnvelope(facts)).run();return {results:[]};})(),
interactionEvent:facts=>(async()=>{await (await nw.interactionEvent(facts)).run();return {results:[]};})(),
interactionReceipt:facts=>(async()=>{await (await nw.receipt(facts)).run();return {results:[]};})(),
uploadReceipt:(facts:{key:string;actorId:string;})=>db.prepare(portalReceiptReadSql).bind(facts.key,facts.actorId).first<{ request_hash: string; result_json: string }>(),
uploadEnabled:(facts:{company_id:string;})=>db.prepare(legalSettingsEnabledSql).bind(facts.company_id).first(),
uploadReplay:(facts:{key:string;actorId:string;})=>db.prepare(portalReceiptReadSql).bind(facts.key,facts.actorId).first<{ request_hash: string; result_json: string }>(),
externalUpload:facts=>(async()=>{await (await nw.externalUpload(facts)).run();return {results:[]};})(),
bindLink:async facts=>stage(await legalNative.bindLink(facts)),
bindSet:async facts=>stage(await legalNative.bindSet(facts)),
advanceUploadItem:async facts=>stage(await nw.advanceUploadItem({companyId:facts.company_id,now:facts.now,item_id:facts.item_id,expectedTargetVersion:facts.expectedTargetVersion})),
bindEvent:async facts=>stage(await legalNative.bindEvent(facts)),
uploadEvent:facts=>(async()=>{await (await nw.uploadEvent(facts)).run();return {results:[]};})(),
uploadCommandReceipt:facts=>(async()=>{await (await nw.receipt(facts)).run();return {results:[]};})(),
item:(facts:{itemId:string;})=>db.prepare("SELECT company_id,branch_id,department_id,version FROM legal_items WHERE id=?").bind(facts.itemId).first<{
      company_id: string;
      branch_id: string | null;
      department_id: string | null;
      version: number;
    }>(),
audit:facts=>journal.audit(facts),
outbox:facts=>journal.outbox(facts),lock:companyId=>lock(db,companyId).run(),batch:steps=>db.batch(steps.map(write=>{const statement=writes.get(write);if(!statement)throw new Error("LEGAL_PORTAL_REPOSITORY_MISMATCH");return statement;})),};}
function application(db:SqlDatabase,a:PortalActor,files?:FileStore,command?:{shareId:string;key:string;assertCurrent?:()=>Promise<void>}){const repository:LegalPortalRepository={...unit(db),transaction:async work=>{if(command&&(command.assertCurrent||await legalPackageEnforcementMode()!=="disabled"))return withLegalPortalPurchasedCommand(db,a,command,tx=>work(unit(tx)),command.assertCurrent);if(!(db instanceof PostgresDatabase))throw new Error("LEGAL_PORTAL_TRANSACTION_REQUIRED");return db.transaction(tx=>work(unit(tx)));}};return createLegalPortalApplication({requestUploadReplay:!!command?.assertCurrent,actor:a,repository,clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},hash,decode:bytes=>Buffer.from(bytes).toString(),error:(status,code,message)=>new ApiError(status,code,message),attachments:{read:input=>{if(!files)throw new Error("LEGAL_PORTAL_FILES_REQUIRED");return portalAttachmentBytes(db,files,input);},stage:(actorId,scope,name,mediaType,bytes,key)=>{if(!files||!(db instanceof PostgresDatabase))throw new Error("LEGAL_PORTAL_UPLOAD_CONTEXT_REQUIRED");return stagePortalAttachment(db,files,actorId,scope,name,mediaType,bytes,key);}}});}
export function portalList(db:SqlDatabase,a:PortalActor){return application(db,a).portalList();}
export async function portalSnapshot(db:SqlDatabase,a:PortalActor,selectedId?:string,assertCurrent?:()=>Promise<void>){return withLegalPortalSnapshotRead(db,a,async database=>{
 const portal=application(database,a),initial=await portal.portalList();
 if(!selectedId)return {initial,initialDetail:undefined};
 const detail=await portal.portalDetail(selectedId),purchase=await legalPortalPurchaseControls(database,a,selectedId);
 return {initial,initialDetail:{...detail,signingEnabled:false as const,...(purchase?{purchase}:{})}};
},assertCurrent);}
export async function portalDetail(db:SqlDatabase,a:PortalActor,id:string,assertCurrent?:()=>Promise<void>){return withLegalPortalOriginalRead(db,a,id,async database=>{const detail=await application(database,a).portalDetail(id),purchase=await legalPortalPurchaseControls(database,a,id);return {...detail,signingEnabled:false as const,...(purchase?{purchase}:{})};},assertCurrent);}
export async function portalDownload(db:SqlDatabase,files:FileStore,a:PortalActor,id:string,assertCurrent?:()=>Promise<void>){return withLegalPortalOriginalRead(db,a,id,async database=>{const content=await application(database,a,files).portalDownload(id);return new Response(new Uint8Array(content.bytes),{headers:{"Content-Type":content.mediaType,"Content-Disposition":`attachment; filename="document"; filename*=UTF-8''${encodeURIComponent(content.filename)}`,"Cache-Control":"private, no-store","X-Content-Type-Options":"nosniff"}});},assertCurrent);}
export function portalCommand(db:PostgresDatabase,a:PortalActor,raw:unknown,key:string,assertCurrent?:()=>Promise<void>){const shareId=raw&&typeof raw==="object"&&"shareId" in raw&&typeof raw.shareId==="string"?raw.shareId:null;return application(db,a,undefined,shareId?{shareId,key,assertCurrent}:undefined).portalCommand(raw,key);}
export function portalUpload(db:PostgresDatabase,files:FileStore,a:PortalActor,shareId:string,name:string,mediaType:string,bytes:Uint8Array,key:string,assertCurrent?:()=>Promise<void>){return application(db,a,files,{shareId,key,assertCurrent}).portalUpload(shareId,name,mediaType,bytes,key);}
/** No storage or business write is repeated. Its private File/receipt witness
 * is checked after deferred effects and never returned as presentation data. */
export function portalUploadRecovery(db:SqlDatabase,a:PortalActor,shareId:string,name:string,mediaType:string,bytes:Uint8Array,key:string,assertCurrent:()=>Promise<void>){return withLegalPortalSnapshotRead(db,a,async database=>{
 const app=application(database,a),before=await app.portalUploadRecovery(shareId,name,mediaType,bytes,key);
 await flushFinanceDeferredEffects(database);
 const after=await app.portalUploadRecovery(shareId,name,mediaType,bytes,key);
 if(before.witness!==after.witness)throw new ApiError(409,'LEGAL_UPLOAD_RECOVERY_CHANGED','The original shared upload could not be confirmed.');
 return after.result;
},assertCurrent);}
