import { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { WorkItem } from "@/lib/work-item";
import { hasPermission } from "@/lib/permissions";

export type LearningRow = Record<string, string | number | null>;
export type LearningAssignment = LearningRow & { id: string; version: number; status: string; learner_id: string; created_by: string; updated_at: number; learner_eligible: number };
const reason = z.string().trim().min(3).max(3000), version = z.number().int().positive(), date = z.string().regex(/^\d{4}-\d{2}-\d{2}$/);
export const courseActionSchema = z.object({ action: z.enum(["publish", "reject", "retire"]), expectedVersion: version, reason });
export const learningAssignSchema = z.object({ courseId: z.string().min(1), courseVersion: version, learnerId: z.string().min(1), dueDate: date, reason });
export const learningActionSchema = z.discriminatedUnion("action", [
  z.object({ action: z.literal("start"), expectedVersion: version, reason }),
  z.object({ action: z.literal("complete"), expectedVersion: version, attendance: z.enum(["attended", "absent"]), scoreBps: z.number().int().min(0).max(10000), evidence: reason, reason, attachmentSetIds: z.array(z.string().min(1)).max(20).default([]), certificateValidFrom: z.string().date().optional(), certificateExpiresOn: z.string().date().nullable().optional() }),
  z.object({ action: z.enum(["waive", "cancel"]), expectedVersion: version, reason }),
]);
export type LearningCourseAction = z.infer<typeof courseActionSchema>;
export type LearningAssignInput = z.infer<typeof learningAssignSchema>;
export type LearningActionInput = z.infer<typeof learningActionSchema>;
export function learningActionStatus(action: LearningActionInput["action"]) { return action === "start" ? "in_progress" : action === "complete" ? "completed" : action === "waive" ? "waived" : "cancelled"; }
export function learningActionAllowed(action: LearningActionInput["action"], status: string) { return action === "start" ? status === "assigned" : action === "complete" ? status === "in_progress" : ["assigned", "in_progress"].includes(status); }
export function learningWorkItems(a: WorkContext, assignments: LearningAssignment[]): WorkItem[] {
  return assignments.map(x => {
    const terminal = ["completed", "waived", "cancelled"].includes(x.status), own = x.learner_id === a.userId && Number(x.learner_eligible) > 0, review = hasPermission(a, "records.review");
    return { id: String(x.id), type: "learning_assignment", title: `Learning · ${x.course_title}`, detail: `${x.learner_name} · due ${x.due_date}`, href: `/learning/${x.id}`, status: String(x.status), updatedAt: Number(x.updated_at), dueAt: Date.parse(String(x.due_date) + "T00:00:00Z"), queue: terminal ? "completed" : own || review ? "todo" : "waiting", canAct: !terminal && (own || review), responsibleQueue: "Assigned learner", blocker: Number(x.learner_eligible) ? null : "Assigned learner is no longer eligible; People reviewer must waive or cancel", relationships: [own ? "assigned" : x.created_by === a.userId ? "requested" : "team"] };
  });
}
