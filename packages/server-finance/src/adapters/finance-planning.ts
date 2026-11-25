import { versionStatusGuardSql } from './finance-read-tail-r52-sql';
import { PLAN_EDITION_POINT_SQL } from './finance-read-tail-r38-sql';
import { planEditionPointSql } from './finance-read-tail-r48-sql';
import { withFinancePurchasedCommand, withFinancePurchasedRead } from "@erpx/server-finance/adapters/purchase-boundary";
import { FINANCE_ADVISORY_XACT_LOCK_SQL } from "./advisory-lock-dedupe-sql";
import { ACTIVE_CHART_BASE_CURRENCY_SQL } from "./chart-base-currency-dedupe-sql";
import {financeCommandReceipts} from "@erpx/server-workflow/public/finance-command-receipts";
import { fiscalCloseSourceSql } from "@erpx/server-finance/public/finance-fiscal-close-source";
import { financeTransaction } from "@erpx/server-finance/public/finance-transactions";
import { nativeFinanceReadAll, nativeFinanceReadFirst, FINANCE_RD_PLAN_ACCOUNTS, FINANCE_RD_PLAN_DIMENSION, FINANCE_RD_PLAN_BANK_ACCOUNT, FINANCE_RD_PLAN_EDITIONS, FINANCE_RD_PLAN_SNAPSHOTS, FINANCE_RD_PLAN_CATEGORY, FINANCE_RD_PLAN_EXPORT_ROW, FINANCE_RD_PLAN_SOURCE_SNAPSHOT } from "./native-finance-read";
import type { z } from "zod";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { isTransactionalSqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { ApiError } from "@erpx/server-runtime/public/http";
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { financeReviewSchema } from "@erpx/server-finance/public/finance-books";
import { requireFinanceTool } from "@erpx/server-finance/public/finance-company-tools";
import { requireFinanceMoneyScale } from "@erpx/server-finance/public/finance-money-policy";
import {createFinancePlanning,type PlanningRepository} from '@erpx/server-finance/application/planning';
import type {PlanRow,PlanAccount as Account,LedgerLine,budgetEditionSchema,forecastEditionSchema,financialReportSchema} from '@erpx/server-finance/domain/planning';
import { financePlanning } from '@erpx/server-collaboration/adapters/native-finance-planning';
export {budgetEditionSchema,forecastEditionSchema,financialReportSchema} from '@erpx/server-finance/domain/planning';
function nativePlanningRepository(db:SqlDatabase):PlanningRepository{const write=db;return {moneyScale:(companyId,currency)=>requireFinanceMoneyScale(db,companyId,currency),tool:companyId=>requireFinanceTool(db,companyId,'planning'),replay:<T>(actor:WorkContext,key:string,op:string,id:string,input:unknown)=>replayCommand<T>(db,actor,key,op,id,input),book:async(actor,chart)=>db
    .prepare(
      ACTIVE_CHART_BASE_CURRENCY_SQL,
    )
    .bind(chart, actor.companyId)
    .first<{base_currency:string}>(),
accounts:async(actor,chart)=>(await nativeFinanceReadAll<Account>(FINANCE_RD_PLAN_ACCOUNTS, [chart, actor.companyId], db)).results,
dimension:async(actor,code,value)=>nativeFinanceReadFirst(FINANCE_RD_PLAN_DIMENSION, [actor.companyId, code, value], db),
commandReplay:async<Result>(actor:WorkContext,key:string,op:string,input:unknown)=>{  const receipt = await financeCommandReceipts(write).record(key);
  if (receipt) {
    const prior = await replayCommand<Result>(
      write,
      actor,
      key,
      op,
      receipt.record_id,
      input,
    );
    if (prior) return prior;
  }
return null;},
parent:async(actor,parentId,chartId,kind)=>write
      .prepare(
        planEditionPointSql(" AND chart_id=? AND kind=? AND status='approved'"),
      )
      .bind(parentId, actor.companyId, chartId, kind)
      .first<PlanRow>(),
saveCommit:async(facts)=>{const {actor,kind,base,content,key,op,input,edition,id,hash,now}=facts;const guards=[];if(base.parentId){    guards.push(
      assertion(
        write,
        key,
        "parent",
        versionStatusGuardSql('finance_plan_editions', "'approved'", ''),
        [base.parentId, actor.companyId, base.expectedVersion!],
      ),
    );}return executeCommand(
    write,
    actor,
    key,
    op,
    input,
    id,
    1,
    { id, version: 1, status: "draft", edition, contentHash: hash },
    [
      ...guards,          await financePlanning(db, actor.companyId).createEdition({ id, companyId: actor.companyId, chartId: base.chartId, kind, name: base.name, parentId: base.parentId ?? null, edition, contentJson: JSON.stringify(content), hash, userId: actor.userId, now }),
    ],
  );},
bankAccount:async(actor,id)=>nativeFinanceReadFirst(FINANCE_RD_PLAN_BANK_ACCOUNT, [id, actor.companyId], write),
decisionRow:async(actor,id)=>write
    .prepare(PLAN_EDITION_POINT_SQL)
    .bind(id, actor.companyId)
    .first<PlanRow>(),
decisionCommit:async(facts)=>{const {actor,id,input,action,key,op,row,now}=facts;return executeCommand(
    write,
    actor,
    key,
    op,
    input,
    id,
    row.version + 1,
    {
      id,
      status: action === "approve" ? "approved" : "rejected",
      version: row.version + 1,
    },
    [
      assertion(
        write,
        key,
        "version",
        versionStatusGuardSql('finance_plan_editions', "'draft'", ''),
        [id, actor.companyId, input.expectedVersion],
      ),await financePlanning(db, actor.companyId).reviewEdition({ status: action === "approve" ? "approved" : "rejected", userId: actor.userId, reason: input.reason, now, id }),
    ],
  );},
editions:async(actor,chartId)=>(await nativeFinanceReadAll(FINANCE_RD_PLAN_EDITIONS, [actor.companyId, chartId], write)).results,
snapshots:async(actor,chartId)=>(await nativeFinanceReadAll(FINANCE_RD_PLAN_SNAPSHOTS, [actor.companyId, chartId], write)).results,
plan:async(actor,id,kind)=>db
    .prepare(
      planEditionPointSql(" AND kind=? AND status='approved'"),
    )
    .bind(id, actor.companyId, kind)
    .first<PlanRow>(),
actual:async(actor,chartId,accountId,month,dims)=>write
          .prepare(
            `SELECT COALESCE(SUM(jl.base_debit_minor-jl.base_credit_minor),0) value FROM finance_journal_lines jl JOIN finance_journals j ON j.id=jl.journal_id WHERE j.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND NOT ${fiscalCloseSourceSql()} AND jl.account_id=? AND left(j.entry_date,7)=? AND jl.dimension_json::jsonb @> ?::jsonb`,
          )
          .bind(actor.companyId, chartId, accountId, month, dims)
          .first("value"),
category:async(accountId)=>nativeFinanceReadFirst<{ category: string }>(FINANCE_RD_PLAN_CATEGORY, [accountId], write),
ledger:async(actor,chartId,endDate)=>(await db
      .prepare(
        `SELECT l.id,j.id AS "journalId",l.account_id AS "accountId",j.entry_date AS "entryDate",l.base_debit_minor AS debit,l.base_credit_minor AS credit,l.dimension_json AS dimensions,j.source_type AS "sourceType",CASE WHEN ${fiscalCloseSourceSql()} THEN 1 ELSE 0 END AS "isFiscalClose",j.source_id AS "sourceId" FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND j.entry_date<=? ORDER BY j.entry_date,j.id,l.ordinal`,
      )
      .bind(actor.companyId, chartId, endDate)
      .all<LedgerLine>()).results,
lockChart:async(chartId)=>db
      .prepare(FINANCE_ADVISORY_XACT_LOCK_SQL)
      .bind(`finance-book-${chartId}`)
      .run(),
snapshotCommit:async(facts)=>{const {actor,id,input,key,op,row,snapshotId,result,sourceHash,csv,exportHash,now}=facts;return executeCommand(db, actor, key, op, input, id, row.version, result, [await financePlanning(db, actor.companyId).createSnapshot({ snapshotId, companyId: actor.companyId, definitionId: id, startDate: input.startDate, endDate: input.endDate, resultJson: JSON.stringify(result), sourceHash, userId: actor.userId, now, csv, exportHash }),
    ]);},
exportRow:async(actor,id)=>nativeFinanceReadFirst<{result_json:string;export_content:string|null;export_sha256:string|null}>(FINANCE_RD_PLAN_EXPORT_ROW, [id, actor.companyId], write),
recordRow:async(actor,id)=>write.prepare(PLAN_EDITION_POINT_SQL).bind(id,actor.companyId).first<PlanRow>(),
budgetSources:async(actor,chartId,accountId,month,dims,page)=>(await write.prepare(`SELECT jl.*,j.id journal_id,j.journal_number,j.entry_date,j.currency,j.status FROM finance_journal_lines jl JOIN finance_journals j ON j.id=jl.journal_id WHERE j.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND NOT ${fiscalCloseSourceSql()} AND jl.account_id=? AND left(j.entry_date,7)=? AND jl.dimension_json::jsonb @> ?::jsonb ORDER BY j.entry_date,j.id,jl.ordinal LIMIT 31 OFFSET ?`).bind(actor.companyId,chartId,accountId,month,dims,(page-1)*30).all()).results,
sourceSnapshot:async(actor,id)=>nativeFinanceReadFirst<{result_json:string;content_json:string;source_hash:string;chart_id:string;start_date:string;end_date:string}>(FINANCE_RD_PLAN_SOURCE_SNAPSHOT, [id, actor.companyId, actor.companyId], write),
sourceJournals:async(actor,selected)=>(await write.prepare(`SELECT id journal_id,journal_number,entry_date,currency,status,(SELECT SUM(l.base_debit_minor) FROM finance_journal_lines l WHERE l.journal_id=finance_journals.id) base_debit_minor,(SELECT SUM(l.base_credit_minor) FROM finance_journal_lines l WHERE l.journal_id=finance_journals.id) base_credit_minor FROM finance_journals WHERE company_id=? AND id IN (${selected.map(()=>'?').join(',')}) ORDER BY entry_date,id`).bind(actor.companyId,...selected).all()).results}}
function nativePlanning(provided?:SqlDatabase,owned=false){const database=async(actor:WorkContext)=>provided??(await holdingResources(actor.holdingId)).write;return createFinancePlanning({error:(...args)=>new ApiError(...args),permission:requirePermission,samePerson,now:Date.now,id:()=>crypto.randomUUID(),hash:sha256,read:async actor=>nativePlanningRepository(await database(actor)),transaction:async(actor,run)=>owned?run(nativePlanningRepository(await database(actor))):financeTransaction(await database(actor),db=>run(nativePlanningRepository(db))),snapshotTransaction:async(actor,run)=>{const write=await database(actor);if(owned)return run(nativePlanningRepository(write));if(!isTransactionalSqlDatabase(write))throw new ApiError(503,'TRANSACTION_REQUIRED','Report snapshots require PostgreSQL.');return financeTransaction(write,db=>run(nativePlanningRepository(db)))}})}
export const prepareBudgetEdition=(actor: WorkContext,input: z.input<typeof budgetEditionSchema>,key: string)=>withFinancePurchasedCommand(actor,{key,operation:"finance.budget.prepare",purpose:"new-work"},tx=>nativePlanning(tx,Boolean(tx)).prepareBudgetEdition(actor,input,key));
export const importBudgetCsv=(actor: WorkContext,input: {
    chartId: string;
    name: string;
    year: number;
    csv: string;
    parentId?: string;
    expectedVersion?: number;
  },key: string)=>withFinancePurchasedCommand(actor,{key,operation:"finance.budget.prepare",purpose:"new-work"},tx=>nativePlanning(tx,Boolean(tx)).importBudgetCsv(actor,input,key));
export const prepareForecastEdition=(actor: WorkContext,input: z.input<typeof forecastEditionSchema>,key: string)=>withFinancePurchasedCommand(actor,{key,operation:"finance.forecast.prepare",purpose:"new-work"},tx=>nativePlanning(tx,Boolean(tx)).prepareForecastEdition(actor,input,key));
export const prepareFinancialReport=(actor: WorkContext,input: z.input<typeof financialReportSchema>,key: string)=>withFinancePurchasedCommand(actor,{key,operation:"finance.report.prepare",purpose:"new-work"},tx=>nativePlanning(tx,Boolean(tx)).prepareFinancialReport(actor,input,key));
export const decideFinancePlan=(actor: WorkContext,id: string,input: z.infer<typeof financeReviewSchema>,action: "approve" | "reject",key: string)=>withFinancePurchasedCommand(actor,{key,operation:`finance.plan.${action}`,purpose:"settle-existing"},tx=>nativePlanning(tx,Boolean(tx)).decideFinancePlan(actor,id,input,action,key));
export const financePlanningWorkspace=(actor: WorkContext,chartId: string)=>withFinancePurchasedRead(actor,db=>nativePlanning(db).financePlanningWorkspace(actor,chartId));
export const budgetActuals=(actor: WorkContext,id: string)=>withFinancePurchasedRead(actor,db=>nativePlanning(db).budgetActuals(actor,id));
export const snapshotFinancialReport=(actor: WorkContext,id: string,input: { startDate: string; endDate: string },key: string)=>withFinancePurchasedCommand(actor,{key,operation:"finance.report.snapshot",purpose:"settle-existing"},tx=>nativePlanning(tx,Boolean(tx)).snapshotFinancialReport(actor,id,input,key));
export const exportFinancialReport=(actor:WorkContext,id:string)=>withFinancePurchasedRead(actor,db=>nativePlanning(db).exportFinancialReport(actor,id));
export const financePlanRecord=(actor:WorkContext,id:string)=>withFinancePurchasedRead(actor,db=>nativePlanning(db).financePlanRecord(actor,id));
export const budgetSourceRecords=(actor:WorkContext,id:string,index:number,page=1)=>withFinancePurchasedRead(actor,db=>nativePlanning(db).budgetSourceRecords(actor,id,index,page));
export const financialReportSourceRecords=(actor:WorkContext,id:string,args:Record<string,string>,page=1)=>withFinancePurchasedRead(actor,db=>nativePlanning(db).financialReportSourceRecords(actor,id,args,page));
