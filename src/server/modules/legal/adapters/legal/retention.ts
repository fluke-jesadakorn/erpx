import "server-only";
import {samePerson} from "@/lib/iam";
import type {SqlDatabase,SqlStatement} from "../../../runtime/public/runtime/types";
import type {WorkContext} from "../../../identity_access/public/authorization";
import {requirePermission} from "../../../identity_access/public/iam-permissions";
import {ApiError} from "../../../runtime/public/http";
import {executeCommand,replayCommand,assertion} from "../../../workflow/public/commands";
import {access,guards,lock} from "./access";
import {legalDestructionManifest} from "./destruction-sources";
import {createLegalRetentionApplication} from "../../application/retention";
import type {LegalWrite} from "../../application/service";
export {drainLegalDestruction} from "./destruction-worker";
function application(db:SqlDatabase,a:WorkContext){const nativeWrites=new WeakMap<LegalWrite,SqlStatement>();const stage=(statement:SqlStatement):LegalWrite=>{const write:LegalWrite={owner:"legal"};nativeWrites.set(write,statement);return write;};return createLegalRetentionApplication({actor:a,access:(id,p)=>access(db,a,id,p),require:p=>requirePermission(a,p),samePerson:id=>samePerson(a,id),manifest:(id,company)=>legalDestructionManifest(db,id,company),clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},error:(status,code,message)=>new ApiError(status,code,message),repository:{
policies:(facts:{companyId:string;})=>db.prepare("SELECT * FROM legal_retention_policies WHERE company_id=? ORDER BY created_at DESC").bind(facts.companyId).all(),
assignment:(facts:{id:string;})=>db.prepare("SELECT * FROM legal_retention_assignments WHERE item_id=?").bind(facts.id).first(),
requests:(facts:{id:string;})=>db.prepare("SELECT * FROM legal_destructions WHERE item_id=? ORDER BY created_at DESC").bind(facts.id).all<{id:string}&Record<string,unknown>>(),
createPolicy:(facts:{key:string;companyId:string;name:string;days:number;reason:string;actorId:string;now:number;})=>stage(db.prepare("INSERT INTO legal_retention_policies(id,company_id,name,days,basis,state,created_by,created_at) VALUES(?,?,?,?,?,'draft',?,?)").bind(facts.key,facts.companyId,facts.name,facts.days,facts.reason,facts.actorId,facts.now)),
policy:(facts:{policyId:string | undefined;companyId:string;})=>db.prepare("SELECT * FROM legal_retention_policies WHERE id=? AND company_id=?").bind(facts.policyId,facts.companyId).first<{
          id: string;
          created_by: string;
          state: string;
          version: number;
        }>(),
decidePolicy:(facts:{fact0:"approved" | "retired";actorId:string;id:string;})=>stage(db.prepare("UPDATE legal_retention_policies SET state=?,approved_by=?,version=version+1 WHERE id=?").bind(facts.fact0,facts.actorId,facts.id)),
approvedPolicy:(facts:{policyId:string | undefined;companyId:string;})=>db.prepare("SELECT * FROM legal_retention_policies WHERE id=? AND company_id=? AND state='approved'").bind(facts.policyId,facts.companyId).first<{ id: string; days: number; version: number }>(),
assignRetention:(facts:{id:string;id2:string;version:number;updated_at:number;timezone:string;days:number;timezone2:string;actorId:string;now:number;})=>stage(db.prepare("INSERT INTO legal_retention_assignments(item_id,policy_id,policy_version,eligible_at,assigned_by,created_at) VALUES(?,?,?,(extract(epoch FROM (((to_timestamp(?::double precision/1000) AT TIME ZONE ?)::date + ?::int)::timestamp AT TIME ZONE ?))*1000)::bigint,?,?) ON CONFLICT(item_id) DO UPDATE SET policy_id=excluded.policy_id,policy_version=excluded.policy_version,eligible_at=excluded.eligible_at,assigned_by=excluded.assigned_by").bind(facts.id,facts.id2,facts.version,facts.updated_at,facts.timezone,facts.days,facts.timezone2,facts.actorId,facts.now)),
eligibleAssignment:(facts:{id:string;now:number;})=>db.prepare("SELECT r.* FROM legal_retention_assignments r JOIN legal_retention_policies p ON p.id=r.policy_id AND p.version=r.policy_version AND p.state='approved' WHERE r.item_id=? AND r.eligible_at<=?").bind(facts.id,facts.now).first<{ policy_id: string; policy_version: number }>(),
requestDestruction:(facts:{key:string;id:string;policy_id:string;policy_version:number;actorId:string;reason:string;manifestJson:string;now:number;})=>stage(db.prepare("INSERT INTO legal_destructions(id,item_id,policy_id,policy_version,requested_by,state,reason,manifest,created_at) VALUES(?,?,?,?,?,'requested',?,?::jsonb,?)").bind(facts.key,facts.id,facts.policy_id,facts.policy_version,facts.actorId,facts.reason,facts.manifestJson,facts.now)),
destruction:(facts:{requestId:string | undefined;id:string;})=>db.prepare("SELECT * FROM legal_destructions WHERE id=? AND item_id=?").bind(facts.requestId,facts.id).first<{
        id: string;
        state: string;
        requested_by: string;
        approved_by: string | null;
      }>(),
changeDestruction:(facts:{state:string;fact1:string | null;fact2:number | null;id:string;})=>stage(db.prepare("UPDATE legal_destructions SET state=?,approved_by=coalesce(?,approved_by),approved_at=coalesce(?,approved_at),version=version+1,error=NULL,attempts=0 WHERE id=?").bind(facts.state,facts.fact1,facts.fact2,facts.id)),
advanceRecord:(facts:{now:number;id:string;})=>stage(db.prepare("UPDATE legal_items SET version=version+1,updated_at=? WHERE id=?").bind(facts.now,facts.id)),
requireRetentionPolicyVersion:(key,values)=>stage(assertion(db,key,"retention-policy-version","EXISTS(SELECT 1 FROM legal_retention_policies WHERE id=? AND version=?)",values)),
requireRetentionNoHold:(key,values)=>stage(assertion(db,key,"retention-no-hold","NOT EXISTS(SELECT 1 FROM legal_hold_targets t JOIN legal_holds h ON h.id=t.hold_id WHERE t.item_id=? AND h.state!='released')",values)),
guardChanges:(id,version,key,edit,allowFrozen)=>guards(db,a,id,version,key,edit,allowFrozen).map(stage),companyLock:()=>stage(lock(db,a.companyId)),event:(id,action,note,version,identifier,now)=>stage(db.prepare("INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) VALUES(?,?,?,?,?,?,?)").bind(identifier,id,action,a.userId,note,version,now)),replay:(key,operation,id,input)=>replayCommand(db,a,key,operation,id,input),commit:(key,operation,input,id,version,result,writes)=>executeCommand(db,a,key,operation,input,id,version,result,writes.map(write=>{const statement=nativeWrites.get(write);if(!statement)throw new Error("LEGAL_RETENTION_REPOSITORY_MISMATCH");return statement;})),
}});}
export function retentionView(db:SqlDatabase,a:WorkContext,id?:string){return application(db,a).retentionView(id);}
export function retentionCommand(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){return application(db,a).retentionCommand(raw,key);}
