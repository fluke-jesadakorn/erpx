import { portalCollaborationAttachmentPolicies } from "@erpx/server-files/domain/attachments/portal-collaboration-policy";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { PostRow } from "@erpx/server-collaboration/public/social-source";
import type { Clock } from "@erpx/server-shared/application/ports";
import type { AttachmentOperation, AttachmentPolicy, AttachmentTargetScope } from "@erpx/server-files/domain/attachments/types";
export type PortalAttachmentUnit<Effect> = {
    readPortalBinding(facts: {
        external: boolean;
        values: Array<string | number>;
    }): Promise<PortalBindingRow | null>;
    readablePost(actor: WorkContext, id: string): Promise<PostRow>;
    versionGuard(commandId: string, targetType: string, targetId: string, companyId: string, version: number, index: number): Effect | null;
    versionStep(targetType: string, targetId: string, expectedVersion: number, now: number): Effect | null;
};
export type PortalBindingRow = {
    company_id: string;
    branch_id: string | null;
    department_id: string | null;
    version: number;
};
export function createPortalAttachmentPolicy<Effect>(ports: {
    clock: Clock;
    error(status: number, code: string, message: string): Error;
    getSupportIncident(actor: WorkContext, id: string): Promise<Record<string, unknown>>;
}) {
    async function portalBindingScope(db: PortalAttachmentUnit<Effect>, actor: WorkContext, targetId: string, operation: AttachmentOperation): Promise<AttachmentTargetScope> {
        const external = actor.role === "external_customer" &&
            actor.assignmentId === `customer-portal:${targetId}`;
        if (!external && operation !== "read")
            throw ports.error(403, "PORTAL_ATTACHMENT_FORBIDDEN", "Only the verified customer may add evidence to this request.");
        if (!external && !hasPermission(actor, "commerce.read") && !hasPermission(actor, "support.read"))
            throw ports.error(403, "PORTAL_ATTACHMENT_FORBIDDEN", "This customer evidence is unavailable to the current role.");
        const now = ports.clock.now();
        const row = await db.readPortalBinding({ external, values: external ? [actor.userId, actor.email.toLowerCase(), targetId, actor.companyId, now] : [targetId, actor.companyId, now] });
        if (!row)
            throw ports.error(404, "PORTAL_ATTACHMENT_TARGET_NOT_FOUND", "Customer evidence is unavailable.");
        if (!external && (actor.branchId && actor.branchId !== row.branch_id || actor.departmentId && actor.departmentId !== row.department_id))
            throw ports.error(404, "PORTAL_ATTACHMENT_TARGET_NOT_FOUND", "Customer evidence is unavailable in this selected work scope.");
        return {
            companyId: row.company_id,
            branchId: row.branch_id,
            departmentId: row.department_id,
            version: Number(row.version),
            editable: external && operation !== "read",
        };
    }
    async function incidentScope(actor: WorkContext, targetId: string, operation: AttachmentOperation): Promise<AttachmentTargetScope> {
        const row = await ports.getSupportIncident(actor, targetId) as Record<string, unknown>;
        const manager = hasPermission(actor, "support.manage") && Number(row.reviewer_eligible) > 0;
        const assigned = hasPermission(actor, "support.work") && row.owner_id === actor.userId && Number(row.owner_eligible) > 0;
        if (!manager && !assigned)
            throw ports.error(403, "INCIDENT_ATTACHMENT_FORBIDDEN", "Incident files are limited to the current owner and eligible Support managers.");
        if (operation !== "read" && !["open", "investigating"].includes(String(row.status)))
            throw ports.error(409, "INCIDENT_ATTACHMENT_LOCKED", "Incident evidence is frozen after mitigation is proposed.");
        return {
            companyId: String(row.company_id),
            branchId: row.branch_id ? String(row.branch_id) : null,
            departmentId: row.department_id ? String(row.department_id) : null,
            version: Number(row.version),
            editable: operation !== "read",
        };
    }
    async function socialPostScope(db: PortalAttachmentUnit<Effect>, actor: WorkContext, policy: AttachmentPolicy, targetId: string, operation: AttachmentOperation): Promise<AttachmentTargetScope> {
        const row = await db.readablePost(actor, targetId);
        const expectedPurpose = row.purpose === "handover" ? "handover_file" : "thread_file";
        if (policy.purposeKey !== expectedPurpose)
            throw ports.error(422, "SOCIAL_ATTACHMENT_PURPOSE", "Choose the file purpose that matches this conversation.");
        if (operation !== "read" && row.author_id !== actor.userId)
            throw ports.error(403, "SOCIAL_ATTACHMENT_AUTHOR_REQUIRED", "Only the conversation author may add a file revision.");
        return {
            companyId: row.company_id,
            branchId: row.branch_id,
            departmentId: row.department_id,
            version: Number(row.version),
            editable: operation !== "read",
        };
    }
    async function authorizePortalCollaborationAttachmentTarget(db: PortalAttachmentUnit<Effect>, actor: WorkContext, policy: AttachmentPolicy, targetId: string, operation: AttachmentOperation): Promise<AttachmentTargetScope | null> {
        if (policy.targetType === "customer_access_binding")
            return portalBindingScope(db, actor, targetId, operation);
        if (policy.targetType === "support_incident")
            return incidentScope(actor, targetId, operation);
        if (policy.targetType === "social_post")
            return socialPostScope(db, actor, policy, targetId, operation);
        return null;
    }
    function portalCollaborationAttachmentTargetVersionGuard(db: PortalAttachmentUnit<Effect>, commandId: string, targetType: string, targetId: string, companyId: string, version: number, index: number): Effect | null { return db.versionGuard(commandId, targetType, targetId, companyId, version, index); }
    function portalCollaborationAttachmentRecordVersionStep(db: PortalAttachmentUnit<Effect>, targetType: string, targetId: string, expectedVersion: number, now = ports.clock.now()): Effect | null { return db.versionStep(targetType, targetId, expectedVersion, now); }
    return { portalCollaborationAttachmentPolicies, authorizePortalCollaborationAttachmentTarget, portalCollaborationAttachmentTargetVersionGuard, portalCollaborationAttachmentRecordVersionStep };
}
