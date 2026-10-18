import {withTalentPurchasedCommand} from "./purchases";
import "server-only";
import type {WorkContext} from "@/server/modules/identity_access/public/authorization";
import type {SqlDatabase} from "@/server/modules/runtime/public/runtime/types";
import {holdingResources} from "@/server/modules/runtime/public/runtime";
import {requirePermission} from "@/server/modules/identity_access/public/iam-permissions";
import {hasModuleAccess} from "@/data/module-access";
import {hasPermission} from "@/lib/identity_access/permissions";
import {executeCommand,replayCommand} from "@/server/modules/workflow/public/commands";
import {ApiError} from "@/server/modules/runtime/public/http";
import {BusinessRuleError} from "../../shared";
import {createTalentRequisitionApplication, type TalentRequisitionInputPorts} from "../application/requisition";
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
