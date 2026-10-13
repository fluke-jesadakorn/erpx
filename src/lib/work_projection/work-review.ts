import type { SourceRecordReference } from "@/lib/records/source-record";
import type { WorkItem } from "@/lib/work_projection/work-item";

export const workReviewKinds = ["purchase_request", "service_request", "equipment_request", "expense", "leave", "staff_task"] as const;
export type WorkReviewKind = typeof workReviewKinds[number];
export type ReviewDecision = "approve" | "reject" | "return";
export type WorkReview = {
  source: SourceRecordReference;
  title: string;
  requester: string;
  status: string;
  version: number;
  description: string;
  facts: { label: string; value: string }[];
  evidence: { name: string; href: string; contentHref?: string; previewHref?: string; mediaType?: string; bytes?: number }[];
  actionKind?: "approval" | "work";
  reasonMinimum?: Partial<Record<ReviewDecision, number>>;
  evidenceMessage?: string;
  steps: { label: string; person: string; state: string }[];
  currentStep: string;
  nextStep: string;
  blocker: string | null;
  permittedActions: ReviewDecision[];
  fullReviewReason?: string;
};
export function supportsWorkReview(item: WorkItem) {
  return item.actionKind !== "override" && workReviewKinds.includes(item.type as WorkReviewKind);
}
/** Construct only the existing owner's command; there is no generic approval write. */
export function workReviewCommand(kind: WorkReviewKind, id: string, version: number, decision: ReviewDecision, note: string) {
  const record = encodeURIComponent(id);
  if (decision === "return" && !kind.endsWith("_request")) throw Error("This workflow does not support return.");
  if (kind === "expense") return { path: `/expenses/${record}/${decision}`, body: { expectedVersion: version, reason: note, override: false } };
  if (kind === "leave") return { path: `/leave-requests/${record}/${decision === "approve" ? "approve" : "transition"}`, body: { expectedVersion: version, note, override: false, ...(decision === "reject" ? { action: "reject" } : {}) } };
  return { path: kind === "staff_task" ? `/staff-tasks/${record}/transition` : `/requests/${record}/actions`, body: { action: decision, expectedVersion: version, note } };
}
