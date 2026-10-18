import {withLegalPortalOriginalRead} from '../portal-original-read';
import {withLegalPortalSnapshotRead} from '../portal-snapshot-read';
import {legalPortalPurchasePredicate,withLegalPortalPurchasedCommand,legalPortalPurchaseControls} from '../portal-purchases';
import {legalPackageEnforcementMode} from '../../../commercial/public/purchase-command-boundary';
import "server-only";
import type {SqlDatabase,SqlStatement,FileStore} from "../../../runtime/public/runtime/types";
import {PostgresDatabase} from "../../../runtime/public/runtime/postgres";
import {holdingBindingBySlug,holdingResources} from "../../../runtime/public/runtime";
import {currentSession} from "../../../identity_access/public/authorization";
import {ApiError} from "../../../runtime/public/http";
import {legalPortalBindingWrites} from "../../../files/public/legal-portal-binding";
import {legalPortalRepository} from '../../../files/public/legal-portal-repository';
import {flushFinanceDeferredEffects} from '../../../finance/public/finance-transactions';
import {legalPortalJournal} from "../../../workflow/public/legal-portal-journal";
import {lock} from "./access";
import {hash} from "./service";
import {portalAttachmentBytes,stagePortalAttachment} from "./portal-attachments";
import type {DocumentVersion} from "../../domain/model";
import {legalPortalActor,type PortalActor} from "../../domain/portal";
import {createLegalPortalApplication,type LegalPortalWrite,type LegalPortalUnit,type LegalPortalRepository,type LegalPortalShare,type LegalPortalShareFacts} from "../../application/portal";
export type {PortalActor} from "../../domain/portal";
export async function portalContext(slug:string){const {session}=await currentSession();const actor=legalPortalActor(session.user,session.session.id,(status,code,message)=>new ApiError(status,code,message)),binding=await holdingBindingBySlug(slug),resources=await holdingResources(binding.id);return {...resources,actor};}
async function shareRecord(database:SqlDatabase,facts:LegalPortalShareFacts,strictHistory=false){return database.prepare(`SELECT s.*,i.company_id,i.branch_id,i.department_id,i.version AS item_version,i.kind FROM legal_shares s JOIN legal_items i ON i.id=s.item_id JOIN legal_documents d ON d.id=s.document_id AND d.item_id=i.id WHERE ${await legalPortalPurchasePredicate('i',strictHistory)} AND s.id=? AND lower(s.email)=? AND (s.user_id IS NULL OR s.user_id=?) AND s.revoked_at IS NULL AND s.expires_at>?`).bind(facts.id,facts.email,facts.actorId,facts.now).first<LegalPortalShare>();}
function unit(db:SqlDatabase):LegalPortalUnit{const fileBinding=legalPortalBindingWrites(db),journal=legalPortalJournal(db),writes=new WeakMap<LegalPortalWrite,SqlStatement>();function stage(statement:SqlStatement):LegalPortalWrite{const write:LegalPortalWrite={owner:"legal"};writes.set(write,statement);return write;}
return {
shares:async(facts:{email:string;actorId:string;now:number;})=>db.prepare(`SELECT s.id,s.document_id,s.expires_at,s.user_id,d.name,d.version,d.checksum FROM legal_shares s JOIN legal_documents d ON d.id=s.document_id AND d.item_id=s.item_id JOIN legal_items i ON i.id=s.item_id WHERE ${await legalPortalPurchasePredicate()} AND lower(s.email)=? AND (s.user_id IS NULL OR s.user_id=?) AND s.revoked_at IS NULL AND s.expires_at>? ORDER BY s.created_at DESC LIMIT 100`).bind(facts.email,facts.actorId,facts.now).all(),
share:facts=>shareRecord(db,facts),
uploadRecoveryShare:facts=>shareRecord(db,facts,true),
uploadRecoverySource:facts=>legalPortalRepository(db).recoverySource(facts),
retentionFrozen:(facts:{item_id:string;})=>db.prepare("SELECT 1 FROM legal_destructions WHERE item_id=? AND state IN ('executing','completed')").bind(facts.item_id).first(),
documentSummary:(facts:{document_id:string;})=>db.prepare("SELECT id,name,body,version,checksum,media_type FROM legal_documents WHERE id=?").bind(facts.document_id).first(),
comments:(facts:{item_id:string;document_id:string;})=>db.prepare("SELECT body,created_at,created_by FROM legal_comments WHERE item_id=? AND document_id=? AND visibility='external' ORDER BY created_at LIMIT 100").bind(facts.item_id,facts.document_id).all(),
envelopes:(facts:{item_id:string;document_id:string;actorId:string;})=>db.prepare("SELECT e.id,e.state,e.consent,e.expires_at,s.state signer_state,s.viewed_at FROM legal_envelopes e JOIN legal_signers s ON s.envelope_id=e.id WHERE e.item_id=? AND e.document_id=? AND s.user_id=? ORDER BY e.created_at DESC LIMIT 10").bind(facts.item_id,facts.document_id,facts.actorId).all(),
viewRead:(facts:{identifier:string;item_id:string;document_id:string;actorId:string;fact4:"portal.view";now:number;})=>db.prepare("INSERT INTO legal_access_events VALUES(?,?,?,?,?,?)").bind(facts.identifier,facts.item_id,facts.document_id,facts.actorId,facts.fact4,facts.now).run(),
billing:(facts:{item_id:string;actorId:string;})=>db.prepare("SELECT id,title,state,data,version FROM legal_counsel_entries WHERE item_id=? AND created_by=? ORDER BY updated_at DESC LIMIT 100").bind(facts.item_id,facts.actorId).all<{id:string;title:string;state:string;data:unknown;version:number}>(),
document:(facts:{document_id:string;})=>db.prepare("SELECT * FROM legal_documents WHERE id=?").bind(facts.document_id).first<DocumentVersion>(),
downloadRead:(facts:{identifier:string;item_id:string;id:string;actorId:string;fact4:"portal.download";now:number;})=>db.prepare("INSERT INTO legal_access_events VALUES(?,?,?,?,?,?)").bind(facts.identifier,facts.item_id,facts.id,facts.actorId,facts.fact4,facts.now).run(),
receipt:(facts:{key:string;})=>db.prepare("SELECT * FROM legal_portal_receipts WHERE id=?").bind(facts.key).first<{ user_id: string; request_hash: string; result_json: string }>(),
enabled:(facts:{company_id:string;})=>db.prepare("SELECT 1 FROM legal_settings WHERE company_id=? AND enabled").bind(facts.company_id).first(),
acceptShare:(facts:{actorId:string;id:string;})=>db.prepare("UPDATE legal_shares SET user_id=?,version=version+1 WHERE id=?").bind(facts.actorId,facts.id).run(),
externalComment:(facts:{identifier:string;item_id:string;document_id:string;body:string;actorId:string;now:number;})=>db.prepare("INSERT INTO legal_comments(id,item_id,document_id,body,visibility,created_by,created_at) VALUES(?,?,?,?,'external',?,?)").bind(facts.identifier,facts.item_id,facts.document_id,facts.body,facts.actorId,facts.now).run(),
counselEntry:(facts:{key:string;item_id:string;title:string;actorId:string;fact4:string;actorId2:string;actorId3:string;now:number;})=>db.prepare("INSERT INTO legal_counsel_entries(id,item_id,category,title,owner_id,state,version,data,created_by,updated_by,updated_at) VALUES(?,?,'counsel_entry',?,?,'submitted',1,?::jsonb,?,?,?)").bind(facts.key,facts.item_id,facts.title,facts.actorId,facts.fact4,facts.actorId2,facts.actorId3,facts.now).run(),
counselHistory:(facts:{identifier:string;item_id:string;key:string;fact3:string;actorId:string;now:number;})=>db.prepare("INSERT INTO legal_feature_history(id,item_id,feature_id,category,version,snapshot,actor_id,reason,created_at) VALUES(?,?,?,'counsel_entry',1,?::jsonb,?,'External counsel submission',?)").bind(facts.identifier,facts.item_id,facts.key,facts.fact3,facts.actorId,facts.now).run(),
advanceItem:(facts:{now:number;item_id:string;})=>db.prepare("UPDATE legal_items SET version=version+1,updated_at=? WHERE id=?").bind(facts.now,facts.item_id).run(),
actionableEnvelope:(facts:{envelopeId:string | undefined;item_id:string;document_id:string;actorId:string;now:number;})=>db.prepare("SELECT e.*,s.ordinal,s.viewed_at FROM legal_envelopes e JOIN legal_signers s ON s.envelope_id=e.id WHERE e.id=? AND e.item_id=? AND e.document_id=? AND s.user_id=? AND e.state='sent' AND s.state='pending' AND e.expires_at>?").bind(facts.envelopeId,facts.item_id,facts.document_id,facts.actorId,facts.now).first<{
          id: string;
          ordinal: number;
          viewed_at: number | null;
          consent: string;
          document_hash: string;
        }>(),
viewEnvelope:(facts:{now:number;id:string;actorId:string;})=>db.prepare("UPDATE legal_signers SET viewed_at=? WHERE envelope_id=? AND user_id=?").bind(facts.now,facts.id,facts.actorId).run(),
interactionEvent:(facts:{identifier:string;fact1:string;actorId:string;document_id:string;now:number;item_id:string;})=>db.prepare("INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) SELECT ?,id,?,?,?,version,? FROM legal_items WHERE id=?").bind(facts.identifier,facts.fact1,facts.actorId,facts.document_id,facts.now,facts.item_id).run(),
interactionReceipt:(facts:{key:string;actorId:string;digest:string;fact3:string;now:number;})=>db.prepare("INSERT INTO legal_portal_receipts VALUES(?,?,?,?,?)").bind(facts.key,facts.actorId,facts.digest,facts.fact3,facts.now).run(),
uploadReceipt:(facts:{key:string;actorId:string;})=>db.prepare("SELECT request_hash,result_json FROM legal_portal_receipts WHERE id=? AND user_id=?").bind(facts.key,facts.actorId).first<{ request_hash: string; result_json: string }>(),
uploadEnabled:(facts:{company_id:string;})=>db.prepare("SELECT 1 FROM legal_settings WHERE company_id=? AND enabled").bind(facts.company_id).first(),
uploadReplay:(facts:{key:string;actorId:string;})=>db.prepare("SELECT request_hash,result_json FROM legal_portal_receipts WHERE id=? AND user_id=?").bind(facts.key,facts.actorId).first<{ request_hash: string; result_json: string }>(),
externalUpload:(facts:{key:string;shareId:string;objectKey:string;checksum:string;name:string;mediaType:string;bytes:number;actorId:string;now:number;linkId:string;sourceId:string;})=>db.prepare(`INSERT INTO legal_external_uploads(
          id,share_id,object_key,checksum,name,media_type,bytes,created_by,created_at,attachment_link_id,attachment_source_id
        ) VALUES(?,?,?,?,?,?,?,?,?,?,?)`).bind(facts.key,facts.shareId,facts.objectKey,facts.checksum,facts.name,facts.mediaType,facts.bytes,facts.actorId,facts.now,facts.linkId,facts.sourceId).run(),
bindLink:facts=>stage(fileBinding.bindLink(facts)),
bindSet:facts=>stage(fileBinding.bindSet(facts)),
advanceUploadItem:(facts:{now:number;item_id:string;company_id:string;expectedTargetVersion:number;})=>stage(db.prepare("UPDATE legal_items SET version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=?").bind(facts.now,facts.item_id,facts.company_id,facts.expectedTargetVersion)),
bindEvent:facts=>stage(fileBinding.bindEvent(facts)),
uploadEvent:(facts:{identifier:string;actorId:string;fact2:string;now:number;item_id:string;})=>db.prepare("INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) SELECT ?,id,'portal.upload',?,?,version,? FROM legal_items WHERE id=?").bind(facts.identifier,facts.actorId,facts.fact2,facts.now,facts.item_id).run(),
uploadCommandReceipt:(facts:{key:string;actorId:string;digest:string;fact3:string;now:number;})=>db.prepare("INSERT INTO legal_portal_receipts VALUES(?,?,?,?,?)").bind(facts.key,facts.actorId,facts.digest,facts.fact3,facts.now).run(),
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
