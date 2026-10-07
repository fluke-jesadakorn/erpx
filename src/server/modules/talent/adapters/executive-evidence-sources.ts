import "server-only";
import { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { inputSchema, type RequisitionCursor, type ApplicationCursor, type TalentEvidenceRow, parseCursor, utcRange } from '@/server/modules/runtime/public/executive/talent-evidence-contract';
export function executiveEvidenceSources(database: SqlDatabase) {
    const db = database;
    return { async searchRows0(actor: WorkContext, input: z.infer<typeof inputSchema>, cursor: ReturnType<typeof parseCursor>): Promise<TalentEvidenceRow[]> {
            const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId];
            const where = [
                "r.company_id=?", "(?::text IS NULL OR r.branch_id=?)", "(?::text IS NULL OR r.department_id=?)",
            ];
            let rows: TalentEvidenceRow[];
            if (input.metric === "open-requisitions") {
                where.push("r.status='open'");
                if (input.query) {
                    where.push(`(position(lower(?) in lower(r.job_title))>0 OR position(lower(?) in lower(r.status))>0
        OR position(lower(?) in r.target_start_date)>0)`);
                    values.push(input.query, input.query, input.query);
                }
                if (cursor) {
                    const requisitionCursor = cursor as RequisitionCursor;
                    where.push("(r.job_title>? OR (r.job_title=? AND r.id>?))");
                    values.push(requisitionCursor.jobTitle, requisitionCursor.jobTitle, requisitionCursor.id);
                }
                rows = (await db.prepare(`SELECT r.id,r.job_title,r.target_start_date,r.status,r.version,r.updated_at,r.branch_id,r.department_id
      FROM talent_requisitions r WHERE ${where.join(" AND ")}
      ORDER BY r.job_title,r.id LIMIT ?`)
                    .bind(...values, input.limit + 1).all<TalentEvidenceRow>()).results;
            }
            else {
                const range = utcRange(input.filters);
                where.push("a.updated_at>=?", "a.updated_at<?");
                values.push(range.from, range.toExclusive);
                if (input.query) {
                    where.push(`(position(lower(?) in lower(r.job_title))>0 OR position(lower(?) in lower(a.status))>0
        OR position(lower(?) in to_char(to_timestamp(a.updated_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD'))>0)`);
                    values.push(input.query, input.query, input.query);
                }
                if (cursor) {
                    const applicationCursor = cursor as ApplicationCursor;
                    where.push("(a.updated_at<? OR (a.updated_at=? AND a.id<?))");
                    values.push(applicationCursor.updatedAt, applicationCursor.updatedAt, applicationCursor.id);
                }
                rows = (await db.prepare(`SELECT a.id,a.requisition_id,a.status,a.version,a.updated_at,r.job_title,r.branch_id,r.department_id
      FROM talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id
      WHERE ${where.join(" AND ")}
      ORDER BY a.updated_at DESC,a.id DESC LIMIT ?`)
                    .bind(...values, input.limit + 1).all<TalentEvidenceRow>()).results;
            }
            return rows;
        } };
}
