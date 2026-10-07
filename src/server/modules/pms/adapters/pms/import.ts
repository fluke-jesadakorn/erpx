import "server-only";
import { createHash } from "node:crypto";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase, TransactionalSqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { ApiError } from "@/server/modules/runtime/public/http";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { loadPms, persistPmsRecords } from "@/server/modules/pms/public/pms";
import { createPmsImport, type Batch, type PmsImportUnit, type PmsImportRepository } from "../../application/import";
import type { SqlStatement } from "../../../runtime/public/runtime/types";
import { pmsImportFinanceFacts } from "../../../finance/public/pms-import-facts";
function unit(db: SqlDatabase): PmsImportUnit<SqlStatement> {
    const finance = pmsImportFinanceFacts(db);
    return {
        batches: (facts: {
            companyId: string;
            propertyId: string;
        }) => db.prepare(`SELECT id,cutover_date,state,version,created_by,approved_by,opening_journal_id,report_json,created_at FROM pms_import_batches WHERE company_id=? AND property_id=? ORDER BY created_at DESC,id LIMIT 30`).bind(facts.companyId, facts.propertyId).all<Batch>(),
        preview: (facts: {
            batchId: string;
            companyId: string;
            propertyId: string;
            cutoverDate: string;
            expectedVersion: number;
            fact5: string;
            fact6: string;
            userId: string;
            now: number;
            now2: number;
        }) => db.prepare(`INSERT INTO pms_import_batches(id,company_id,property_id,cutover_date,property_version,definition_json,report_json,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)`).bind(facts.batchId, facts.companyId, facts.propertyId, facts.cutoverDate, facts.expectedVersion, facts.fact5, facts.fact6, facts.userId, facts.now, facts.now2),
        batch: (facts: {
            batchId: string;
            companyId: string;
        }) => db.prepare("SELECT * FROM pms_import_batches WHERE id=? AND company_id=? FOR UPDATE").bind(facts.batchId, facts.companyId).first<Batch>(),
        customer: facts => finance.customer(facts),
        openingJournal: facts => finance.openingJournal(facts),
        openingInvoice: facts => finance.openingInvoice(facts),
        receivableSchedules: facts => finance.receivableSchedules(facts),
        accountBalance: facts => finance.accountBalance(facts),
        approve: (facts: {
            userId: string;
            openingJournalId: string | null;
            fact2: number;
            id: string;
        }) => db.prepare("UPDATE pms_import_batches SET state='approved',approved_by=?,opening_journal_id=?,version=version+1,updated_at=? WHERE id=?").bind(facts.userId, facts.openingJournalId, facts.fact2, facts.id),
        writeContext: () => db.prepare("SELECT set_config('erpx.pms_write','on',true)").bind().first(),
        party: facts => finance.party(facts),
        payment: facts => finance.payment(facts),
        balance: (facts: {
            id: string;
            sourceKey: string;
            property_id: string;
            id2: string;
            id3: string;
            pocket: "advance" | "deposit" | "due";
            total: number;
            paymentId: string | null;
            fact8: string | null;
        }) => db.prepare(`INSERT INTO pms_import_balances(batch_id,source_key,property_id,folio_id,entry_id,pocket,amount_minor,finance_payment_id,finance_invoice_id) VALUES(?,?,?,?,?,?,?,?,?)`).bind(facts.id, facts.sourceKey, facts.property_id, facts.id2, facts.id3, facts.pocket, facts.total, facts.paymentId, facts.fact8).run(),
        document: (facts: {
            fact0: string;
            property_id: string;
            id: string;
            fact3: string | null;
            fact4: string;
            now: number;
        }) => db.prepare(`INSERT INTO pms_documents(id,property_id,folio_id,kind,finance_id,snapshot_json,created_at) VALUES(?,?,?,'opening',?,?,?)`).bind(facts.fact0, facts.property_id, facts.id, facts.fact3, facts.fact4, facts.now).run(),
        property: (facts: {
            now: number;
            property_id: string;
        }) => db.prepare("UPDATE pms_properties SET version=version+1,updated_at=? WHERE id=?").bind(facts.now, facts.property_id),
        applied: (facts: {
            now: number;
            id: string;
        }) => db.prepare("UPDATE pms_import_batches SET state='applied',version=version+1,updated_at=? WHERE id=?").bind(facts.now, facts.id), snapshot: (actor, id, lock) => loadPms(db, actor, id, lock), persist: (s, original) => persistPmsRecords(db, s, original), replay: (...args) => replayCommand(db, ...args), execute: (...args) => executeCommand(db, ...args)
    };
}
function repository(db: TransactionalSqlDatabase): PmsImportRepository<SqlStatement> { return { ...unit(db), transaction: work => db.transaction(tx => work(unit(tx))) }; }
const application = createPmsImport<SqlStatement>({ clock: { now: () => Date.now() }, hash: value => createHash("sha256").update(value).digest("hex"), require: requirePermission, error: (status, code, message) => new ApiError(status, code, message), isBusinessError: (error): error is ApiError => error instanceof ApiError });
export const pmsImportDecisionSchema = application.pmsImportDecisionSchema;
export function pmsImportBatches(db: SqlDatabase, actor: WorkContext, propertyId: string) { return application.pmsImportBatches(unit(db), actor, propertyId); }
export function previewPmsImport(db: TransactionalSqlDatabase, actor: WorkContext, raw: unknown, key: string) { return application.previewPmsImport(repository(db), actor, raw, key); }
export function approvePmsImport(db: TransactionalSqlDatabase, actor: WorkContext, raw: unknown, key: string) { return application.approvePmsImport(repository(db), actor, raw, key); }
export function applyPmsImport(db: TransactionalSqlDatabase, actor: WorkContext, raw: unknown, key: string) { return application.applyPmsImport(repository(db), actor, raw, key); }
