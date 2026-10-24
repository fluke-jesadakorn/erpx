import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {approvalView} from '@erpx/server-workflow/public/approval-planning';
import { ApiError } from "@erpx/server-runtime/public/http";
import { attachmentsV2Enabled } from "@erpx/server-files/public/attachments/feature";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { BusinessRuleError } from '@erpx/server-shared/index';
import { leaveEvidenceSources } from '@erpx/server-files/public/leave-evidence-sources';
import { leaveEvidenceDecisions } from '@erpx/server-workflow/public/leave-evidence-decisions';
import { type LeaveEvidencePurpose, type LeaveEvidencePolicy, type LeaveAttachmentTarget } from '@erpx/server-people_payroll/domain/leave-evidence';
import { createLeaveEvidenceApplication, type LeaveEvidenceRepository } from '@erpx/server-people_payroll/application/leave-evidence';
export { leaveEvidencePurposes, parseLeaveEvidencePolicy } from '@erpx/server-people_payroll/domain/leave-evidence';
export type { LeaveEvidencePolicy, LeaveEvidencePurpose } from '@erpx/server-people_payroll/domain/leave-evidence';
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
