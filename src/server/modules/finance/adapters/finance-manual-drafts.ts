import "server-only";
import { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { assertion, executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { financeTransaction } from "@/server/modules/finance/public/finance-transactions";
import { prepareJournal } from "@/server/modules/finance/public/finance";
import { ApiError } from "@/server/modules/runtime/public/http";
import { sha256 } from "@/server/modules/runtime/public/crypto";
import type { SqlDatabase } from '@/server/modules/runtime/public/runtime/types';
import { createManualDrafts } from '../application/manual-drafts';
import { manualDraftEditSchema, type DraftRow } from '../domain/manual-drafts';
export { manualDraftEditSchema } from '../domain/manual-drafts';
export async function editManualDraft(actor: WorkContext, journalId: string, raw: z.input<typeof manualDraftEditSchema>, key: string) { return nativeManualDrafts().editManualDraft(actor, journalId, raw, key); }
function nativeManualDrafts() {
    let pending: Promise<SqlDatabase> | undefined, owned: SqlDatabase | undefined, prepared: Awaited<ReturnType<typeof prepareJournal>>;
    const database = (actor: WorkContext) => owned ? Promise.resolve(owned) : pending ??= holdingResources(actor.holdingId).then(r => r.write);
    return createManualDrafts({ error: (...args) => new ApiError(...args), permission: requirePermission, ready: async (actor) => { await database(actor); }, now: Date.now, hash: sha256, transaction: async <Result>(actor: WorkContext, work: () => Promise<Result>) => financeTransaction(await database(actor), async (tx) => { owned = tx; try {
            return await work();
        }
        finally {
            owned = undefined;
        } }), replay: async (actor, key, op, id, input) => replayCommand(await database(actor), actor, key, op, id, input), journal: async (actor, id) => (await database(actor)).prepare("SELECT * FROM finance_journals WHERE id=? AND company_id=?").bind(id, actor.companyId).first<DraftRow>(), lines: async (actor, id) => (await (await database(actor)).prepare("SELECT * FROM finance_journal_lines WHERE journal_id=? ORDER BY ordinal,id").bind(id).all<DraftRow>()).results, prepareJournal: async (actor, input, key) => { prepared = await prepareJournal(await database(actor), actor, input, key); return { result: { fxRateMicros: prepared.result.fxRateMicros }, preparedLines: prepared.preparedLines }; }, commit: async ({ actor, input, key, op, journalId, old, now, version, nextLines, originalSnapshot, snapshot, originalHash, snapshotHash }) => {
            const db = await database(actor);
            return executeCommand(db, actor, key, op, input, journalId, version, { id: journalId, status: "draft", version }, [
                assertion(db, key, "manual-draft-version", "EXISTS(SELECT 1 FROM finance_journals WHERE id=? AND company_id=? AND source_type='manual' AND status='draft' AND version=?)", [journalId, actor.companyId, input.expectedVersion]),
                ...prepared.guards,
                db
                    .prepare("INSERT INTO finance_manual_draft_editions(id,company_id,journal_id,journal_version,snapshot_json,snapshot_sha256,reason,prepared_by,created_at) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(journal_id,journal_version) DO NOTHING")
                    .bind(`${journalId}:v${old.version}`, actor.companyId, journalId, old.version, originalSnapshot, originalHash, "Retained original draft before the first edit", old.created_by, now),
                db
                    .prepare("DELETE FROM finance_journal_lines WHERE journal_id=?")
                    .bind(journalId),
                db
                    .prepare("UPDATE finance_journals SET period_id=?,entry_date=?,memo=?,fx_rate_micros=?,created_by=?,version=?,updated_at=? WHERE id=?")
                    .bind(input.periodId, input.entryDate, input.memo, prepared.result.fxRateMicros, actor.userId, version, now, journalId),
                ...nextLines.map((l) => db
                    .prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) VALUES(?,?,?,?,?,?,?,?,?,?)")
                    .bind(l.id, l.journal_id, l.account_id, l.description, l.debit_minor, l.credit_minor, l.base_debit_minor, l.base_credit_minor, l.dimension_json, l.ordinal)),
                db
                    .prepare("INSERT INTO finance_manual_draft_editions(id,company_id,journal_id,journal_version,snapshot_json,snapshot_sha256,reason,prepared_by,created_at) VALUES(?,?,?,?,?,?,?,?,?)")
                    .bind(`${journalId}:v${version}`, actor.companyId, journalId, version, snapshot, snapshotHash, input.reason, actor.userId, now),
            ]);
        } });
}
