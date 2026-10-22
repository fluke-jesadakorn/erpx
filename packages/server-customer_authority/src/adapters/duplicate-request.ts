import { finalRootOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/final-root-owner-projection-sources";
import {prepareCustomerDuplicateRequestNotice1} from "@erpx/server-collaboration/public/customer-authority-notifications";
import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {ApiError} from "@erpx/server-runtime/public/http";
import {BusinessRuleError} from "@erpx/server-shared/index";
import {createDuplicateRequestApplication, type DuplicateRequestPorts} from "@erpx/server-customer_authority/application/duplicate-request";
export async function requestCustomerDuplicate(a:WorkContext,raw:unknown,key:string){
let resource:Promise<SqlDatabase>|null=null;const database=()=>resource??=(holdingResources(a.holdingId).then(x=>x.write));
const ports:DuplicateRequestPorts={clock:{now:()=>Date.now()},companyScope:!a.branchId&&!a.departmentId,async authorize(){requirePermission(a,"commerce.sales");requirePermission(a,"commerce.read");},async replay(key,op,id,input){return replayCommand(await database(),a,key,op,id,input);},async persist(command){const write=await database(),{key,operation:op,id,input,pair,now}=command;
return executeCommand(write,a,key,op,input,id,1,{id,version:1,status:'pending'},[
 assertion(write,key,'customers',`(SELECT COUNT(*)=2 FROM (${financeProjectionSources.finance_parties}) finance_parties WHERE company_id=? AND kind='customer' AND status='active' AND id IN (?,?))`,[a.companyId,pair[0],pair[1]]),
 write.prepare('INSERT INTO customer_duplicate_reviews(id,company_id,left_customer_id,right_customer_id,evidence,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)').bind(id,a.companyId,pair[0],pair[1],input.evidence,a.userId,now,now),
 write.prepare("INSERT INTO customer_duplicate_events VALUES(?,?,1,'request',?,?,NULL,?)").bind(`customer-duplicate-event-${key}`,id,a.userId,input.evidence,now),
 prepareCustomerDuplicateRequestNotice1(write,key,id)
 ]);
}};try{return await createDuplicateRequestApplication(ports)(raw,key);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}
}
