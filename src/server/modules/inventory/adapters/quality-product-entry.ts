import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import type {QualityProductEntryScope} from "../../quality/public/rules/product-entry";
import {assignmentReadSql} from "../../identity_access/public/record-source-authority";
import {packageClockSql,purchasedCapabilityPredicate} from "../../commercial/public/purchased-capability-sql";
import {scheduledFeaturePredicate} from "../../pms/public/hospitality-rollout";
import {qualityDisposalCountRows} from "./quality-count-purchases";
/** Only a proven existing Sales disposal Count. This does not grant Supply's
 * collection, preparation, amendment or recount capabilities. */
export async function salesQualityCountProductEntry(database:SqlDatabase,scope:QualityProductEntryScope,id:string){
 if(scope.departmentId||!Number.isSafeInteger(scope.assignmentVersion)||scope.assignmentVersion<1)return null;
 const row=await database.prepare(`WITH count_entry_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms)
 SELECT 1 FROM (SELECT source.*,NULL::text department_id FROM (${qualityDisposalCountRows}) source WHERE kind='customer-return') count_entry CROSS JOIN count_entry_clock
 WHERE count_entry.id=? AND count_entry.company_id=? AND (?::text IS NULL OR count_entry.branch_id=?)
 AND ${assignmentReadSql("count_entry","inventory.read")} AND EXISTS(SELECT 1 FROM assignments WHERE id=? AND version=?)
 AND ${scheduledFeaturePredicate("count_entry.company_id","inventory")} AND ${scheduledFeaturePredicate("count_entry.company_id","quality")}
 AND ${purchasedCapabilityPredicate("count_entry.company_id","sales.orders","history","count_entry_clock.now_ms")} AND ${purchasedCapabilityPredicate("count_entry.company_id","inventory.stock-core","history","count_entry_clock.now_ms")}`)
 .bind(id,scope.companyId,scope.branchId,scope.branchId,scope.assignmentId,scope.userId,scope.role,scope.assignmentId,scope.assignmentVersion).first();
 return row?{capability:"sales.orders",purchasePurpose:"history" as const}:null;
}
