import {financeCommandReceipts} from "../../workflow/public/finance-command-receipts";
import { financeTransaction } from "@/server/modules/finance/public/finance-transactions";
import type { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { isTransactionalSqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { assertion, executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { ApiError } from "@/server/modules/runtime/public/http";
import { samePerson } from "@/lib/iam";
import { sha256 } from "@/server/modules/runtime/public/crypto";
import { requireFinanceMoneyScale } from "@/server/modules/finance/public/finance-money-policy";

import {createFinanceBooks,type BookReviewRepository} from '../application/books';
import type {OpeningRow,dimensionSchema,bookRuleSchema} from '../domain/books';
import {financeReviewSchema} from '../domain/review';
export {dimensionSchema,bookRuleSchema} from '../domain/books';
export {financeReviewSchema} from '../domain/review';
const openingSql = `SELECT l.account_id AS "accountId",l.dimension_json AS dimensions,SUM(l.base_debit_minor-l.base_credit_minor) AS minor
 FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
 WHERE j.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND j.entry_date<?
 GROUP BY l.account_id,l.dimension_json HAVING SUM(l.base_debit_minor-l.base_credit_minor)<>0 ORDER BY l.account_id,l.dimension_json`;
async function opening(
  db: SqlDatabase,
  company: string,
  chart: string,
  date: string,
) {
  return (
    await db.prepare(openingSql).bind(company, chart, date).all<OpeningRow>()
  ).results;
}
async function commandReplay<Result=Record<string,unknown>>(
  db: SqlDatabase,
  actor: WorkContext,
  key: string,
  op: string,
  input: unknown,
) {
  const receipt = await financeCommandReceipts(db).record(key);
  return receipt
    ? replayCommand<Result>(
        db,
        actor,
        key,
        op,
        receipt.record_id,
        input,
      )
    : null;
}
function nativeBookReview(db:SqlDatabase):BookReviewRepository{return {replay:(actor,key,op,id,input)=>replayCommand(db,actor,key,op,id,input),opening:(companyId,chartId,date)=>opening(db,companyId,chartId,date),rule:async(actor,id)=>db
      .prepare(
        "SELECT * FROM finance_book_rules WHERE id=? AND company_id=? FOR UPDATE",
      )
      .bind(id, actor.companyId)
      .first<{
        status: string;
        version: number;
        primary_chart_id: string;
        secondary_chart_id: string;
        effective_date: string;
        opening_json: string;
        opening_hash: string;
        opening_period_id: string;
        prepared_by: string;
      }>(),
lockPrimary:async(chartId)=>db
      .prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))")
      .bind(`finance-book-${chartId}`)
      .run(),
sourcePostedSince:async(chartId,effectiveDate)=>db
          .prepare(
            "SELECT 1 FROM finance_journals WHERE chart_id=? AND status IN ('posted','reversed') AND entry_date>=?",
          )
          .bind(chartId, effectiveDate)
          .first(),
alreadyActive:async(chartId)=>db
        .prepare(
          "SELECT 1 FROM finance_book_rules WHERE secondary_chart_id=? AND status='approved'",
        )
        .bind(chartId)
        .first(),
targetPosted:async(chartId)=>db
          .prepare(
            "SELECT 1 FROM finance_journals WHERE chart_id=? AND status IN ('posted','reversed')",
          )
          .bind(chartId)
          .first(),
decisionCommit:async(facts)=>{const {actor,id,input,action,key,op,rule,now,rows,createOpening}=facts;const steps=[];if(action==='approve'){
      steps.push(
        assertion(
          db,
          key,
          "opening-period",
          "EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?)",
          [
            rule.opening_period_id,
            actor.companyId,
            rule.effective_date,
            rule.effective_date,
          ],
        ),
      );
      if (createOpening) {
        const jid = `book-opening-${id}`;
        steps.push(
          db
            .prepare(
              "INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,is_opening,created_by,posted_by,posted_at,created_at,updated_at) SELECT ?,company_id,id,?,?,?,base_currency,1000000,'book_opening',?,'Reviewed secondary book opening','posted',1,?,?,?,?,? FROM finance_charts WHERE id=?",
            )
            .bind(
              jid,
              rule.opening_period_id,
              jid,
              rule.effective_date,
              id,
              rule.prepared_by,
              actor.userId,
              now,
              now,
              now,
              rule.secondary_chart_id,
            ),
        );
        rows.forEach((r, i) =>
          steps.push(
            db
              .prepare(
                "INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) SELECT ?,?,secondary_account_id,?,?,?,?,?,? FROM finance_book_account_mappings WHERE rule_id=? AND primary_account_id=?",
              )
              .bind(
                `${jid}-${i}`,
                jid,
                Math.max(r.minor, 0),
                Math.max(-r.minor, 0),
                Math.max(r.minor, 0),
                Math.max(-r.minor, 0),
                r.dimensions,
                i,
                id,
                r.accountId,
              ),
          ),
        );
      }}
    steps.push(
      db
        .prepare(
          "UPDATE finance_book_rules SET status=?,version=version+1,reviewed_by=?,review_reason=?,reviewed_at=? WHERE id=?",
        )
        .bind(
          action === "approve" ? "approved" : "rejected",
          actor.userId,
          input.reason,
          now,
          id,
        ),
    );
    return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      rule.version + 1,
      {
        id,
        status: action === "approve" ? "approved" : "rejected",
        version: rule.version + 1,
      },
      steps,
    );}}}
function nativeBooks(provided?:SqlDatabase){let write:SqlDatabase;return createFinanceBooks({error:(...args)=>new ApiError(...args),permission:requirePermission,samePerson,ready:async actor=>{write=provided??(await holdingResources(actor.holdingId)).write},now:Date.now,id:()=>crypto.randomUUID(),hash:sha256,commandReplay:<Result>(actor:WorkContext,key:string,op:string,input:unknown)=>commandReplay<Result>(write,actor,key,op,input),moneyScale:(companyId,currency)=>requireFinanceMoneyScale(write,companyId,currency),opening:(companyId,chartId,date)=>opening(write,companyId,chartId,date),reviewTransaction:async(actor,run)=>{if(!isTransactionalSqlDatabase(write))throw new ApiError(503,'TRANSACTION_REQUIRED','Book approval requires PostgreSQL transactions.');return financeTransaction(write,db=>run(nativeBookReview(db)))},workspace:async(actor)=>{  const [definitions, values, rules, mappings, charts, accounts, periods] =
    await Promise.all([
      write
        .prepare(
          "SELECT * FROM finance_dimension_definitions WHERE company_id=? ORDER BY code",
        )
        .bind(actor.companyId)
        .all(),
      write
        .prepare(
          "SELECT * FROM finance_dimension_values WHERE company_id=? ORDER BY dimension_id,code",
        )
        .bind(actor.companyId)
        .all(),
      write
        .prepare(
          "SELECT r.*,p.name primary_name,s.name secondary_name FROM finance_book_rules r JOIN finance_charts p ON p.id=r.primary_chart_id JOIN finance_charts s ON s.id=r.secondary_chart_id WHERE r.company_id=? ORDER BY r.effective_date DESC,r.created_at DESC",
        )
        .bind(actor.companyId)
        .all(),
      write
        .prepare(
          "SELECT m.* FROM finance_book_account_mappings m JOIN finance_book_rules r ON r.id=m.rule_id WHERE r.company_id=? ORDER BY m.rule_id,m.primary_account_id",
        )
        .bind(actor.companyId)
        .all(),
      write
        .prepare(
          "SELECT id,name,base_currency FROM finance_charts WHERE company_id=? AND status='active' ORDER BY name",
        )
        .bind(actor.companyId)
        .all(),
      write
        .prepare(
          "SELECT id,chart_id,code,name FROM finance_accounts WHERE company_id=? AND status='active' ORDER BY chart_id,code",
        )
        .bind(actor.companyId)
        .all(),
      write
        .prepare(
          "SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date DESC",
        )
        .bind(actor.companyId)
        .all(),
    ]);
  return {
    definitions: definitions.results,
    values: values.results,
    rules: rules.results,
    mappings: mappings.results,
    charts: charts.results,
    accounts: accounts.results,
    periods: periods.results,
  };},
dimensionCommit:async(facts)=>{const {actor,input,key,op,id,now,valueIds}=facts;let valueIndex=0;return executeCommand(
    write,
    actor,
    key,
    op,
    input,
    id,
    1,
    { id, status: "active", version: 1 },
    [
      write
        .prepare(
          "INSERT INTO finance_dimension_definitions(id,company_id,code,name,required,created_by,created_at) VALUES(?,?,?,?,?,?,?)",
        )
        .bind(
          id,
          actor.companyId,
          input.code,
          input.name,
          input.required ? 1 : 0,
          actor.userId,
          now,
        ),
      ...input.values.map((v) =>
        write
          .prepare(
            "INSERT INTO finance_dimension_values(id,dimension_id,company_id,code,name,created_at) VALUES(?,?,?,?,?,?)",
          )
          .bind(valueIds[valueIndex++], id, actor.companyId, v.code, v.name, now),
      ),
    ],
  );},
charts:async(actor,primaryChartId,secondaryChartId)=>(await write
      .prepare(
        "SELECT * FROM finance_charts WHERE company_id=? AND id IN (?,?) AND status='active'",
      )
      .bind(actor.companyId, primaryChartId, secondaryChartId)
      .all<{ id: string; base_currency: string }>()).results,
chain:async(actor,primaryChartId,secondaryChartId)=>write
      .prepare(
        "SELECT 1 FROM finance_book_rules WHERE company_id=? AND status='approved' AND (secondary_chart_id=? OR primary_chart_id=?)",
      )
      .bind(actor.companyId, primaryChartId, secondaryChartId)
      .first(),
existingRule:async(secondaryChartId)=>write
    .prepare(
      "SELECT primary_chart_id FROM finance_book_rules WHERE secondary_chart_id=? AND status='approved' ORDER BY effective_date DESC LIMIT 1",
    )
    .bind(secondaryChartId)
    .first<{ primary_chart_id: string }>(),
targetPosted:async(secondaryChartId)=>write
      .prepare(
        "SELECT 1 FROM finance_journals WHERE chart_id=? AND status IN ('posted','reversed')",
      )
      .bind(secondaryChartId)
      .first(),
openingPeriod:async(actor,periodId,effectiveDate)=>write
    .prepare(
      "SELECT 1 FROM finance_periods WHERE company_id=? AND id=? AND state='open' AND start_date<=? AND end_date>=?",
    )
    .bind(
      actor.companyId,
      periodId,
      effectiveDate,
      effectiveDate,
    )
    .first(),
accounts:async(actor,primaryChartId,secondaryChartId)=>(await write
      .prepare(
        "SELECT id,chart_id,category,control_kind,currency,status FROM finance_accounts WHERE company_id=? AND chart_id IN (?,?)",
      )
      .bind(actor.companyId, primaryChartId, secondaryChartId)
      .all<{
        id: string;
        chart_id: string;
        category: string;
        control_kind: string;
        currency: string | null;
        status: string;
      }>()).results,
mappingChanged:async(secondaryChartId,primaryAccountId,secondaryAccountId)=>write
        .prepare(
          "SELECT 1 FROM finance_book_account_mappings m JOIN finance_book_rules r ON r.id=m.rule_id WHERE r.secondary_chart_id=? AND r.status='approved' AND m.primary_account_id=? AND m.secondary_account_id<>?",
        )
        .bind(secondaryChartId, primaryAccountId, secondaryAccountId)
        .first(),
prepareCommit:async(facts)=>{const {actor,input,key,op,id,now,openingHash,rows}=facts;return executeCommand(
    write,
    actor,
    key,
    op,
    input,
    id,
    1,
    { id, status: "draft", version: 1, openingHash, opening: rows },
    [
      write
        .prepare(
          "INSERT INTO finance_book_rules(id,company_id,primary_chart_id,secondary_chart_id,effective_date,prepared_by,reason,opening_json,opening_hash,opening_period_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          id,
          actor.companyId,
          input.primaryChartId,
          input.secondaryChartId,
          input.effectiveDate,
          actor.userId,
          input.reason,
          JSON.stringify(rows),
          openingHash,
          input.openingPeriodId,
          now,
        ),
      ...input.mappings.map((m) =>
        write
          .prepare(
            "INSERT INTO finance_book_account_mappings(rule_id,primary_account_id,secondary_account_id) VALUES(?,?,?)",
          )
          .bind(id, m.primaryAccountId, m.secondaryAccountId),
      ),
    ],
  );}})}
export const financeBookWorkspace=(actor:WorkContext,provided?:SqlDatabase)=>nativeBooks(provided).financeBookWorkspace(actor);
export const createFinanceDimension=(actor:WorkContext,input:z.infer<typeof dimensionSchema>,key:string)=>nativeBooks().createFinanceDimension(actor,input,key);
export const prepareBookRule=(actor:WorkContext,input:z.infer<typeof bookRuleSchema>,key:string)=>nativeBooks().prepareBookRule(actor,input,key);
export const decideBookRule=(actor:WorkContext,id:string,input:z.infer<typeof financeReviewSchema>,action:'approve'|'reject',key:string)=>nativeBooks().decideBookRule(actor,id,input,action,key);
export const financeBookRecord=(actor:WorkContext,id:string,provided?:SqlDatabase)=>nativeBooks(provided).financeBookRecord(actor,id);
