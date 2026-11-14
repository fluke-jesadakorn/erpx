import type {WorkCommandCompletion} from "@erpx/feature-work_projection/lib/work-fetch";
export type FinanceDocumentPostControl = {path: string; canPost?: boolean};
/** A source inclusion applies only to posting this exact persisted document. */
export function financeDocumentCommandAllowed(baseAllowed: boolean, path: string, post?: FinanceDocumentPostControl) {
  return post?.path === path ? post.canPost ?? baseAllowed : baseAllowed;
}
export function financeDocumentFormUnchanged(completion: WorkCommandCompletion | undefined) {
  return !completion || completion.form.isConnected && completion.form.getAttribute("data-erpx-edit-version") === completion.formEditVersion;
}
export function financeDocumentCommandContext() {
  const context = typeof document === "undefined" ? null : document.querySelector<HTMLElement>("[data-work-revision]");
  return context ? JSON.stringify([context.dataset.workUser,context.dataset.workRevision,context.dataset.workCompany,context.dataset.workAssignment,context.dataset.workAssignmentVersion]) : "server";
}
