import { z } from "zod";
import { filtersSchema } from "@erpx/server-runtime/public/rules/canvas-domain";
import { BusinessRuleError } from '@erpx/server-shared/index';
export const agentEvidenceMetrics = ["approved-agent-versions", "agent-invocation-outcomes"] as const;
export type AgentEvidenceMetric = typeof agentEvidenceMetrics[number];
export const providerId = "agent-governance";
export const approvedVersionMetric = "approved-agent-versions" as const;
export const approvedVersionFields = ["agent.name", "agent.status", "version.version_number", "version.status", "version.version", "version.created_at", "version.updated_at"];
export const invocationFields = ["turn_id", "outcome", "created_at"];
export const inputSchema = z.object({
    metric: z.enum(agentEvidenceMetrics),
    filters: filtersSchema,
    query: z.string().trim().max(100).default(""),
    cursor: z.string().max(2000).optional(),
    limit: z.coerce.number().int().min(1).max(50).default(25),
}).strict();
export type Cursor = {
    createdAt: number;
    id: string;
};
export type ApprovedVersionRow = {
    id: string;
    agent_id: string;
    agent_name: string;
    agent_status: string;
    version_status: string;
    version_number: number | string;
    version: number | string;
    created_at: number | string;
    updated_at: number | string;
};
export type InvocationRow = {
    id: string;
    turn_id: string;
    outcome: "succeeded" | "failed";
    created_at: number | string;
};
export function parseCursor(value?: string): Cursor | null {
    if (!value)
        return null;
    try {
        const parsed = JSON.parse(Buffer.from(value, "base64url").toString("utf8")) as {
            createdAt?: unknown;
            id?: unknown;
        };
        if (!Number.isSafeInteger(parsed.createdAt) || typeof parsed.id !== "string")
            throw new Error();
        return { createdAt: Number(parsed.createdAt), id: z.string().min(1).max(200).parse(parsed.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeCursor(row: {
    created_at: number | string;
    id: string;
}) {
    return Buffer.from(JSON.stringify({ createdAt: Number(row.created_at), id: row.id })).toString("base64url");
}
export function exactNumber(value: number | string, field: string) {
    const result = Number(value);
    if (!Number.isSafeInteger(result) || result < 0)
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", `The source returned an invalid ${field}.`);
    return result;
}
