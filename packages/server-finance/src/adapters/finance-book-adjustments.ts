import { withFinancePurchasedCommand, withFinancePurchasedRead } from "@erpx/server-finance/adapters/purchase-boundary";
import type { z } from "zod";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { financeTransaction } from "@erpx/server-finance/public/finance-transactions";
import {prepareJournal} from '@erpx/server-finance/public/finance';
import { prepareOwnedJournalReversal } from "@erpx/server-finance/public/finance-owned-reversal";
import { financeReviewSchema } from "@erpx/server-finance/public/finance-books";
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { ApiError } from "@erpx/server-runtime/public/http";
import {createBookAdjustments,type BookAdjustmentRepository} from '@erpx/server-finance/application/book-adjustments';
import type {BookAdjustmentRow as Row,bookAdjustmentSchema,bookAdjustmentReversalSchema} from '@erpx/server-finance/domain/book-adjustments';
import {stageBookAdjustmentDecision} from '@erpx/server-workflow/public/finance-book-adjustment-effects';
export {bookAdjustmentSchema,bookAdjustmentReversalSchema} from '@erpx/server-finance/domain/book-adjustments';
async function nativeBookAdjustmentWorkspace(write:SqlDatabase,actor:WorkContext){  return (
    await write
      .prepare(
        "SELECT * FROM finance_book_adjustments WHERE company_id=? ORDER BY created_at DESC LIMIT 200",
      )
      .bind(actor.companyId)
      .all<Row>()
  ).results;}
async function nativeBookAdjustmentConsole(write:SqlDatabase,actor:WorkContext){  const [rows, books, accounts, sources, periods] = await Promise.all([
    nativeBookAdjustmentWorkspace(write,actor),
    write
      .prepare(
        "SELECT DISTINCT c.id,c.name,c.base_currency FROM finance_charts c JOIN finance_book_rules r ON r.secondary_chart_id=c.id WHERE c.company_id=? AND c.status='active' AND r.status='approved' ORDER BY c.name",
      )
      .bind(actor.companyId)
      .all<Row>(),
    write
      .prepare(
        "SELECT id,chart_id,code,name FROM finance_accounts WHERE company_id=? AND status='active' AND is_control=0 AND control_kind='none' ORDER BY code",
      )
      .bind(actor.companyId)
      .all<Row>(),
    write
      .prepare(
        "SELECT p.id,p.journal_number,p.entry_date,p.memo,s.chart_id FROM finance_book_postings b JOIN finance_journals p ON p.id=b.primary_journal_id JOIN finance_journals s ON s.id=b.secondary_journal_id WHERE b.company_id=? AND p.status='posted' AND s.status='posted' ORDER BY p.entry_date DESC,p.id LIMIT 500",
      )
      .bind(actor.companyId)
      .all<Row>(),
    write
      .prepare(
        "SELECT id,name FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date DESC",
      )
      .bind(actor.companyId)
      .all<Row>(),
  ]);
  return {
    rows,
    books: books.results,
    accounts: accounts.results,
    sources: sources.results,
    periods: periods.results,
  };}
function nativeAdjustmentRepository(db:SqlDatabase):BookAdjustmentRepository{const journals=new Map<string,Awaited<ReturnType<typeof prepareJournal>>>(),reversals=new Map<string,Awaited<ReturnType<typeof prepareOwnedJournalReversal>>>();return {replay:(actor,key,op,id,input)=>replayCommand(db,actor,key,op,id,input),source:async(actor,primaryJournalId,chartId)=>db
    .prepare(
      "SELECT p.id,p.version,p.status,p.entry_date,p.chart_id,s.base_currency FROM finance_journals p JOIN finance_book_postings b ON b.primary_journal_id=p.id JOIN finance_journals j ON j.id=b.secondary_journal_id JOIN finance_charts s ON s.id=j.chart_id AND s.status='active' WHERE p.id=? AND p.company_id=? AND j.chart_id=? AND p.status='posted' AND j.status='posted'",
    )
    .bind(primaryJournalId, actor.companyId, chartId)
    .first<Row>(),
accounts:async(actor,chartId,ids)=>(await db
        .prepare(
          `SELECT id,version,category,currency FROM finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND is_control=0 AND control_kind='none' AND id IN (${ids.map(() => "?").join(",")}) ORDER BY id`,
        )
        .bind(actor.companyId, chartId, ...ids)
        .all<Row>()).results,
validateJournal:async(actor,input,id,currency,key)=>{    await prepareJournal(
      db,
      actor,
      {
        ...input,
        currency: currency,
        sourceType: "book_adjustment",
        sourceId: id,
        memo: input.reason,
        isOpening: false,
      },
      key,
    ); },
prepareCommit:async(facts)=>{const {actor,input,key,op,id,source,createdAt,updatedAt}=facts;return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      1,
      { id, status: "draft", version: 1, sourceHash: source.hash },
      [
        db
          .prepare(
            "INSERT INTO finance_book_adjustments(id,company_id,chart_id,primary_journal_id,source_version,content_json,source_hash,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
          )
          .bind(
            id,
            actor.companyId,
            input.chartId,
            input.primaryJournalId,
            source.source.version,
            JSON.stringify(input),
            source.hash,
            actor.userId,
            input.reason,
            createdAt,
            updatedAt,
          ),
      ],
    );},
row:async(actor,id)=>db
      .prepare(
        "SELECT * FROM finance_book_adjustments WHERE id=? AND company_id=?",
      )
      .bind(id, actor.companyId)
      .first<Row>(),
preparePosting:async(actor,content,id,currency,key)=>{const journal=await prepareJournal(
            db,
            actor,
            {
              ...content,
              currency: currency,
              sourceType: "book_adjustment",
              sourceId: id,
              memo: content.reason,
              isOpening: false,
            },
            key,
            "posted",
          )
;journals.set(String(journal.result.id),journal);return String(journal.result.id);},
decisionCommit:async(facts)=>{const {actor,id,input,key,op,status,version,journalId,updatedAt,evidenceId,inputHash,evidenceAt}=facts;const journal=journalId?journals.get(journalId):null;return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      version,
      { id, status, version, journalId: journal?.result.id ?? null },
      [
        assertion(
          db,
          key,
          "adjustment-version",
          "EXISTS(SELECT 1 FROM finance_book_adjustments WHERE id=? AND company_id=? AND status='draft' AND version=?)",
          [id, actor.companyId, input.expectedVersion],
        ),
        ...(journal?.steps ?? []),
        db
          .prepare(
            "UPDATE finance_book_adjustments SET status=?,version=?,reviewed_by=?,journal_id=?,updated_at=? WHERE id=?",
          )
          .bind(
            status,
            version,
            actor.userId,
            journal?.result.id ?? null,
            updatedAt,
            id,
          ),
        stageBookAdjustmentDecision(db,{evidenceId,key,id,op,userId:actor.userId,assignmentId:actor.assignmentId,expectedVersion:input.expectedVersion,inputHash,evidenceAt}),
      ],
    );},
rowForReversal:async(actor,id)=>db
      .prepare(
        "SELECT * FROM finance_book_adjustments WHERE id=? AND company_id=?",
      )
      .bind(id, actor.companyId)
      .first<Row>(),
prepareReversal:async(actor,journalId,id,input,key)=>{    const reversal = await prepareOwnedJournalReversal(
      db,
      actor,
      journalId,
      "book_adjustment",
      id,
      input,
      key,
    );reversals.set(reversal.id,reversal);return reversal.id;},
reverseCommit:async(facts)=>{const {actor,id,input,key,op,reversalId,updatedAt}=facts;const reversal=reversals.get(reversalId)!;return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      input.expectedVersion + 1,
      {
        id,
        status: "reversed",
        version: input.expectedVersion + 1,
        reversalJournalId: reversal.id,
      },
      [
        assertion(
          db,
          key,
          "adjustment-version",
          "EXISTS(SELECT 1 FROM finance_book_adjustments WHERE id=? AND company_id=? AND status='posted' AND version=?)",
          [id, actor.companyId, input.expectedVersion],
        ),
        ...reversal.steps,
        db
          .prepare(
            "UPDATE finance_book_adjustments SET status='reversed',version=version+1,reversal_journal_id=?,reason=?,updated_at=? WHERE id=?",
          )
          .bind(reversal.id, input.reason, updatedAt, id),
      ],
    );}}}
function nativeAdjustments(provided?:SqlDatabase){let write:SqlDatabase;return createBookAdjustments({error:(...args)=>new ApiError(...args),permission:requirePermission,samePerson,now:Date.now,id:()=>crypto.randomUUID(),hash:sha256,ready:async actor=>{write=provided??(await holdingResources(actor.holdingId)).write},transaction:async(actor,run)=>provided?run(nativeAdjustmentRepository(provided)):financeTransaction((await holdingResources(actor.holdingId)).write,db=>run(nativeAdjustmentRepository(db))),workspace:actor=>nativeBookAdjustmentWorkspace(write,actor),console:actor=>nativeBookAdjustmentConsole(write,actor)})}
export const prepareBookAdjustment=(actor:WorkContext,raw:z.input<typeof bookAdjustmentSchema>,key:string)=>withFinancePurchasedCommand(actor,{key,operation:"finance.book-adjustment.prepare",purpose:"settle-existing",capability:"finance.advanced"},tx=>nativeAdjustments(tx).prepareBookAdjustment(actor,raw,key));
export const decideBookAdjustment=(actor:WorkContext,id:string,raw:z.input<typeof financeReviewSchema>,action:'post'|'reject',key:string)=>withFinancePurchasedCommand(actor,{key,operation:`finance.book-adjustment.${action}`,purpose:"settle-existing",capability:"finance.advanced"},tx=>nativeAdjustments(tx).decideBookAdjustment(actor,id,raw,action,key));
export const reverseBookAdjustment=(actor:WorkContext,id:string,raw:z.input<typeof bookAdjustmentReversalSchema>,key:string)=>withFinancePurchasedCommand(actor,{key,operation:"finance.book-adjustment.reverse",purpose:"settle-existing",capability:"finance.advanced"},tx=>nativeAdjustments(tx).reverseBookAdjustment(actor,id,raw,key));
export const bookAdjustmentWorkspace=(actor:WorkContext,provided?:SqlDatabase)=>provided?nativeAdjustments(provided).bookAdjustmentWorkspace(actor):withFinancePurchasedRead(actor,db=>nativeAdjustments(db).bookAdjustmentWorkspace(actor),"finance.advanced");
export const bookAdjustmentConsoleWorkspace=(actor:WorkContext,provided?:SqlDatabase)=>provided?nativeAdjustments(provided).bookAdjustmentConsoleWorkspace(actor):withFinancePurchasedRead(actor,db=>nativeAdjustments(db).bookAdjustmentConsoleWorkspace(actor),"finance.advanced");
