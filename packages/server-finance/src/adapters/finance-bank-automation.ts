import { versionStatusGuardSql } from './finance-read-tail-r52-sql';
import { columnByIdSql } from './finance-read-tail-r53-sql';
import { financeReferenceListSql } from './finance-read-tail-r42-sql';
import { tableByIdSql } from './finance-read-tail-r49-sql';
import { bankCashLinesSql } from './finance-read-tail-r39-sql';
import { bankTxnGuardSql } from './finance-read-tail-r48-sql';
import {withFinancePurchasedCommand,withFinancePurchasedRead} from "@erpx/server-finance/adapters/purchase-boundary";
import { BANK_TRANSACTION_BY_ID_SQL } from './finance-read-tail-r17-sql';
import { nativeFinanceBankAutomation } from '@erpx/server-collaboration/adapters/native-finance-bank-automation';
import {financeCommandReceipts} from "@erpx/server-workflow/public/finance-command-receipts";
import {financeBankPosCashAvailableSql} from '@erpx/server-pos/public/finance-bank-cash-source';
import { financeTransaction } from "@erpx/server-finance/public/finance-transactions";
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
import {importBankStatement} from '@erpx/server-finance/public/finance';
import type {financeReviewSchema} from "@erpx/server-finance/domain/review";
import { requireFinanceTool } from "@erpx/server-finance/public/finance-company-tools";
import { readListPage } from "@erpx/server-work_projection/public/list-page";
import {createBankAutomation} from '@erpx/server-finance/application/bank-automation';
import type {BankRow,LedgerRow,BankAutomationQuery,bankCsvMappingSchema,bankCsvImportSchema,bankMatchGroupSchema} from '@erpx/server-finance/domain/bank-automation';
export {bankCsvMappingSchema,bankCsvImportSchema,bankMatchGroupSchema} from '@erpx/server-finance/domain/bank-automation';
export type {BankAutomationQuery} from '@erpx/server-finance/domain/bank-automation';
// These owners already retain the bank confirmation with their cash journal.
// POS can also adopt a manual journal before its own bank confirmation.
const availableCashSource = `j.source_type NOT IN ('book_projection','book_opening','withholding_remittance','withholding_remittance_reversal','payroll_remittance','payroll_payment')
  AND ${financeBankPosCashAvailableSql}
  AND NOT EXISTS(SELECT 1 FROM finance_bank_match_targets WHERE journal_line_id=l.id)
  AND NOT EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_payments p ON p.id=t.matched_id WHERE t.company_id=j.company_id AND t.match_status='matched' AND t.matched_type='payment' AND p.journal_id=j.id)
  AND NOT EXISTS(SELECT 1 FROM finance_bank_transactions t WHERE t.company_id=j.company_id AND t.match_status='matched' AND t.matched_type='journal' AND t.matched_id=j.id)`;
async function replay<Result=Record<string,unknown>>(
  db: SqlDatabase,
  actor: WorkContext,
  key: string,
  operation: string,
  input: unknown,
) {
  const r = await financeCommandReceipts(db).record(key);
  return r
    ? replayCommand<Result>(
        db,
        actor,
        key,
        operation,
        r.record_id,
        input,
      )
    : null;
}
function nativeBankAutomation(provided?:SqlDatabase,owned=false){let write:SqlDatabase;const transaction=async<T>(run:()=>Promise<T>,message:string):Promise<T>=>{if(owned)return run();if(!isTransactionalSqlDatabase(write))throw new ApiError(503,'TRANSACTION_REQUIRED',message);return financeTransaction(write,async tx=>{const prior=write;write=tx;try{return await run()}finally{write=prior}})};return createBankAutomation({error:(...args)=>new ApiError(...args),permission:requirePermission,samePerson,ready:async actor=>{write=provided??(await holdingResources(actor.holdingId)).write},now:Date.now,id:()=>crypto.randomUUID(),hash:sha256,tool:actor=>requireFinanceTool(write,actor.companyId,'bank_automation'),replay:<Result>(actor:WorkContext,key:string,op:string,input:unknown)=>replay<Result>(write,actor,key,op,input),reviewReplay:(actor,key,op,id,input)=>replayCommand(write,actor,key,op,id,input),mappedTransaction:run=>transaction(run,'Mapped imports require PostgreSQL.'),reviewTransaction:run=>transaction(run,'Bank match approval requires PostgreSQL.'),importStatement:(actor,input,key)=>importBankStatement(actor,input,key,write),mappingCommit:async(facts)=>{const {actor,key,op,input,id}=facts;return executeCommand(write, actor, key, op, input, id, 1, { id }, [
      await nativeFinanceBankAutomation(write, actor.companyId).csvMappingInsert([id, actor.companyId, input.name, JSON.stringify(input), actor.userId, Date.now(),]),
  ]);},
mapping:async(actor,mappingId)=>write
      .prepare(
        columnByIdSql('columns_json', 'finance_bank_csv_mappings'),
      )
      .bind(mappingId, actor.companyId)
      .first<{ columns_json: string }>(),
account:async(actor,accountId)=>write
        .prepare(
          columnByIdSql('currency', 'finance_accounts'),
        )
        .bind(accountId, actor.companyId)
        .first<{ currency: string }>(),
importedFile:async(actor,accountId,fileHash)=>write.prepare("SELECT s.statement_number FROM finance_bank_import_files f JOIN finance_bank_statements s ON s.id=f.statement_id AND s.company_id=f.company_id WHERE f.company_id=? AND f.bank_account_id=? AND f.file_hash=?").bind(actor.companyId,accountId,fileHash).first<{statement_number:string}>(),
importCommit:async(facts)=>{const {actor,key,op,input,statement,fileHash}=facts;return executeCommand(
      write,
      actor,
      key,
      op,
      input,
      String(statement.id),
      1,
      { ...statement, fileHash },
      [
          await nativeFinanceBankAutomation(write, actor.companyId).importFileInsert([crypto.randomUUID(), actor.companyId, input.bankAccountId, input.mappingId, fileHash, statement.id, Date.now(),]),
      ],
    );},
bank:async(actor,id)=>write
      .prepare(
        BANK_TRANSACTION_BY_ID_SQL,
      )
      .bind(id, actor.companyId)
      .first<BankRow>(),
line:async(actor,id)=>write
      .prepare(
        `SELECT l.id,l.account_id,l.debit_minor,l.credit_minor,j.id journal_id,j.currency,j.status,j.version,j.entry_date FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE l.id=? AND j.company_id=? AND j.status='posted' AND j.is_opening=0 AND ${availableCashSource}`,
      )
      .bind(id, actor.companyId)
      .first<LedgerRow>(),
proposeCommit:async(facts)=>{const {actor,key,op,input,id,source,now,allocations}=facts;return executeCommand(
    write,
    actor,
    key,
    op,
    input,
    id,
    1,
    { id, status: "proposed", version: 1 },
    [
        await nativeFinanceBankAutomation(write, actor.companyId).matchGroupInsert([id, actor.companyId, source.currency, source.hash, JSON.stringify({ input, source }), actor.userId, now, now,]),
      ...(await Promise.all(allocations.map(a=>nativeFinanceBankAutomation(write, actor.companyId).matchAllocationInsert([a.id,id,a.bankId,a.lineId,a.amount,now])))),
    ],
  );},
group:async(actor,id)=>write
      .prepare(
        tableByIdSql('finance_bank_match_groups') + ' FOR UPDATE',
      )
      .bind(id, actor.companyId)
      .first<{
        version: number;
        status: string;
        source_hash: string;
        explanation_json: string;
        proposed_by: string;
      }>(),
decisionCommit:async(facts)=>{const {actor,id,input,key,op,group,prepared,current,action,status,now}=facts;    const steps = [];
    if (action === "confirm") {
      for (const b of current!.banks)
        steps.push(
          assertion(
            write,
            key,
            `bank-${b.id}`,
            bankTxnGuardSql(" AND version=? AND match_status IN ('unmatched','proposed')"),
            [b.id, actor.companyId, b.version],
          ),
            await nativeFinanceBankAutomation(write, actor.companyId).txnMatch([id, b.id]),
        );
      for (const l of current!.lines)
        steps.push(
          assertion(
            write,
            key,
            `journal-${l.id}`,
            versionStatusGuardSql('finance_journals', "'posted'", ''),
            [l.journal_id, actor.companyId, l.version],
          ),
            await nativeFinanceBankAutomation(write, actor.companyId).matchTargetInsert([l.id, id]),
        );
    } else if (action === "reverse") {
      for (const b of prepared.input.bankTransactionIds)
        steps.push(
          assertion(
            write,
            key,
            `matched-${b}`,
            bankTxnGuardSql(" AND match_status='matched' AND matched_type='group' AND matched_id=?"),
            [b, actor.companyId, id],
          ),
            await nativeFinanceBankAutomation(write, actor.companyId).txnUnmatch([b]),
        );
      steps.push(
          await nativeFinanceBankAutomation(write, actor.companyId).matchTargetDelete([id]),
      );
    }
    steps.push(
        await nativeFinanceBankAutomation(write, actor.companyId).matchGroupDecision([status, status, actor.userId, status, actor.userId, now, id]),
    );
    return executeCommand(
      write,
      actor,
      key,
      op,
      input,
      id,
      group.version + 1,
      { id, status, version: group.version + 1 },
      steps,
    );},
picker:async(actor,query,chart,search,cashSearch)=>{  const bankSql = `SELECT t.*,s.bank_account,t.transaction_date page_key FROM finance_bank_transactions t
    JOIN finance_bank_statements s ON s.id=t.statement_id AND s.company_id=t.company_id
    JOIN finance_accounts a ON a.id=s.bank_account AND a.company_id=t.company_id
    WHERE t.company_id=? AND t.match_status='unmatched' AND (?='' OR a.chart_id=?)
    AND (?='' OR strpos(lower(t.external_id||' '||COALESCE(t.description,'')),lower(?))>0)`;
  const cashSql = bankCashLinesSql(availableCashSource);
  const cashArgs = [actor.companyId, chart, chart];
  const [mappings, groups, rows, targets] = await Promise.all([
    write
      .prepare(
        financeReferenceListSql('finance_bank_csv_mappings','company_id=?','name'),
      )
      .bind(actor.companyId)
      .all(),
    readListPage<Record<string, unknown> & { id: string; page_key: string }>(
      write,
      {
        sql: "SELECT *,lpad(created_at::text,20,'0') page_key FROM finance_bank_match_groups WHERE company_id=?",
        args: [actor.companyId],
        descending: true,
        cursor: query.groupCursor,
        scope: [actor.companyId, "finance-bank-match-groups"],
      },
    ),
    readListPage<BankRow & { page_key: string }>(write, {
      sql: bankSql,
      args: [actor.companyId, chart, chart, search, search],
      cursor: query.bankCursor,
      scope: [actor.companyId, "finance-bank-unmatched", chart, search],
    }),
    readListPage<LedgerRow & { page_key: string }>(write, {
      sql: `SELECT * FROM (${cashSql}) cash WHERE (?='' OR strpos(lower(journal_number||' '||COALESCE(description,'')),lower(?))>0)`,
      args: [...cashArgs, cashSearch, cashSearch],
      descending: true,
      cursor: query.cashCursor,
      scope: [actor.companyId, "finance-bank-cash-lines", chart, cashSearch],
    }),
  ]);
return {mappings:mappings.results,groups,rows,targets};},
candidates:async(actor,chart,ids)=>{  const cashSql = bankCashLinesSql(availableCashSource);
  const cashArgs = [actor.companyId, chart, chart];
return write
          .prepare(
            `WITH cash AS (${cashSql})
    SELECT b.id,COUNT(c.id) FILTER(WHERE c.debit_minor-c.credit_minor=b.amount_minor) exact_count,
    MIN(c.id) FILTER(WHERE c.debit_minor-c.credit_minor=b.amount_minor) exact_id,
    COUNT(c.id) FILTER(WHERE c.entry_date=b.transaction_date) same_day_count,
    COALESCE(SUM(c.debit_minor-c.credit_minor) FILTER(WHERE c.entry_date=b.transaction_date),0)::text same_day_amount,
    CASE WHEN COUNT(c.id) FILTER(WHERE c.entry_date=b.transaction_date)<=100
      THEN COALESCE(jsonb_agg(c.id ORDER BY c.id) FILTER(WHERE c.entry_date=b.transaction_date),'[]'::jsonb)::text
      ELSE '[]' END same_day_ids
    FROM finance_bank_transactions b JOIN finance_bank_statements s ON s.id=b.statement_id AND s.company_id=b.company_id
    LEFT JOIN cash c ON c.account_id=s.bank_account AND c.currency=b.currency
      AND c.entry_date::date BETWEEN b.transaction_date::date-7 AND b.transaction_date::date+7
      AND sign(c.debit_minor-c.credit_minor)=sign(b.amount_minor)
    WHERE b.company_id=? AND b.id IN (${ids.map(() => "?").join(",")}) GROUP BY b.id`,
          )
          .bind(
            ...cashArgs,
            actor.companyId,
            ...ids,
          )
          .all<{
            id: string;
            exact_count: number;
            exact_id: string | null;
            same_day_count: number;
            same_day_amount: string;
            same_day_ids: string;
          }>().then(result=>result.results);},
suggestionTargets:async(actor,chart,suggestedIds)=>{  const cashSql = bankCashLinesSql(availableCashSource);
  const cashArgs = [actor.companyId, chart, chart];
return write
          .prepare(
            `SELECT * FROM (${cashSql}) cash WHERE id IN (${suggestedIds.map(() => "?").join(",")})`,
          )
          .bind(...cashArgs, ...suggestedIds)
          .all<LedgerRow>().then(result=>result.results);}});}
export const createBankCsvMapping=(actor:WorkContext,input:z.infer<typeof bankCsvMappingSchema>,key:string)=>withFinancePurchasedCommand(actor,{key,operation:"finance.bank-mapping.create",purpose:"new-work",capability:"finance.banking"},tx=>nativeBankAutomation(tx,Boolean(tx)).createBankCsvMapping(actor,input,key));
export const importMappedBankCsv=(actor:WorkContext,input:z.infer<typeof bankCsvImportSchema>,key:string)=>withFinancePurchasedCommand(actor,{key,operation:"finance.bank-csv.import",purpose:"settle-existing",capability:"finance.banking"},tx=>nativeBankAutomation(tx,Boolean(tx)).importMappedBankCsv(actor,input,key));
export const proposeBankMatchGroup=(actor:WorkContext,input:z.infer<typeof bankMatchGroupSchema>,key:string)=>withFinancePurchasedCommand(actor,{key,operation:"finance.bank-group.propose",purpose:"settle-existing",capability:"finance.banking"},tx=>nativeBankAutomation(tx,Boolean(tx)).proposeBankMatchGroup(actor,input,key));
export const decideBankMatchGroup=(actor:WorkContext,id:string,input:z.infer<typeof financeReviewSchema>,action:'confirm'|'reverse'|'reject',key:string)=>withFinancePurchasedCommand(actor,{key,operation:`finance.bank-group.${action}`,purpose:"settle-existing",capability:"finance.banking"},tx=>nativeBankAutomation(tx,Boolean(tx)).decideBankMatchGroup(actor,id,input,action,key));
export const bankAutomationWorkspace=(actor:WorkContext,query:BankAutomationQuery={})=>withFinancePurchasedRead(actor,db=>nativeBankAutomation(db).bankAutomationWorkspace(actor,query),"finance.banking");
