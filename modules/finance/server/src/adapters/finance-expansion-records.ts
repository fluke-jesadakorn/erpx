import { withFinancePurchasedRead } from "@erpx/finance-server/adapters/purchase-boundary";
import 'server-only';
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import {requirePermission} from "@erpx/identity-access-server/public/iam-permissions";
import {holdingResources} from "../../../../../platform/runtime/server/src/public/runtime";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {nativeFinanceReadAll,nativeFinanceReadFirst,FINANCE_RD_ASSET_POLICY_ROW,FINANCE_RD_ASSET_POLICY_RUNS,FINANCE_RD_INSTRUMENT_EDITION,FINANCE_RD_INSTRUMENT_RUNS} from "./native-finance-read";
import {financeCompanyTools} from "@erpx/finance-server/public/finance-company-tools";
import {instrumentWorkspace} from "@erpx/finance-server/public/finance-instruments";
import {assetPolicyWorkspace} from "@erpx/finance-server/public/finance-asset-policies";
import {financeReviewerNamesSql} from "@erpx/people-payroll-server/public/finance-reviewer-sources";
import {createFinanceExpansionRecords} from "@erpx/finance-server/application/expansion-records";
import type {FinanceExpansionKind,FinanceRecordRow as Row} from "@erpx/finance-server/domain/record-kinds";
export {isFinanceExpansionKind,type FinanceExpansionKind} from "@erpx/finance-server/domain/record-kinds";
const tables={finance_instrument:'finance_instrument_editions',finance_instrument_run:'finance_instrument_runs',finance_asset_policy:'finance_asset_policy_editions',finance_asset_policy_run:'finance_asset_policy_runs'} as const;
export async function financeExpansionRecord(actor:WorkContext,kind:FinanceExpansionKind,id:string){
 return withFinancePurchasedRead(actor,async database=>{
 let write:SqlDatabase;
 return createFinanceExpansionRecords({error:(...args)=>new ApiError(...args),permission:requirePermission,ready:async actor=>{write=database??(await holdingResources(actor.holdingId)).write},
 row:async(actor,kind,id)=>write.prepare(`SELECT r.*,${financeReviewerNamesSql} FROM ${tables[kind]} r WHERE r.company_id=? AND r.id=?`).bind(actor.companyId,id).first<Row>(),
 tools:financeCompanyTools,instrumentWorkspace,assetPolicyWorkspace,
 instrumentEdition:async(actor,id)=>nativeFinanceReadFirst<Row>(FINANCE_RD_INSTRUMENT_EDITION,[actor.companyId,id],write),
 instrumentRuns:async(actor,id)=>(await nativeFinanceReadAll<Row>(FINANCE_RD_INSTRUMENT_RUNS,[actor.companyId,id],write)).results,
 assetPolicy:async(actor,id)=>nativeFinanceReadFirst<Row>(FINANCE_RD_ASSET_POLICY_ROW,[actor.companyId,id],write),
 assetPolicyRuns:async(actor,id)=>(await nativeFinanceReadAll<Row>(FINANCE_RD_ASSET_POLICY_RUNS,[actor.companyId,id],write)).results
 }).financeExpansionRecord(actor,kind,id);
 },kind==="finance_instrument"||kind==="finance_instrument_run"?"finance.advanced":"finance.accounting");
}
