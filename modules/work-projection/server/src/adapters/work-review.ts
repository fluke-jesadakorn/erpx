import "server-only";
import { publicScope } from "@erpx/identity-access-server/public/authorization";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import {employeeRequestDetail} from '@erpx/records-server/public/employee-requests-read';
import {expenseDetail} from '@erpx/people-payroll-server/public/expenses';
import {getLeave} from '@erpx/people-payroll-server/public/leave-read';
import {taskDetail} from '@erpx/operations-server/public/staff-tasks';
import {resolveFlow} from '@erpx/workflow-server/public/flow-routing';
import { attachmentContentUrl } from "@erpx/files-ui/lib/attachment-client";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { createWorkRead } from "@erpx/work-projection-server/application/work";
import { workReadUnit } from "@erpx/work-projection-server/adapters/work";
import { attachmentsV2Readable } from "@erpx/files-server/public/attachments/feature";
import { workReviewPeopleFacts } from "@erpx/people-payroll-server/public/work-review-facts";
import { createWorkReview, type WorkReviewUnit } from "@erpx/work-projection-server/application/work-review";
export function workReviewUnit(db: SqlDatabase): WorkReviewUnit {
    return {
        ...workReviewPeopleFacts(db), expense: (actor, id) => expenseDetail(actor, id, db), leave: (actor, id) => getLeave(actor.holdingId, id, actor, db),
        leaveEvidence: (actor, id) => createWorkRead({ resource: async () => workReadUnit(db), readable: attachmentsV2Readable }).getLeaveEvidence(actor.holdingId, id, actor),
        task: (actor, id) => taskDetail(db, actor, id), request: (actor, id) => employeeRequestDetail(db, actor, id), flow: (actor, kind, id) => resolveFlow(db, actor, kind, id)
    };
}
const service = createWorkReview({ resource: async (holdingId) => workReviewUnit((await holdingResources(holdingId)).write), publicScope, contentUrl: attachmentContentUrl, error: (...args) => new ApiError(...args), errorStatus: error => error instanceof ApiError ? error.status : undefined });
export const getWorkReview = service.getWorkReview;
