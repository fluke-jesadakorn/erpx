import "server-only";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { approvalView } from "@erpx/server-workflow/public/approvals";
import { getLeave } from "@erpx/server-people_payroll/public/leave";
import { assertLeaveAttachmentAccess } from "@erpx/server-people_payroll/public/leave-evidence";
import { attachmentsV2Readable } from "@erpx/server-files/public/attachments/feature";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { createWorkRead, type WorkReadUnit } from "@erpx/server-work_projection/application/work";
import { workReadPeoplePayrollFacts } from "@erpx/server-people_payroll/public/work-read-facts";
import { workReadFilesFacts } from "@erpx/server-files/public/work-read-facts";
export function workReadUnit(db: SqlDatabase): WorkReadUnit {
    return { pendingLeave: facts => workReadPeoplePayrollFacts(db).pendingLeave(facts),
        visibleLeave: facts => workReadPeoplePayrollFacts(db).visibleLeave(facts),
        evidenceSets: facts => workReadFilesFacts(db).evidenceSets(facts),
        legacyEvidence: facts => workReadFilesFacts(db).legacyEvidence(facts), approval: (actor, id) => approvalView(db, "leave", id, actor), leave: (holdingId, id, actor) => getLeave(holdingId, id, actor, db), evidenceAccess: (actor, input) => assertLeaveAttachmentAccess(db, actor, input) };
}
const service = createWorkRead({ resource: async (holdingId) => workReadUnit((await holdingResources(holdingId)).write), readable: attachmentsV2Readable });
export const getWork = service.getWork, getLeaveEvidence = service.getLeaveEvidence;
