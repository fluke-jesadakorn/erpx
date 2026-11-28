import 'server-only';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {requirePermission} from '@erpx/identity-access-server/public/iam-permissions';
import {sha256} from '@erpx/platform-runtime-server/public/crypto';
import {getInvoice} from '@erpx/finance-server/public/finance-invoice-read';
import { BANK_TRANSACTION_STATEMENT_SQL } from './finance-read-tail-r34-sql';
import {statements} from '@erpx/finance-server/public/finance';
import {closeWorkspace,financeWorkbench} from '@erpx/finance-server/public/finance-workspace';
import {vatWorkspace} from '@erpx/finance-server/public/finance-tax-register';
import {financeAiDate} from "@erpx/finance-ui/lib/finance-ai";
import {financeAiScope,validateFinanceAiResult as validate,type FinanceAiInput,type FinanceAiSnapshot} from '@erpx/finance-server/domain/ai-source';
import {createFinanceAiSource} from '@erpx/finance-server/application/ai-source';
export type {FinanceAiInput,FinanceAiSource,FinanceAiSnapshot} from '@erpx/finance-server/domain/ai-source';
type Row=Record<string,string|number|null>;
export const requireFinanceAiScope=(actor:WorkContext)=>financeAiScope(actor,requirePermission,(...args)=>new ApiError(...args));
export const validateFinanceAiResult=(raw:unknown,snapshot:FinanceAiSnapshot)=>validate(raw,snapshot,(...args)=>new ApiError(...args));
// Owner commands write their source changes and authority-stamped receipt together.
// This also invalidates aggregate explanations after an upstream PMS/Commerce change.
export {financeSourceRevisionSql as financeRevisionSql,financeSourceRevision as financeAiRevision} from '@erpx/workflow-server/public/finance-ai-revision-sources';
import {financeSourceRevision as financeAiRevision} from '@erpx/workflow-server/public/finance-ai-revision-sources';


export function financeAiSnapshot(db:SqlDatabase,actor:WorkContext,raw:FinanceAiInput,timezone:string){return createFinanceAiSource({error:(...args)=>new ApiError(...args),permission:requirePermission,hash:sha256,businessDate:financeAiDate,revision:companyId=>financeAiRevision(db,companyId),workbench:(a,day)=>financeWorkbench(a,day,db),invoice:(a,id)=>getInvoice(a,id,db),closeWorkspace:a=>closeWorkspace(a,db),vatWorkspace,statements:(a,period,chart,dates)=>statements(a,period,chart,dates,db),invoiceChart:async(actor,accountId)=>db.prepare("SELECT c.id,c.name,c.base_currency FROM finance_charts c JOIN finance_accounts a ON a.chart_id=c.id AND a.company_id=c.company_id WHERE a.id=? AND c.company_id=?").bind(accountId, actor.companyId).first<Row>(),
invoiceAccounts:async(actor,chartId,currency)=>(await db.prepare("SELECT id,code,name,category,currency,control_kind FROM finance_accounts WHERE company_id=? AND chart_id=? AND currency=? AND status='active' ORDER BY code LIMIT 200").bind(actor.companyId, chartId, currency).all<Row>()).results,
bankMovement:async(actor,id)=>db.prepare(BANK_TRANSACTION_STATEMENT_SQL).bind(id, actor.companyId).first<Row>(),
compatiblePayments:async(actor,transaction)=>(await db.prepare(`SELECT p.id,p.version,p.payment_number,p.payment_date,p.currency,p.amount_minor,p.kind,p.bank_account FROM finance_payments p JOIN finance_journals j ON j.id=p.journal_id AND j.company_id=p.company_id WHERE p.company_id=? AND p.currency=? AND p.bank_account=? AND p.status IN ('released','settled') AND j.status='posted' AND CASE WHEN p.kind='receipt' THEN p.amount_minor ELSE -p.amount_minor END=? AND NOT EXISTS(SELECT 1 FROM finance_bank_transactions t WHERE t.company_id=p.company_id AND t.matched_type='payment' AND t.matched_id=p.id AND t.match_status='matched') ORDER BY p.payment_date,p.id LIMIT 100`).bind(actor.companyId, transaction.currency, transaction.bank_account, transaction.amount_minor).all<Row>()).results,
periodVersion:async(actor,id)=>db.prepare("SELECT close_version FROM finance_periods WHERE company_id=? AND id=?").bind(actor.companyId, id).first<Row>(),
reportChart:async(actor,id)=>db.prepare("SELECT id,name,base_currency,accounting_basis FROM finance_charts WHERE id=? AND company_id=?").bind(id, actor.companyId).first<Row>()}).financeAiSnapshot(actor,raw,timezone);}
