import {financeCommandReceipts} from "../../workflow/public/finance-command-receipts";
import { fiscalCloseSourceSql } from "@/server/modules/finance/public/finance-fiscal-close-source";
import { financeTransaction } from "@/server/modules/finance/public/finance-transactions";
import type { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { isTransactionalSqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { executeCommand, replayCommand, assertion } from "@/server/modules/workflow/public/commands";
import { ApiError } from "@/server/modules/runtime/public/http";
import { samePerson } from "@/lib/iam";
import { sha256 } from "@/server/modules/runtime/public/crypto";
import { financeReviewSchema } from "@/server/modules/finance/public/finance-books";
import { requireFinanceTool } from "@/server/modules/finance/public/finance-company-tools";
import { requireFinanceMoneyScale } from "@/server/modules/finance/public/finance-money-policy";
import {createFinancePlanning,type PlanningRepository} from '../application/planning';
import type {PlanRow,PlanAccount as Account,LedgerLine,budgetEditionSchema,forecastEditionSchema,financialReportSchema} from '../domain/planning';
export {budgetEditionSchema,forecastEditionSchema,financialReportSchema} from '../domain/planning';
function nativePlanningRepository(db:SqlDatabase):PlanningRepository{const write=db;return {moneyScale:(companyId,currency)=>requireFinanceMoneyScale(db,companyId,currency),tool:companyId=>requireFinanceTool(db,companyId,'planning'),replay:<T>(actor:WorkContext,key:string,op:string,id:string,input:unknown)=>replayCommand<T>(db,actor,key,op,id,input),book:async(actor,chart)=>db
    .prepare(
      "SELECT base_currency FROM finance_charts WHERE id=? AND company_id=? AND status='active'",
    )
    .bind(chart, actor.companyId)
    .first<{base_currency:string}>(),
accounts:async(actor,chart)=>(await db
      .prepare(
        "SELECT id,code,name,category FROM finance_accounts WHERE chart_id=? AND company_id=? AND status='active'",
      )
      .bind(chart, actor.companyId)
      .all<Account>()).results,
dimension:async(actor,code,value)=>db
          .prepare(
            "SELECT 1 FROM finance_dimension_definitions d JOIN finance_dimension_values v ON v.dimension_id=d.id WHERE d.company_id=? AND d.code=? AND d.status='active' AND v.code=? AND v.status='active'",
          )
          .bind(actor.companyId, code, value)
          .first(),
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
        "SELECT * FROM finance_plan_editions WHERE id=? AND company_id=? AND chart_id=? AND kind=? AND status='approved'",
      )
      .bind(parentId, actor.companyId, chartId, kind)
      .first<PlanRow>(),
saveCommit:async(facts)=>{const {actor,kind,base,content,key,op,input,edition,id,hash,now}=facts;const guards=[];if(base.parentId){    guards.push(
      assertion(
        write,
        key,
        "parent",
        "EXISTS(SELECT 1 FROM finance_plan_editions WHERE id=? AND company_id=? AND version=? AND status='approved')",
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
      ...guards,
      write
        .prepare(
          "INSERT INTO finance_plan_editions(id,company_id,chart_id,kind,name,parent_id,edition,content_json,content_hash,prepared_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          id,
          actor.companyId,
          base.chartId,
          kind,
          base.name,
          base.parentId ?? null,
          edition,
          JSON.stringify(content),
          hash,
          actor.userId,
          now,
        ),
    ],
  );},
bankAccount:async(actor,id)=>write
          .prepare(
            "SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND control_kind='bank'",
          )
          .bind(id, actor.companyId)
          .first(),
decisionRow:async(actor,id)=>write
    .prepare("SELECT * FROM finance_plan_editions WHERE id=? AND company_id=?")
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
        "EXISTS(SELECT 1 FROM finance_plan_editions WHERE id=? AND company_id=? AND version=? AND status='draft')",
        [id, actor.companyId, input.expectedVersion],
      ),
      write
        .prepare(
          "UPDATE finance_plan_editions SET status=?,version=version+1,reviewed_by=?,review_reason=?,reviewed_at=? WHERE id=?",
        )
        .bind(
          action === "approve" ? "approved" : "rejected",
          actor.userId,
          input.reason,
          now,
          id,
        ),
    ],
  );},
editions:async(actor,chartId)=>(await write
        .prepare(
          "SELECT * FROM finance_plan_editions WHERE company_id=? AND chart_id=? ORDER BY created_at DESC,id",
        )
        .bind(actor.companyId, chartId)
        .all()).results,
snapshots:async(actor,chartId)=>(await write.prepare("SELECT s.id,s.definition_id,s.start_date,s.end_date,s.source_hash,s.created_at,p.name FROM finance_report_snapshots s JOIN finance_plan_editions p ON p.id=s.definition_id WHERE s.company_id=? AND p.chart_id=? ORDER BY s.created_at DESC LIMIT 200").bind(actor.companyId,chartId).all()).results,
plan:async(actor,id,kind)=>db
    .prepare(
      "SELECT * FROM finance_plan_editions WHERE id=? AND company_id=? AND kind=? AND status='approved'",
    )
    .bind(id, actor.companyId, kind)
    .first<PlanRow>(),
actual:async(actor,chartId,accountId,month,dims)=>write
          .prepare(
            `SELECT COALESCE(SUM(jl.base_debit_minor-jl.base_credit_minor),0) value FROM finance_journal_lines jl JOIN finance_journals j ON j.id=jl.journal_id WHERE j.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND NOT ${fiscalCloseSourceSql()} AND jl.account_id=? AND left(j.entry_date,7)=? AND jl.dimension_json::jsonb @> ?::jsonb`,
          )
          .bind(actor.companyId, chartId, accountId, month, dims)
          .first("value"),
category:async(accountId)=>write
        .prepare("SELECT category FROM finance_accounts WHERE id=?")
        .bind(accountId)
        .first<{ category: string }>(),
ledger:async(actor,chartId,endDate)=>(await db
      .prepare(
        `SELECT l.id,j.id AS "journalId",l.account_id AS "accountId",j.entry_date AS "entryDate",l.base_debit_minor AS debit,l.base_credit_minor AS credit,l.dimension_json AS dimensions,j.source_type AS "sourceType",CASE WHEN ${fiscalCloseSourceSql()} THEN 1 ELSE 0 END AS "isFiscalClose",j.source_id AS "sourceId" FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND j.entry_date<=? ORDER BY j.entry_date,j.id,l.ordinal`,
      )
      .bind(actor.companyId, chartId, endDate)
      .all<LedgerLine>()).results,
lockChart:async(chartId)=>db
      .prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))")
      .bind(`finance-book-${chartId}`)
      .run(),
snapshotCommit:async(facts)=>{const {actor,id,input,key,op,row,snapshotId,result,sourceHash,csv,exportHash,now}=facts;return executeCommand(db, actor, key, op, input, id, row.version, result, [
      db
        .prepare(
          "INSERT INTO finance_report_snapshots(id,company_id,definition_id,start_date,end_date,result_json,source_hash,created_by,created_at,export_content,export_sha256) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          snapshotId,
          actor.companyId,
          id,
          input.startDate,
          input.endDate,
          JSON.stringify(result),
          sourceHash,
          actor.userId,
          now,
          csv,
          exportHash,
        ),
    ]);},
exportRow:async(actor,id)=>write.prepare("SELECT result_json,export_content,export_sha256 FROM finance_report_snapshots WHERE id=? AND company_id=?").bind(id,actor.companyId).first<{result_json:string;export_content:string|null;export_sha256:string|null}>(),
recordRow:async(actor,id)=>write.prepare('SELECT * FROM finance_plan_editions WHERE id=? AND company_id=?').bind(id,actor.companyId).first<PlanRow>(),
budgetSources:async(actor,chartId,accountId,month,dims,page)=>(await write.prepare(`SELECT jl.*,j.id journal_id,j.journal_number,j.entry_date,j.currency,j.status FROM finance_journal_lines jl JOIN finance_journals j ON j.id=jl.journal_id WHERE j.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND NOT ${fiscalCloseSourceSql()} AND jl.account_id=? AND left(j.entry_date,7)=? AND jl.dimension_json::jsonb @> ?::jsonb ORDER BY j.entry_date,j.id,jl.ordinal LIMIT 31 OFFSET ?`).bind(actor.companyId,chartId,accountId,month,dims,(page-1)*30).all()).results,
sourceSnapshot:async(actor,id)=>write.prepare('SELECT s.*,p.content_json,p.chart_id FROM finance_report_snapshots s JOIN finance_plan_editions p ON p.id=s.definition_id WHERE s.id=? AND s.company_id=? AND p.company_id=?').bind(id,actor.companyId,actor.companyId).first<{result_json:string;content_json:string;source_hash:string;chart_id:string;start_date:string;end_date:string}>(),
sourceJournals:async(actor,selected)=>(await write.prepare(`SELECT id journal_id,journal_number,entry_date,currency,status,(SELECT SUM(l.base_debit_minor) FROM finance_journal_lines l WHERE l.journal_id=finance_journals.id) base_debit_minor,(SELECT SUM(l.base_credit_minor) FROM finance_journal_lines l WHERE l.journal_id=finance_journals.id) base_credit_minor FROM finance_journals WHERE company_id=? AND id IN (${selected.map(()=>'?').join(',')}) ORDER BY entry_date,id`).bind(actor.companyId,...selected).all()).results}}
function nativePlanning(provided?:SqlDatabase){const database=async(actor:WorkContext)=>provided??(await holdingResources(actor.holdingId)).write;return createFinancePlanning({error:(...args)=>new ApiError(...args),permission:requirePermission,samePerson,now:Date.now,id:()=>crypto.randomUUID(),hash:sha256,read:async actor=>nativePlanningRepository(await database(actor)),transaction:async(actor,run)=>financeTransaction(await database(actor),db=>run(nativePlanningRepository(db))),snapshotTransaction:async(actor,run)=>{const write=await database(actor);if(!isTransactionalSqlDatabase(write))throw new ApiError(503,'TRANSACTION_REQUIRED','Report snapshots require PostgreSQL.');return financeTransaction(write,db=>run(nativePlanningRepository(db)))}})}
export const prepareBudgetEdition=(actor: WorkContext,input: z.input<typeof budgetEditionSchema>,key: string)=>nativePlanning().prepareBudgetEdition(actor,input,key);
export const importBudgetCsv=(actor: WorkContext,input: {
    chartId: string;
    name: string;
    year: number;
    csv: string;
    parentId?: string;
    expectedVersion?: number;
  },key: string)=>nativePlanning().importBudgetCsv(actor,input,key);
export const prepareForecastEdition=(actor: WorkContext,input: z.input<typeof forecastEditionSchema>,key: string)=>nativePlanning().prepareForecastEdition(actor,input,key);
export const prepareFinancialReport=(actor: WorkContext,input: z.input<typeof financialReportSchema>,key: string)=>nativePlanning().prepareFinancialReport(actor,input,key);
export const decideFinancePlan=(actor: WorkContext,id: string,input: z.infer<typeof financeReviewSchema>,action: "approve" | "reject",key: string)=>nativePlanning().decideFinancePlan(actor,id,input,action,key);
export const financePlanningWorkspace=(actor: WorkContext,chartId: string)=>nativePlanning().financePlanningWorkspace(actor,chartId);
export const budgetActuals=(actor: WorkContext,id: string)=>nativePlanning().budgetActuals(actor,id);
export const snapshotFinancialReport=(actor: WorkContext,id: string,input: { startDate: string; endDate: string },key: string)=>nativePlanning().snapshotFinancialReport(actor,id,input,key);
export const exportFinancialReport=(actor:WorkContext,id:string)=>nativePlanning().exportFinancialReport(actor,id);
export const financePlanRecord=(actor:WorkContext,id:string)=>nativePlanning().financePlanRecord(actor,id);
export const budgetSourceRecords=(actor:WorkContext,id:string,index:number,page=1)=>nativePlanning().budgetSourceRecords(actor,id,index,page);
export const financialReportSourceRecords=(actor:WorkContext,id:string,args:Record<string,string>,page=1)=>nativePlanning().financialReportSourceRecords(actor,id,args,page);
