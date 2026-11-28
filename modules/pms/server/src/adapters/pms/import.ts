import "server-only";
import { PMS_WRITE_CONTEXT_SQL } from "../pms-read-tail-r17-sql";
import { createHash } from "node:crypto";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase, TransactionalSqlDatabase, SqlStatement } from "@erpx/platform-runtime-server/public/runtime/types";
import { executeCommand, replayCommand } from "@erpx/workflow-server/public/commands";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";
import { loadPms, persistPmsRecords } from "../../public/pms";
import { createPmsImport, type Batch, type PmsImportUnit, type PmsImportRepository } from "@erpx/pms-server/application/import";
import { pmsImportFinanceFacts } from "@erpx/finance-server/public/pms-import-facts";
import { nativePmsConfiguration, nativePmsImportCommand, NativePmsError } from "@erpx/pms-ui/public/native-pms";
import { importBatches } from "../native-lane-20/import-batch-client";

/**
 * Cutover-import write authority is native. The six retained write facts are
 * dispatched to the pms axum command router (typed CommandKind, exact Host and
 * Origin derived from the registered Holding, Idempotency-Key); this adapter
 * keeps only owner reads and the Finance opening facts. There is no TypeScript
 * SQL fallback: a missing native configuration fails closed.
 */
function nativeWrite(actor: WorkContext, command: string, facts: Record<string, unknown>, key: string): any {
    const raw = process.env.ERPX_NATIVE_PMS_CONFIG;
    if (raw === undefined)
        throw new NativePmsError("NATIVE_PMS_UNAVAILABLE", 503, "The native Hotel import service is unavailable.");
    const origin = nativePmsConfiguration(raw).holdings.find(holding => holding.holdingSlug === actor.holdingSlug)?.origin;
    if (!origin)
        throw new NativePmsError("NATIVE_PMS_AUTHORITY_INVALID", 403, "This Holding is not registered for native import writes.");
    const request = new Request(origin, { headers: { host: new URL(origin).host, origin } });
    return nativePmsImportCommand(request, actor, actor.holdingSlug, command, facts, key);
}

function unit(db: SqlDatabase, actor: WorkContext, key = ""): PmsImportUnit<SqlStatement> {
    const finance = pmsImportFinanceFacts(db);
    const write = (command: string, facts: Record<string, unknown>): any =>
        nativeWrite(actor, command, facts, key);
    return {
        // The bounded Company/Property batch list is owned by native lane 20.
        // No SQL is issued here; a foreign Property yields the original empty
        // result and any native failure fails closed in the client.
        batches: async (facts: {
            companyId: string;
            propertyId: string;
        }) => ({
            results: ((await importBatches(db, actor, facts.companyId, facts.propertyId)) ?? []) as Batch[],
        }),
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
        }) => write("preview", {
            batchId: facts.batchId,
            companyId: facts.companyId,
            propertyId: facts.propertyId,
            cutoverDate: facts.cutoverDate,
            expectedVersion: facts.expectedVersion,
            definitionJson: facts.fact5,
            reportJson: facts.fact6,
            userId: facts.userId,
            now: facts.now,
            now2: facts.now2,
        }),
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
        }) => write("approve", {
            userId: facts.userId,
            openingJournalId: facts.openingJournalId,
            now: facts.fact2,
            id: facts.id,
        }),
        writeContext: () => db.prepare(PMS_WRITE_CONTEXT_SQL).bind().first(),
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
        }) => write("balance", {
            id: facts.id,
            sourceKey: facts.sourceKey,
            propertyId: facts.property_id,
            guestAccountId: facts.id2,
            entryId: facts.id3,
            pocket: facts.pocket,
            total: facts.total,
            paymentId: facts.paymentId,
            invoiceId: facts.fact8,
        }),
        document: (facts: {
            fact0: string;
            property_id: string;
            id: string;
            fact3: string | null;
            fact4: string;
            now: number;
        }) => write("document", {
            id: facts.fact0,
            propertyId: facts.property_id,
            guestAccountId: facts.id,
            financeId: facts.fact3,
            snapshotJson: facts.fact4,
            now: facts.now,
        }),
        property: (facts: {
            now: number;
            property_id: string;
        }) => write("property", { now: facts.now, propertyId: facts.property_id }),
        applied: (facts: {
            now: number;
            id: string;
        }) => write("applied", { now: facts.now, id: facts.id }),
        snapshot: (actor, id, lock) => loadPms(db, actor, id, lock),
        persist: (s, original) => persistPmsRecords(db, s, original),
        replay: (...args) => replayCommand(db, ...args),
        execute: (...args) => executeCommand(db, ...args)
    };
}
function repository(db: TransactionalSqlDatabase, actor: WorkContext, key = ""): PmsImportRepository<SqlStatement> {
    return { ...unit(db, actor, key), transaction: work => db.transaction(tx => work(unit(tx, actor, key))) };
}
const application = createPmsImport<SqlStatement>({ clock: { now: () => Date.now() }, hash: value => createHash("sha256").update(value).digest("hex"), require: requirePermission, error: (status, code, message) => new ApiError(status, code, message), isBusinessError: (error): error is ApiError => error instanceof ApiError });
export const pmsImportDecisionSchema = application.pmsImportDecisionSchema;
export function pmsImportBatches(db: SqlDatabase, actor: WorkContext, propertyId: string) { return application.pmsImportBatches(unit(db, actor), actor, propertyId); }
export function previewPmsImport(db: TransactionalSqlDatabase, actor: WorkContext, raw: unknown, key: string) { return application.previewPmsImport(repository(db, actor, key), actor, raw, key); }
export function approvePmsImport(db: TransactionalSqlDatabase, actor: WorkContext, raw: unknown, key: string) { return application.approvePmsImport(repository(db, actor, key), actor, raw, key); }
export function applyPmsImport(db: TransactionalSqlDatabase, actor: WorkContext, raw: unknown, key: string) { return application.applyPmsImport(repository(db, actor, key), actor, raw, key); }
