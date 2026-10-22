import {withFinancePurchasedCommand,withFinancePurchasedRead,requireFinancePurchasedRead} from "@erpx/server-finance/adapters/purchase-boundary";
import {financeCommandReceipts} from "@erpx/server-workflow/public/finance-command-receipts";
import type { z } from "zod";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { ApiError } from "@erpx/server-runtime/public/http";
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { invoiceHistoryCte } from "@erpx/server-finance/public/finance-history";
import type {financeReviewSchema} from "@erpx/server-finance/domain/review";
import { requireFinanceTool } from "@erpx/server-finance/public/finance-company-tools";
import { readListPage } from "@erpx/server-work_projection/public/list-page";
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {createFinanceCollections} from '@erpx/server-finance/application/collections';
import type {CollectionInvoice,CollectionQuery,collectionCaseSchema,reviseCollectionCaseSchema} from '@erpx/server-finance/domain/collections';
export {collectionCaseSchema,reviseCollectionCaseSchema} from '@erpx/server-finance/domain/collections';
export type {CollectionQuery} from '@erpx/server-finance/domain/collections';
function nativeCollections(provided?:SqlDatabase){let write:SqlDatabase;return createFinanceCollections({error:(...args)=>new ApiError(...args),permission:requirePermission,samePerson,ready:async actor=>{write=provided??(await holdingResources(actor.holdingId)).write},now:Date.now,id:()=>crypto.randomUUID(),hash:sha256,tool:actor=>requireFinanceTool(write,actor.companyId,'collections'),replay:async(actor,key,op,input)=>{const r=await financeCommandReceipts(write).record(key);return r?replayCommand(write,actor,key,op,r.record_id,input):null},reviewReplay:(actor,key,op,id,input)=>replayCommand(write,actor,key,op,id,input),invoice:async(actor,id)=>write
    .prepare(
      "SELECT i.*,p.name party_name,p.version party_version FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id AND p.company_id=i.company_id WHERE i.id=? AND i.company_id=? AND i.kind='receivable' AND i.status IN ('posted','partially_paid') AND i.outstanding_minor>0",
    )
    .bind(id, actor.companyId)
    .first<CollectionInvoice>(),
existingStage:async(actor,row,ageStage)=>write
      .prepare(
        "SELECT 1 FROM finance_collection_cases WHERE company_id=? AND invoice_id=? AND invoice_version=? AND stage=?",
      )
      .bind(actor.companyId, row.id, row.version, ageStage)
      .first(),
prepareCommit:async(facts)=>{const {actor,key,op,input,id,row,ageStage,message,hash}=facts;return executeCommand(
    write,
    actor,
    key,
    op,
    input,
    id,
    1,
    { id, status: "draft", version: 1, message, stage: ageStage },
    [
      assertion(
        write,
        key,
        "tool",
        "EXISTS(SELECT 1 FROM finance_company_tools WHERE company_id=? AND tool='collections' AND enabled=1)",
        [actor.companyId],
      ),
      assertion(
        write,
        key,
        "invoice",
        "EXISTS(SELECT 1 FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id AND p.company_id=i.company_id WHERE i.id=? AND i.company_id=? AND i.version=? AND i.outstanding_minor=? AND i.status IN ('posted','partially_paid') AND p.version=?)",
        [
          row.id,
          actor.companyId,
          row.version,
          row.outstanding_minor,
          row.party_version,
        ],
      ),
      write
        .prepare(
          "INSERT INTO finance_collection_cases(id,company_id,invoice_id,invoice_version,as_of_date,stage,amount_minor,currency,message,source_hash,prepared_by,created_at,root_id) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          id,
          actor.companyId,
          row.id,
          row.version,
          input.asOf,
          ageStage,
          row.outstanding_minor,
          row.currency,
          message,
          hash,
          actor.userId,
          Date.now(),
          id,
        ),
    ],
  );},
parent:async(actor,parentId)=>write
    .prepare(
      "SELECT * FROM finance_collection_cases WHERE id=? AND company_id=?",
    )
    .bind(parentId, actor.companyId)
    .first<{
      id: string;
      root_id: string;
      revision_number: number;
      version: number;
      invoice_id: string;
      stage: string;
      status: string;
      as_of_date: string;
    }>(),
hasChild:async(parentId)=>write
      .prepare("SELECT 1 FROM finance_collection_cases WHERE parent_case_id=?")
      .bind(parentId)
      .first(),
hasDelivery:async(rootId)=>write
      .prepare(
        "SELECT 1 FROM finance_collection_deliveries d JOIN finance_collection_cases c ON c.id=d.case_id WHERE c.root_id=?",
      )
      .bind(rootId)
      .first(),
reviseCommit:async(facts)=>{const {actor,key,op,parentId,input,id,parent,invoice,message,sourceHash,now}=facts;return executeCommand(
    write,
    actor,
    key,
    op,
    { parentId, ...input },
    id,
    1,
    {
      id,
      rootId: parent.root_id,
      parentId,
      revision: parent.revision_number + 1,
      version: 1,
      status: "draft",
      message,
      stage: parent.stage,
    },
    [
      assertion(
        write,
        key,
        "tool",
        "EXISTS(SELECT 1 FROM finance_company_tools WHERE company_id=? AND tool='collections' AND enabled=1)",
        [actor.companyId],
      ),
      assertion(
        write,
        key,
        "parent",
        "EXISTS(SELECT 1 FROM finance_collection_cases WHERE id=? AND company_id=? AND version=? AND status=?) AND NOT EXISTS(SELECT 1 FROM finance_collection_cases WHERE parent_case_id=?) AND NOT EXISTS(SELECT 1 FROM finance_collection_deliveries d JOIN finance_collection_cases c ON c.id=d.case_id WHERE c.root_id=?)",
        [
          parentId,
          actor.companyId,
          input.expectedVersion,
          parent.status,
          parentId,
          parent.root_id,
        ],
      ),
      assertion(
        write,
        key,
        "invoice",
        "EXISTS(SELECT 1 FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id AND p.company_id=i.company_id WHERE i.id=? AND i.company_id=? AND i.version=? AND i.outstanding_minor=? AND i.status=? AND p.version=?)",
        [
          invoice.id,
          actor.companyId,
          invoice.version,
          invoice.outstanding_minor,
          invoice.status,
          invoice.party_version,
        ],
      ),
      write
        .prepare(
          "UPDATE finance_collection_cases SET status='superseded',version=version+1 WHERE id=?",
        )
        .bind(parentId),
      write
        .prepare(
          "INSERT INTO finance_collection_cases(id,company_id,invoice_id,invoice_version,as_of_date,stage,amount_minor,currency,message,source_hash,prepared_by,created_at,root_id,revision_number,parent_case_id,parent_version,revision_reason) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          id,
          actor.companyId,
          invoice.id,
          invoice.version,
          input.asOf,
          parent.stage,
          invoice.outstanding_minor,
          invoice.currency,
          message,
          sourceHash,
          actor.userId,
          now,
          parent.root_id,
          parent.revision_number + 1,
          parentId,
          parent.version + 1,
          input.reason,
        ),
    ],
  );},
case:async(actor,id)=>write
    .prepare(
      "SELECT * FROM finance_collection_cases WHERE id=? AND company_id=?",
    )
    .bind(id, actor.companyId)
    .first<{
      status: string;
      version: number;
      invoice_id: string;
      invoice_version: number;
      source_hash: string;
      message: string;
      prepared_by: string;
      reviewed_by: string | null;
    }>(),
decisionCommit:async(facts)=>{const {actor,id,input,action,key,op,row,invoice,status,now,payloadHash}=facts;  const steps = [];
  if (action !== "cancel") {
    steps.push(
      assertion(
        write,
        key,
        "invoice",
        "EXISTS(SELECT 1 FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id WHERE i.id=? AND i.company_id=? AND i.version=? AND i.outstanding_minor=? AND i.status=? AND p.version=?)",
        [
          invoice!.id,
          actor.companyId,
          invoice!.version,
          invoice!.outstanding_minor,
          invoice!.status,
          invoice!.party_version,
        ],
      ),
    );
  }
  steps.push(
    assertion(
      write,
      key,
      "case-version",
      "EXISTS(SELECT 1 FROM finance_collection_cases WHERE id=? AND company_id=? AND version=? AND status=?)",
      [id, actor.companyId, input.expectedVersion, row.status],
    ),
  );
  if (action === "test-deliver")
    steps.push(
      write
        .prepare(
          "INSERT INTO finance_collection_deliveries(case_id,company_id,adapter,delivered_by,payload_hash,delivered_at) VALUES(?,?,'test',?,?,?)",
        )
        .bind(
          id,
          actor.companyId,
          actor.userId,
          payloadHash,
          now,
        ),
    );
  steps.push(
    write
      .prepare(
        "UPDATE finance_collection_cases SET status=?,version=version+1,reviewed_by=CASE WHEN ?='approved' THEN ? ELSE reviewed_by END,review_reason=CASE WHEN ?='approved' THEN ? ELSE review_reason END,reviewed_at=CASE WHEN ?='approved' THEN ? ELSE reviewed_at END WHERE id=?",
      )
      .bind(
        status,
        status,
        actor.userId,
        status,
        input.reason,
        status,
        now,
        id,
      ),
  );
  return executeCommand(
    write,
    actor,
    key,
    op,
    input,
    id,
    row.version + 1,
    {
      id,
      status,
      version: row.version + 1,
      adapter: action === "test-deliver" ? "test" : null,
    },
    steps,
  );},
picker:async(actor,asOf,query,search)=>{  const [invoices, cases] = await Promise.all([
    readListPage<Record<string, unknown> & { id: string; page_key: string }>(
      write,
      {
        sql: "SELECT i.id,i.invoice_number,i.due_date,i.currency,i.outstanding_minor,i.version,p.name party_name,i.due_date page_key FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id AND p.company_id=i.company_id WHERE i.company_id=? AND i.kind='receivable' AND i.status IN ('posted','partially_paid') AND i.outstanding_minor>0 AND i.due_date<? AND (?='' OR strpos(lower(i.invoice_number||' '||p.name),lower(?))>0)",
        args: [actor.companyId, asOf, search, search],
        cursor: query.invoiceCursor,
        scope: [actor.companyId, "finance-collection-invoices", asOf, search],
      },
    ),
    readListPage<Record<string, unknown> & { id: string; page_key: string }>(
      write,
      {
        sql: "SELECT c.*,i.version current_invoice_version,CASE WHEN EXISTS(SELECT 1 FROM finance_collection_cases child WHERE child.parent_case_id=c.id) THEN 0 ELSE 1 END is_current,lpad(c.created_at::text,20,'0') page_key FROM finance_collection_cases c JOIN finance_invoices i ON i.id=c.invoice_id AND i.company_id=c.company_id JOIN finance_parties p ON p.id=i.party_id AND p.company_id=c.company_id WHERE c.company_id=? AND (?='' OR strpos(lower(c.message||' '||i.invoice_number||' '||p.name),lower(?))>0)",
        args: [actor.companyId, search, search],
        cursor: query.caseCursor,
        descending: true,
        scope: [actor.companyId, "finance-collection-cases", search],
      },
    ),
  ]);
return {invoices,cases};},
chart:async(actor,chartId)=>write
      .prepare(
        "SELECT base_currency FROM finance_charts WHERE id=? AND company_id=?",
      )
      .bind(chartId, actor.companyId)
      .first<{ base_currency: string }>(),
bookSource:async(actor,chartId,asOf)=>write
    .prepare(
      "SELECT primary_chart_id FROM finance_book_rules WHERE company_id=? AND secondary_chart_id=? AND status='approved' AND effective_date<=? ORDER BY effective_date DESC LIMIT 1",
    )
    .bind(actor.companyId, chartId, asOf)
    .first<{ primary_chart_id: string }>(),
exposures:async(actor,asOf,chartId)=>(await write
      .prepare(
        `${invoiceHistoryCte} SELECT id,kind,due_date,currency,historical_outstanding_minor,historical_base_outstanding_minor FROM historical_invoice_balances WHERE chart_id=? AND historical_base_outstanding_minor<>0 ORDER BY due_date,id`,
      )
      .bind(
        actor.companyId,
        asOf,
        chartId,
      )
      .all<{
        id: string;
        kind: string;
        due_date: string;
        currency: string;
        historical_outstanding_minor: number;
        historical_base_outstanding_minor: number;
      }>()).results,
opening:async(actor,chartId,asOf)=>write
      .prepare(
        "SELECT COALESCE(SUM(l.base_debit_minor-l.base_credit_minor),0) value FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id JOIN finance_accounts a ON a.id=l.account_id WHERE j.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND j.entry_date<=? AND a.control_kind='bank'",
      )
      .bind(actor.companyId, chartId, asOf)
      .first("value")});}
export const prepareCollectionCase=(actor:WorkContext,input:z.infer<typeof collectionCaseSchema>,key:string)=>withFinancePurchasedCommand(actor,{key,operation:"finance.collection.prepare",purpose:"settle-existing",capability:"finance.receivables"},tx=>nativeCollections(tx).prepareCollectionCase(actor,input,key));
export const reviseCollectionCase=(actor:WorkContext,parentId:string,input:z.infer<typeof reviseCollectionCaseSchema>,key:string)=>withFinancePurchasedCommand(actor,{key,operation:"finance.collection.revise",purpose:"settle-existing",capability:"finance.receivables"},tx=>nativeCollections(tx).reviseCollectionCase(actor,parentId,input,key));
export const decideCollectionCase=(actor:WorkContext,id:string,input:z.infer<typeof financeReviewSchema>,action:'approve'|'cancel'|'test-deliver',key:string)=>withFinancePurchasedCommand(actor,{key,operation:`finance.collection.${action}`,purpose:"settle-existing",capability:"finance.receivables"},tx=>nativeCollections(tx).decideCollectionCase(actor,id,input,action,key));
export const collectionWorkspace=async(actor:WorkContext,asOf:string,query:CollectionQuery={},provided?:SqlDatabase)=>{if(provided){await requireFinancePurchasedRead(provided,actor,"finance.receivables");return nativeCollections(provided).collectionWorkspace(actor,asOf,query);}return withFinancePurchasedRead(actor,db=>nativeCollections(db).collectionWorkspace(actor,asOf,query),"finance.receivables");};
export const cashDueForecast=async(actor:WorkContext,input:{chartId:string;asOf:string},provided?:SqlDatabase)=>{if(provided){await requireFinancePurchasedRead(provided,actor,"finance.banking");return nativeCollections(provided).cashDueForecast(actor,input);}return withFinancePurchasedRead(actor,db=>nativeCollections(db).cashDueForecast(actor,input),"finance.banking");};
