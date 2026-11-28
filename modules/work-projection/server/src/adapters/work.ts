import "server-only";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import {approvalView} from '@erpx/workflow-server/public/approval-planning';
import {getLeave} from '@erpx/people-payroll-server/public/leave-read';
import { assertLeaveAttachmentAccess } from "@erpx/people-payroll-server/public/leave-evidence";
import { attachmentsV2Readable } from "@erpx/files-server/public/attachments/feature";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { createWorkRead, type WorkReadUnit } from "@erpx/work-projection-server/application/work";
import { workReadPeoplePayrollFacts } from "@erpx/people-payroll-server/public/work-read-facts";
import { workReadFilesFacts } from "@erpx/files-server/public/work-read-facts";
export function workReadUnit(db: SqlDatabase): WorkReadUnit {
    return { pendingLeave: facts => workReadPeoplePayrollFacts(db).pendingLeave(facts),
        visibleLeave: facts => workReadPeoplePayrollFacts(db).visibleLeave(facts),
        evidenceSets: facts => workReadFilesFacts(db).evidenceSets(facts),
        legacyEvidence: facts => workReadFilesFacts(db).legacyEvidence(facts), approval: (actor, id) => approvalView(db, "leave", id, actor), leave: (holdingId, id, actor) => getLeave(holdingId, id, actor, db), evidenceAccess: (actor, input) => assertLeaveAttachmentAccess(db, actor, input) };
}
const service = createWorkRead({ resource: async (holdingId) => workReadUnit((await holdingResources(holdingId)).write), readable: attachmentsV2Readable });
export const getWork = service.getWork, getLeaveEvidence = service.getLeaveEvidence;
