import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import type {QualityProductEntryScope} from "@erpx/quality-server/public/rules/product-entry";
import {assignmentReadSql} from "@erpx/identity-access-server/public/record-source-authority";
import {purchasedCapabilityPredicate} from "@erpx/commercial-server/public/purchased-capability-sql";
import { scheduledFeaturePredicate } from "@erpx/pms-server/public/feature-availability";
import {qualityDisposalCountRows} from "@erpx/inventory-server/adapters/quality-count-purchases";
import {clockCteSql,sourceScopeRowsSql} from './read-tail-sql';
/** Only a proven existing Sales disposal Count. This does not grant Supply's
 * collection, preparation, amendment or recount capabilities. */
export async function salesQualityCountProductEntry(database:SqlDatabase,scope:QualityProductEntryScope,id:string){
 if(scope.departmentId||!Number.isSafeInteger(scope.assignmentVersion)||scope.assignmentVersion<1)return null;
 const row=await database.prepare(`WITH ${clockCteSql('count_entry_clock')}
 SELECT 1 FROM (${sourceScopeRowsSql(qualityDisposalCountRows,'source')} WHERE kind='customer-return') count_entry CROSS JOIN count_entry_clock
 WHERE count_entry.id=? AND count_entry.company_id=? AND (?::text IS NULL OR count_entry.branch_id=?)
 AND ${assignmentReadSql("count_entry","inventory.read")} AND EXISTS(SELECT 1 FROM assignments WHERE id=? AND version=?)
 AND ${scheduledFeaturePredicate("count_entry.company_id","inventory")} AND ${scheduledFeaturePredicate("count_entry.company_id","quality")}
 AND ${purchasedCapabilityPredicate("count_entry.company_id","sales.orders","history","count_entry_clock.now_ms")} AND ${purchasedCapabilityPredicate("count_entry.company_id","inventory.stock-core","history","count_entry_clock.now_ms")}`)
 .bind(id,scope.companyId,scope.branchId,scope.branchId,scope.assignmentId,scope.userId,scope.role,scope.assignmentId,scope.assignmentVersion).first();
 return row?{capability:"sales.orders",purchasePurpose:"history" as const}:null;
}
