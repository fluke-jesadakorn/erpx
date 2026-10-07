import { z } from 'zod';
import { sourceRecordKind } from '../../shared';
import type { AuthorizedSourceRecord } from '../../records/public/source-record-contract';
export type Row = Record<string, string | number | boolean | null>;
export const sourceType = sourceRecordKind, reason = z.string().trim().min(3).max(3000), version = z.number().int().positive();
export const relationSchema = z.object({
    sourceType,
    sourceId: z.string().min(1),
    targetType: sourceType,
    targetId: z.string().min(1),
    relation: z.enum(["related", "blocks", "duplicates", "depends_on"]),
    reason,
});
export const cardSchema = z.object({
    sourceType,
    sourceId: z.string().min(1),
    title: z.string().trim().min(3).max(160),
    ownerId: z.string().min(1),
    dueAt: z.number().int().positive(),
    reason,
});
export const actionSchema = z.discriminatedUnion("action", [
    z.object({
        action: z.literal("complete"),
        expectedVersion: version,
        evidence: z.string().trim().min(3).max(4000),
        reason,
    }),
    z.object({
        action: z.literal("reassign"),
        expectedVersion: version,
        ownerId: z.string().min(1),
        dueAt: z.number().int().positive(),
        reason,
    }),
    z.object({ action: z.literal("cancel"), expectedVersion: version, reason }),
]);
export const csvCell = (value: unknown) => {
    const text = String(value ?? "");
    return `"${/^[=+@\-\t\r]/.test(text) ? "'" : ""}${text.replaceAll('"', '""')}"`;
};
export type Source = AuthorizedSourceRecord & {
    type: z.infer<typeof sourceType>;
    id: string;
    version: number;
    title: string;
    companyId: string;
    branchId: string | null;
    departmentId: string | null;
    href: string;
};
export type Card = Row & {
    id: string;
    version: number;
    status: string;
    owner_id: string;
    created_by: string;
    updated_at: number;
    due_at: number;
    owner_eligible: number;
};
