import type { Clock, IdentifierSource } from "@erpx/server-shared/application/ports";
import type { ExecutedCopy, LegalItem } from "@erpx/server-legal/domain/model";
import { executedCopyReview, type ExecutedCopyReview } from "@erpx/server-legal/domain/executed-copies";
export type ExecutedCopySource = ExecutedCopy & { document_author: string; approved_author: string };
export type ExecutedCopyRepository = {
  list(id: string): Promise<(ExecutedCopy & { document_name: string; document_version: number })[]>; reviews(id: string): Promise<Record<string, unknown>[]>;
  member(itemId: string): Promise<boolean>; replay(key: string, itemId: string, input: ExecutedCopyReview): Promise<Record<string, unknown> | null>;
  row(id: string, itemId: string): Promise<ExecutedCopySource | null>; samePerson(author: string): Promise<boolean>; approval(itemId: string): Promise<{ cycle: number } | null>;
  commit(input: ExecutedCopyReview, item: LegalItem, copy: ExecutedCopySource, authors: string[], state: string, now: number, event: { id: string; now: number }, key: string): Promise<{ id: string; version: number; state: string; executedCopyId: string; executedCopyState: string }>;
};
export function createExecutedCopyApplication(ports: { repository: ExecutedCopyRepository; access(id: string, permission?: string): Promise<LegalItem>; eligible(item: LegalItem): Promise<void>; clock: Clock; identifiers: IdentifierSource; error(status: number, code: string, message: string): Error }) {
  const source = ports.repository; function fail(message: string, code = "LEGAL_INVALID", status = 422): never { throw ports.error(status, code, message); }
  async function executedCopies(id: string) { await ports.access(id); const rows = await source.list(id); return Promise.all(rows.map(async row => ({ ...row, metadata: typeof row.metadata === "string" ? JSON.parse(row.metadata) : row.metadata, reviews: (await source.reviews(row.id)).map(review => ({ ...review, checks: typeof review.checks === "string" ? JSON.parse(review.checks) : review.checks })) }))); }
  async function reviewExecutedCopy(raw: unknown, key: string) {
    const input = executedCopyReview.parse(raw), item = await ports.access(input.id, "legal.approve"); await ports.eligible(item);
    if (!await source.member(item.id)) fail("The reviewer needs explicit contract membership.", "LEGAL_REVIEWER_UNAVAILABLE", 403);
    const prior = await source.replay(key, item.id, input); if (prior) return prior;
    if (item.version !== input.expectedVersion) fail("The contract changed. Refresh first.", "LEGAL_VERSION_CONFLICT", 409);
    const copy = await source.row(input.executedCopyId, item.id); if (!copy) return fail("Executed copy unavailable.", "LEGAL_NOT_FOUND", 404);
    if (copy.state === "confirmed" || item.kind !== "contract" || !["approved", "executed", "active"].includes(item.state)) fail("Import a new revision to correct a confirmed copy.", "LEGAL_EXECUTED_STATE", 409);
    const authors = [copy.imported_by, copy.document_author, copy.approved_author, item.created_by];
    for (const author of authors) if (await source.samePerson(author)) fail("An independent reviewer must inspect the signed original.", "LEGAL_INDEPENDENT_REVIEW", 403);
    if (input.decision === "confirm" && !Object.values(input.checks).every(Boolean)) fail("Confirm content, complete pages, parties, signatures and authority before accepting an executed copy.", "LEGAL_EXECUTED_INCOMPLETE", 409);
    if ((await source.approval(item.id))?.cycle !== copy.approved_cycle) fail("The approved version changed. Import against the current approval.", "LEGAL_APPROVAL_STALE", 409);
    const now = ports.clock.now(), state = input.decision === "confirm" && item.state === "approved" ? "executed" : item.state;
    return source.commit(input, item, copy, authors, state, now, { id: ports.identifiers.next(), now: ports.clock.now() }, key);
  }
  return { executedCopies, reviewExecutedCopy };
}
