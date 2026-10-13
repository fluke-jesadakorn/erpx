import { hasPermission } from "@/lib/identity_access/permissions";
import type { WorkContext } from '../../identity_access/public/authorization';
import type { AttachmentManifestItem } from '../../files/public/attachments/types';
import type { ApprovalView } from '../../workflow/public/approvals';
import { BusinessRuleError } from '../../shared';
import { leaveEvidencePurposes, type LeaveEvidencePurpose, type LeaveEvidencePolicy, type LeaveAttachmentTarget, parseLeaveEvidencePolicy } from '../domain/leave-evidence';
export interface LeaveEvidenceRepository {
    target(leaveRequestId: string, companyId: string): Promise<LeaveAttachmentTarget | null>;
    recordedDecision(companyId: string, leaveRequestId: string, userId: string): Promise<Record<string, unknown> | null>;
    availableCanonical(leaveRequestId: string, purposeKey: "medical_certificate" | "leave_supporting_photo"): Promise<{
        results: {
            id: string;
            source_id: string;
        }[];
    }>;
    availableLegacy(leaveRequestId: string, purposeKey: "medical_certificate" | "leave_supporting_photo"): Promise<{
        results: {
            id: string;
        }[];
    }>;
    canonicalManifest(leaveRequestId: string): Promise<{
        results: {
            set_id: string;
            set_state: "draft" | "bound";
            purpose_key: string;
            link_id: string;
            source_id: string;
            checksum_sha256: string;
            detected_media_type: string | null;
            media_type: string;
            bytes: number;
        }[];
    }>;
    legacyManifest(leaveRequestId: string): Promise<{
        results: {
            id: string;
            checksum_sha256: string;
            detected_media_type: string | null;
            media_type: string;
            bytes: number;
            purpose: string;
        }[];
    }>;
    pendingEvidence(leaveRequestId: string): Promise<{
        count: number;
    } | null>;
}
export interface LeaveEvidencePorts {
    repository: LeaveEvidenceRepository;
    canonicalAttachments(): boolean;
    approval(kind: 'leave', id: string, actor: WorkContext): Promise<ApprovalView>;
}
export function createLeaveEvidenceApplication(ports: LeaveEvidencePorts) {
    const repository = ports.repository;
    async function leaveAttachmentTarget(actor: WorkContext, leaveRequestId: string) {
        const row = await repository.target(leaveRequestId, actor.companyId);
        if (!row)
            throw new BusinessRuleError(404, "NOT_FOUND", "Leave request not found.");
        return row;
    }
    async function recordedLeaveDecision(actor: WorkContext, leaveRequestId: string) {
        return Boolean(await repository.recordedDecision(actor.companyId, leaveRequestId, actor.userId));
    }
    async function assertLeaveAttachmentAccess(actor: WorkContext, input: {
        leaveRequestId: string;
        purposeKey: LeaveEvidencePurpose;
        operation: "read" | "write" | "bind";
    }): Promise<{
        companyId: string;
        branchId: string | null;
        departmentId: string | null;
        version: number;
        editable: boolean;
    }> {
        if (!leaveEvidencePurposes.includes(input.purposeKey))
            throw new BusinessRuleError(422, "LEAVE_EVIDENCE_PURPOSE", "Choose a supported leave evidence purpose.");
        const row = await leaveAttachmentTarget(actor, input.leaveRequestId);
        const requester = row.created_by === actor.userId;
        const editable = requester && ["draft", "rejected", "pending_approval"].includes(row.state);
        if (input.operation === "write") {
            if (!editable)
                throw new BusinessRuleError(403, "LEAVE_EVIDENCE_REQUESTER_REQUIRED", "Only the requester can add or withdraw leave evidence before final approval.");
        }
        else {
            const privileged = hasPermission(actor, "leave.evidence.read");
            const priorDecision = await recordedLeaveDecision(actor, row.id);
            const currentView = row.state === "pending_approval"
                ? await ports.approval("leave", row.id, actor)
                : null;
            const currentApprover = Boolean(currentView?.canDecide || currentView?.canOverride);
            const allowed = requester || privileged || priorDecision || currentApprover;
            if (!allowed)
                throw new BusinessRuleError(403, "LEAVE_EVIDENCE_FORBIDDEN", "Restricted leave evidence is available only to the requester, the current reviewer, recorded decision makers, and authorized HR.");
            if (input.operation === "bind" && !currentApprover)
                throw new BusinessRuleError(403, "CURRENT_APPROVER_REQUIRED", "Only the current eligible approver can bind the reviewed leave evidence.");
        }
        return {
            companyId: row.company_id,
            branchId: row.branch_id || null,
            departmentId: row.department_id || null,
            version: row.version,
            editable,
        };
    }
    async function availableLeaveEvidence(leaveRequestId: string, policy: Pick<LeaveEvidencePolicy, "purposeKey" | "minimumCount">) {
        if (!policy.purposeKey)
            return { count: 0, setIds: [] as string[] };
        const rows = (await repository.availableCanonical(leaveRequestId, policy.purposeKey)).results;
        const sourceIds = new Set(rows.map((row) => row.source_id));
        if (!ports.canonicalAttachments()) {
            const legacy = (await repository.availableLegacy(leaveRequestId, policy.purposeKey)).results;
            for (const source of legacy)
                sourceIds.add(source.id);
        }
        return {
            // Canonical reuse must not let one physical source satisfy a multi-file
            // policy more than once, even if it has several links or revisions.
            count: sourceIds.size,
            setIds: [...new Set(rows.map((row) => row.id))],
        };
    }
    async function leaveEvidenceManifest(leaveRequestId: string): Promise<{
        manifest: AttachmentManifestItem[];
        canonicalManifest: AttachmentManifestItem[];
        legacyManifest: AttachmentManifestItem[];
        legacyIncluded: boolean;
        draftSetIds: string[];
    }> {
        const rows = (await repository.canonicalManifest(leaveRequestId)).results;
        const canonicalManifest = rows.map((row) => ({
            linkId: row.link_id,
            sourceId: row.source_id,
            purposeKey: row.purpose_key,
            checksumSha256: row.checksum_sha256,
            mediaType: row.detected_media_type ?? row.media_type,
            bytes: Number(row.bytes),
        }));
        const legacyIncluded = !ports.canonicalAttachments();
        const legacyManifest: AttachmentManifestItem[] = [];
        if (legacyIncluded) {
            const legacy = (await repository.legacyManifest(leaveRequestId)).results;
            legacyManifest.push(...legacy.map((source) => ({
                linkId: source.id,
                sourceId: source.id,
                purposeKey: leaveEvidencePurposes.includes(source.purpose as LeaveEvidencePurpose)
                    ? source.purpose
                    : "leave_supporting_photo",
                checksumSha256: source.checksum_sha256,
                mediaType: source.detected_media_type ?? source.media_type,
                bytes: Number(source.bytes),
            })));
        }
        return {
            manifest: [...canonicalManifest, ...legacyManifest],
            canonicalManifest,
            legacyManifest,
            legacyIncluded,
            draftSetIds: [
                ...new Set(rows.filter((row) => row.set_state === "draft").map((row) => row.set_id)),
            ],
        };
    }
    async function pendingLeaveEvidenceCount(leaveRequestId: string) {
        const row = await repository.pendingEvidence(leaveRequestId);
        return Number(row?.count ?? 0);
    }
    return { parseLeaveEvidencePolicy, assertLeaveAttachmentAccess, availableLeaveEvidence, leaveEvidenceManifest, pendingLeaveEvidenceCount };
}
