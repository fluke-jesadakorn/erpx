import "server-only";
import {marketingMetricSource} from "@erpx/marketing-server/adapters/metric-purchases";
import {marketingEvidenceCandidateRows,marketingEvidenceEventRows} from "./executive-evidence-rows";
import { z } from "zod";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { inputSchema, type CandidateCursor, type EventCursor, type MarketingEvidenceRow, parseCursor, timeRange } from '@erpx/platform-runtime-server/public/executive/marketing-evidence-contract';
export function executiveEvidenceSources(database: SqlDatabase) {
    const db = database;
    return { async searchRows0(actor: WorkContext, input: z.infer<typeof inputSchema>, cursor: ReturnType<typeof parseCursor>, query: string): Promise<MarketingEvidenceRow[]> {
            const purchased=await marketingMetricSource(db,actor,input.metric),candidateSource=`(${purchased.sql})`;
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
                rows = (await db.prepare(`${marketingEvidenceCandidateRows(candidateSource)}
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
                rows = (await db.prepare(`${marketingEvidenceEventRows(candidateSource)}
      WHERE ${where.join(" AND ")} ORDER BY e.created_at DESC,e.id DESC LIMIT ?`)
                    .bind(...values, input.limit + 1).all<MarketingEvidenceRow>()).results;
            }
            return rows;
        } };
}
