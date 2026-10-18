import { withFinancePurchasedCommand, withFinancePurchasedRead } from "./purchase-boundary";
import "server-only";
import type { z } from "zod";
import { samePerson } from "@/lib/identity_access/iam";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase, SqlStatement } from "@/server/modules/runtime/public/runtime/types";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { executeCommand, replayCommand, assertion } from "@/server/modules/workflow/public/commands";
import { financeTransaction, financeOwnedTransaction } from "@/server/modules/finance/public/finance-transactions";
import { requireFinanceTool } from "@/server/modules/finance/public/finance-company-tools";
import { requireFinanceMoneyScale } from "@/server/modules/finance/public/finance-money-policy";
import { prepareJournal } from "@/server/modules/finance/public/finance";
import { prepareOwnedJournalReversal } from "@/server/modules/finance/public/finance-owned-reversal";
import { sha256 } from "@/server/modules/runtime/public/crypto";
import { ApiError } from "@/server/modules/runtime/public/http";

import {createFinanceInstruments,type InstrumentRepository} from '../application/instruments';
import type {InstrumentRow as Row,instrumentEditionSchema} from '../domain/instruments';
import type {instrumentDecisionSchema,instrumentReversalSchema} from '../domain/instrument-decisions';
import type {instrumentRunSchema} from '../domain/due-inputs';
export {instrumentEditionSchema} from '../domain/instruments';
export {instrumentDecisionSchema,instrumentReversalSchema} from '../domain/instrument-decisions';
export {instrumentRunSchema} from '../domain/due-inputs';
function nativeInstrumentRepository(db:SqlDatabase):InstrumentRepository{const journals=new Map<string,Awaited<ReturnType<typeof prepareJournal>>>(),reversals=new Map<string,Awaited<ReturnType<typeof prepareOwnedJournalReversal>>>();return {moneyScale:(companyId,currency)=>requireFinanceMoneyScale(db,companyId,currency),tool:companyId=>requireFinanceTool(db,companyId,'instruments'),replay:(actor,key,op,id,input)=>replayCommand(db,actor,key,op,id,input),prepareJournal:async(actor,input,key,status)=>{const prepared=await prepareJournal(db,actor,input,key,status);const id=String(prepared.result.id);journals.set(id,prepared);return id},prepareReversal:async(actor,journalId,sourceType,id,input,key)=>{const prepared=await prepareOwnedJournalReversal(db,actor,journalId,sourceType,id,input,key);reversals.set(prepared.id,prepared);return prepared.id},edition:async(actor,id)=>db
    .prepare(
      "SELECT * FROM finance_instrument_editions WHERE id=? AND company_id=?",
    )
    .bind(id, actor.companyId)
    .first<Row>(),
chart:async(actor,chartId)=>db
    .prepare(
      "SELECT id,base_currency,status FROM finance_charts WHERE id=? AND company_id=? AND status='active'",
    )
    .bind(chartId, actor.companyId)
    .first<Row>(),
secondary:async(actor,chartId)=>db
      .prepare(
        "SELECT 1 FROM finance_book_rules WHERE company_id=? AND secondary_chart_id=? AND status='approved'",
      )
      .bind(actor.companyId, chartId)
      .first(),
accounts:async(actor,chartId,ids)=>(await db
        .prepare(
          `SELECT id,version,currency,category,control_kind FROM finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND id IN (${ids.map(() => "?").join(",")}) ORDER BY id`,
        )
        .bind(actor.companyId, chartId, ...ids)
        .all<Row>()).results,
bank:async(actor,bankId)=>db
    .prepare(
      "SELECT t.*,s.bank_account,s.source_hash FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=?",
    )
    .bind(bankId, actor.companyId)
    .first<Row>(),
history:async(id)=>(await db
      .prepare(
        "SELECT r.id,r.ordinal,r.result_json,r.source_hash,j.status journal_status,j.entry_date FROM finance_instrument_runs r JOIN finance_journals j ON j.id=r.journal_id WHERE r.instrument_id=? AND r.status='posted' ORDER BY r.ordinal",
      )
      .bind(id)
      .all<Row>()).results,
pendingRuns:async(id)=>db
        .prepare(
          "SELECT 1 FROM finance_instrument_runs WHERE instrument_id=? AND status='draft'",
        )
        .bind(id)
        .first(),
prepareEditionCommit:async(facts)=>{const {actor,input,key,op,id,e,now}=facts;return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      1,
      { id, status: "draft", version: 1, principal: e.calculation.principal },
      [
        db
          .prepare(
            "INSERT INTO finance_instrument_editions(id,company_id,root_id,parent_id,kind,number,name,chart_id,edition,content_json,schedule_json,source_hash,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
          )
          .bind(
            id,
            actor.companyId,
            e.parent?.root_id ?? id,
            e.parent?.id ?? null,
            input.kind,
            input.number,
            input.name,
            input.chartId,
            Number(e.parent?.edition ?? 0) + 1,
            JSON.stringify(input),
            JSON.stringify(e.calculation),
            e.hash,
            actor.userId,
            input.reason,
            now,
            now,
          ),
      ],
    );},
decideEditionCommit:async(facts)=>{const {actor,id,input,key,op,e,journalId,status,parentUpdatedAt,updatedAt}=facts;const journal=journalId?journals.get(journalId):null;      const steps: SqlStatement[] = [];
    if (e?.parent)
      steps.push(
        db
          .prepare(
            "UPDATE finance_instrument_editions SET status='superseded',version=version+1,updated_at=? WHERE id=?",
          )
          .bind(parentUpdatedAt!, e.parent.id),
      );
    if (journal) steps.push(...journal.steps);
    if (e?.bank && journal)
      steps.push(
        db
          .prepare(
            "UPDATE finance_bank_transactions SET match_status='matched',matched_type='journal',matched_id=?,version=version+1 WHERE id=?",
          )
          .bind(journal.result.id, e.bank.id),
      );
    steps.push(
      assertion(
        db,
        key,
        "instrument-version",
        "EXISTS(SELECT 1 FROM finance_instrument_editions WHERE id=? AND company_id=? AND version=? AND status='draft')",
        [id, actor.companyId, input.expectedVersion],
      ),
      db
        .prepare(
          "UPDATE finance_instrument_editions SET status=?,reviewed_by=?,activation_journal_id=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(
          status,
          actor.userId,
          journal?.result.id ?? null,
          updatedAt,
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
      input.expectedVersion + 1,
      {
        id,
        status: status,
        version: input.expectedVersion + 1,
        journalId: journal?.result.id ?? null,
      },
      steps,
    );},
prepareRunCommit:async(facts)=>{const {actor,id,input,key,op,runId,e,now}=facts;return executeCommand(
      db,
      actor,
      key,
      op,
      { id, input },
      runId,
      1,
      { id: runId, status: "draft", version: 1, result: e.result },
      [
        db
          .prepare(
            "INSERT INTO finance_instrument_runs(id,company_id,instrument_id,kind,ordinal,input_json,result_json,source_hash,bank_transaction_id,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)",
          )
          .bind(
            runId,
            actor.companyId,
            id,
            input.kind,
            e.ordinal,
            JSON.stringify(input),
            JSON.stringify(e.result),
            e.hash,
            input.bankTransactionId,
            actor.userId,
            input.reason,
            now,
            now,
          ),
      ],
    );},
run:async(actor,id)=>db
    .prepare(
      "SELECT * FROM finance_instrument_runs WHERE id=? AND company_id=?",
    )
    .bind(id, actor.companyId)
    .first<Row>(),
decideRunCommit:async(facts)=>{const {actor,id,input,key,op,row,run,journalId,status,agreementStatus,agreementUpdatedAt,updatedAt}=facts;const journal=journalId?journals.get(journalId):null;    const steps = journal
      ? [
          ...journal.steps,
          db
            .prepare(
              "UPDATE finance_bank_transactions SET match_status='matched',matched_type='journal',matched_id=?,version=version+1 WHERE id=?",
            )
            .bind(journal.result.id, run.bank_transaction_id),
          db
            .prepare(
              "UPDATE finance_instrument_editions SET status=?,version=version+1,updated_at=? WHERE id=?",
            )
            .bind(
              agreementStatus,
              agreementUpdatedAt!,
              row.id,
            ),
        ]
      : [];
    steps.push(
      assertion(
        db,
        key,
        "instrument-run-version",
        "EXISTS(SELECT 1 FROM finance_instrument_runs WHERE id=? AND company_id=? AND version=? AND status='draft')",
        [id, actor.companyId, input.expectedVersion],
      ),
      db
        .prepare(
          "UPDATE finance_instrument_runs SET status=?,reviewed_by=?,journal_id=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(
          status,
          actor.userId,
          journal?.result.id ?? null,
          updatedAt,
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
      input.expectedVersion + 1,
      {
        id,
        status: status,
        version: input.expectedVersion + 1,
        journalId: journal?.result.id ?? null,
      },
      steps,
    );},
laterRuns:async(id,ordinal)=>db
        .prepare(
          "SELECT 1 FROM finance_instrument_runs WHERE instrument_id=? AND ((status='posted' AND ordinal>?) OR status='draft')",
        )
        .bind(id, ordinal)
        .first(),
laterEdition:async(id)=>db
        .prepare(
          "SELECT 1 FROM finance_instrument_editions WHERE parent_id=? AND status='draft'",
        )
        .bind(id)
        .first(),
reverseRunCommit:async(facts)=>{const {actor,id,input,key,op,row,run,reversalId,now}=facts;const reversal=reversals.get(reversalId)!;return executeCommand(
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
        ...reversal.steps,
        assertion(
          db,
          key,
          "instrument-bank-owned",
          "EXISTS(SELECT 1 FROM finance_bank_transactions WHERE id=? AND company_id=? AND match_status='matched' AND matched_type='journal' AND matched_id=?)",
          [run.bank_transaction_id, actor.companyId, run.journal_id],
        ),
        db
          .prepare(
            "UPDATE finance_bank_transactions SET match_status='unmatched',matched_type=NULL,matched_id=NULL,version=version+1 WHERE id=?",
          )
          .bind(run.bank_transaction_id),
        db
          .prepare(
            "UPDATE finance_instrument_runs SET status='reversed',reversal_journal_id=?,version=version+1,updated_at=? WHERE id=?",
          )
          .bind(reversal.id, now, id),
        db
          .prepare(
            "UPDATE finance_instrument_editions SET status='active',version=version+1,updated_at=? WHERE id=?",
          )
          .bind(now, row.id),
      ],
    );}}}
function nativeInstruments(ownedTransaction?:SqlDatabase,readDatabase?:SqlDatabase){return createFinanceInstruments({error:(...args)=>new ApiError(...args),permission:requirePermission,samePerson,now:Date.now,hash:sha256,transaction:async(actor,run)=>ownedTransaction?run(nativeInstrumentRepository(ownedTransaction)):financeTransaction((await holdingResources(actor.holdingId)).write,db=>run(nativeInstrumentRepository(db))),ownedTransaction:async(actor,run)=>financeOwnedTransaction((await holdingResources(actor.holdingId)).write,ownedTransaction,db=>run(nativeInstrumentRepository(db))),workspace:async actor=>{const write=readDatabase??(await holdingResources(actor.holdingId)).write;  const [editions, runs, accounts, periods, bank] = await Promise.all([
    write
      .prepare(
        "SELECT * FROM finance_instrument_editions WHERE company_id=? ORDER BY created_at DESC,id LIMIT 500",
      )
      .bind(actor.companyId)
      .all<Row>(),
    write
      .prepare(
        "SELECT * FROM finance_instrument_runs WHERE company_id=? ORDER BY created_at DESC,id LIMIT 500",
      )
      .bind(actor.companyId)
      .all<Row>(),
    write
      .prepare(
        "SELECT a.id,a.chart_id,a.code,a.name,a.currency,a.category,a.control_kind,c.name chart_name FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.company_id=? AND a.status='active' AND c.status='active' AND NOT EXISTS(SELECT 1 FROM finance_book_rules r WHERE r.company_id=a.company_id AND r.secondary_chart_id=a.chart_id AND r.status='approved') ORDER BY a.chart_id,a.code",
      )
      .bind(actor.companyId)
      .all<Row>(),
    write
      .prepare(
        "SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date",
      )
      .bind(actor.companyId)
      .all<Row>(),
    write
      .prepare(
        "SELECT t.id,t.version,t.amount_minor,t.currency,t.transaction_date,t.external_id,s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.company_id=? AND t.match_status='unmatched' ORDER BY t.transaction_date DESC LIMIT 500",
      )
      .bind(actor.companyId)
      .all<Row>(),
  ]);
  return {
    editions: editions.results,
    runs: runs.results,
    accounts: accounts.results,
    periods: periods.results,
    bank: bank.results,
  };}})}
export const prepareInstrument=(actor: WorkContext,raw: z.input<typeof instrumentEditionSchema>,key: string)=>withFinancePurchasedCommand(actor,{key,operation:"finance.instrument.prepare",purpose:"new-work",capability:"finance.advanced"},tx=>nativeInstruments(tx).prepareInstrument(actor,raw,key));
export const decideInstrument=(actor: WorkContext,id: string,raw: z.input<typeof instrumentDecisionSchema>,action: "approve" | "reject",key: string)=>withFinancePurchasedCommand(actor,{key,operation:`finance.instrument.${action}`,purpose:"settle-existing",capability:"finance.advanced"},tx=>nativeInstruments(tx).decideInstrument(actor,id,raw,action,key));
export const prepareInstrumentRun=(actor: WorkContext,id: string,raw: z.input<typeof instrumentRunSchema>,key: string,ownedTransaction?:SqlDatabase)=>withFinancePurchasedCommand(actor,{key,operation:"finance.instrument-run.prepare",purpose:"settle-existing",capability:"finance.advanced"},tx=>nativeInstruments(tx).prepareInstrumentRun(actor,id,raw,key),ownedTransaction);
export const decideInstrumentRun=(actor: WorkContext,id: string,raw: z.input<typeof instrumentDecisionSchema>,action: "post" | "reject",key: string)=>withFinancePurchasedCommand(actor,{key,operation:`finance.instrument-run.${action}`,purpose:"settle-existing",capability:"finance.advanced"},tx=>nativeInstruments(tx).decideInstrumentRun(actor,id,raw,action,key));
export const reverseInstrumentRun=(actor: WorkContext,id: string,raw: z.input<typeof instrumentReversalSchema>,key: string)=>withFinancePurchasedCommand(actor,{key,operation:"finance.instrument-run.reverse",purpose:"settle-existing",capability:"finance.advanced"},tx=>nativeInstruments(tx).reverseInstrumentRun(actor,id,raw,key));
export const instrumentWorkspace=(actor: WorkContext)=>withFinancePurchasedRead(actor,db=>nativeInstruments(undefined,db).instrumentWorkspace(actor),"finance.advanced");
