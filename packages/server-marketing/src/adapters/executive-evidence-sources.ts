import "server-only";
import {marketingMetricSource} from "@erpx/server-marketing/adapters/metric-purchases";
import { z } from "zod";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { inputSchema, type CandidateCursor, type EventCursor, type MarketingEvidenceRow, parseCursor, timeRange } from '@erpx/server-runtime/public/executive/marketing-evidence-contract';
export function executiveEvidenceSources(database: SqlDatabase) {
    const db = database;
    return { async searchRows0(actor: WorkContext, input: z.infer<typeof inputSchema>, cursor: ReturnType<typeof parseCursor>, query: string): Promise<MarketingEvidenceRow[]> {
            const purchased=await marketingMetricSource(db,actor,input.metric);
            let rows: MarketingEvidenceRow[];
            if (input.metric === "marketing-lead-candidates") {
                const values: unknown[] = [...purchased.args,actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId];
                const where = ["c.company_id=?", "(?::text IS NULL OR c.branch_id=?)", "(?::text IS NULL OR c.department_id=?)"];
                if (query) {
                    where.push(`(position(lower(?) in lower(c.contact_name))>0 OR position(lower(?) in lower(f.name))>0
        OR position(lower(?) in lower(c.status))>0 OR position(lower(?) in to_char(to_timestamp(c.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD'))>0)`);
                    values.push(query, query, query, query);
                }
                if (cursor) {
                    const candidateCursor = cursor as CandidateCursor;
                    where.push("(c.contact_name>? OR (c.contact_name=? AND c.id>?))");
                    values.push(candidateCursor.name, candidateCursor.name, candidateCursor.id);
                }
                rows = (await db.prepare(`SELECT c.id,c.contact_name,c.status,c.form_version,c.version,c.created_at,c.updated_at,c.branch_id,c.department_id,f.name form_name
      FROM (${purchased.sql}) c JOIN marketing_assets f ON f.id=c.form_id AND f.company_id=c.company_id
      WHERE ${where.join(" AND ")} ORDER BY c.contact_name,c.id LIMIT ?`)
                    .bind(...values, input.limit + 1).all<MarketingEvidenceRow>()).results;
            }
            else {
                const range = timeRange(input.filters), values: unknown[] = [...purchased.args,actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, range.from, range.toExclusive];
                const where = ["c.company_id=?", "(?::text IS NULL OR c.branch_id=?)", "(?::text IS NULL OR c.department_id=?)", "e.created_at>=?", "e.created_at<?"];
                if (query) {
                    where.push(`(position(lower(?) in lower(c.contact_name))>0 OR position(lower(?) in lower(f.name))>0
        OR position(lower(?) in lower(e.event_type))>0 OR position(lower(?) in lower(e.channel))>0
        OR position(lower(?) in to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD'))>0)`);
                    values.push(query, query, query, query, query);
                }
                if (cursor) {
                    const eventCursor = cursor as EventCursor;
                    where.push("(e.created_at<? OR (e.created_at=? AND e.id<?))");
                    values.push(eventCursor.createdAt, eventCursor.createdAt, eventCursor.id);
                }
                rows = (await db.prepare(`SELECT e.id,e.candidate_id,e.event_type,e.channel,e.version event_version,e.created_at,
        c.contact_name,c.status candidate_status,c.version candidate_version,c.branch_id,c.department_id,f.name form_name
      FROM marketing_engagement_events e JOIN (${purchased.sql}) c ON c.id=e.candidate_id
      JOIN marketing_assets f ON f.id=c.form_id AND f.company_id=c.company_id
      WHERE ${where.join(" AND ")} ORDER BY e.created_at DESC,e.id DESC LIMIT ?`)
                    .bind(...values, input.limit + 1).all<MarketingEvidenceRow>()).results;
            }
            return rows;
        } };
}
