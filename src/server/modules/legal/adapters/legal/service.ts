import {legalWorkspaceSnapshot} from "@/lib/legal/workspace-snapshot";
import {legalPackageEnforcementMode} from '../../../commercial/public/purchase-command-boundary';
import {legalCorePermission,type LegalPurchaseCapability} from '../../domain/purchases';
import {legalReadScope,withLegalPurchasedCommand,inLegalOwnerUnit} from '../purchases';
import {parseItem} from '../../domain/model';
import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "../../../identity_access/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as workflowProjectionSources } from "../../../workflow/public/final-root-owner-projection-sources";
import {legalHoldNotificationInsert} from "../../../collaboration/public/legal-hold-notifications";
import "server-only";
import {createHash} from "node:crypto";
import {samePerson} from "@/lib/identity_access/iam";
import {hasPermission,permissionCatalog} from "@/lib/identity_access/permissions";
import {productionFeatureGuard} from "../../../pms/public/hospitality-rollout";
import type {WorkContext} from "../../../identity_access/public/authorization";
import {requirePermission} from "../../../identity_access/public/iam-permissions";
import type {SqlDatabase,SqlStatement} from "../../../runtime/public/runtime/types";
import {ApiError} from "../../../runtime/public/http";
import {executeCommand,replayCommand,assertion} from "../../../workflow/public/commands";
import {tables,access,enabled,visibility,guards,lock,eligible,personGuard} from "./access";
import {legalPeople} from "./assignees";
import type {LegalItem,LegalDetail,DocumentVersion} from "../../domain/model";
import {createLegalRegisterApplication,type LegalWrite,type LegalListQuery} from "../../application/service";
export const hash=(value:string|Uint8Array)=>createHash("sha256").update(value).digest("hex");
function application(db:SqlDatabase,a:WorkContext,readScope?:Promise<Awaited<ReturnType<typeof legalReadScope>>>){
 let readObservation=readScope;const purchasedRead=()=>readObservation??=legalReadScope(db,a);
const v=visibility(a),linkScope=visibility(a,"t"),holdScope=visibility(a,"source"),nativeWrites=new WeakMap<LegalWrite,SqlStatement>();
const stage=(statement:SqlStatement):LegalWrite=>{const write:LegalWrite={owner:"legal"};nativeWrites.set(write,statement);return write;};
function listScope(q:LegalListQuery){
  const clause = `${v.sql} AND (?::text IS NULL OR i.kind=?) AND (?='' OR strpos(lower(i.title||' '||i.description),lower(?))>0 OR EXISTS(SELECT 1 FROM legal_documents d WHERE d.item_id=i.id AND strpos(lower(d.name||' '||d.body),lower(?))>0) OR EXISTS(SELECT 1 FROM legal_document_inspections x JOIN legal_documents d ON d.id=x.document_id WHERE d.item_id=i.id AND x.kind='ocr' AND strpos(lower(x.result_json::jsonb->>'text'),lower(?))>0)) AND (?='' OR i.state=?) AND (?='false' OR i.owner_id=?)`;
  const args = [
    ...v.values,
    q.kind ?? null,
    q.kind ?? null,
    q.search,
    q.search,
    q.search,
    q.search,
    q.state,
    q.state,
    q.mine,
    a.userId,
  ];
return {clause,args};}
return createLegalRegisterApplication({actor:a,clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},require:permission=>requirePermission(a,permission),can:permission=>hasPermission(a,permission),samePerson:userId=>samePerson(a,userId),permissionCatalog,error:(status,code,message)=>new ApiError(status,code,message),hash,utf8Length:value=>Buffer.byteLength(value),access:(id,permission)=>access(db,a,id,permission),eligible:(id,scope,permission)=>eligible(db,a,id,scope,permission),people:()=>legalPeople(db,a),repository:{
items:async(query:LegalListQuery)=>{const {clause,args}=listScope(query),purchase=await purchasedRead();return db
      .prepare(
        `SELECT i.* FROM legal_items i WHERE ${clause} AND ${purchase.where} ORDER BY i.updated_at DESC,i.id LIMIT 30 OFFSET ?`,
      )
      .bind(...args, (query.page - 1) * 30)
      .all<LegalItem>();},
total:async(query:LegalListQuery)=>{const {clause,args}=listScope(query),purchase=await purchasedRead();return db
      .prepare(`SELECT count(*)::int n FROM legal_items i WHERE ${clause} AND ${purchase.where}`)
      .bind(...args)
      .first<{ n: number }>();},
counts:async()=>{const purchase=await purchasedRead();return db
      .prepare(
        `SELECT i.state,count(*)::int count FROM legal_items i WHERE ${v.sql} AND ${purchase.where} GROUP BY i.state`,
      )
      .bind(...v.values)
      .all<{ state: string; count: number }>();},
documents:(id:string)=>{return db
        .prepare(
          "SELECT * FROM legal_documents WHERE item_id=? ORDER BY version DESC LIMIT 100",
        )
        .bind(id)
        .all<DocumentVersion>();},
events:(id:string)=>{return db
        .prepare(
          "SELECT * FROM legal_events WHERE item_id=? ORDER BY created_at DESC,id DESC LIMIT 100",
        )
        .bind(id)
        .all<LegalDetail["events"][number]>();},
members:(id:string)=>{return db
        .prepare("SELECT user_id,access FROM legal_members WHERE item_id=?")
        .bind(id)
        .all<LegalDetail["members"][number]>();},
holds:(id:string)=>{return db
        .prepare(
          `SELECT h.id,CASE WHEN ${holdScope.sql} THEN h.reason ELSE 'Protected by a related legal hold' END reason,h.state,(h.item_id=?) direct,(SELECT n.acknowledged_at FROM legal_hold_notices n WHERE n.hold_id=h.id AND n.item_id=t.item_id AND n.user_id=?) acknowledged_at FROM legal_holds h JOIN legal_hold_targets t ON t.hold_id=h.id JOIN legal_items source ON source.id=h.item_id WHERE t.item_id=? AND h.state!='released'`,
        )
        .bind(...holdScope.values, id, a.userId, id)
        .all<LegalDetail["holds"][number]>();},
links:(id:string)=>{return db
        .prepare(
          `SELECT l.target_id,l.relation,t.title FROM legal_links l JOIN legal_items t ON t.id=l.target_id WHERE l.source_id=? AND ${linkScope.sql}`,
        )
        .bind(id, ...linkScope.values)
        .all<LegalDetail["links"][number]>();},
comments:(id:string)=>{return db
        .prepare(
          "SELECT id,body,visibility,created_by,created_at FROM legal_comments WHERE item_id=? ORDER BY created_at DESC LIMIT 100",
        )
        .bind(id)
        .all<LegalDetail["comments"][number]>();},
envelopes:(id:string)=>{return db
        .prepare(
          "SELECT id,state,document_id,expires_at,consent FROM legal_envelopes WHERE item_id=? ORDER BY created_at DESC LIMIT 20",
        )
        .bind(id)
        .all<Omit<LegalDetail["envelopes"][number], "signers">>();},
signers:(envelopeId:string)=>{return db
          .prepare(
            "SELECT user_id,ordinal,state,signed_at FROM legal_signers WHERE envelope_id=? ORDER BY ordinal",
          )
          .bind(envelopeId)
          .all<LegalDetail["envelopes"][number]["signers"][number]>();},
recordRead:async (id:string,identifier:string,now:number)=>{await db.prepare("INSERT INTO legal_access_events VALUES(?,?,?,?,?,?)").bind(identifier,id,null,a.userId,"record.read",now).run();},
shares:(id:string)=>{return db
          .prepare(
            "SELECT id,email,document_id,user_id,expires_at,revoked_at FROM legal_shares WHERE item_id=? ORDER BY created_at DESC LIMIT 100",
          )
          .bind(id)
          .all<LegalDetail["shares"][number]>();},
submissions:(id:string)=>{return db
      .prepare(
        "SELECT u.id,u.name,u.checksum,u.created_by,u.created_at FROM legal_external_uploads u JOIN legal_shares s ON s.id=u.share_id WHERE s.item_id=? ORDER BY u.created_at DESC LIMIT 100",
      )
      .bind(id)
      .all<LegalDetail["submissions"][number]>();},
configure:(facts:{companyId:string;enabled:boolean;aiEnabled:boolean;reminderDaysJson:string | null;reminderDaysJson2:string | null;})=>{return stage(db.prepare("INSERT INTO legal_settings(company_id,enabled,ai_enabled,reminder_days) VALUES(?,?,?,coalesce(?::jsonb,'[90,60,30,7,1]'::jsonb)) ON CONFLICT(company_id) DO UPDATE SET enabled=excluded.enabled,ai_enabled=excluded.ai_enabled,reminder_days=coalesce(?::jsonb,legal_settings.reminder_days),version=legal_settings.version+1").bind(facts.companyId,facts.enabled,facts.aiEnabled,facts.reminderDaysJson,facts.reminderDaysJson2));},
managerAvailable:(facts:{companyId:string;branchId:string | null;departmentId:string | null;})=>{return db.prepare(`SELECT 1 FROM (${identity_accessProjectionSources.assignments}) assignments WHERE company_id=? AND role='legal_manager' AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND legal_person_eligible(company_id,user_id,?,?,'legal.review') LIMIT 1`).bind(facts.companyId,facts.branchId,facts.departmentId).first();},
createRecord:(facts:{id:string;companyId:string;branchId:string | null;departmentId:string | null;kind:"request" | "matter" | "contract" | "obligation" | "compliance" | "license" | "corporate" | "ip" | "privacy" | "counsel" | "template" | "rule";title:string;description:string;classification:"internal" | "confidential" | "restricted";country:string;jurisdiction:string;language:string;currency:string;timezone:string;priority:"normal" | "high" | "urgent";dueDate:string | null;ownerId:string;actorId:string;now:number;now2:number;})=>{return stage(db.prepare("INSERT INTO legal_items(id,company_id,branch_id,department_id,kind,title,description,classification,country,jurisdiction,language,currency,timezone,priority,due_date,owner_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(facts.id,facts.companyId,facts.branchId,facts.departmentId,facts.kind,facts.title,facts.description,facts.classification,facts.country,facts.jurisdiction,facts.language,facts.currency,facts.timezone,facts.priority,facts.dueDate,facts.ownerId,facts.actorId,facts.now,facts.now2));},
createSubtype:(facts:{id:string;dataJson:string;recordKind:LegalItem["kind"];})=>{return stage(db.prepare(`INSERT INTO ${tables[facts.recordKind]} VALUES(?,?::jsonb)`).bind(facts.id,facts.dataJson));},
addInitialMember:(facts:{id:string;u:string;})=>{return stage(db.prepare("INSERT INTO legal_members VALUES(?,?,'edit')").bind(facts.id,facts.u));},
routeRequester:(facts:{id:string;companyId:string;branchId:string | null;departmentId:string | null;})=>{return stage(db.prepare(`INSERT INTO legal_members(item_id,user_id,access) SELECT DISTINCT ?,user_id,'edit' FROM (${identity_accessProjectionSources.assignments}) assignments WHERE company_id=? AND role='legal_manager' AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND legal_person_eligible(company_id,user_id,?,?,'legal.review') ON CONFLICT DO NOTHING`).bind(facts.id,facts.companyId,facts.branchId,facts.departmentId));},
lastAuthor:(facts:{id:string;})=>{return db.prepare("SELECT actor_id FROM legal_events WHERE item_id=? AND action IN ('created','update','document','document.upload','ai.accepted') ORDER BY version DESC,created_at DESC LIMIT 1").bind(facts.id).first<{ actor_id: string }>();},
updateRecord:(facts:{title:string;description:string;classification:"internal" | "confidential" | "restricted";country:string;jurisdiction:string;language:string;currency:string;timezone:string;priority:"normal" | "high" | "urgent";dueDate:string | null;ownerId:string;id:string;})=>{return stage(db.prepare("UPDATE legal_items SET title=?,description=?,classification=?,country=?,jurisdiction=?,language=?,currency=?,timezone=?,priority=?,due_date=?,owner_id=? WHERE id=?").bind(facts.title,facts.description,facts.classification,facts.country,facts.jurisdiction,facts.language,facts.currency,facts.timezone,facts.priority,facts.dueDate,facts.ownerId,facts.id));},
updateSubtype:(facts:{dataJson:string;id:string;recordKind:LegalItem["kind"];})=>{return stage(db.prepare(`UPDATE ${tables[facts.recordKind]} SET data=?::jsonb WHERE item_id=?`).bind(facts.dataJson,facts.id));},
retainOwnerMembership:(facts:{id:string;ownerId:string;})=>{return stage(db.prepare("INSERT INTO legal_members VALUES(?,?,'edit') ON CONFLICT(item_id,user_id) DO UPDATE SET access='edit'").bind(facts.id,facts.ownerId));},
assignOwner:(facts:{ownerId:string;id:string;})=>{return stage(db.prepare("UPDATE legal_items SET owner_id=? WHERE id=?").bind(facts.ownerId,facts.id));},
retainAssignedMembership:(facts:{id:string;ownerId:string;})=>{return stage(db.prepare("INSERT INTO legal_members VALUES(?,?,'edit') ON CONFLICT(item_id,user_id) DO UPDATE SET access='edit'").bind(facts.id,facts.ownerId));},
executedContract:(facts:{id:string;id2:string;id3:string;})=>{return db.prepare("SELECT 1 FROM legal_envelopes WHERE item_id=? AND state='completed' UNION ALL SELECT 1 FROM legal_executed_copies WHERE item_id=? AND state='confirmed' AND approved_cycle=(SELECT max(cycle) FROM legal_approvals WHERE item_id=?)").bind(facts.id,facts.id2,facts.id3).first();},
cancelEnvelopes:(facts:{id:string;})=>{return stage(db.prepare("UPDATE legal_envelopes SET state='cancelled' WHERE item_id=? AND state='sent'").bind(facts.id));},
removeMember:(facts:{id:string;userId:string;})=>{return stage(db.prepare("DELETE FROM legal_members WHERE item_id=? AND user_id=?").bind(facts.id,facts.userId));},
setMember:(facts:{id:string;userId:string;access:"read" | "edit";})=>{return stage(db.prepare("INSERT INTO legal_members VALUES(?,?,?) ON CONFLICT(item_id,user_id) DO UPDATE SET access=excluded.access").bind(facts.id,facts.userId,facts.access));},
documentBelongs:(facts:{documentId:string;id:string;})=>{return db.prepare("SELECT 1 FROM legal_documents WHERE id=? AND item_id=?").bind(facts.documentId,facts.id).first();},
addComment:(facts:{identifier:string;id:string;section:string;body:string;visibility:"internal" | "external";actorId:string;timestamp:number;documentId:string | null;})=>{return stage(db.prepare("INSERT INTO legal_comments(id,item_id,section,body,visibility,created_by,created_at,document_id) VALUES(?,?,?,?,?,?,?,?)").bind(facts.identifier,facts.id,facts.section,facts.body,facts.visibility,facts.actorId,facts.timestamp,facts.documentId));},
nextDocumentVersion:(facts:{id:string;})=>{return db.prepare("SELECT coalesce(max(version),0)+1 n FROM legal_documents WHERE item_id=?").bind(facts.id).first<{ n: number }>();},
addAuthoredDocument:(facts:{identifier:string;id:string;next:number;name:string;checksum:string;byteLength:number;body:string;actorId:string;timestamp:number;})=>{return stage(db.prepare("INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,body,created_by,created_at) VALUES(?,?,?,?,'text/plain',?,?,?,?,?)").bind(facts.identifier,facts.id,facts.next,facts.name,facts.checksum,facts.byteLength,facts.body,facts.actorId,facts.timestamp));},
approvalPending:(facts:{id:string;})=>{return db.prepare(`SELECT 1 FROM (${workflowProjectionSources.approval_instances}) approval_instances WHERE record_type='legal' AND record_id=? AND state='pending'`).bind(facts.id).first();},
approvalDocument:(facts:{documentId:string;id:string;})=>{return db.prepare("SELECT * FROM legal_documents WHERE id=? AND item_id=?").bind(facts.documentId,facts.id).first<DocumentVersion>();},
isExecutedCopy:(facts:{documentId:string;})=>{return db.prepare("SELECT 1 FROM legal_executed_copies WHERE document_id=?").bind(facts.documentId).first();},
internalSigner:(facts:{id:string;signer:string;})=>{return db.prepare("SELECT 1 FROM legal_members WHERE item_id=? AND user_id=?").bind(facts.id,facts.signer).first();},
externalSigner:(facts:{id:string;documentId:string;signer:string;timestamp:number;})=>{return db.prepare("SELECT 1 FROM legal_shares WHERE item_id=? AND document_id=? AND user_id=? AND revoked_at IS NULL AND expires_at>?").bind(facts.id,facts.documentId,facts.signer,facts.timestamp).first();},
lockApprovedDocument:(facts:{documentId:string;})=>{return stage(db.prepare("UPDATE legal_documents SET locked=true WHERE id=?").bind(facts.documentId));},
recordApproval:(facts:{id:string;nextVersion:number;documentId:string;documentChecksum:string;signerIdsJson:string;actorId:string;timestamp:number;})=>{return stage(db.prepare("INSERT INTO legal_approvals VALUES(?,?,?,?,?::jsonb,?,'approved',?)").bind(facts.id,facts.nextVersion,facts.documentId,facts.documentChecksum,facts.signerIdsJson,facts.actorId,facts.timestamp));},
linkRecord:(facts:{id:string;targetId:string;relation:"obligation" | "related" | "evidence" | "amends" | "derived";})=>{return stage(db.prepare("INSERT INTO legal_links VALUES(?,?,?) ON CONFLICT DO NOTHING").bind(facts.id,facts.targetId,facts.relation));},
preservationNotice:(facts:{holdId:string;id:string;actorId:string;})=>{return db.prepare("SELECT 1 FROM legal_hold_notices WHERE hold_id=? AND item_id=? AND user_id=?").bind(facts.holdId,facts.id,facts.actorId).first();},
acknowledgeNotice:(facts:{timestamp:number;note:string;holdId:string;id:string;actorId:string;})=>{return stage(db.prepare("UPDATE legal_hold_notices SET acknowledged_at=?,statement=? WHERE hold_id=? AND item_id=? AND user_id=?").bind(facts.timestamp,facts.note,facts.holdId,facts.id,facts.actorId));},
createHold:(facts:{hold:string;id:string;note:string;actorId:string;timestamp:number;})=>{return stage(db.prepare("INSERT INTO legal_holds(id,item_id,reason,state,created_by,created_at) VALUES(?,?,?,'active',?,?)").bind(facts.hold,facts.id,facts.note,facts.actorId,facts.timestamp));},
includeHoldTarget:(facts:{hold:string;id:string;})=>{return stage(db.prepare("INSERT INTO legal_hold_targets VALUES(?,?)").bind(facts.hold,facts.id));},
hold:(facts:{requestedId:string;id:string;})=>{return db.prepare("SELECT * FROM legal_holds WHERE id=? AND item_id=?").bind(facts.requestedId,facts.id).first<{
        id: string;
        state: string;
        created_by: string;
        release_requested_by: string | null;
      }>();},
requestHoldRelease:(facts:{actorId:string;holdId:string;})=>{return stage(db.prepare("UPDATE legal_holds SET state='release_requested',release_requested_by=? WHERE id=?").bind(facts.actorId,facts.holdId));},
releaseHold:(facts:{actorId:string;timestamp:number;holdId:string;})=>{return stage(db.prepare("UPDATE legal_holds SET state='released',released_by=?,released_at=? WHERE id=?").bind(facts.actorId,facts.timestamp,facts.holdId));},
shareDocumentBelongs:(facts:{id:string;documentId:string;})=>{return db.prepare("SELECT 1 FROM legal_documents WHERE item_id=? AND id=?").bind(facts.id,facts.documentId).first();},
shareDocument:(facts:{shareId:string;id:string;documentId:string;value3:string;expiresAt:number;actorId:string;timestamp:number;})=>{return stage(db.prepare("INSERT INTO legal_shares(id,item_id,document_id,email,expires_at,created_by,created_at) VALUES(?,?,?,?,?,?,?)").bind(facts.shareId,facts.id,facts.documentId,facts.value3,facts.expiresAt,facts.actorId,facts.timestamp));},
queueInvitation:(facts:{identifier:string;shareId:string;holdingSlug:string;timestamp:number;})=>{return stage(db.prepare("INSERT INTO legal_invitation_jobs(id,share_id,holding_slug,state,created_at) VALUES(?,?,?,'queued',?)").bind(facts.identifier,facts.shareId,facts.holdingSlug,facts.timestamp));},
revokeShare:(facts:{timestamp:number;share:string;id:string;})=>{return stage(db.prepare("UPDATE legal_shares SET revoked_at=?,version=version+1 WHERE id=? AND item_id=?").bind(facts.timestamp,facts.share,facts.id));},
advanceRecord:(facts:{state:string;timestamp:number;id:string;})=>{return stage(db.prepare("UPDATE legal_items SET state=?,version=version+1,updated_at=? WHERE id=?").bind(facts.state,facts.timestamp,facts.id));},
expandHoldTargets:()=>{return stage(db.prepare("WITH RECURSIVE targets(hold_id,item_id) AS (SELECT id,item_id FROM legal_holds WHERE state!='released' UNION SELECT t.hold_id,l.target_id FROM targets t JOIN legal_links l ON l.source_id=t.item_id WHERE l.relation IN ('evidence','derived','amends','obligation')) INSERT INTO legal_hold_targets SELECT * FROM targets ON CONFLICT DO NOTHING"));},
notifyHoldTargets:(facts:{timestamp:number;companyId:string;})=>{return stage(db.prepare(`WITH added AS (INSERT INTO legal_hold_notices(hold_id,item_id,user_id,notified_at) SELECT h.id,t.item_id,m.user_id,? FROM legal_holds h JOIN legal_hold_targets t ON t.hold_id=h.id JOIN legal_members m ON m.item_id=t.item_id WHERE h.state!='released' AND t.item_id IN (SELECT id FROM legal_items WHERE company_id=?) ON CONFLICT DO NOTHING RETURNING *) ${legalHoldNotificationInsert()}`).bind(facts.timestamp,facts.companyId));},
requireEnabled:(key:string)=>stage(assertion(
          db,
          key,
          "legal-enabled",
          "EXISTS(SELECT 1 FROM legal_settings WHERE company_id=? AND enabled)",
          [a.companyId],
        )),
requireRequesterRouting:(key:string)=>stage(assertion(db,key,"legal-request-routing",`EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) assignments WHERE company_id=? AND role='legal_manager' AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND legal_person_eligible(company_id,user_id,?,?,'legal.review'))`,[a.companyId,a.branchId,a.departmentId])),
requireHoldPreservable:(key:string,id:string)=>stage(assertion(
        db,
        key,
        "hold-preservable",
        "NOT EXISTS(SELECT 1 FROM legal_hold_targets t JOIN legal_holds h ON h.id=t.hold_id JOIN legal_destructions d ON d.item_id=t.item_id WHERE h.state!='released' AND d.state IN ('executing','completed') AND (h.item_id=? OR EXISTS(SELECT 1 FROM legal_hold_targets own WHERE own.hold_id=h.id AND own.item_id=?)))",
        [id, id],
      )),
requireHoldSourcesPreservable:(key:string,id:string)=>stage(assertion(
        db,
        key,
        "hold-sources-preservable",
        "NOT EXISTS(SELECT 1 FROM legal_hold_targets target JOIN legal_holds hold ON hold.id=target.hold_id WHERE hold.state!='released' AND legal_item_has_destroyed_attachment(target.item_id) AND (hold.item_id=? OR EXISTS(SELECT 1 FROM legal_hold_targets own WHERE own.hold_id=hold.id AND own.item_id=?)))",
        [id, id],
      )),
enabled:()=>enabled(db,a),companyLock:()=>stage(lock(db,a.companyId)),guardChanges:(id,version,key,edit,allowFrozen,preparation)=>guards(db,a,id,version,key,edit,allowFrozen,preparation).map(stage),personGuard:(key,id,scope,permission)=>stage(personGuard(db,a,key,id,scope,permission)),featureGuard:key=>productionFeatureGuard(db,key,a.companyId,"legal").map(stage),
event:(id,action,note,version,identifier,now)=>stage(db.prepare("INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) VALUES(?,?,?,?,?,?,?)").bind(identifier,id,action,a.userId,note,version,now)),
replay:(key,operation,id,input)=>replayCommand(db,a,key,operation,id,input),
commit:(key,operation,input,id,version,result,writes)=>executeCommand(db,a,key,operation,input,id,version,result,writes.map(write=>{const statement=nativeWrites.get(write);if(!statement)throw new Error("LEGAL_WRITE_REPOSITORY_MISMATCH");return statement;})),
}});}
export async function legalData(db:SqlDatabase,a:WorkContext,query:Record<string,unknown>={}){const scope=await legalReadScope(db,a);const data=await application(db,a,Promise.resolve(scope)).legalData(query);return {...data,purchases:scope.purchases};}
export async function legalRecordData(db:SqlDatabase,a:WorkContext){const scope=await legalReadScope(db,a);const data=await application(db,a,Promise.resolve(scope)).legalRecordData();return {...data,purchases:scope.purchases};}
export async function detail(db:SqlDatabase,a:WorkContext,id:string){const scope=await legalReadScope(db,a);const data=await application(db,a,Promise.resolve(scope)).detail(id),purchases=scope.purchases;return {...data,purchases,...(purchases?.["legal.documents"]?.history===false?{documents:[],submissions:[],envelopes:[]}:{}),...(purchases?.["legal.access"]?.history===false?{shares:[],members:[]}: {})};}
export async function legalCommand(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){
 if(await legalPackageEnforcementMode()==='disabled')return inLegalOwnerUnit(db,a.companyId,tx=>application(tx,a).legalCommand(raw,key));
 const input=(raw&&typeof raw==='object'?raw:{}) as Record<string,unknown>,action=String(input?.action??''),creating=action==='create',configuration=action==='settings',kind=creating?parseItem(input.item).kind:undefined,id=creating?key:configuration?undefined:typeof input.id==='string'?input.id:undefined;
 const additional:LegalPurchaseCapability[]=['document','approve'].includes(action)?['legal.documents']:['member','share','share.revoke'].includes(action)?['legal.access']:action.startsWith('hold.')?['legal.obligations']:[];
 const purpose=action.startsWith('hold.')?'history':configuration&&input.enabled===true||['create','update','document','share','link','assign'].includes(action)?'new-work':'settle-existing';
 return withLegalPurchasedCommand(db,a,{key,operation:`legal.${action}`,id,createKind:kind,purpose,permission:configuration?'legal.manage':creating?'legal.create':legalCorePermission(a.role,action,input.state),additional,configuration,ownerSource:configuration?{table:"legal_settings",id:a.companyId}:undefined},tx=>application(tx,a).legalCommand(raw,key));
}

export async function legalWorkspaceDetail(db:SqlDatabase,a:WorkContext,id:string){return legalWorkspaceSnapshot(await detail(db,a,id));}
