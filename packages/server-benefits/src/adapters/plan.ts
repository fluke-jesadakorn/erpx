import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {hasModuleAccess} from "@erpx/catalogue/module-access";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {ApiError} from "@erpx/server-runtime/public/http";
import {decimalToMinor} from "@erpx/feature-finance/lib/finance-money";
import {BusinessRuleError} from "@erpx/server-shared/index";
import {createBenefitPlanApplication, type BenefitPlanInputPorts} from "@erpx/server-benefits/application/plan";
import {withBenefitPurchasedCommand} from "@erpx/server-benefits/adapters/purchases";
export function createBenefitPlan(a:WorkContext,raw:unknown,key:string){return withBenefitPurchasedCommand(a,{key,operation:"benefits.plan.create",target:{kind:"plan",id:`benefit-plan-${key}`},purpose:"new-work",review:true},database=>nativeCreateBenefitPlan(a,raw,key,database));}
async function nativeCreateBenefitPlan(a:WorkContext,raw:unknown,key:string,provided:SqlDatabase){
 let resource:Promise<SqlDatabase>|null=null;const database=()=>provided?Promise.resolve(provided):resource??=(holdingResources(a.holdingId).then(x=>x.write));
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
