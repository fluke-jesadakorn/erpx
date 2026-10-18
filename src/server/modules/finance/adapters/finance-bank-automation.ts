import {withFinancePurchasedCommand,withFinancePurchasedRead} from "./purchase-boundary";
import {financeCommandReceipts} from "../../workflow/public/finance-command-receipts";
import {financeBankPosCashAvailableSql} from '@/server/modules/pos/public/finance-bank-cash-source';
import { financeTransaction } from "@/server/modules/finance/public/finance-transactions";
import type { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { isTransactionalSqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { executeCommand, replayCommand, assertion } from "@/server/modules/workflow/public/commands";
import { ApiError } from "@/server/modules/runtime/public/http";
import { samePerson } from "@/lib/identity_access/iam";
import { sha256 } from "@/server/modules/runtime/public/crypto";
import { importBankStatement } from "@/server/modules/finance/public/finance";
import type {financeReviewSchema} from "../domain/review";
import { requireFinanceTool } from "@/server/modules/finance/public/finance-company-tools";
import { readListPage } from "@/server/modules/work_projection/public/list-page";
import {createBankAutomation} from '../application/bank-automation';
import type {BankRow,LedgerRow,BankAutomationQuery,bankCsvMappingSchema,bankCsvImportSchema,bankMatchGroupSchema} from '../domain/bank-automation';
export {bankCsvMappingSchema,bankCsvImportSchema,bankMatchGroupSchema} from '../domain/bank-automation';
export type {BankAutomationQuery} from '../domain/bank-automation';
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
    write
      .prepare(
        "INSERT INTO finance_bank_csv_mappings(id,company_id,name,columns_json,created_by,created_at) VALUES(?,?,?,?,?,?)",
      )
      .bind(
        id,
        actor.companyId,
        input.name,
        JSON.stringify(input),
        actor.userId,
        Date.now(),
      ),
  ]);},
mapping:async(actor,mappingId)=>write
      .prepare(
        "SELECT columns_json FROM finance_bank_csv_mappings WHERE id=? AND company_id=?",
      )
      .bind(mappingId, actor.companyId)
      .first<{ columns_json: string }>(),
account:async(actor,accountId)=>write
        .prepare(
          "SELECT currency FROM finance_accounts WHERE id=? AND company_id=?",
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
        write
          .prepare(
            "INSERT INTO finance_bank_import_files(id,company_id,bank_account_id,mapping_id,file_hash,statement_id,created_at) VALUES(?,?,?,?,?,?,?)",
          )
          .bind(
            crypto.randomUUID(),
            actor.companyId,
            input.bankAccountId,
            input.mappingId,
            fileHash,
            statement.id,
            Date.now(),
          ),
      ],
    );},
bank:async(actor,id)=>write
      .prepare(
        "SELECT t.*,s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=?",
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
      write
        .prepare(
          "INSERT INTO finance_bank_match_groups(id,company_id,currency,status,source_hash,explanation_json,proposed_by,created_at,updated_at) VALUES(?,?,?,'proposed',?,?,?,?,?)",
        )
        .bind(
          id,
          actor.companyId,
          source.currency,
          source.hash,
          JSON.stringify({ input, source }),
          actor.userId,
          now,
          now,
        ),
      ...allocations.map(a=>write.prepare("INSERT INTO finance_bank_match_allocations(id,group_id,bank_transaction_id,target_type,target_id,amount_minor,created_at) VALUES(?,?,?,'journal_line',?,?,?)").bind(a.id,id,a.bankId,a.lineId,a.amount,now)),
    ],
  );},
group:async(actor,id)=>write
      .prepare(
        "SELECT * FROM finance_bank_match_groups WHERE id=? AND company_id=? FOR UPDATE",
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
            "EXISTS(SELECT 1 FROM finance_bank_transactions WHERE id=? AND company_id=? AND version=? AND match_status IN ('unmatched','proposed'))",
            [b.id, actor.companyId, b.version],
          ),
          write
            .prepare(
              "UPDATE finance_bank_transactions SET match_status='matched',matched_type='group',matched_id=?,version=version+1 WHERE id=?",
            )
            .bind(id, b.id),
        );
      for (const l of current!.lines)
        steps.push(
          assertion(
            write,
            key,
            `journal-${l.id}`,
            "EXISTS(SELECT 1 FROM finance_journals WHERE id=? AND company_id=? AND version=? AND status='posted')",
            [l.journal_id, actor.companyId, l.version],
          ),
          write
            .prepare(
              "INSERT INTO finance_bank_match_targets(journal_line_id,group_id) VALUES(?,?)",
            )
            .bind(l.id, id),
        );
    } else if (action === "reverse") {
      for (const b of prepared.input.bankTransactionIds)
        steps.push(
          assertion(
            write,
            key,
            `matched-${b}`,
            "EXISTS(SELECT 1 FROM finance_bank_transactions WHERE id=? AND company_id=? AND match_status='matched' AND matched_type='group' AND matched_id=?)",
            [b, actor.companyId, id],
          ),
          write
            .prepare(
              "UPDATE finance_bank_transactions SET match_status='unmatched',matched_type=NULL,matched_id=NULL,version=version+1 WHERE id=?",
            )
            .bind(b),
        );
      steps.push(
        write
          .prepare("DELETE FROM finance_bank_match_targets WHERE group_id=?")
          .bind(id),
      );
    }
    steps.push(
      write
        .prepare(
          "UPDATE finance_bank_match_groups SET status=?,version=version+1,confirmed_by=CASE WHEN ?='matched' THEN ? ELSE confirmed_by END,reversed_by=CASE WHEN ?='reversed' THEN ? ELSE reversed_by END,updated_at=? WHERE id=?",
        )
        .bind(status, status, actor.userId, status, actor.userId, now, id),
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
  const cashSql = `SELECT l.id,l.account_id,l.description,l.debit_minor,l.credit_minor,j.id journal_id,j.journal_number,j.currency,j.entry_date,j.entry_date page_key
    FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
    JOIN finance_accounts a ON a.id=l.account_id AND a.company_id=j.company_id
    WHERE j.company_id=? AND j.status='posted' AND a.control_kind='bank' AND j.is_opening=0
    AND (?='' OR j.chart_id=?) AND ${availableCashSource}`;
  const cashArgs = [actor.companyId, chart, chart];
  const [mappings, groups, rows, targets] = await Promise.all([
    write
      .prepare(
        "SELECT * FROM finance_bank_csv_mappings WHERE company_id=? ORDER BY name",
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
candidates:async(actor,chart,ids)=>{  const cashSql = `SELECT l.id,l.account_id,l.description,l.debit_minor,l.credit_minor,j.id journal_id,j.journal_number,j.currency,j.entry_date,j.entry_date page_key
    FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
    JOIN finance_accounts a ON a.id=l.account_id AND a.company_id=j.company_id
    WHERE j.company_id=? AND j.status='posted' AND a.control_kind='bank' AND j.is_opening=0
    AND (?='' OR j.chart_id=?) AND ${availableCashSource}`;
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
suggestionTargets:async(actor,chart,suggestedIds)=>{  const cashSql = `SELECT l.id,l.account_id,l.description,l.debit_minor,l.credit_minor,j.id journal_id,j.journal_number,j.currency,j.entry_date,j.entry_date page_key
    FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
    JOIN finance_accounts a ON a.id=l.account_id AND a.company_id=j.company_id
    WHERE j.company_id=? AND j.status='posted' AND a.control_kind='bank' AND j.is_opening=0
    AND (?='' OR j.chart_id=?) AND ${availableCashSource}`;
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
