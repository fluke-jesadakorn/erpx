import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {financePackageEnforcementMode,checkPurchasedCapability} from "@erpx/server-commercial/public/purchase-command-boundary";
import {purchasedCapabilityPredicate,packageClockSql} from "@erpx/server-commercial/public/purchased-capability-sql";
import {supplierBatchOwnedSql} from "@erpx/server-finance/adapters/supplier-payment-purchases";
/** Calendar keeps the original person/node/date filters. The owner capability
 * and fresh assignment predicates run before its LIMIT, on the actual Company. */
export async function financeBankCalendarPurchase(database:SqlDatabase,actor:WorkContext,alias:"o"|"b"){
 const mode=await financePackageEnforcementMode();if(mode==="disabled")return {clock:"",where:"TRUE",args:[] as (string|null)[]};
 if(mode==="shadow")await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:"finance.banking",purpose:"history"});
 const purchase=mode==="enforced"?`(SELECT (${purchasedCapabilityPredicate(`${alias}.company_id`,"finance.banking","history","finance_calendar_clock.now_ms")}${alias==="b"?` OR (${purchasedCapabilityPredicate("b.company_id","finance.inventory-valuation","history","finance_calendar_clock.now_ms")} AND ${supplierBatchOwnedSql("b")})`:""}) FROM finance_calendar_clock)`:"TRUE";
 const branch=alias==="o"?"o.branch_id":"NULL::text",department=alias==="o"?"o.department_id":"NULL::text";
 return {clock:`WITH finance_calendar_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) `,where:`${purchase} AND ${alias}.company_id=? AND (?::text IS NULL OR ${branch}=?) AND (?::text IS NULL OR ${department}=?) AND (SELECT ${assignmentReadSql("finance_calendar_scope","finance.read")} FROM (SELECT ${alias}.company_id,${branch} branch_id,${department} department_id) finance_calendar_scope)`,args:[actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId,actor.assignmentId,actor.userId,actor.role]};
}
