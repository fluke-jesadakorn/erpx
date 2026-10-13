import type { PermissionActor } from "@/lib/identity_access/iam";
export type IamActor = PermissionActor & {
    userId: string;
    assignmentId: string;
    companyId: string;
    sessionId: string;
    holdingId: string | null;
    scopeKind: string;
    scopeNodeId: string | null;
    branchId: string | null;
    departmentId: string | null;
    platform: boolean;
    authority?: {
        revision: number;
        assignmentVersion: number;
        sessionId: string;
    };
};
export function iamPermission(actor: IamActor, id: string) {
    return actor.platform ? `platform.${id}` : id;
}
