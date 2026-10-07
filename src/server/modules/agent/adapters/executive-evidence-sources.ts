import "server-only";
import { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { approvedVersionMetric, approvedVersionFields, invocationFields, inputSchema, type ApprovedVersionRow, type InvocationRow, parseCursor } from '@/server/modules/runtime/public/executive/agent-evidence-contract';
export function executiveEvidenceSources(database: SqlDatabase) {
    const db = database;
    return { async searchRows0(actor: WorkContext, input: z.infer<typeof inputSchema>, cursor: ReturnType<typeof parseCursor>): Promise<{
            rows: ApprovedVersionRow[] | InvocationRow[];
            fields: string[];
        }> {
            const conditions: string[] = ["a.company_id=?", "a.status='approved'", "v.status='approved'"], values: unknown[] = [actor.companyId];
            let rows: ApprovedVersionRow[] | InvocationRow[];
            let fields: string[];
            if (input.metric === approvedVersionMetric) {
                if (input.query) {
                    conditions.push("(position(lower(?) in lower(a.name))>0 OR position(lower(?) in lower(v.version_number::text))>0)");
                    values.push(input.query, input.query);
                }
                if (cursor) {
                    conditions.push("(v.created_at<? OR (v.created_at=? AND v.id<?))");
                    values.push(cursor.createdAt, cursor.createdAt, cursor.id);
                }
                rows = (await db.prepare(`SELECT v.id,v.agent_id,a.name agent_name,a.status agent_status,v.status version_status,
        v.version_number,v.version,v.created_at,v.updated_at
      FROM agent_versions v JOIN agent_catalog a ON a.id=v.agent_id
      WHERE ${conditions.join(" AND ")} ORDER BY v.created_at DESC,v.id DESC LIMIT ?`)
                    .bind(...values, input.limit + 1).all<ApprovedVersionRow>()).results;
                fields = approvedVersionFields;
            }
            else {
                const invocationConditions = ["e.company_id=?", "e.created_at>=?", "e.created_at<?"];
                const invocationValues: unknown[] = [actor.companyId, Date.parse(input.filters.from), Date.parse(input.filters.to) + 86400000];
                if (input.query) {
                    invocationConditions.push("(position(lower(?) in lower(e.outcome))>0 OR position(lower(?) in lower(e.turn_id))>0)");
                    invocationValues.push(input.query, input.query);
                }
                if (cursor) {
                    invocationConditions.push("(e.created_at<? OR (e.created_at=? AND e.id<?))");
                    invocationValues.push(cursor.createdAt, cursor.createdAt, cursor.id);
                }
                rows = (await db.prepare(`SELECT e.id,e.turn_id,e.outcome,e.created_at FROM ai_invocation_evidence e
      WHERE ${invocationConditions.join(" AND ")} ORDER BY e.created_at DESC,e.id DESC LIMIT ?`)
                    .bind(...invocationValues, input.limit + 1).all<InvocationRow>()).results;
                fields = invocationFields;
            }
            return { rows, fields };
        }, source1(companyId: string, id: string) {
            return database.prepare(`SELECT v.id,v.agent_id,a.name agent_name,a.status agent_status,v.status version_status,
        v.version_number,v.version,v.created_at,v.updated_at
      FROM agent_versions v JOIN agent_catalog a ON a.id=v.agent_id
      WHERE a.company_id=? AND a.status='approved' AND v.status='approved' AND v.id=? LIMIT 1`)
                .bind(companyId, id).first<ApprovedVersionRow>();
        }, source2(companyId: string, value: number, value2: number, id: string) {
            return database.prepare(`SELECT e.id,e.turn_id,e.outcome,e.created_at FROM ai_invocation_evidence e
      WHERE e.company_id=? AND e.created_at>=? AND e.created_at<? AND e.id=? LIMIT 1`)
                .bind(companyId, value, value2, id).first<InvocationRow>();
        } };
}
