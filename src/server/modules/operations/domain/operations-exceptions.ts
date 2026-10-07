import { z } from "zod";
import type { AuthorizedSourceRecord } from "@/server/modules/records/public/source-record-policy";
import { sourceRecordInput } from '../../shared';
export const reason = z.string().trim().min(3).max(4000), version = z.number().int().positive(), id = z.string().min(1).max(180);
export const createSchema = z.object({ source: sourceRecordInput.extend({ version }), policyId: id, policyVersion: version, title: z.string().trim().min(3).max(160), issue: reason, priority: z.enum(["normal", "high", "critical"]).default("normal"), ownerId: id });
export const actionSchema = z.discriminatedUnion("action", [
    z.object({ action: z.enum(["acknowledge", "start", "reject", "approve", "close", "reopen", "cancel"]), expectedVersion: version, reason }),
    z.object({ action: z.literal("assign"), expectedVersion: version, ownerId: id, reason }),
    z.object({ action: z.literal("prioritize"), expectedVersion: version, priority: z.enum(["normal", "high", "critical"]), reason }),
    z.object({ action: z.literal("escalate"), expectedVersion: version, escalationOwnerId: id, reason }),
    z.object({ action: z.literal("resolve"), expectedVersion: version, resolution: reason, reason }),
]);
export type Row = Record<string, string | number | null>;
export type Exception = {
    id: string;
    company_id: string;
    branch_id: string | null;
    department_id: string | null;
    area: string;
    source_kind: AuthorizedSourceRecord["kind"];
    source_id: string;
    source_version: number;
    source_reference_json: string;
    policy_id: string;
    policy_snapshot_json: string;
    title: string;
    issue: string;
    priority: string;
    status: string;
    owner_id: string;
    escalation_owner_id: string | null;
    resolution: string | null;
    created_by: string;
    resolved_by: string | null;
    reviewed_by: string | null;
    version: number;
    updated_at: number;
    due_at: number;
};
