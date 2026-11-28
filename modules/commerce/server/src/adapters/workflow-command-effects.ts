import { nativeOwnerStatement } from "@erpx/commerce-server/adapters/native-owner-write";
/**
 * Native posted-journal order revision. The guarded bump executes in the
 * native Commerce owner unit after the Finance journal post step commits in
 * the same batch order, so the `posted` guard reads the just-posted journal.
 * Carries no SQL and no TS fallback: the owner crate owns the statement.
 */
export function stageWorkflowPostedOrderRevision(now: number, journalId: string, companyId: string, actorId: string, key: string) {
  return nativeOwnerStatement('order.posted-revision', [now, journalId, companyId], journalId, actorId, key, companyId);
}
