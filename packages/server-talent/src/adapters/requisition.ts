import {withTalentPurchasedCommand} from "@erpx/server-talent/adapters/purchases";
import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {hasModuleAccess} from "@erpx/catalogue/module-access";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {ApiError} from "@erpx/server-runtime/public/http";
import {BusinessRuleError} from "@erpx/server-shared/index";
import {createTalentRequisitionApplication, type TalentRequisitionInputPorts} from "@erpx/server-talent/application/requisition";
async function createTalentRequisitionOwned(a:WorkContext,raw:unknown,key:string,provided?:SqlDatabase){
 let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;const database=()=>resource??=(holdingResources(a.holdingId).then(x=>x.write));
 const ports:TalentRequisitionInputPorts={clock:{now:()=>Date.now()},branchId:a.branchId,
 async authorize(){requirePermission(a,"records.write");if(!hasModuleAccess(a.modules,"talent"))throw new ApiError(403,"MODULE_FORBIDDEN","Your current role cannot access Talent.");if(!hasPermission(a,"records.review"))throw new ApiError(403,"TALENT_PRIVACY_FORBIDDEN","Candidate data requires Talent reviewer access.");},
 async replay(key,operation,id,input){return replayCommand(await database(),a,key,operation,id,input);},
 async persist(command){const write=await database(),i=command.input,id=command.id,key=command.key,op=command.operation,now=command.now,result=command.result;
 return executeCommand(write, a, key, op, i, id, 1, result, [
    write
      .prepare(
        "INSERT INTO talent_requisitions VALUES(?,?,?,?,?,?,?,?,'draft',?,NULL,1,?,?)",
      )
      .bind(
        id,
        a.companyId,
        a.branchId,
        a.departmentId,
        i.jobTitle,
        i.openings,
        i.targetStartDate,
        i.description,
        a.userId,
        now,
        now,
      ),
    write
      .prepare(
        "INSERT INTO talent_requisition_events VALUES(?,?,1,'create',?,'Hiring need drafted','draft',?)",
      )
      .bind(`talent-req-event-${key}`, id, a.userId, now),
  ]);

 }
 };try{return await createTalentRequisitionApplication(ports)(raw,key);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}
}

export function createTalentRequisition(a:WorkContext,raw:unknown,key:string){return withTalentPurchasedCommand(a,{key,operation:"talent.requisition.create",capability:"recruit.positions",purpose:"new-work",target:{kind:"requisition",id:`talent-req-${key}`},creating:true,permissions:["records.write","records.review"]},db=>createTalentRequisitionOwned(a,raw,key,db));}
