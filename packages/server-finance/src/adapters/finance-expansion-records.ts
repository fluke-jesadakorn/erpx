import { withFinancePurchasedRead } from "@erpx/server-finance/adapters/purchase-boundary";
import 'server-only';
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {financeCompanyTools} from "@erpx/server-finance/public/finance-company-tools";
import {instrumentWorkspace} from "@erpx/server-finance/public/finance-instruments";
import {assetPolicyWorkspace} from "@erpx/server-finance/public/finance-asset-policies";
import {financeReviewerNamesSql} from "@erpx/server-people_payroll/public/finance-reviewer-sources";
import {createFinanceExpansionRecords} from "@erpx/server-finance/application/expansion-records";
import type {FinanceExpansionKind,FinanceRecordRow as Row} from "@erpx/server-finance/domain/record-kinds";
export {isFinanceExpansionKind,type FinanceExpansionKind} from "@erpx/server-finance/domain/record-kinds";
const tables={finance_instrument:'finance_instrument_editions',finance_instrument_run:'finance_instrument_runs',finance_asset_policy:'finance_asset_policy_editions',finance_asset_policy_run:'finance_asset_policy_runs'} as const;
export async function financeExpansionRecord(actor:WorkContext,kind:FinanceExpansionKind,id:string){
 return withFinancePurchasedRead(actor,async database=>{
 let write:SqlDatabase;
 return createFinanceExpansionRecords({error:(...args)=>new ApiError(...args),permission:requirePermission,ready:async actor=>{write=database??(await holdingResources(actor.holdingId)).write},
 row:async(actor,kind,id)=>write.prepare(`SELECT r.*,${financeReviewerNamesSql} FROM ${tables[kind]} r WHERE r.company_id=? AND r.id=?`).bind(actor.companyId,id).first<Row>(),
 tools:financeCompanyTools,instrumentWorkspace,assetPolicyWorkspace,
 instrumentEdition:async(actor,id)=>write.prepare('SELECT * FROM finance_instrument_editions WHERE company_id=? AND id=?').bind(actor.companyId,id).first<Row>(),
 instrumentRuns:async(actor,id)=>(await write.prepare('SELECT * FROM finance_instrument_runs WHERE company_id=? AND instrument_id=? ORDER BY ordinal,id').bind(actor.companyId,id).all<Row>()).results,
 assetPolicy:async(actor,id)=>write.prepare('SELECT * FROM finance_asset_policy_editions WHERE company_id=? AND id=?').bind(actor.companyId,id).first<Row>(),
 assetPolicyRuns:async(actor,id)=>(await write.prepare('SELECT * FROM finance_asset_policy_runs WHERE company_id=? AND policy_id=? ORDER BY ordinal,id').bind(actor.companyId,id).all<Row>()).results
 }).financeExpansionRecord(actor,kind,id);
 },kind==="finance_instrument"||kind==="finance_instrument_run"?"finance.advanced":"finance.accounting");
}
