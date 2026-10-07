import "server-only";
import type {SqlDatabase,SqlStatement} from "../../../runtime/public/runtime/types";
import {PostgresDatabase} from "../../../runtime/public/runtime/postgres";
import type {WorkContext} from "../../../identity_access/public/authorization";
import {requirePermission} from "../../../identity_access/public/iam-permissions";
import {ApiError} from "../../../runtime/public/http";
import {executeCommand,replayCommand} from "../../../workflow/public/commands";
import {saveEmployeeRequest,employeeRequestSchema} from "../../../records/public/employee-requests";
import {access,guards,visibility,tables} from "./access";
import {legalCommand,hash} from "./service";
import {executedCopies} from "./executed-copies";
import {previewLegalImport} from "../../domain/operations";
import type {LegalItem} from "../../domain/model";
import type {LegalWrite} from "../../application/service";
import {createLegalOperationsApplication,type LegalOperationUnit,type LegalOperationsRepository} from "../../application/operations";
function unit(db:SqlDatabase,a:WorkContext):LegalOperationUnit{const v=visibility(a),writes=new WeakMap<LegalWrite,SqlStatement>();function stage(statement:SqlStatement):LegalWrite{const write:LegalWrite={owner:"legal"};writes.set(write,statement);return write;}function resolve(write:LegalWrite){const statement=writes.get(write);if(!statement)throw new Error("LEGAL_OPERATIONS_REPOSITORY_MISMATCH");return statement;}
return {
registerRows:(kind)=>db
      .prepare(
        `SELECT i.*,t.data FROM legal_items i JOIN ${tables[kind]} t ON t.item_id=i.id WHERE ${v.sql} AND i.kind=? ORDER BY i.id LIMIT 10001`,
      )
      .bind(...v.values, kind)
      .all<LegalItem>(),
exportRead:(facts:{identifier:string;id:string;null:null;actorId:string;fact4:"register.export";now:number;})=>stage(db.prepare("INSERT INTO legal_access_events VALUES(?,?,?,?,?,?)").bind(facts.identifier,facts.id,facts.null,facts.actorId,facts.fact4,facts.now)),
savedViews:(facts:{companyId:string;actorId:string;})=>db.prepare("SELECT name,query FROM legal_saved_views WHERE company_id=? AND user_id=? ORDER BY name LIMIT 100").bind(facts.companyId,facts.actorId).all(),
saveView:(facts:{companyId:string;actorId:string;name:string;fact3:string;})=>stage(db.prepare("INSERT INTO legal_saved_views VALUES(?,?,?,?::jsonb) ON CONFLICT(company_id,user_id,name) DO UPDATE SET query=excluded.query").bind(facts.companyId,facts.actorId,facts.name,facts.fact3)),
derivedLink:(facts:{id:string;id2:string;})=>db.prepare("INSERT INTO legal_links VALUES(?,?,'derived') ON CONFLICT DO NOTHING").bind(facts.id,facts.id2).run(),
envelopes:(facts:{id:string;})=>db.prepare("SELECT e.*,s.user_id,s.ordinal,s.state signer_state,s.signed_at,s.evidence FROM legal_envelopes e JOIN legal_signers s ON s.envelope_id=e.id WHERE e.item_id=? ORDER BY e.created_at,s.ordinal").bind(facts.id).all(),
documents:(facts:{id:string;})=>db.prepare("SELECT id,version,name,checksum,bytes,media_type,locked,created_at FROM legal_documents WHERE item_id=? ORDER BY version").bind(facts.id).all(),
events:(facts:{id:string;})=>db.prepare("SELECT * FROM legal_events WHERE item_id=? ORDER BY created_at,id").bind(facts.id).all(),
accesses:(facts:{id:string;})=>db.prepare("SELECT * FROM legal_access_events WHERE item_id=? ORDER BY created_at,id LIMIT 10001").bind(facts.id).all(),
evidenceRead:(facts:{identifier:string;id:string;null:null;actorId:string;fact4:"evidence.export";now:number;})=>db.prepare("INSERT INTO legal_access_events VALUES(?,?,?,?,?,?)").bind(facts.identifier,facts.id,facts.null,facts.actorId,facts.fact4,facts.now).run(),
featureHistory:(facts:{id:string;})=>db.prepare("SELECT * FROM legal_feature_history WHERE item_id=? ORDER BY created_at,id").bind(facts.id).all(),
retention:(facts:{id:string;})=>db.prepare("SELECT id,state,manifest,requested_by,approved_by,approved_at,completed_at FROM legal_destructions WHERE item_id=? ORDER BY created_at").bind(facts.id).all(),
holdPreview:(id)=>db
      .prepare(
        `WITH RECURSIVE linked(id) AS (SELECT ?::text UNION SELECT l.target_id FROM linked t JOIN legal_links l ON l.source_id=t.id WHERE l.relation IN ('evidence','derived','amends','obligation')) SELECT i.id,i.title FROM linked t JOIN legal_items i ON i.id=t.id WHERE ${v.sql}`,
      )
      .bind(id, ...v.values)
      .all(),
proposedAi:(facts:{proposalId:string;id:string;actorId:string;})=>db.prepare("SELECT source_version,source_hash,proposal FROM legal_ai_proposals WHERE id=? AND item_id=? AND created_by=? AND state='proposed'").bind(facts.proposalId,facts.id,facts.actorId).first<{ source_version: number; source_hash: string; proposal: string }>(),
nextDocumentVersion:(facts:{id:string;})=>db.prepare("SELECT coalesce(max(version),0)+1 n FROM legal_documents WHERE item_id=?").bind(facts.id).first<{ n: number }>(),
reviewedAiDocument:(facts:{documentId:string;id:string;next:number;name:string;checksum:string;bytes:number;body:string;actorId:string;now:number;})=>stage(db.prepare("INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,body,created_by,created_at) VALUES(?,?,?,?,'text/plain',?,?,?,?,?)").bind(facts.documentId,facts.id,facts.next,facts.name,facts.checksum,facts.bytes,facts.body,facts.actorId,facts.now)),
acceptProposal:(facts:{fact0:string;proposalId:string;})=>stage(db.prepare("UPDATE legal_ai_proposals SET state='accepted',proposal=proposal||?::jsonb WHERE id=?").bind(facts.fact0,facts.proposalId)),
advanceItem:(facts:{now:number;id:string;})=>stage(db.prepare("UPDATE legal_items SET version=version+1,updated_at=? WHERE id=?").bind(facts.now,facts.id)),
ruleImpact:(id)=>db
        .prepare(
          `SELECT i.id,i.kind,i.title,i.state,i.due_date,i.version FROM legal_items i JOIN legal_links l ON (l.source_id=? AND l.target_id=i.id) OR (l.target_id=? AND l.source_id=i.id) WHERE ${v.sql} ORDER BY i.id LIMIT 500`,
        )
        .bind(id, id, ...v.values)
        .all(),
access:(id,p)=>access(db,a,id,p),command:(raw,key)=>legalCommand(db,a,raw,key),purchase:async(raw,key)=>saveEmployeeRequest(db,a,null,employeeRequestSchema.parse(raw),key),executedCopies:id=>executedCopies(db,a,id),batch:steps=>db.batch(steps.map(resolve)),guardChanges:(id,version,key,edit,allowFrozen,preparation)=>guards(db,a,id,version,key,edit,allowFrozen,preparation).map(stage),event:(id,action,note,version,identifier,now)=>stage(db.prepare("INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) VALUES(?,?,?,?,?,?,?)").bind(identifier,id,action,a.userId,note,version,now)),replay:(key,operation,id,input)=>replayCommand(db,a,key,operation,id,input),commit:(key,operation,input,id,version,result,steps)=>executeCommand(db,a,key,operation,input,id,version,result,steps.map(resolve)),
};}
function application(db:SqlDatabase,a:WorkContext){const repository:LegalOperationsRepository={...unit(db,a),transaction:work=>{if(!(db instanceof PostgresDatabase))throw new Error("LEGAL_OPERATIONS_TRANSACTION_REQUIRED");return db.transaction(tx=>work(unit(tx,a)));}};return createLegalOperationsApplication({actor:a,repository,require:p=>requirePermission(a,p),clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},hash,byteLength:text=>Buffer.byteLength(text),error:(status,code,message)=>new ApiError(status,code,message)});}
export async function importPreview(a:WorkContext,raw:unknown){requirePermission(a,"legal.create");return previewLegalImport(raw,hash);}
export function importCommit(db:PostgresDatabase,a:WorkContext,raw:unknown,key:string){return application(db,a).importCommit(raw,key);}
export function exportRegister(db:SqlDatabase,a:WorkContext,kind:string){return application(db,a).exportRegister(kind);}
export function savedViews(db:SqlDatabase,a:WorkContext){return application(db,a).savedViews();}
export function saveView(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){return application(db,a).saveView(raw,key);}
export function applyTemplate(db:PostgresDatabase,a:WorkContext,raw:unknown,key:string){return application(db,a).applyTemplate(raw,key);}
export function convertRequest(db:PostgresDatabase,a:WorkContext,raw:unknown,key:string){return application(db,a).convertRequest(raw,key);}
export function evidenceBundle(db:SqlDatabase,a:WorkContext,id:string){return application(db,a).evidenceBundle(id);}
export function holdPreview(db:SqlDatabase,a:WorkContext,id:string){return application(db,a).holdPreview(id);}
export function applyAiDraft(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){return application(db,a).applyAiDraft(raw,key);}
export function prepareCounselPurchase(db:PostgresDatabase,a:WorkContext,raw:unknown,key:string){return application(db,a).prepareCounselPurchase(raw,key);}
export function startRevision(db:PostgresDatabase,a:WorkContext,raw:unknown,key:string){return application(db,a).startRevision(raw,key);}
export function ruleImpact(db:SqlDatabase,a:WorkContext,id:string){return application(db,a).ruleImpact(id);}
