import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/final-root-owner-projection-sources";
import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {hasModuleAccess} from "@erpx/catalogue/module-access";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {ApiError} from "@erpx/server-runtime/public/http";
import {BusinessRuleError} from "@erpx/server-shared/index";
import {createQueuePolicyApplication, type QueuePolicyPorts} from "@erpx/server-operations/application/queue-policy";
export async function draftQueuePolicy(a:WorkContext,raw:unknown,key:string){
let resource:Promise<SqlDatabase>|null=null;const database=()=>resource??=(holdingResources(a.holdingId).then(x=>x.write));
const ports:QueuePolicyPorts={clock:{now:()=>Date.now()},async authorize(){requirePermission(a,"records.write");if(!hasModuleAccess(a.modules,"operations"))throw new ApiError(403,"MODULE_FORBIDDEN","Operations is unavailable to the selected role.");requirePermission(a,"records.review");},
async replay(key,op,id,input){return replayCommand(await database(),a,key,op,id,input);},
async persist(command){const write=await database();const {key,operation:op,id:recordId,input,now}=command;
return executeCommand(write,a,key,op,input,recordId,1,{id:recordId,status:"draft",version:1},[
  write.prepare(`SELECT id FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=? AND kind='company' AND status='active' FOR UPDATE`).bind(a.companyId),
  assertion(write,key,"company-policy-scope",`EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) assignments WHERE id=? AND user_id=? AND company_id=? AND scope_kind='holding' AND iam_role_allows(role,company_id,'records.review'))`,[a.assignmentId,a.userId,a.companyId]),
  write.prepare("INSERT INTO operations_queue_policies(id,company_id,area,name,revision,source_kinds_json,target_minutes,created_by,created_at,updated_at) SELECT ?,?,?,?,COALESCE(max(revision),0)+1,?,?,?,?,? FROM operations_queue_policies WHERE company_id=? AND area=?")
   .bind(recordId,a.companyId,input.area,input.name,JSON.stringify(command.sourceKinds),input.targetMinutes,a.userId,now,now,a.companyId,input.area),
 ]);
}
};try{return await createQueuePolicyApplication(ports)(raw,key);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}
}
