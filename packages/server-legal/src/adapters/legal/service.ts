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
import { legalPointByKeySql } from "../legal-read-tail-r36-sources";
import { legalOrderedKeyHeadSql } from "../legal-read-tail-r54-sources";
import contract from '../legal-service-sources.json' with {type:'json'};
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
const fill = (template: string, values: Record<string, string>) => {
  let rendered = template;
  for (const [name, value] of Object.entries(values)) {
    const token = `{${name}}`;
    if (!rendered.includes(token)) throw new Error('LEGAL_SERVICE_CONTRACT_INVALID');
    rendered = rendered.split(token).join(value);
  }
  return rendered;
};
export const hash=(value:string|Uint8Array)=>createHash("sha256").update(value).digest("hex");
function application(db:SqlDatabase,a:WorkContext,readScope?:Promise<Awaited<ReturnType<typeof legalReadScope>>>){const nativeSvc=legalService(db,a.companyId,a.userId,identity_accessProjectionSources.assignments,tables);
 let readObservation=readScope;const purchasedRead=()=>readObservation??=legalReadScope(db,a);
const v=visibility(a),linkScope=visibility(a,"t"),holdScope=visibility(a,"source"),nativeWrites=new WeakMap<LegalWrite,SqlStatement>();
const stage=(statement:SqlStatement):LegalWrite=>{const write:LegalWrite={owner:"legal"};nativeWrites.set(write,statement);return write;};
function listScope(q:LegalListQuery){
  const clause = fill(contract.listClause,{visibility:v.sql});
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
      .prepare(fill(contract.total,{clause,purchase:purchase.where}))
      .bind(...args)
      .first<{ n: number }>();},
counts:async()=>{const purchase=await purchasedRead();return db
      .prepare(
        fill(contract.stateCounts,{visibility:v.sql,purchase:purchase.where}),
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
        .prepare(legalPointByKeySql("user_id,access","legal_members","item_id"))
        .bind(id)
        .all<LegalDetail["members"][number]>();},
holds:(id:string)=>{return db
        .prepare(
          fill(contract.holds,{holdScope:holdScope.sql}),
        )
        .bind(...holdScope.values, id, a.userId, id)
        .all<LegalDetail["holds"][number]>();},
links:(id:string)=>{return db
        .prepare(
          fill(contract.links,{linkScope:linkScope.sql}),
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
            legalOrderedKeyHeadSql("user_id,ordinal,state,signed_at","legal_signers","envelope_id")+" ORDER BY ordinal",
          )
          .bind(envelopeId)
          .all<LegalDetail["envelopes"][number]["signers"][number]>();},
recordRead:async(id,identifier,now)=>{await (await nativeSvc.accessEvent({identifier,id,now})).run();},
shares:(id:string)=>{return db
          .prepare(
            legalItemOrderedSql("id,email,document_id,user_id,expires_at,revoked_at","legal_shares","created_at DESC",100),
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
        fill(contract.holdPreservable,{holdOwnTarget:legalHoldOwnTargetSql('h')}),
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
