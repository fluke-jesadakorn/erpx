import { recognitionRunSchema } from '../domain/due-inputs';
import { billingRevisionSchema } from '../domain/business-billing-revision';
import "server-only";
import { z } from "zod";
import { businessContractSchema, businessBillingSchema, type BusinessContractInput } from "@/server/modules/finance/public/rules/finance-contract-policy";
import { prepareOwnedJournalReversal } from "@/server/modules/finance/public/finance-owned-reversal";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { assertion, executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { ApiError } from "@/server/modules/runtime/public/http";
import { sha256 } from "@/server/modules/runtime/public/crypto";
import { financeTransaction, financeOwnedTransaction } from "@/server/modules/finance/public/finance-transactions";
import { requireFinanceTool } from "@/server/modules/finance/public/finance-company-tools";
import { requireFinanceMoneyScale } from "@/server/modules/finance/public/finance-money-policy";
import { createInvoice, prepareJournal } from "@/server/modules/finance/public/finance";
import type { SqlStatement } from '../../runtime/public/runtime/types';
import { BusinessRuleError } from '../../shared';
import { financeBusinessContractEffects } from '../../workflow/public/finance-business-contract-effects';
import { createBusinessContractApplication, type BusinessContractPorts, type BusinessContractRepository } from '../application/business-contracts';
import { businessDecisionSchema, recognitionReversalSchema, type Row, type BillingRevision } from '../domain/business-contracts';
export { businessDecisionSchema, recognitionReversalSchema } from '../domain/business-contracts';
export { businessContractSchema, businessBillingSchema, recognitionRunSchema, billingRevisionSchema };
async function inward<T>(action: () => Promise<T>) {
    try {
        return await action();
    }
    catch (error) {
        if (error instanceof ApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
async function exposed<T>(action: () => Promise<T>) {
    try {
        return await action();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}
function bindPorts(read: () => SqlDatabase, base: () => SqlDatabase, owned: SqlDatabase | undefined, prepare: (holdingId: string) => Promise<void>): BusinessContractPorts<SqlStatement> {
    const repository: BusinessContractRepository<SqlStatement> = { async contract(id: string, companyId: string) {
            const db = read();
            return db
                .prepare("SELECT * FROM finance_business_contracts WHERE id=? AND company_id=?")
                .bind(id, companyId)
                .first<Row>();
        }, async configurationChart(chartId: string, companyId: string) {
            const db = read();
            return db
                .prepare("SELECT id,base_currency,status FROM finance_charts WHERE id=? AND company_id=?")
                .bind(chartId, companyId)
                .first<Row>();
        }, async secondaryBook(companyId: string, chartId: string) {
            const db = read();
            return db
                .prepare("SELECT 1 FROM finance_book_rules WHERE company_id=? AND secondary_chart_id=? AND status='approved'")
                .bind(companyId, chartId)
                .first();
        }, async configurationAccounts(companyId: string, chartId: string, ids: string[]) {
            const db = read();
            return db
                .prepare(`SELECT id,category,control_kind,currency,version FROM finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND id IN (${ids.map(() => "?").join(",")}) ORDER BY id`)
                .bind(companyId, chartId, ...ids)
                .all<Row>();
        }, async customer(partyId: string, companyId: string, currency: string) {
            const db = read();
            return db
                .prepare("SELECT id,version FROM finance_parties WHERE id=? AND company_id=? AND status='active' AND kind='customer' AND currency=? AND person_id IS NULL")
                .bind(partyId, companyId, currency)
                .first<Row>();
        }, async dimensions(companyId: string) {
            const db = read();
            return db
                .prepare("SELECT id,code,required FROM finance_dimension_definitions WHERE company_id=? AND status='active' ORDER BY code")
                .bind(companyId)
                .all<Row>();
        }, async dimensionValue(companyId: string, id: string | number | null, value: string) {
            const db = read();
            return db
                .prepare("SELECT 1 FROM finance_dimension_values WHERE company_id=? AND dimension_id=? AND code=? AND status='active'")
                .bind(companyId, id, value)
                .first();
        }, async taxCode(companyId: string, taxCode: string | undefined, taxCode2: string | undefined, startDate: string, startDate2: string) {
            const db = read();
            return db
                .prepare("SELECT id,code,rate_micros,effective_from,effective_to FROM finance_tax_codes WHERE company_id=? AND status='active' AND (code=? OR id=?) AND rate_micros=70000 AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from DESC LIMIT 1")
                .bind(companyId, taxCode, taxCode2, startDate, startDate2)
                .first<Row>();
        }, async billedWindow(root_id: string | number | null, startDate: string) {
            const db = read();
            return db
                .prepare("SELECT 1 FROM finance_business_billing_runs WHERE root_id=? AND service_to>=?")
                .bind(root_id, startDate)
                .first();
        }, guardContractParent(key: string, value: "contract-parent", id: string | number | null, companyId: string, value2: number) {
            const db = read();
            return assertion(db, key, value, "EXISTS(SELECT 1 FROM finance_business_contracts WHERE id=? AND company_id=? AND status='active' AND version=?)", [id, companyId, value2]);
        }, prepareContract(id: string, companyId: string, root: string, value: string | number | null, number: string, name: string, chartId: string, partyId: string, startDate: string, endDate: string, edition: number, value2: string, sourceHash: string, userId: string, reason: string, now: number, now2: number) {
            const db = read();
            return db
                .prepare("INSERT INTO finance_business_contracts(id,company_id,root_id,parent_id,number,name,chart_id,party_id,start_date,end_date,edition,content_json,source_hash,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(id, companyId, root, value, number, name, chartId, partyId, startDate, endDate, edition, value2, sourceHash, userId, reason, now, now2);
        }, async retainedBilledWindow(root_id: string | number | null, startDate: string) {
            const db = read();
            return db
                .prepare("SELECT 1 FROM finance_business_billing_runs WHERE root_id=? AND service_to>=?")
                .bind(root_id, startDate)
                .first();
        }, guardContractVersion(key: string, value: "contract-version", id: string, companyId: string, status: "draft" | "active", expectedVersion: number) {
            const db = read();
            return assertion(db, key, value, "EXISTS(SELECT 1 FROM finance_business_contracts WHERE id=? AND company_id=? AND status=? AND version=?)", [id, companyId, status, expectedVersion]);
        }, supersedeParent(now: number, id: string | number | null) {
            const db = read();
            return db
                .prepare("UPDATE finance_business_contracts SET status='superseded',version=version+1,updated_at=? WHERE id=?")
                .bind(now, id);
        }, decideContract(status: "active" | "rejected" | "cancelled", userId: string, version: number, now: number, id: string) {
            const db = read();
            return db
                .prepare("UPDATE finance_business_contracts SET status=?,reviewed_by=?,version=?,updated_at=? WHERE id=?")
                .bind(status, userId, version, now, id);
        }, async postedOwnerCredit(postedCorrectionId: string, companyId: string, userId: string, id: string) { const db = read(); return db.prepare("SELECT * FROM finance_billing_corrections WHERE id=? AND company_id=? AND status='posted' AND reviewed_by=? AND replacement_bill_id=?").bind(postedCorrectionId, companyId, userId, id).first<Row>(); }, async parentBill(parentId: unknown, companyId: string, contractId: string) {
            const db = read();
            return db
                .prepare("SELECT b.*,i.status original_invoice_status,i.version original_invoice_version,i.journal_id original_journal_id,i.posted_at original_posted_at FROM finance_business_billing_runs b LEFT JOIN finance_invoices i ON i.id=b.invoice_id WHERE b.id=? AND b.company_id=? AND b.contract_id=?")
                .bind(parentId, companyId, contractId)
                .first<Row>();
        }, async childBill(parentId: unknown) {
            const db = read();
            return db
                .prepare("SELECT 1 FROM finance_business_billing_runs WHERE parent_run_id=?")
                .bind(parentId)
                .first();
        }, async recognitionExists(id: string | number | null) {
            const db = read();
            return db
                .prepare("SELECT 1 FROM finance_recognition_runs WHERE billing_run_id=? AND status IN ('draft','posted')")
                .bind(id)
                .first();
        }, async billingExists(root_id: string | number | null, billingKey: string) {
            const db = read();
            return db
                .prepare("SELECT 1 FROM finance_business_billing_runs WHERE root_id=? AND billing_key=?")
                .bind(root_id, billingKey)
                .first();
        }, guardBillParent(key: string, value: "billing-parent-version", id: string | number | null, companyId: string, version: string | number | null, id2: string | number | null) {
            const db = read();
            return assertion(db, key, value, "EXISTS(SELECT 1 FROM finance_business_billing_runs WHERE id=? AND company_id=? AND version=?) AND NOT EXISTS(SELECT 1 FROM finance_business_billing_runs WHERE parent_run_id=?)", [id, companyId, version, id2]);
        }, guardParentInvoice(key: string, value: "billing-parent-invoice", invoice_id: string | number, companyId: string, original_invoice_version: string | number | null) {
            const db = read();
            return assertion(db, key, value, "EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND version=? AND status IN ('draft','voided') AND journal_id IS NULL AND posted_at IS NULL)", [
                invoice_id,
                companyId,
                original_invoice_version,
            ]);
        }, voidParentInvoice(now: number, invoice_id: string | number, companyId: string) {
            const db = read();
            return db
                .prepare("UPDATE finance_invoices SET status='voided',outstanding_minor=0,base_outstanding_minor=0,version=version+1,updated_at=? WHERE id=? AND company_id=?")
                .bind(now, invoice_id, companyId);
        }, guardBillingContract(key: string, value: "contract-version", contractId: string, companyId: string, version: number, retainedEdition: boolean) {
            const db = read();
            return assertion(db, key, value, `EXISTS(SELECT 1 FROM finance_business_contracts WHERE id=? AND company_id=? AND status ${retainedEdition ? "IN ('active','cancelled','superseded')" : "='active'"} AND version=?)`, [contractId, companyId, version]);
        }, prepareBill(id: string, companyId: string, contractId: string, root_id: string | number | null, version: number, billingKey: string, serviceFrom: string, serviceTo: string, value: string, value2: string, sourceHash: string, value3: string | null, value4: number | null, value5: string | number, now: number, value6: "draft" | null, value7: string | number | null, value8: string | number | null, value9: number, value10: string | null, value11: string | null) {
            const db = read();
            return db
                .prepare("INSERT INTO finance_business_billing_runs(id,company_id,contract_id,root_id,contract_version,billing_key,service_from,service_to,input_json,result_json,source_hash,invoice_id,invoice_version,created_by,created_at,no_charge_status,parent_run_id,parent_version,bill_edition,contract_content_json,revision_reason) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(id, companyId, contractId, root_id, version, billingKey, serviceFrom, serviceTo, value, value2, sourceHash, value3, value4, value5, now, value6, value7, value8, value9, value10, value11);
        }, async billContractId(parentId: string, companyId: string) {
            const db = read();
            return db
                .prepare("SELECT contract_id FROM finance_business_billing_runs WHERE id=? AND company_id=?")
                .bind(parentId, companyId)
                .first<{
                contract_id: string;
            }>();
        }, async billSource(billingId: string, companyId: string) {
            const db = read();
            return db
                .prepare("SELECT b.*,COALESCE(b.contract_content_json,c.content_json) content_json FROM finance_business_billing_runs b JOIN finance_business_contracts c ON c.id=b.contract_id WHERE b.id=? AND b.company_id=?")
                .bind(billingId, companyId)
                .first<Row>();
        }, async postedCorrection(id: string | number | null) { const db = read(); return db.prepare("SELECT 1 FROM finance_billing_corrections WHERE billing_run_id=? AND status='posted'").bind(id).first(); }, async recognitionInvoice(invoice_id: string | number | null, companyId: string) {
            const db = read();
            return db
                .prepare("SELECT i.id,i.status,i.line_revision,i.subtotal_minor,i.tax_minor,i.currency,i.debit_account_id,i.credit_account_id,j.id journal_id,j.version journal_version,j.status journal_status,j.entry_date posted_date FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id WHERE i.id=? AND i.company_id=?")
                .bind(invoice_id, companyId)
                .first<Row>();
        }, async openPeriod(periodId: unknown, companyId: string, entryDate: unknown, entryDate2: unknown) {
            const db = read();
            return db
                .prepare("SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?")
                .bind(periodId, companyId, entryDate, entryDate2)
                .first();
        }, async recognitionExistsItem(billingRunId: unknown, itemKey: unknown) {
            const db = read();
            return db
                .prepare("SELECT 1 FROM finance_recognition_runs WHERE billing_run_id=? AND item_key=? AND status IN ('draft','posted')")
                .bind(billingRunId, itemKey)
                .first();
        }, prepareRecognition(id: string, companyId: string, billingRunId: unknown, itemKey: unknown, date: string, entryDate: unknown, periodId: unknown, value: number, accountId: string, sourceHash: string, userId: string, reason: unknown, now: number, now2: number) {
            const db = read();
            return db
                .prepare("INSERT INTO finance_recognition_runs(id,company_id,billing_run_id,item_key,due_date,entry_date,period_id,amount_minor,account_id,source_hash,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(id, companyId, billingRunId, itemKey, date, entryDate, periodId, value, accountId, sourceHash, userId, reason, now, now2);
        }, async recognition(id: string, companyId: string) {
            const db = read();
            return db
                .prepare("SELECT * FROM finance_recognition_runs WHERE id=? AND company_id=?")
                .bind(id, companyId)
                .first<Row>();
        }, guardRecognitionDraft(key: string, value: "recognition-version", id: string, companyId: string, expectedVersion: number) {
            const db = read();
            return assertion(db, key, value, "EXISTS(SELECT 1 FROM finance_recognition_runs WHERE id=? AND company_id=? AND status='draft' AND version=?)", [id, companyId, expectedVersion]);
        }, decideRecognition(status: "rejected" | "posted", version: number, userId: string, value: string | null, reason: string, now: number, id: string) {
            const db = read();
            return db
                .prepare("UPDATE finance_recognition_runs SET status=?,version=?,reviewed_by=?,journal_id=?,reason=?,updated_at=? WHERE id=?")
                .bind(status, version, userId, value, reason, now, id);
        }, async postedRecognition(id: string, companyId: string) {
            const db = read();
            return db
                .prepare("SELECT * FROM finance_recognition_runs WHERE id=? AND company_id=? AND status='posted'")
                .bind(id, companyId)
                .first<Row>();
        }, guardPostedRecognition(key: string, value: "recognition-version", id: string, companyId: string, expectedVersion: number) {
            const db = read();
            return assertion(db, key, value, "EXISTS(SELECT 1 FROM finance_recognition_runs WHERE id=? AND company_id=? AND status='posted' AND version=?)", [id, companyId, expectedVersion]);
        }, reverseRecognition(reversalId: string, reason: string, now: number, id: string) {
            const db = read();
            return db
                .prepare("UPDATE finance_recognition_runs SET status='reversed',version=version+1,reversal_journal_id=?,reason=?,updated_at=? WHERE id=?")
                .bind(reversalId, reason, now, id);
        }, async zeroBill(id: string, companyId: string) {
            const db = read();
            return db
                .prepare("SELECT * FROM finance_business_billing_runs WHERE id=? AND company_id=? AND invoice_id IS NULL")
                .bind(id, companyId)
                .first<Row>();
        }, async zeroChildBill(id: string) {
            const db = read();
            return db
                .prepare("SELECT 1 FROM finance_business_billing_runs WHERE parent_run_id=?")
                .bind(id)
                .first();
        }, async zeroParentBill(parent_run_id: string | number, companyId: string) {
            const db = read();
            return db
                .prepare("SELECT * FROM finance_business_billing_runs WHERE id=? AND company_id=?")
                .bind(parent_run_id, companyId)
                .first<Row>();
        }, guardZeroBill(key: string, value: "zero-usage-version", id: string, companyId: string, expectedVersion: number, id2: string) {
            const db = read();
            return assertion(db, key, value, "EXISTS(SELECT 1 FROM finance_business_billing_runs WHERE id=? AND company_id=? AND version=? AND invoice_id IS NULL AND no_charge_status='draft') AND NOT EXISTS(SELECT 1 FROM finance_business_billing_runs WHERE parent_run_id=?)", [id, companyId, expectedVersion, id2]);
        }, decideZeroBill(value: "rejected" | "approved", userId: string, reason: string, id: string) {
            const db = read();
            return db
                .prepare("UPDATE finance_business_billing_runs SET no_charge_status=?,reviewed_by=?,review_reason=?,version=version+1 WHERE id=?")
                .bind(value, userId, reason, id);
        }, async workspaceContracts(companyId: string) {
            const db = read();
            return db
                .prepare("SELECT * FROM finance_business_contracts WHERE company_id=? ORDER BY created_at DESC LIMIT 200")
                .bind(companyId)
                .all<Row>();
        }, async workspaceBills(companyId: string) {
            const db = read();
            return db
                .prepare("SELECT b.*,COALESCE(b.contract_content_json,c.content_json) effective_contract_content_json,COALESCE(i.status,b.no_charge_status) invoice_status,i.invoice_number,i.version current_invoice_version,CASE WHEN EXISTS(SELECT 1 FROM finance_business_billing_runs child WHERE child.parent_run_id=b.id) THEN 0 ELSE 1 END is_current FROM finance_business_billing_runs b JOIN finance_business_contracts c ON c.id=b.contract_id AND c.company_id=b.company_id LEFT JOIN finance_invoices i ON i.id=b.invoice_id WHERE b.company_id=? ORDER BY b.created_at DESC LIMIT 500")
                .bind(companyId)
                .all<Row>();
        }, async workspaceRecognitions(companyId: string) {
            const db = read();
            return db
                .prepare("SELECT * FROM finance_recognition_runs WHERE company_id=? ORDER BY created_at DESC LIMIT 500")
                .bind(companyId)
                .all<Row>();
        }, async workspaceAccounts(companyId: string) {
            const db = read();
            return db
                .prepare("SELECT id,chart_id,code,name,category,currency,control_kind FROM finance_accounts WHERE company_id=? AND status='active' ORDER BY code")
                .bind(companyId)
                .all<Row>();
        }, async workspaceCustomers(companyId: string) {
            const db = read();
            return db
                .prepare("SELECT id,name,currency FROM finance_parties WHERE company_id=? AND kind='customer' AND status='active' AND person_id IS NULL ORDER BY name")
                .bind(companyId)
                .all<Row>();
        }, async workspaceCharts(companyId: string) {
            const db = read();
            return db
                .prepare("SELECT c.id,c.name,c.base_currency FROM finance_charts c WHERE c.company_id=? AND c.status='active' AND NOT EXISTS(SELECT 1 FROM finance_book_rules r WHERE r.company_id=c.company_id AND r.secondary_chart_id=c.id AND r.status='approved') ORDER BY c.name")
                .bind(companyId)
                .all<Row>();
        }, async workspacePeriods(companyId: string) {
            const db = read();
            return db
                .prepare("SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date DESC")
                .bind(companyId)
                .all<Row>();
        }, async workspaceCorrections(companyId: string) { const db = read(); return db.prepare('SELECT * FROM finance_billing_corrections WHERE company_id=? ORDER BY created_at DESC LIMIT 500').bind(companyId).all<Row>(); }, contractDecisionEvidence(value: string, key: string, id: string, op: string, userId: string, assignmentId: string, expectedVersion: number, value2: string, now: number) { return financeBusinessContractEffects(read()).contractDecisionEvidence(value, key, id, op, userId, assignmentId, expectedVersion, value2, now); }, recognitionDecisionEvidence(value: string, key: string, id: string, op: string, userId: string, assignmentId: string, expectedVersion: number, value2: string, now: number) { return financeBusinessContractEffects(read()).recognitionDecisionEvidence(value, key, id, op, userId, assignmentId, expectedVersion, value2, now); } };
    return { repository, prepare, clock: { now: () => Date.now() }, identifiers: { next: () => crypto.randomUUID() }, hash: sha256, requirePermission: (actor, permission) => {
            try {
                return requirePermission(actor, permission);
            }
            catch (error) {
                if (error instanceof ApiError)
                    throw new BusinessRuleError(error.status, error.code, error.message);
                throw error;
            }
        }, requireMoneyScale: (...args) => inward(() => requireFinanceMoneyScale(read(), ...args)), requireTool: (...args) => inward(() => requireFinanceTool(read(), ...args)), transaction: work => inward(() => financeTransaction(base(), db => work(bindPorts(() => db, base, db, async (holdingId) => { await holdingResources(holdingId); })))), ownedTransaction: (available, work) => inward(() => financeOwnedTransaction(base(), available ? owned : undefined, db => work(bindPorts(() => db, base, db, async (holdingId) => { await holdingResources(holdingId); })))), createInvoice: (actor, input, key) => inward(async () => await createInvoice(actor, input, key, undefined, read()) as {
            id: string;
            version: number;
        }), prepareJournal: (...args) => inward(() => prepareJournal(read(), ...args)), prepareReversal: (...args) => inward(() => prepareOwnedJournalReversal(read(), ...args)), commands: { replay: (...args) => inward(() => replayCommand(read(), ...args)), commit: (...args) => inward(() => executeCommand(read(), ...args)) } };
}
function application(owned?: SqlDatabase) {
    let db: SqlDatabase | undefined;
    const read = () => {
        if (!db)
            throw Error('BUSINESS_CONTRACT_CONTEXT_REQUIRED');
        return db;
    };
    const ports = bindPorts(read, read, owned, async (holdingId) => { db = (await holdingResources(holdingId)).write; });
    return createBusinessContractApplication<SqlStatement>(ports);
}
export function prepareBusinessContract(actor: WorkContext, raw: z.input<typeof businessContractSchema>, key: string) { return exposed(() => application().prepareBusinessContract(actor, raw, key)); }
export function decideBusinessContract(actor: WorkContext, id: string, raw: z.input<typeof businessDecisionSchema>, action: "approve" | "reject" | "cancel", key: string) { return exposed(() => application().decideBusinessContract(actor, id, raw, action, key)); }
export function prepareBusinessBilling(actor: WorkContext, contractId: string, raw: z.input<typeof businessBillingSchema>, key: string, ownedTransaction?: SqlDatabase, revision?: BillingRevision, postedCorrectionId?: string) { return exposed(() => application(ownedTransaction).prepareBusinessBilling(actor, contractId, raw, key, Boolean(ownedTransaction), revision, postedCorrectionId)); }
export function prepareBusinessBillingRevision(actor: WorkContext, parentId: string, raw: z.input<typeof billingRevisionSchema>, key: string) { return exposed(() => application().prepareBusinessBillingRevision(actor, parentId, raw, key)); }
export function prepareRecognitionRun(actor: WorkContext, raw: z.input<typeof recognitionRunSchema>, key: string, ownedTransaction?: SqlDatabase) { return exposed(() => application(ownedTransaction).prepareRecognitionRun(actor, raw, key, Boolean(ownedTransaction))); }
export function decideRecognitionRun(actor: WorkContext, id: string, raw: z.input<typeof businessDecisionSchema>, action: "post" | "reject", key: string) { return exposed(() => application().decideRecognitionRun(actor, id, raw, action, key)); }
export function reverseRecognitionRun(actor: WorkContext, id: string, raw: z.input<typeof recognitionReversalSchema>, key: string) { return exposed(() => application().reverseRecognitionRun(actor, id, raw, key)); }
export function decideNoChargeBilling(actor: WorkContext, id: string, raw: z.input<typeof businessDecisionSchema>, action: "approve" | "reject", key: string) { return exposed(() => application().decideNoChargeBilling(actor, id, raw, action, key)); }
export function businessContractWorkspace(actor: WorkContext) { return exposed(() => application().businessContractWorkspace(actor)); }
export function businessContractConfiguration(db: SqlDatabase, actor: WorkContext, input: BusinessContractInput) { return exposed(() => createBusinessContractApplication<SqlStatement>(bindPorts(() => db, () => db, db, async () => { })).businessContractConfiguration(actor, input)); }
