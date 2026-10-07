import { z } from "zod";
import { filtersSchema, type Filters } from "@/server/modules/runtime/public/rules/canvas-domain";
import { BusinessRuleError } from '../../../shared';
export const learningEvidenceMetrics = ["learning-completions", "overdue-required-learning"] as const;
export type LearningEvidenceMetric = typeof learningEvidenceMetrics[number];
export const providerId = "learning-assignment-records";
export const completionFields = [
    "event.id", "event.assignment_id", "event.version", "event.action", "event.status", "event.created_at", "course.title",
];
export const overdueFields = [
    "assignment.id", "assignment.version", "assignment.status", "assignment.due_date", "assignment.updated_at", "course.title", "course.required_for",
];
export const inputSchema = z.object({
    metric: z.enum(learningEvidenceMetrics),
    filters: filtersSchema,
    query: z.string().trim().max(100).default(""),
    cursor: z.string().max(2000).optional(),
    limit: z.coerce.number().int().min(1).max(50).default(25),
}).strict();
export type CompletionCursor = {
    createdAt: number;
    id: string;
};
export type OverdueCursor = {
    dueDate: string;
    id: string;
};
export type LearningEvidenceRow = {
    id: string;
    assignment_id?: string;
    event_version?: number | string | bigint;
    version?: number | string | bigint;
    status: string;
    created_at?: number | string | bigint;
    due_date?: string;
    updated_at?: number | string | bigint;
    course_title: string;
    required_for?: string | null;
};
export function parseCursor(value: string | undefined, metric: LearningEvidenceMetric) {
    if (!value)
        return null;
    try {
        const parsed = JSON.parse(Buffer.from(value, "base64url").toString("utf8")) as Record<string, unknown>;
        const id = z.string().min(1).max(200).parse(parsed.id);
        if (metric === "learning-completions")
            return {
                createdAt: z.number().int().positive().parse(parsed.createdAt), id,
            } satisfies CompletionCursor;
        return { dueDate: z.iso.date().parse(parsed.dueDate), id } satisfies OverdueCursor;
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeCursor(metric: LearningEvidenceMetric, row: LearningEvidenceRow) {
    const cursor = metric === "learning-completions"
        ? { createdAt: safeTimestamp(row.created_at), id: row.id }
        : { dueDate: z.iso.date().parse(row.due_date), id: row.id };
    return Buffer.from(JSON.stringify(cursor)).toString("base64url");
}
export function safeRevision(value: number | string | bigint | undefined) {
    const revision = Number(value);
    if (!Number.isSafeInteger(revision) || revision < 1)
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", "The source returned an invalid learning revision.");
    return revision;
}
export function safeTimestamp(value: number | string | bigint | undefined) {
    const timestamp = Number(value);
    if (!Number.isSafeInteger(timestamp) || timestamp < 1)
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", "The source returned an invalid learning timestamp.");
    return timestamp;
}
export function notFound() {
    return new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
}
export function utcRange(filters: Filters) {
    return { from: Date.parse(filters.from), toExclusive: Date.parse(filters.to) + 86400000 };
}
