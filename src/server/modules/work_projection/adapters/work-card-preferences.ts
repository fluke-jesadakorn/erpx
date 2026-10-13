import "server-only";
import type {WorkContext} from "../../identity_access/public/authorization";
import type {SqlDatabase,SqlStatement} from "../../runtime/public/runtime/types";
import {ApiError} from "../../runtime/public/http";
import {assertion,executeCommand,replayCommand} from "../../workflow/public/commands";
import {serviceAuthoritySnapshot,serviceMember} from "../../support/public/service-access";
import type {WorkCardId} from "@/lib/work_projection/work-cards";
import type {WorkCardPreferenceInput} from "../domain/work-card-preferences";
import {createWorkCardPreferenceService,type WorkCardPreferenceUnit} from "../application/work-card-preferences";
export {workCardPreferenceSchema} from "../domain/work-card-preferences";
function unit(db:SqlDatabase):WorkCardPreferenceUnit<SqlStatement>{return {member:actor=>serviceMember(db,{company_id:actor.companyId,branch_id:actor.branchId,department_id:actor.departmentId},actor.userId),preference:actor=>db.prepare("SELECT pinned_ids_json,version FROM work_card_preferences WHERE company_id=? AND user_id=?").bind(actor.companyId,actor.userId).first(),authority:companyId=>serviceAuthoritySnapshot(db,companyId),versionGuard:(key,actor,version)=>assertion(db,key,"work-cards-version","COALESCE((SELECT version FROM work_card_preferences WHERE company_id=? AND user_id=?),0)=?",[actor.companyId,actor.userId,version]),savePreference:(actor,result,now)=>db.prepare("INSERT INTO work_card_preferences(company_id,user_id,pinned_ids_json,version,updated_at) VALUES(?,?,?,?,?) ON CONFLICT(company_id,user_id) DO UPDATE SET pinned_ids_json=excluded.pinned_ids_json,version=excluded.version,updated_at=excluded.updated_at").bind(actor.companyId,actor.userId,result.pinnedIds===null?null:JSON.stringify(result.pinnedIds),result.version,now),replayCommand:(...args)=>replayCommand(db,...args),executeCommand:(...args)=>executeCommand(db,...args)};}
const service=createWorkCardPreferenceService<SqlStatement>({clock:{now:()=>Date.now()},error:(status,code,message)=>new ApiError(status,code,message)});
export function readWorkCardPreferences(db:SqlDatabase,actor:WorkContext){return service.readWorkCardPreferences(unit(db),actor);}
export function saveWorkCardPreferences(db:SqlDatabase,actor:WorkContext,input:WorkCardPreferenceInput,key:string,available:()=>Promise<WorkCardId[]>){return service.saveWorkCardPreferences(unit(db),actor,input,key,available);}
