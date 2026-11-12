import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {serviceMember} from "@erpx/server-support/public/service-access";
import {createWorkCardPreferenceService,type WorkCardPreferenceUnit} from "@erpx/server-work_projection/application/work-card-preferences";
export {workCardPreferenceSchema} from "@erpx/server-work_projection/domain/work-card-preferences";
function unit(db:SqlDatabase):WorkCardPreferenceUnit{return {member:(actor:WorkContext)=>serviceMember(db,{company_id:actor.companyId,branch_id:actor.branchId,department_id:actor.departmentId},actor.userId),preference:(actor:WorkContext)=>db.prepare("SELECT pinned_ids_json,version FROM work_card_preferences WHERE company_id=? AND user_id=?").bind(actor.companyId,actor.userId).first<{pinned_ids_json:string|null;version:number}>()};}
const service=createWorkCardPreferenceService({error:(status,code,message)=>new ApiError(status,code,message)});
export function readWorkCardPreferences(db:SqlDatabase,actor:WorkContext){return service.readWorkCardPreferences(unit(db),actor);}
