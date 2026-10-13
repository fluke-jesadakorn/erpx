import "server-only";
import type {WorkContext} from "@/server/modules/identity_access/public/authorization";
import type {SqlDatabase} from "@/server/modules/runtime/public/runtime/types";
import {holdingResources} from "@/server/modules/runtime/public/runtime";
import {requirePermission} from "@/server/modules/identity_access/public/iam-permissions";
import {hasModuleAccess} from "@/data/module-access";
import {executeCommand,replayCommand} from "@/server/modules/workflow/public/commands";
import {ApiError} from "@/server/modules/runtime/public/http";
import {decimalToMinor} from "@/lib/finance/finance-money";
import {BusinessRuleError} from "../../shared";
import {createBenefitPlanApplication, type BenefitPlanInputPorts} from "../application/plan";
export async function createBenefitPlan(a:WorkContext,raw:unknown,key:string){
 let resource:Promise<SqlDatabase>|null=null;const database=()=>resource??=(holdingResources(a.holdingId).then(x=>x.write));
 const ports:BenefitPlanInputPorts={clock:{now:()=>Date.now()},
 async authorize(){requirePermission(a,"records.write");if(!hasModuleAccess(a.modules,"benefits"))throw new ApiError(403,"MODULE_FORBIDDEN","Your current role cannot access Benefits.");requirePermission(a, "benefits.review");},
 async replay(key,operation,id,input){return replayCommand(await database(),a,key,operation,id,input);},
 async persist(command){const write=await database(),i=command.input,id=command.id,key=command.key,op=command.operation,now=command.now,result=command.result;
 return executeCommand(write, a, key, op, i, id, 1, result, [
    write
      .prepare(
        "INSERT INTO benefit_plans(id,company_id,name,provider,coverage_type,currency,employee_deduction_minor,employer_cost_minor,effective_from,effective_to,status,created_by,reviewed_by,version,created_at,updated_at,required_eligibility_code) VALUES(?,?,?,?,?,?,?,?,?,?, 'draft',?,NULL,1,?,?,?)",
      )
      .bind(
        id,
        a.companyId,
        i.name,
        i.provider,
        i.coverageType,
        i.currency,
        decimalToMinor(i.employeeDeduction),
        decimalToMinor(i.employerCost),
        i.effectiveFrom,
        i.effectiveTo ?? null,
        a.userId,
        now,
        now,
        i.requiredEligibilityCode??null,
      ),
    write
      .prepare(
        "INSERT INTO benefit_plan_events VALUES(?,?,1,'create',?,'Plan drafted for independent review','draft',?)",
      )
      .bind(`benefit-plan-event-${key}`, id, a.userId, now),
  ]);

 }
 };try{return await createBenefitPlanApplication(ports)(raw,key);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}
}
