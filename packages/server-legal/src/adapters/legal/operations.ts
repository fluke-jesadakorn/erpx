import {inLegalOwnerUnit,inLegacyLegalOperationsUnit,legalReadScope,withLegalPurchasedCommand,requireLegalItemPurchasedRead} from '@erpx/server-legal/adapters/purchases';
import {legalPackageEnforcementMode} from '@erpx/server-commercial/public/purchase-command-boundary';
import "server-only";
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {ApiError} from "@erpx/server-runtime/public/http";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {saveEmployeeRequest,employeeRequestSchema} from '@erpx/server-records/public/employee-requests';
import {access,guards,visibility,tables} from "@erpx/server-legal/adapters/legal/access";
import {nextDocumentVersionSql} from "@erpx/server-legal/adapters/legal/documents";
import {legalCommand,hash} from "@erpx/server-legal/adapters/legal/service";
import {executedCopies} from "@erpx/server-legal/adapters/legal/executed-copies";
import {previewLegalImport} from "@erpx/server-legal/domain/operations";
import type {LegalItem} from "@erpx/server-legal/domain/model";
import type {LegalWrite} from "@erpx/server-legal/application/service";
import {createLegalOperationsApplication,type LegalOperationUnit,type LegalOperationsRepository} from "@erpx/server-legal/application/operations";
import { legalOperations } from '@erpx/server-collaboration/adapters/native-legal-operations';
import {nativeLegalAuxiliaryRead} from '../native-legal-auxiliary';
import { legalEnvelopeSignerReadHead } from '../legal-read-tail-r43-sources';
import {legalEventsByItemSql} from '../legal-read-tail-r18-sources';
import {legalOrderedHistorySql} from '../legal-read-tail-r30-sources';
import {legalItemOrderedSql} from '../legal-read-tail-r35-sources';
import {legalPointByIdAndKeySql} from '../legal-read-tail-r34-sources';
import contract from '../legal-operations-sources.json' with {type:'json'};
const fill = (template: string, values: Record<string, string>) => {
  let rendered = template;
  for (const [name, value] of Object.entries(values)) {
    const token = `{${name}}`;
    if (!rendered.includes(token)) throw new Error('LEGAL_OPERATIONS_CONTRACT_INVALID');
    rendered = rendered.split(token).join(value);
  }
  return rendered;
};
/** Exact projection of the native `views` auxiliary read; no view SQL remains in TypeScript. */
type LegalSavedViewsView={name:string;query:unknown}[];
function unit(db:SqlDatabase,a:WorkContext,views?:LegalSavedViewsView):LegalOperationUnit{const nativeOps=legalOperations(db,a.companyId,a.userId);let observation:ReturnType<typeof legalReadScope>|undefined;const purchase=()=>observation??=legalReadScope(db,a);const v=visibility(a),writes=new WeakMap<LegalWrite,SqlStatement>();function stage(statement:SqlStatement):LegalWrite{const write:LegalWrite={owner:"legal"};writes.set(write,statement);return write;}function resolve(write:LegalWrite){const statement=writes.get(write);if(!statement)throw new Error("LEGAL_OPERATIONS_REPOSITORY_MISMATCH");return statement;}
return {
registerRows:async(kind)=>db
      .prepare(
        fill(contract.registerRows,{table:tables[kind],visibility:v.sql,purchase:(await purchase()).where}),
      )
      .bind(...v.values, kind)
      .all<LegalItem>(),
async exportRead(facts){return stage(await nativeOps.accessEvent({identifier:facts.identifier,id:facts.id,actorId:facts.actorId,action:facts.fact4,now:facts.now}));},
savedViews:async()=>({results:views??[]}),
async saveView(facts){return stage(await nativeOps.saveView({companyId:facts.companyId,actorId:facts.actorId,name:facts.name,queryJson:facts.fact3}));},
async derivedLink(facts){return nativeOps.derivedLink({id:facts.id,id2:facts.id2});},
envelopes:(facts:{id:string;})=>db.prepare(legalEnvelopeSignerReadHead("e.*,s.user_id,s.ordinal,s.state signer_state,s.signed_at,s.evidence")+"e.item_id=? ORDER BY e.created_at,s.ordinal").bind(facts.id).all(),
documents:(facts:{id:string;})=>db.prepare(legalItemOrderedSql("id,version,name,checksum,bytes,media_type,locked,created_at","legal_documents","version")).bind(facts.id).all(),
events:(facts:{id:string;})=>db.prepare(`${legalEventsByItemSql},id`).bind(facts.id).all(),
accesses:(facts:{id:string;})=>db.prepare(legalItemOrderedSql("*","legal_access_events","created_at,id",10001)).bind(facts.id).all(),
async evidenceRead(facts){return (await nativeOps.accessEvent({identifier:facts.identifier,id:facts.id,actorId:facts.actorId,action:facts.fact4,now:facts.now})).run();},
featureHistory:(facts:{id:string;})=>db.prepare(legalOrderedHistorySql("legal_feature_history","item_id")).bind(facts.id).all(),
retention:(facts:{id:string;})=>db.prepare(legalItemOrderedSql("id,state,manifest,requested_by,approved_by,approved_at,completed_at","legal_destructions","created_at")).bind(facts.id).all(),
holdPreview:async(id)=>db
      .prepare(
        fill(contract.holdPreview,{visibility:v.sql,purchase:(await purchase()).where}),
      )
      .bind(id, ...v.values)
      .all(),
proposedAi:(facts:{proposalId:string;id:string;actorId:string;})=>db.prepare(legalPointByIdAndKeySql("source_version,source_hash,proposal","legal_ai_proposals","item_id")+" AND created_by=? AND state='proposed'").bind(facts.proposalId,facts.id,facts.actorId).first<{ source_version: number; source_hash: string; proposal: string }>(),
nextDocumentVersion:(facts:{id:string;})=>db.prepare(nextDocumentVersionSql).bind(facts.id).first<{ n: number }>(),
async reviewedAiDocument(facts){return stage(await nativeOps.aiDocument({documentId:facts.documentId,itemId:facts.id,version:facts.next,name:facts.name,checksum:facts.checksum,bytes:facts.bytes,body:facts.body,actorId:facts.actorId,now:facts.now}));},
async acceptProposal(facts){return stage(await nativeOps.acceptProposal({proposalJson:facts.fact0,proposalId:facts.proposalId}));},
async advanceItem(facts){return stage(await nativeOps.advanceItem({now:facts.now,id:facts.id}));},
access:(id,p)=>access(db,a,id,p),command:(raw,key)=>legalCommand(db,a,raw,key),purchase:async(raw,key)=>saveEmployeeRequest(db,a,null,employeeRequestSchema.parse(raw),key),executedCopies:id=>executedCopies(db,a,id),batch:steps=>db.batch(steps.map(resolve)),guardChanges:(id,version,key,edit,allowFrozen,preparation)=>guards(db,a,id,version,key,edit,allowFrozen,preparation).map(stage),event:async(id,action,note,version,identifier,now)=>stage(await nativeOps.event({id,action,note,version,identifier,now})),replay:(key,operation,id,input)=>replayCommand(db,a,key,operation,id,input),commit:(key,operation,input,id,version,result,steps)=>executeCommand(db,a,key,operation,input,id,version,result,steps.map(resolve)),
};}
function application(db:SqlDatabase,a:WorkContext,views?:LegalSavedViewsView){const repository:LegalOperationsRepository={...unit(db,a,views),transaction:async work=>{if(await legalPackageEnforcementMode()!=="disabled")return inLegalOwnerUnit(db,a.companyId,tx=>work(unit(tx,a,views)));return inLegacyLegalOperationsUnit(db,a.companyId,tx=>work(unit(tx,a,views)));}};return createLegalOperationsApplication({actor:a,repository,require:p=>requirePermission(a,p),clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},hash,byteLength:text=>Buffer.byteLength(text),error:(status,code,message)=>new ApiError(status,code,message)});}
export async function importPreview(a:WorkContext,raw:unknown){requirePermission(a,"legal.create");return previewLegalImport(raw,hash);}
export function importCommit(db:PostgresDatabase,a:WorkContext,raw:unknown,key:string){return application(db,a).importCommit(raw,key);}
export function exportRegister(db:SqlDatabase,a:WorkContext,kind:string){return application(db,a).exportRegister(kind);}
export async function savedViews(db:SqlDatabase,a:WorkContext){const views=await nativeLegalAuxiliaryRead<LegalSavedViewsView>(a,'views');return application(db,a,views).savedViews();}
export function saveView(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){return application(db,a).saveView(raw,key);}
export function applyTemplate(db:PostgresDatabase,a:WorkContext,raw:unknown,key:string){return application(db,a).applyTemplate(raw,key);}
export function convertRequest(db:PostgresDatabase,a:WorkContext,raw:unknown,key:string){return application(db,a).convertRequest(raw,key);}
export async function evidenceBundle(db:SqlDatabase,a:WorkContext,id:string){if(await legalPackageEnforcementMode()!=="disabled")await requireLegalItemPurchasedRead(db,a,await access(db,a,id),["legal.documents","legal.obligations"]);return application(db,a).evidenceBundle(id);}
export function holdPreview(db:SqlDatabase,a:WorkContext,id:string){return application(db,a).holdPreview(id);}
export async function applyAiDraft(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){
 if(await legalPackageEnforcementMode()==='disabled')return inLegacyLegalOperationsUnit(db,a.companyId,tx=>application(tx,a).applyAiDraft(raw,key));
 const input=(raw&&typeof raw==="object"?raw:{}) as Record<string,unknown>;return withLegalPurchasedCommand(db,a,{key,id:typeof input.id==="string"?input.id:undefined,operation:"legal.ai.apply",purpose:"new-work",permission:"legal.ai",additionalPermissions:["legal.edit"],additional:["legal.documents"]},tx=>application(tx,a).applyAiDraft(raw,key));
}
export function prepareCounselPurchase(db:PostgresDatabase,a:WorkContext,raw:unknown,key:string){return application(db,a).prepareCounselPurchase(raw,key);}
export function startRevision(db:PostgresDatabase,a:WorkContext,raw:unknown,key:string){return application(db,a).startRevision(raw,key);}
