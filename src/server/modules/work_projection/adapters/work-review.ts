import "server-only";
import { publicScope } from "@/server/modules/identity_access/public/authorization";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { employeeRequestDetail } from "@/server/modules/records/public/employee-requests";
import { expenseDetail } from "@/server/modules/people_payroll/public/expenses";
import { getLeave } from "@/server/modules/people_payroll/public/leave";
import { taskDetail } from "@/server/modules/operations/public/staff-tasks";
import { resolveFlow } from "@/server/modules/workflow/public/flow-routing";
import { attachmentContentUrl } from "@/lib/files/attachment-client";
import { ApiError } from "@/server/modules/runtime/public/http";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import { createWorkRead } from "../application/work";
import { workReadUnit } from "./work";
import { attachmentsV2Readable } from "../../files/public/attachments/feature";
import { workReviewPeopleFacts } from "../../people_payroll/public/work-review-facts";
import { createWorkReview, type WorkReviewUnit } from "../application/work-review";
export function workReviewUnit(db: SqlDatabase): WorkReviewUnit {
    return {
        ...workReviewPeopleFacts(db), expense: (actor, id) => expenseDetail(actor, id, db), leave: (actor, id) => getLeave(actor.holdingId, id, actor, db),
        leaveEvidence: (actor, id) => createWorkRead({ resource: async () => workReadUnit(db), readable: attachmentsV2Readable }).getLeaveEvidence(actor.holdingId, id, actor),
        task: (actor, id) => taskDetail(db, actor, id), request: (actor, id) => employeeRequestDetail(db, actor, id), flow: (actor, kind, id) => resolveFlow(db, actor, kind, id)
    };
}
const service = createWorkReview({ resource: async (holdingId) => workReviewUnit((await holdingResources(holdingId)).write), publicScope, contentUrl: attachmentContentUrl, error: (...args) => new ApiError(...args), errorStatus: error => error instanceof ApiError ? error.status : undefined });
export const getWorkReview = service.getWorkReview;
