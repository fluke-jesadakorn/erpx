import {learningMetricSource} from "@erpx/server-learning/adapters/metric-purchases";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { inputSchema, type CompletionCursor, type OverdueCursor, type LearningEvidenceRow, parseCursor, utcRange } from '@erpx/server-runtime/public/executive/learning-evidence-contract';
export function executiveEvidenceSources(database: SqlDatabase) {
    const db = database;
    return { async searchRows0(actor: WorkContext, input: z.infer<typeof inputSchema>, cursor: ReturnType<typeof parseCursor>): Promise<LearningEvidenceRow[]> {
            const source=await learningMetricSource(db,actor,input.metric);
            const scope = [
                "a.company_id=?", "(?::text IS NULL OR a.branch_id=?)", "(?::text IS NULL OR a.department_id=?)",
            ];
            const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId];
            let rows: LearningEvidenceRow[];
            if (input.metric === "learning-completions") {
                const range = utcRange(input.filters);
                scope.push("e.action='complete'", "e.status='completed'", "e.created_at>=?", "e.created_at<?");
                values.push(range.from, range.toExclusive);
                if (input.query) {
                    scope.push(`(position(lower(?) in lower(c.title))>0 OR position(lower(?) in to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD'))>0)`);
                    values.push(input.query, input.query);
                }
                if (cursor) {
                    const completionCursor = cursor as CompletionCursor;
                    scope.push("(e.created_at<? OR (e.created_at=? AND e.id<?))");
                    values.push(completionCursor.createdAt, completionCursor.createdAt, completionCursor.id);
                }
                rows = (await db.prepare(`SELECT e.id,e.assignment_id,e.version event_version,e.status,e.created_at,c.title course_title
      FROM learning_assignment_events e
      JOIN (${source.sql}) a ON a.id=e.assignment_id
      JOIN learning_courses c ON c.id=a.course_id AND c.company_id=a.company_id
      WHERE ${scope.join(" AND ")}
      ORDER BY e.created_at DESC,e.id DESC LIMIT ?`)
                    .bind(...source.args,...values, input.limit + 1).all<LearningEvidenceRow>()).results;
            }
            else {
                scope.push("c.required_for IS NOT NULL", "a.due_date<to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')", "a.status IN ('assigned','in_progress')");
                if (input.query) {
                    scope.push(`(position(lower(?) in lower(c.title))>0 OR position(lower(?) in lower(a.status))>0
        OR position(lower(?) in a.due_date)>0 OR position(lower(?) in lower(c.required_for))>0)`);
                    values.push(input.query, input.query, input.query, input.query);
                }
                if (cursor) {
                    const overdueCursor = cursor as OverdueCursor;
                    scope.push("(a.due_date>? OR (a.due_date=? AND a.id>?))");
                    values.push(overdueCursor.dueDate, overdueCursor.dueDate, overdueCursor.id);
                }
                rows = (await db.prepare(`SELECT a.id,a.version,a.status,a.due_date,a.updated_at,c.title course_title,c.required_for
      FROM (${source.sql}) a JOIN learning_courses c ON c.id=a.course_id AND c.company_id=a.company_id
      WHERE ${scope.join(" AND ")}
      ORDER BY a.due_date,a.id LIMIT ?`)
                    .bind(...source.args,...values, input.limit + 1).all<LearningEvidenceRow>()).results;
            }
            return rows;
        } };
}
