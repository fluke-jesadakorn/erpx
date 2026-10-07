import "server-only";
import {samePerson} from "@/lib/iam";
import type {SqlDatabase,SqlStatement} from "../../../runtime/public/runtime/types";
import {ApiError} from "../../../runtime/public/http";
import type {WorkContext} from "../../../identity_access/public/authorization";
import {requirePermission} from "../../../identity_access/public/iam-permissions";
import {executeCommand,replayCommand} from "../../../workflow/public/commands";
import {access,guards,eligible,personGuard,lock} from "./access";
import type {LegalWrite} from "../../application/service";
import {featureSpecs,featureKinds,type FeatureRow} from "@/lib/legal/features";
import {createLegalFeaturesApplication} from "../../application/features";
function application(db:SqlDatabase,a:WorkContext){const nativeWrites=new WeakMap<LegalWrite,SqlStatement>();const stage=(statement:SqlStatement):LegalWrite=>{const write:LegalWrite={owner:"legal"};nativeWrites.set(write,statement);return write;};
return createLegalFeaturesApplication({actor:a,access:(id,p)=>access(db,a,id,p),eligible:(id,item,p)=>eligible(db,a,id,item,p),samePerson:id=>samePerson(a,id),require:permission=>requirePermission(a,permission),clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},error:(status,code,message)=>new ApiError(status,code,message),repository:{
featureCount:(facts:{id:string;search:string;search2:string;capability:(typeof featureKinds)[number];})=>db.prepare(`SELECT count(*) n FROM ${featureSpecs[facts.capability].table} WHERE item_id=? AND (?='' OR strpos(lower(title||' '||data::text),lower(?))>0)`).bind(facts.id,facts.search,facts.search2).first("n"),
features:(facts:{id:string;search:string;search2:string;fact3:number;capability:(typeof featureKinds)[number];})=>db.prepare(`SELECT * FROM ${featureSpecs[facts.capability].table} WHERE item_id=? AND (?='' OR strpos(lower(title||' '||data::text),lower(?))>0) ORDER BY updated_at DESC,id LIMIT 50 OFFSET ?`).bind(facts.id,facts.search,facts.search2,facts.fact3).all<FeatureRow>(),
feature:(facts:{featureId:string;itemId:string;capability:(typeof featureKinds)[number];})=>db.prepare(`SELECT * FROM ${featureSpecs[facts.capability].table} WHERE id=? AND item_id=?`).bind(facts.featureId,facts.itemId).first<FeatureRow>(),
ownerMembership:(facts:{itemId:string;ownerId:string;})=>db.prepare("SELECT 1 FROM legal_members WHERE item_id=? AND user_id=?").bind(facts.itemId,facts.ownerId).first(),
attachedDocument:(facts:{documentId:string;itemId:string;})=>db.prepare("SELECT 1 FROM legal_documents WHERE id=? AND item_id=?").bind(facts.documentId,facts.itemId).first(),
createFeature:(facts:{id:string;itemId:string;category:"case_event" | "compliance_action" | "privacy_case" | "corporate_action" | "ip_action" | "counsel_entry" | "schedule" | "custody" | "policy_ack" | "intake";title:string;owner_id:string;due_date:string | null;state:string;dataJson:string;actorId:string;actorId2:string;updated_at:number;capability:(typeof featureKinds)[number];})=>stage(db.prepare(`INSERT INTO ${featureSpecs[facts.capability].table}(id,item_id,category,title,owner_id,due_date,state,version,data,created_by,updated_by,updated_at) VALUES(?,?,?,?,?,?,?,1,?::jsonb,?,?,?)`).bind(facts.id,facts.itemId,facts.category,facts.title,facts.owner_id,facts.due_date,facts.state,facts.dataJson,facts.actorId,facts.actorId2,facts.updated_at)),
conflictCleared:(facts:{itemId:string;})=>db.prepare("SELECT 1 FROM legal_counsel_entries WHERE item_id=? AND data->>'entryType'='conflict' AND state IN ('approved','closed')").bind(facts.itemId).first(),
approvedFees:(facts:{itemId:string;id:string;})=>db.prepare("SELECT coalesce(sum(coalesce(nullif(data->>'amount',''),'0')::bigint),0) amount FROM legal_counsel_entries WHERE item_id=? AND id<>? AND state IN ('approved','billed','closed') AND data->>'entryType' IN ('fee','invoice')").bind(facts.itemId,facts.id).first<{ amount: number }>(),
updateFeature:(facts:{state:string;dataJson:string;actorId:string;updated_at:number;id:string;version:number;capability:(typeof featureKinds)[number];})=>stage(db.prepare(`UPDATE ${featureSpecs[facts.capability].table} SET state=?,data=?::jsonb,version=version+1,updated_by=?,updated_at=? WHERE id=? AND version=?`).bind(facts.state,facts.dataJson,facts.actorId,facts.updated_at,facts.id,facts.version)),
recordFeatureHistory:(facts:{identifier:string;itemId:string;id:string;category:"case_event" | "compliance_action" | "privacy_case" | "corporate_action" | "ip_action" | "counsel_entry" | "schedule" | "custody" | "policy_ack" | "intake";version:number;snapshotJson:string;actorId:string;reason:string;now:number;})=>stage(db.prepare("INSERT INTO legal_feature_history(id,item_id,feature_id,category,version,snapshot,actor_id,reason,created_at) VALUES(?,?,?,?,?,?::jsonb,?,?,?)").bind(facts.identifier,facts.itemId,facts.id,facts.category,facts.version,facts.snapshotJson,facts.actorId,facts.reason,facts.now)),
advanceRecord:(facts:{now:number;itemId:string;})=>stage(db.prepare("UPDATE legal_items SET version=version+1,updated_at=? WHERE id=?").bind(facts.now,facts.itemId)),
guardChanges:(id,version,key,edit)=>guards(db,a,id,version,key,edit).map(stage),companyLock:()=>stage(lock(db,a.companyId)),personGuard:(key,id,item,permission)=>stage(personGuard(db,a,key,id,item,permission)),
event:(id,action,note,version,identifier,now)=>stage(db.prepare("INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) VALUES(?,?,?,?,?,?,?)").bind(identifier,id,action,a.userId,note,version,now)),
replay:(key,operation,id,input)=>replayCommand(db,a,key,operation,id,input),
commit:(key,operation,input,id,version,result,writes)=>executeCommand(db,a,key,operation,input,id,version,result,writes.map(write=>{const statement=nativeWrites.get(write);if(!statement)throw new Error("LEGAL_REPOSITORY_MISMATCH");return statement;})),
}});}
export function featureDetail(db:SqlDatabase,a:WorkContext,id:string,query:Record<string,unknown>={}){return application(db,a).featureDetail(id,query);}
export function featureCommand(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){return application(db,a).featureCommand(raw,key);}
