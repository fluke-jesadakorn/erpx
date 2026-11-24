import {legalWorkspaceSnapshot} from "@erpx/feature-legal/lib/workspace-snapshot";
import {legalPackageEnforcementMode} from '@erpx/server-commercial/public/purchase-command-boundary';
import {legalCorePermission,type LegalPurchaseCapability} from '@erpx/server-legal/domain/purchases';
import {legalReadScope,withLegalPurchasedCommand,inLegalOwnerUnit} from '@erpx/server-legal/adapters/purchases';
import {parseItem} from '@erpx/server-legal/domain/model';
import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";
import { legalManagerAvailableSql, legalSettingsEnabledSql } from "../legal-read-tail-r9-sources";
import { legalItemSelectSql } from "../legal-read-tail-r16-sources";
import { legalEventsByItemSql } from "../legal-read-tail-r18-sources";
import { legalHoldTargetGuardPrefix } from "../legal-read-tail-r24-sources";
import { legalRecentItemListSql } from "../legal-read-tail-r31-sources";
import { legalItemOrderedSql } from "../legal-read-tail-r35-sources";
import { legalHoldOwnTargetSql } from "../legal-read-tail-r39-sources";
import { legalExternalUploadShareJoinHead } from "../legal-read-tail-r44-sources";
import { finalRootOwnerProjectionSources as workflowProjectionSources } from "@erpx/server-workflow/public/final-root-owner-projection-sources";
import {nativeLegalHoldNotice} from "../native-legal-hold-notice";
import "server-only";
import {createHash} from "node:crypto";
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import {hasPermission,permissionCatalog} from "@erpx/feature-identity_access/lib/permissions";
import { productionFeatureGuard } from "@erpx/server-pms/public/feature-availability";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {tables,access,enabled,visibility,guards,lock,eligible,personGuard} from "@erpx/server-legal/adapters/legal/access";
import {legalPeople} from "@erpx/server-legal/adapters/legal/assignees";
import type {LegalItem,LegalDetail,DocumentVersion} from "@erpx/server-legal/domain/model";
import {createLegalRegisterApplication,type LegalWrite,type LegalListQuery} from "@erpx/server-legal/application/service";
import { legalService } from '@erpx/server-collaboration/adapters/native-legal-service';
export const hash=(value:string|Uint8Array)=>createHash("sha256").update(value).digest("hex");
function application(db:SqlDatabase,a:WorkContext,readScope?:Promise<Awaited<ReturnType<typeof legalReadScope>>>){const nativeSvc=legalService(db,a.companyId,a.userId,identity_accessProjectionSources.assignments,tables);
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
        `${legalItemSelectSql}${clause} AND ${purchase.where} ORDER BY i.updated_at DESC,i.id LIMIT 30 OFFSET ?`,
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
          legalItemOrderedSql("*","legal_documents","version DESC",100),
        )
        .bind(id)
        .all<DocumentVersion>();},
events:(id:string)=>{return db
        .prepare(
          `${legalEventsByItemSql} DESC,id DESC LIMIT 100`,
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
          legalRecentItemListSql("id,body,visibility,created_by,created_at","legal_comments",100),
        )
        .bind(id)
        .all<LegalDetail["comments"][number]>();},
envelopes:(id:string)=>{return db
        .prepare(
          legalRecentItemListSql("id,state,document_id,expires_at,consent","legal_envelopes",20),
        )
        .bind(id)
        .all<Omit<LegalDetail["envelopes"][number], "signers">>();},
signers:(envelopeId:string)=>{return db
          .prepare(
            "SELECT user_id,ordinal,state,signed_at FROM legal_signers WHERE envelope_id=? ORDER BY ordinal",
          )
          .bind(envelopeId)
          .all<LegalDetail["envelopes"][number]["signers"][number]>();},
recordRead:async(id,identifier,now)=>{await (await nativeSvc.accessEvent({identifier,id,now})).run();},
shares:(id:string)=>{return db
          .prepare(
            "SELECT id,email,document_id,user_id,expires_at,revoked_at FROM legal_shares WHERE item_id=? ORDER BY created_at DESC LIMIT 100",
          )
          .bind(id)
          .all<LegalDetail["shares"][number]>();},
submissions:(id:string)=>{return db
      .prepare(
        legalExternalUploadShareJoinHead('u.id,u.name,u.checksum,u.created_by,u.created_at','u','s')+'WHERE s.item_id=? ORDER BY u.created_at DESC LIMIT 100',
      )
      .bind(id)
      .all<LegalDetail["submissions"][number]>();},
configure:async facts=>stage(await nativeSvc.configure(facts)),
managerAvailable:(facts:{companyId:string;branchId:string | null;departmentId:string | null;})=>{return db.prepare(`${legalManagerAvailableSql} LIMIT 1`).bind(facts.companyId,facts.branchId,facts.departmentId).first();},
createRecord:async facts=>stage(await nativeSvc.createRecord(facts)),
createSubtype:async facts=>stage(await nativeSvc.createSubtype(facts)),
addInitialMember:async facts=>stage(await nativeSvc.addInitialMember({id:facts.id,u:facts.u})),
routeRequester:async facts=>stage(await nativeSvc.routeRequester(facts)),
updateRecord:async facts=>stage(await nativeSvc.updateRecord(facts)),
updateSubtype:async facts=>stage(await nativeSvc.updateSubtype(facts)),
retainOwnerMembership:async facts=>stage(await nativeSvc.retainOwnerMembership({id:facts.id,ownerId:facts.ownerId})),
advanceRecord:async facts=>stage(await nativeSvc.advanceRecord(facts)),
expandHoldTargets:async ()=>stage(await nativeSvc.expandHoldTargets()),
notifyHoldTargets:async(facts:{timestamp:number;companyId:string;})=>{return stage(await nativeLegalHoldNotice(db,a,facts.timestamp));},
requireEnabled:(key:string)=>stage(assertion(
          db,
          key,
          "legal-enabled",
          `EXISTS(${legalSettingsEnabledSql})`,
          [a.companyId],
        )),
requireRequesterRouting:(key:string)=>stage(assertion(db,key,"legal-request-routing",`EXISTS(${legalManagerAvailableSql})`,[a.companyId,a.branchId,a.departmentId])),
requireHoldPreservable:(key:string,id:string)=>stage(assertion(
        db,
        key,
        "hold-preservable",
        `NOT EXISTS(SELECT 1 FROM legal_hold_targets t JOIN legal_holds h ON h.id=t.hold_id JOIN legal_destructions d ON d.item_id=t.item_id WHERE h.state!='released' AND d.state IN ('executing','completed') AND (h.item_id=? OR ${legalHoldOwnTargetSql('h')}))`,
        [id, id],
      )),
requireHoldSourcesPreservable:(key:string,id:string)=>stage(assertion(
        db,
        key,
        "hold-sources-preservable",
        "NOT EXISTS("+legalHoldTargetGuardPrefix+`hold.state!='released' AND legal_item_has_destroyed_attachment(target.item_id) AND (hold.item_id=? OR ${legalHoldOwnTargetSql('hold')}))`,
        [id, id],
      )),
enabled:()=>enabled(db,a),companyLock:()=>stage(lock(db,a.companyId)),guardChanges:(id,version,key,edit,allowFrozen,preparation)=>guards(db,a,id,version,key,edit,allowFrozen,preparation).map(stage),personGuard:(key,id,scope,permission)=>stage(personGuard(db,a,key,id,scope,permission)),featureGuard:key=>productionFeatureGuard(db,key,a.companyId,"legal").map(stage),
event:async(id,action,note,version,identifier,now)=>stage(await nativeSvc.event({id,action,note,version,identifier,now})),
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
