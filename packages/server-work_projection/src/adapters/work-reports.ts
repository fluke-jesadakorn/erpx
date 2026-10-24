import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { requireFlowAccess } from "@erpx/server-workflow/public/flow-access";
import {resolveFlow} from '@erpx/server-workflow/public/flow-routing';
import { withPolicyReadSnapshot } from "@erpx/server-identity_access/public/iam-read-snapshot";
import { ApiError } from "@erpx/server-runtime/public/http";
import { createWorkReports, type WorkReportUnit, type WorkReportProjection } from "@erpx/server-work_projection/application/work-reports";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { workReportPeoplePayrollFacts } from "@erpx/server-people_payroll/public/work-report-facts";
import { workReportOrganizationFacts } from "@erpx/server-organization/public/work-report-facts";
import { workReportWorkflowFacts } from "@erpx/server-workflow/public/work-report-facts";
import { workReportRecordsFacts } from "@erpx/server-records/public/work-report-facts";
import { workReportPeoplePayrollCandidates } from "@erpx/server-people_payroll/public/work-report-candidates";
import { workReportRecordsCandidates } from "@erpx/server-records/public/work-report-candidates";
export { reportQuery, workReportCsv } from "@erpx/server-work_projection/application/work-reports";
function matchingProjection(projection: WorkReportProjection) { const predicate = projection.mode === "open" ? "(row->>'open')::boolean" : projection.mode === "drafts" ? "(row->>'draft')::boolean" : "(row->>'submittedAt')::bigint BETWEEN ? AND ?"; return { cte: `WITH visible AS (SELECT value row FROM jsonb_array_elements(?::jsonb)), matching AS (SELECT row FROM visible WHERE ${predicate}) `, bindings: projection.mode === "period" ? [projection.scoped, projection.start, projection.end] : [projection.scoped] }; }
export function workReportUnit(db: SqlDatabase): WorkReportUnit {
    return {
        candidates: async (companyId) => (await Promise.all([workReportPeoplePayrollCandidates(db, companyId), workReportRecordsCandidates(db, companyId)])).flat(),
        access: (actor, kind, id) => requireFlowAccess(actor, kind, id, db), flow: (actor, kind, id) => resolveFlow(db, actor, kind, id),
        people: (companyId: string) => workReportPeoplePayrollFacts(db).people(companyId),
        nodes: (companyId: string) => workReportOrganizationFacts(db).nodes(companyId),
        decisions: (companyId: string, ids: string) => workReportWorkflowFacts(db).decisions(companyId, ids),
        events: (companyId: string, ids: string) => workReportRecordsFacts(db).events(companyId, ids),
        savedRoutes: (companyId: string, ids: string) => workReportWorkflowFacts(db).savedRoutes(companyId, ids),
        counts: (projection: WorkReportProjection) => { const { cte, bindings } = matchingProjection(projection); return db.prepare(cte + `SELECT (SELECT COUNT(*) FROM matching) total,(SELECT COUNT(*) FROM visible WHERE (row->>'open')::boolean) open,(SELECT COUNT(*) FROM visible WHERE (row->>'draft')::boolean) drafts,(SELECT COUNT(*) FROM visible WHERE NOT (row->>'draft')::boolean AND row->>'submittedAt' IS NULL) unknown,(SELECT COUNT(*) FROM matching WHERE row->>'blocker' IS NOT NULL) blocked,(SELECT COALESCE(SUM((row->>'units')::numeric),0) FROM matching) units`).bind(...bindings).first<{
            total: number;
            open: number;
            drafts: number;
            unknown: number;
            blocked: number;
            units: number;
        }>(); },
        money: (projection: WorkReportProjection) => { const { cte, bindings } = matchingProjection(projection); return db.prepare(cte + `SELECT row->>'currency' currency,SUM((row->>'amount')::bigint) amount FROM matching WHERE row->>'currency' IS NOT NULL AND row->>'amount' IS NOT NULL GROUP BY row->>'currency'`).bind(...bindings).all<{
            currency: string;
            amount: number;
        }>(); },
        estimates: (projection: WorkReportProjection) => { const { cte, bindings } = matchingProjection(projection); return db.prepare(cte + `SELECT row->>'currency' currency,SUM((row->>'estimate')::bigint) amount FROM matching WHERE row->>'currency' IS NOT NULL AND row->>'estimate' IS NOT NULL GROUP BY row->>'currency'`).bind(...bindings).all<{
            currency: string;
            amount: number;
        }>(); },
        stageCounts: (projection: WorkReportProjection) => { const { cte, bindings } = matchingProjection(projection); return db.prepare(cte + `SELECT row->>'stage' stage,COUNT(*) count,MAX((row->>'stageWaitDays')::int) oldest FROM matching GROUP BY row->>'stage' ORDER BY COUNT(*) DESC`).bind(...bindings).all<{
            stage: string;
            count: number;
            oldest: number | null;
        }>(); },
        page: <Row>(projection: WorkReportProjection) => { const { cte, bindings } = matchingProjection(projection); return db.prepare(cte + `SELECT row FROM matching ORDER BY (row->>'updated_at')::bigint DESC,row->>'id' LIMIT ? OFFSET ?`).bind(...bindings, 50, (projection.page - 1) * 50).all<{
            row: Row;
        }>(); },
        allRows: <Row>(projection: WorkReportProjection) => { const { cte, bindings } = matchingProjection(projection); return db.prepare(cte + `SELECT row FROM matching ORDER BY (row->>'updated_at')::bigint DESC,row->>'id'`).bind(...bindings).all<{
            row: Row;
        }>(); }
    };
}
const service = createWorkReports({ resource: async (holdingId) => workReportUnit((await holdingResources(holdingId)).write), now: Date.now, error: (...args) => new ApiError(...args), errorStatus: error => error instanceof ApiError ? error.status : undefined });
export function workReports(actor: WorkContext, raw: Record<string, unknown>, exportAll = false) { return withPolicyReadSnapshot(() => service.workReports(actor, raw, exportAll)); }
