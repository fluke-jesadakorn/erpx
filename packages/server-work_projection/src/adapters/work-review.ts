import "server-only";
import { publicScope } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { employeeRequestDetail } from "@erpx/server-records/public/employee-requests";
import { expenseDetail } from "@erpx/server-people_payroll/public/expenses";
import { getLeave } from "@erpx/server-people_payroll/public/leave";
import { taskDetail } from "@erpx/server-operations/public/staff-tasks";
import { resolveFlow } from "@erpx/server-workflow/public/flow-routing";
import { attachmentContentUrl } from "@erpx/feature-files/lib/attachment-client";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { createWorkRead } from "@erpx/server-work_projection/application/work";
import { workReadUnit } from "@erpx/server-work_projection/adapters/work";
import { attachmentsV2Readable } from "@erpx/server-files/public/attachments/feature";
import { workReviewPeopleFacts } from "@erpx/server-people_payroll/public/work-review-facts";
import { createWorkReview, type WorkReviewUnit } from "@erpx/server-work_projection/application/work-review";
export function workReviewUnit(db: SqlDatabase): WorkReviewUnit {
    return {
        ...workReviewPeopleFacts(db), expense: (actor, id) => expenseDetail(actor, id, db), leave: (actor, id) => getLeave(actor.holdingId, id, actor, db),
        leaveEvidence: (actor, id) => createWorkRead({ resource: async () => workReadUnit(db), readable: attachmentsV2Readable }).getLeaveEvidence(actor.holdingId, id, actor),
        task: (actor, id) => taskDetail(db, actor, id), request: (actor, id) => employeeRequestDetail(db, actor, id), flow: (actor, kind, id) => resolveFlow(db, actor, kind, id)
    };
}
const service = createWorkReview({ resource: async (holdingId) => workReviewUnit((await holdingResources(holdingId)).write), publicScope, contentUrl: attachmentContentUrl, error: (...args) => new ApiError(...args), errorStatus: error => error instanceof ApiError ? error.status : undefined });
export const getWorkReview = service.getWorkReview;
