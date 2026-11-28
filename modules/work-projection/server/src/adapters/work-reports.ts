import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import { requireFlowAccess } from "@erpx/workflow-server/public/flow-access";
import {resolveFlow} from '@erpx/workflow-server/public/flow-routing';
import { withPolicyReadSnapshot } from "@erpx/identity-access-server/public/iam-read-snapshot";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { createWorkReports, type WorkReportUnit, type WorkReportProjection } from "@erpx/work-projection-server/application/work-reports";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { workReportPeoplePayrollFacts } from "@erpx/people-payroll-server/public/work-report-facts";
import { workReportOrganizationFacts } from "@erpx/organization-server/public/work-report-facts";
import { workReportWorkflowFacts } from "@erpx/workflow-server/public/work-report-facts";
import { workReportRecordsFacts } from "@erpx/records-server/public/work-report-facts";
import { workReportPeoplePayrollCandidates } from "@erpx/people-payroll-server/public/work-report-candidates";
import { workReportRecordsCandidates } from "@erpx/records-server/public/work-report-candidates";
import {
  nativeWorkReportConfigured,
  readNativeWorkReport,
  type NativeWorkReportProjection,
} from "@erpx/work-projection-ui/lib/native-work-report";
export { reportQuery, workReportCsv } from "@erpx/work-projection-server/application/work-reports";

type NativeReport = (
  projection: WorkReportProjection,
  exportAll: boolean,
) => Promise<NativeWorkReportProjection>;

/**
 * Builds the Work report unit. Owner facts stay with their owners; the
 * matching/aggregation/pagination projection is the owner-native PostgreSQL
 * read through the private loopback route. There is no SQL or JSON SQL mirror
 * here and no fallback: an unconfigured native reader fails closed.
 */
export function workReportUnit(db: SqlDatabase, report?: NativeReport): WorkReportUnit {
  const pending = new Map<string, Promise<NativeWorkReportProjection>>();
  const native = (projection: WorkReportProjection, exportAll: boolean) => {
    if (!report)
      throw new ApiError(
        503,
        "NATIVE_WORK_REPORT_UNAVAILABLE",
        "The native Work report is not configured.",
      );
    const key = JSON.stringify([
      projection.mode,
      projection.scoped,
      projection.start,
      projection.end,
      projection.page,
      exportAll,
    ]);
    let request = pending.get(key);
    if (!request) {
      request = report(projection, exportAll);
      pending.set(key, request);
    }
    return request;
  };
  return {
        candidates: async (companyId) => (await Promise.all([workReportPeoplePayrollCandidates(db, companyId), workReportRecordsCandidates(db, companyId)])).flat(),
        access: (actor, kind, id) => requireFlowAccess(actor, kind, id, db), flow: (actor, kind, id) => resolveFlow(db, actor, kind, id),
        people: (companyId: string) => workReportPeoplePayrollFacts(db).people(companyId),
        nodes: (companyId: string) => workReportOrganizationFacts(db).nodes(companyId),
        decisions: (companyId: string, ids: string) => workReportWorkflowFacts(db).decisions(companyId, ids),
        events: (companyId: string, ids: string) => workReportRecordsFacts(db).events(companyId, ids),
        savedRoutes: (companyId: string, ids: string) => workReportWorkflowFacts(db).savedRoutes(companyId, ids),
        counts: async (projection: WorkReportProjection) => (await native(projection, false)).counts,
        money: async (projection: WorkReportProjection) => ({results: (await native(projection, false)).money}),
        estimates: async (projection: WorkReportProjection) => ({results: (await native(projection, false)).estimates}),
        stageCounts: async (projection: WorkReportProjection) => ({results: (await native(projection, false)).stageCounts}),
        page: async <Row>(projection: WorkReportProjection) => ({results: (await native(projection, false)).rows.map(row => ({row} as {row: Row}))}),
        allRows: async <Row>(projection: WorkReportProjection) => ({results: (await native(projection, true)).allRows.map(row => ({row} as {row: Row}))}),
    };
}

export function workReports(actor: WorkContext, raw: Record<string, unknown>, exportAll = false) {
  return withPolicyReadSnapshot(async () => {
    if (!nativeWorkReportConfigured())
      throw new ApiError(
        503,
        "NATIVE_WORK_REPORT_CONFIGURATION_REQUIRED",
        "The native Work report is not configured.",
      );
    const authority = actor.authority;
    if (!authority)
      throw new ApiError(409, "WORK_CONTEXT_CHANGED", "The current work context changed.");
    const scope = actor.scope;
    const branchSlug = scope.kind === "branch" || scope.kind === "department" ? scope.branchSlug : undefined;
    const departmentSlug = scope.kind === "department" ? scope.departmentSlug : undefined;
    const report: NativeReport = (projection, exportAll) =>
      readNativeWorkReport(
        {
          mode: projection.mode,
          scoped: projection.scoped,
          start: projection.start,
          end: projection.end,
          page: projection.page,
          exportAll,
        },
        {
          companyId: actor.companyId,
          assignmentId: actor.assignmentId,
          revision: authority.revision,
          assignmentVersion: authority.assignmentVersion,
          branchSlug,
          departmentSlug,
        },
      );
    const service = createWorkReports({
      resource: async (holdingId) => workReportUnit((await holdingResources(holdingId)).write, report),
      now: Date.now,
      error: (...args) => new ApiError(...args),
      errorStatus: error => error instanceof ApiError ? error.status : undefined,
    });
    return service.workReports(actor, raw, exportAll);
  });
}
