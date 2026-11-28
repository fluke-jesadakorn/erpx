import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import {approvalView} from '@erpx/workflow-server/public/approval-planning';
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { attachmentsV2Enabled } from "@erpx/files-server/public/attachments/feature";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
import { leaveEvidenceSources } from '@erpx/files-server/public/leave-evidence-sources';
import { leaveEvidenceDecisions } from '@erpx/workflow-server/public/leave-evidence-decisions';
import { type LeaveEvidencePurpose, type LeaveEvidencePolicy, type LeaveAttachmentTarget } from '@erpx/people-payroll-server/domain/leave-evidence';
import { createLeaveEvidenceApplication, type LeaveEvidenceRepository } from '@erpx/people-payroll-server/application/leave-evidence';
export { leaveEvidencePurposes, parseLeaveEvidencePolicy } from '@erpx/people-payroll-server/domain/leave-evidence';
export type { LeaveEvidencePolicy, LeaveEvidencePurpose } from '@erpx/people-payroll-server/domain/leave-evidence';
function application(database: SqlDatabase) {
    const repository: LeaveEvidenceRepository = { target(leaveRequestId: string, companyId: string) {
            return database.prepare(`SELECT id,company_id,branch_id,department_id,created_by,state,version
       FROM leave_requests WHERE id=? AND company_id=?`)
                .bind(leaveRequestId, companyId)
                .first<LeaveAttachmentTarget>();
        },
        recordedDecision(...args) { return leaveEvidenceDecisions(database).recordedDecision(...args); },
        availableCanonical(...args) { return leaveEvidenceSources(database).availableCanonical(...args); },
        availableLegacy(...args) { return leaveEvidenceSources(database).availableLegacy(...args); },
        canonicalManifest(...args) { return leaveEvidenceSources(database).canonicalManifest(...args); },
        legacyManifest(...args) { return leaveEvidenceSources(database).legacyManifest(...args); },
        pendingEvidence(...args) { return leaveEvidenceSources(database).pendingEvidence(...args); } };
    return createLeaveEvidenceApplication({ repository, canonicalAttachments: attachmentsV2Enabled, approval: (kind, id, actor) => approvalView(database, kind, id, actor) });
}
async function invoke<Result>(run: () => Promise<Result>) {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}
export async function assertLeaveAttachmentAccess(db: SqlDatabase, actor: WorkContext, input: {
    leaveRequestId: string;
    purposeKey: LeaveEvidencePurpose;
    operation: "read" | "write" | "bind";
}) { return invoke(() => application(db).assertLeaveAttachmentAccess(actor, input)); }
export async function availableLeaveEvidence(db: SqlDatabase, leaveRequestId: string, policy: Pick<LeaveEvidencePolicy, "purposeKey" | "minimumCount">) { return invoke(() => application(db).availableLeaveEvidence(leaveRequestId, policy)); }
export async function leaveEvidenceManifest(db: SqlDatabase, leaveRequestId: string) { return invoke(() => application(db).leaveEvidenceManifest(leaveRequestId)); }
export async function pendingLeaveEvidenceCount(db: SqlDatabase, leaveRequestId: string) { return invoke(() => application(db).pendingLeaveEvidenceCount(leaveRequestId)); }
